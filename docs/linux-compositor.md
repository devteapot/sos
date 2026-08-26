# SOS compositor gates

Date: 2026-08-09

SOS now has a minimal Smithay compositor with a nested development backend and
a direct DRM/GBM/libinput backend. Both keep Wayland beneath the generated
presentation layer while SOS owns client identity, surface ordering, focus,
input quiescing, and revision evidence. The direct backend and boot-owned
logind/tty session pass in the reference Debian VM. This remains virtual-device,
not physical-hardware, evidence.

## Boundary

```text
revision supervisor
    | boot / prepare / present / confirm
    v
permanent sos-experience-host PID
    | authenticated bounded control protocol
    | register_shell / quiesce_input / arm_presentation <- compositor evidence
    |
    | Luau -> Scene ABI v3 -> retained GPUI
    v
authenticated shell wl_surface
    |
sos-compositor (Smithay 0.7.0)
    | one shell + bounded Wayland/X11 compatibility toplevels
    | shared focus/input/activation policy
    +-- nested winit submit -> outer development compositor
    `-- libseat + udev + DRM/GBM + libinput -> KMS output
```

Luau still sees only the versioned Scene and provider capabilities. It never
receives a Wayland object, socket, file descriptor, placement primitive, or
compositor capability. `compositor-control-protocol` is a separate bounded
newline-JSON ABI between the trusted permanent host and compositor.

The compositor creates a mode-0600 control socket in a caller-owned mode-0700
runtime directory. A client must present the launch token and a PID equal to
the socket's `SO_PEERCRED` PID before opening its GPUI Wayland connection. The
compositor records that PID as the shell; all other Wayland client PIDs are
compatibility clients. This is a development-session authenticator. A
production session must also isolate service users/credentials so another
same-UID process cannot inspect launch credentials.

The policy admits one fullscreen shell, one fixed-policy native Wayland
compatibility toplevel, and—only when explicitly enabled—up to eight bounded
rootless XWayland windows. Compatibility surfaces stay above the shell. They
are not embedded into GPUI or exposed as raw generated nodes. Popups and X11
configure requests are constrained to the aggregate output layout; layer shell
and arbitrary placement remain outside this gate.

## Activation fence

Input quiescing begins before the provider/state authority changes. After the
candidate VM is prepared, the supervisor asks the permanent host to quiesce the
exact candidate revision and waits for the compositor acknowledgement. The
compositor removes keyboard and pointer focus, sends releases for keys/buttons
that the old scene saw pressed, drops subsequent backend events, and suppresses
the eventual release of any device held across the boundary. The host clears
its bounded pending-event and gesture queues. Only then may the coordinator
promote provider/state authority.

At `present`, the host asks the Luau worker to commit the prepared VM. When the
worker confirms that commit, it performs this handoff on the GPUI event thread:

1. Ask the already-quiesced compositor to arm the exact request/revision pair
   and wait for its current shell-commit sequence.
2. Install the worker-confirmed retained scene and assets in the same host.
3. Request the new GPUI frame.
4. Let the compositor tag the first later root shell commit.
5. Keep compositor input quiesced until that shell commit participates in a
   successful nested submit, or until the exact queued direct frame produces a
   DRM VBlank/page-flip event.
6. Return the request ID, revision ID, shell-commit sequence, backend-submit
   sequence, and typed evidence to the host.
7. Only then emit the supervisor's `presented`; `confirm` still proves the host
   loop responds before `current` advances.

If authority promotion aborts before presentation, `discard` resumes the exact
revision-bound compositor fence before discarding the candidate VM. A control
disconnect clears the fence without restoring focus to the unauthenticated
stale shell. Successful presentation restores focus only to the still-live
surface that owned it before quiescing.

Arming at the actual retained-scene handoff matters. Arming in the earlier
supervisor request handler would permit an animated frame from the old scene to
commit while the Luau worker was still switching. Waiting for the worker's
commit result also prevents a visible revision from being certified while its
active VM is unavailable. If arming then fails, the host exits so the supervisor
recovers the durable committed revision instead of continuing with divergent
runtime and visible state.

The compositor also advertises `wp_presentation`. In nested mode it completes
client feedback and the SOS event after a successful winit-backend submit. That
is stronger than GPUI's next-frame callback but the outer compositor may still
discard or delay the buffer. In direct mode, queueing a DRM frame only records
the candidate; `frame_submitted` on the matching VBlank completes Wayland
feedback and emits `drm_page_flip` with the kernel sequence, timestamp, and
clock domain. Only that event releases input and lets the host tell the
supervisor `presented`.

If the permanent host dies, its control disconnect drops an armed fence and
releases input. The supervisor starts the committed revision in a new PID; the
compositor authenticates it, replaces any stale shell surface, and fences the
recovery frame without restarting itself.

## Reproduce

On a Linux development machine with Xvfb, Weston, and
`weston-simple-shm` installed:

```sh
./tools/linux-compositor/verify-nested
```

The verifier builds the locked graph and creates only disposable state. It
runs Weston's X11 backend in Xvfb, runs `sos-compositor` through its Smithay
winit backend, boots the coordinated Linux session inside that compositor,
activates `daily-flow.luau`, kills the exact host PID, waits for supervisor
recovery, and maps `weston-simple-shm` as the compatibility client. It requires:

- unchanged PID across the normal Luau activation;
- exact authority/current-pointer agreement;
- no `gpui_next_frame` fallback;
- three compositor submit fences: boot, activation, and recovered boot;
- native XTest -> Weston -> Smithay -> `wl_keyboard` delivery, exact persisted
  `wayland` text replacement, and Enter submission;
- restricted input-method-v2 attach, pinyin preedit/candidates/selection/CJK
  commit, dead key composition, keyboard grab, popup, and cursor rectangle;
- Wayland clipboard copy/cut/paste ownership through the native editor;
- live Linux-provider note and video-frame mutation, system/resource model
  refresh, and worker rerender without another revision presentation;
- semantic snapshot, next/previous traversal, focus, activation, scrolling,
  selection, copy/cut/paste, status waits, and service recovery after host kill;
- at least one compositor-owned backend event suppressed while activation is
  quiesced;
- a new authenticated PID after forced host failure;
- shell/native-compatibility role classification and fixed placement;
- an opt-in real `Xwayland` process and bounded rootless `xmessage` window.

Expected leading output:

```text
linux_nested_compositor_passed activation_pid=... restarted_pid=... revision_id=... evidence=nested_backend_submit
```

The gate passes both on the ARM64 Ubuntu 24.04 development host and inside the
reference Debian 13.6 ARM64 KVM guest. The Debian run activated revision
`552f06968bbc5c69de3db581454f60d4303289f304eaaf47a6e9dc3200297cdb` in
unchanged PID 11310, then recovered the same committed revision in PID 11514.
Its compositor evidence was:

```text
boot:      commit_sequence=1  submit_sequence=928
activate:  commit_sequence=9  submit_sequence=936
recovery:  commit_sequence=14 submit_sequence=1009
```

The compatibility client mapped at `(280, 140)`. Mesa software rendering is
acceptable for this functional gate and makes no GPU, latency, or physical
display claim.

## Direct Debian VM gate

Run this only inside the disposable reference VM:

```sh
./tools/linux-vm/verify-direct-session
```

The verifier refuses a non-virtualized or non-Debian-13 environment. It builds
the direct feature, records whether GDM was active, stops GDM, acquires `seat0`
through libseat's seatd backend, and restores GDM on every exit path. It
requires the compositor's dark recovery view to page flip before starting the
SOS shell. It then repeats the nested gate's activation, exact host kill and
recovery, authority agreement, and compatibility-client checks, while rejecting
both `gpui_next_frame` and `nested_backend_submit` evidence.

The ARM64 Debian 13.6/KVM run passed with revision
`552f06968bbc5c69de3db581454f60d4303289f304eaaf47a6e9dc3200297cdb` in
unchanged PID 59723 and recovered the committed revision in PID 59849. The
three shell fences were:

```text
boot:      commit_sequence=1  submit_sequence=3
activate:  commit_sequence=14 submit_sequence=11
recovery:  commit_sequence=20 submit_sequence=17
```

All were emitted from DRM VBlank callbacks with monotonic kernel timestamps.
The VirtIO driver reported output sequence `0`; this remains exact driver
metadata, not a fabricated counter. The compositor presented its recovery view
both before boot and between the killed/restarted hosts, and the separate
compatibility client mapped at `(280, 140)`.

The direct backend remains intentionally one seat, but accepts multiple DRM
devices and simultaneous connected outputs. Its default mirror policy computes
the largest logical canvas that fits every connected output, centers that one
canvas on each physical mode, and resizes the shell once. This lets the
Framework's 1920x1200 panel show the same 1920x1080 scene as PiKVM with 60
logical pixels of compositor background above and below. `"layout": "extend"`
retains the connector-sorted horizontal desktop when independent output space
is wanted. Both policies survive connector and whole-device removal/addition.
`SOS_OUTPUT_MODE`, `SOS_OUTPUT_SCALE`, and `SOS_OUTPUT_ROTATION` set boot
configuration. A bounded JSON file selected by `SOS_OUTPUT_CONFIG_FILE` can
change those values and `layout` on a DRM udev event; the backend recreates
outputs without restarting the compositor.
That file can also associate an exact libinput device name with a connector in
`input_outputs`. Absolute mice, touchscreens, and tablets use the configured
connector's logical geometry regardless of connector discovery order. They
remain automatic on a single output, but fail closed when multiple outputs make
an unconfigured route ambiguous or when the configured connector is absent.
Relative pointers stay inside the shared mirror canvas. In extended mode they
traverse the complete connected-output layout and clamp only to the nearest
valid output rectangle, including across gaps between outputs.
The direct VM gate changes Virtual-1 to 1024x768, 1.25 scale, and 180-degree
rotation, then hot-adds Virtual-2 and requires a nonempty first frame and page
flip on that second CRTC. The render graph composites either a live client
cursor surface with its hotspot or a deterministic compositor-owned fallback
above the shell.

The direct verifier creates kernel `uinput` devices named `SOS Gate Keyboard`,
`SOS Gate Relative Pointer`, and `SOS Gate Multitouch`. Direct libinput reports
their add/remove events and the compositor records keyboard, relative motion,
button, and two-slot touch classes. It holds one modifier, one button, and two
contacts through a successful activation and through an injected
before-promotion authority failure. Both paths require `keys=1 buttons=1
touches=2` at quiesce and suppressed releases after presentation or abort.
Revision `250b1573…` activated in PID 8641 and recovered after the exact host
kill in PID 8888 with DRM page-flip evidence.

Activation quiescing includes touch. Existing contacts receive one
`wl_touch.cancel`, their physical motion/release is suppressed across either a
successful presentation or an aborted candidate, and contacts that begin and
end while quiesced never enter the client epoch. A fresh down may reuse a
released slot. Smithay keeps each slot focused on its down target, which is the
Wayland-level capture behavior. The pinned GPUI Wayland client binds
`wl_touch` and forwards down/move/up/cancel into the shared bounded Scene
raw-pointer router, including multi-touch,
surface capture, cancellation, gesture arbitration, and pointer-count updates.
The first direct run exposed a mutable-client borrow across the touch callback;
dispatching callbacks/frames after releasing that borrow fixed the crash.
Core `wl_touch` has no pressure field, so finger samples explicitly report
pressure unavailable. A separate pressure stylus is carried through
tablet-v2; the direct VM gate observes normalized nonzero Scene pressure.
The VM's software GBM/EGL stack lacks `EGL_WL_bind_wayland_display`; the cited
run therefore used the compositor's advertised `wl_shm` path. The direct
backend now also advertises explicit Linux dmabuf feedback from the formats
importable by every renderer with a connected output. It validates each client
dmabuf against those active renderers before completing buffer creation and
updates the feedback across connector/device changes. This is independent of
the optional EGL Wayland binding and leaves `wl_shm` as the safe fallback. The
cited VM result predates that protocol path and remains a software-rendering
result; none of it is a physical display, latency, touch, GPU-performance,
suspend/resume, or thermal claim.

## Source pin and remaining boundary

The crate pins `smithay = 0.7.0`. Its state, input, and nested-winit setup were
reduced from the upstream MIT-licensed `smallvil` and `anvil` examples at tag
`v0.7.0`, commit `a166cf4c94b5aedc332a65aa1dd753e8148829c3`; the required notice is preserved
in `crates/sos-compositor/SMITHAY-LICENSE.txt`. See the
[Smithay repository](https://github.com/Smithay/smithay/tree/v0.7.0) and
[backend documentation](https://smithay.github.io/smithay/smithay/backend/index.html).

Boot-to-SOS packaging now passes in the same VM: a systemd/PAM service owns the
active logind tty1 session, waits for the recovery page flip before provider and
host startup, receives its token through systemd credentials, and recovers both
host and whole-session failures without seatd. The nested campaign additionally
proves the restricted input-method-v2 client, non-Latin candidate/preedit/
commit, dead key, cursor rectangles, Wayland clipboard, Linux semantic service,
live provider/media refresh, and focus/state recovery. The direct recovery
layer is a readable compositor-owned panel with restart, rollback, safe-mode,
and provider-disable controls. Multi-output, runtime mode/scale/rotation,
rootless XWayland, tablet pressure, and executable bounded shaders now have VM
or software-renderer evidence. Physical touch calibration, a physical panel,
real GPU performance, full platform sleep/wake, latency, memory-pressure, and
thermal evidence remain outside the waived hardware gate.
