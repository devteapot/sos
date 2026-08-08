# SOS compositor gates

Date: 2026-08-09

SOS now has a minimal Smithay compositor with a nested development backend and
a direct DRM/GBM/libinput backend. Both keep Wayland beneath the generated
presentation layer while SOS owns client identity, surface ordering, focus,
input quiescing, and revision evidence. The direct backend has passed in the
reference Debian VM; it is not yet boot-session packaging or physical-hardware
evidence.

## Boundary

```text
revision supervisor
    | boot / prepare / present / confirm
    v
permanent sos-experience-host PID
    | authenticated bounded control protocol
    | register_shell / arm_presentation <- compositor evidence
    |
    | Luau -> Scene ABI v3 -> retained GPUI
    v
authenticated shell wl_surface
    |
sos-compositor (Smithay 0.7.0)
    | one shell + one fixed compatibility toplevel
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

The policy currently admits one fullscreen shell toplevel and one 720 by 520
compatibility toplevel at the deterministic 1280 by 800 test location (280,
140). The compatibility surface is kept above the single shell surface. It is
not embedded into GPUI or exposed as a raw generated node. Popups are
constrained to the output; XWayland, layer shell, arbitrary placement, and
multiple compatibility clients are intentionally outside this gate.

## Activation fence

At `present`, the host first asks the Luau worker to commit the already prepared
VM. When the worker confirms that commit, it performs this handoff on the GPUI
event thread:

1. Ask the compositor to arm the exact request/revision pair and wait for its
   current shell-commit sequence.
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
- a new authenticated PID after forced host failure;
- shell and compatibility role classification plus fixed placement.

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

The current direct backend is intentionally one seat, one DRM device, and one
connected output. Its recovery view is a compositor-owned solid color. Hotplug
requires restart; cursor rendering and injected input evidence are still open.
The VM's software GBM/EGL stack lacks `EGL_WL_bind_wayland_display`, so the
compositor uses its advertised `wl_shm` path for clients. None of this run is a
physical display, latency, touch, GPU-performance, suspend/resume, or thermal
claim.

## Source pin and remaining gate

The crate pins `smithay = 0.7.0`. Its state, input, and nested-winit setup were
reduced from the upstream MIT-licensed `smallvil` and `anvil` examples at tag
`v0.7.0`, commit `a166cf4c94b5aedc332a65aa1dd753e8148829c3`; the required notice is preserved
in `crates/sos-compositor/SMITHAY-LICENSE.txt`. See the
[Smithay repository](https://github.com/Smithay/smithay/tree/v0.7.0) and
[backend documentation](https://smithay.github.io/smithay/smithay/backend/index.html).

The next gate is boot-to-SOS packaging in the same VM: systemd/logind active-VT
ownership, ordered provider/supervisor/compositor startup, system-managed shell
credentials, and a recovery target that does not rely on an SSH-launched seatd
session. Native Linux text editing/IME, clipboard, accessibility,
touch/multi-pointer and cursor policy, hotplug/multiple outputs, XWayland, and
physical-device performance remain open.
