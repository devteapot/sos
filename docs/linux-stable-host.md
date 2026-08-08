# Linux stable-host vertical slice

Date: 2026-08-08

SOS now has a real Linux presentation host. It is an ordinary native Wayland
client today; it is not yet the Wayland compositor. This deliberately proves
the generated experience, permanent-host lifecycle, and local service boundary
before adding session ownership, DRM/KMS, and compatibility-client policy.

## Current process and presentation boundary

```text
revision supervisor
    | newline JSON: boot / prepare / present / confirm / discard / shutdown
    v
permanent sos-experience-host process
    | Luau -> validated Scene ABI v3 -> retained GPUI elements
    v
one GPUI Wayland surface
    v
existing Wayland compositor
```

`linux-run` also owns a sibling provider/state authority. The Linux-only
`sos-linux-session` helper verifies the revision store, bootstraps an empty
authority from the committed pointer, and stages each candidate before asking
the coordinated supervisor to activate it. The same authority socket is passed
to the supervisor and permanent host; either top-level service exiting tears
down the developer session. A non-empty mismatch remains fatal unless the
durable activation journal binds the pointer as its previous revision and the
authority as its candidate; that one crash boundary is handed to the existing
coordinator recovery path instead of being mistaken for corruption.

`experience-host-protocol` owns the transport ABI shared by the supervisor and
host. Host stdout is reserved for newline-delimited protocol events; all GPUI,
runtime, timing, and recovery diagnostics go to stderr. A candidate is compiled
and rendered in a fresh Luau VM while the accepted scene remains active. On
`present`, the worker commits that prepared VM and the same GPUI entity renders
the new scene. In an ordinary Wayland session, a GPUI next-frame callback emits
`presented`. Under `sos-compositor`, the host instead arms the scene handoff and
waits for the compositor to observe its later shell commit in a successful
nested backend submit. A later `confirm` request proves that the event loop is
still responsive before the supervisor advances `current`.

The Linux adapter consumes Scene ABI v3: orthogonal layout/content/paint/
interaction/animation facets, bounded containing-block layout programs, nested
clips/transforms/layers, host-shaped glyph runs, revision images/fonts, and tap,
double-tap, long-press, swipe, drag, drop, and single-pointer events all render
through GPUI. Android and Linux use the same revision asset/font registry and
retained paint/gesture surface. A small shared hook lets Android register each
surface with its raw NDK multi-pointer router without making that router part of
the Linux client.

The host validates manifest format 3, file sizes, SHA-256 values, source/state
binding, schema, and API version 3 again before activation. It separately loads
and re-verifies the candidate revision's content-addressed image/font/shader
sidecars, then supplies only that set to the candidate's fresh VM. A candidate
therefore cannot inherit assets from the boot or accepted revision. Wayland
objects and file descriptors are never exposed to Luau.

When `--service-socket` is configured, interaction results are committed on a
background thread through the versioned provider/state Unix-socket protocol.
The Linux boundary currently maps only the allowed typed
`notes.attach_to_event` effect; unknown actions are rejected. It reconciles an
ambiguous promotion by stable transaction ID and accepts the new scene only when
the authority returns the exact expected state.

## Run it in an existing Wayland session

On a Linux desktop or VM logged into a Wayland session:

```sh
./tools/sosctl linux-run --windowed
```

Omit `--windowed` for a fullscreen shell surface. The first run creates an
ignored content-addressed store at `.cache/linux-revisions-v3`, installs
`experiences/default.luau`, and boots it through the real supervisor. The
command stays in the foreground so host and recovery logs remain visible.

From a second terminal:

```sh
./tools/sosctl linux-script experiences/daily-flow.luau
./tools/sosctl linux-status
./tools/sosctl linux-stop
```

`linux-script` accepts an optional second `state.json` argument and otherwise
starts the candidate with `{}`. It stages that immutable state in the authority,
then supplies the stable transaction ID to the coordinated supervisor. Set
`SOS_LINUX_REVISION_ROOT` to use a disposable or isolated revision store. Use
the supervisor's `install --asset ID:KIND:FILE` interface described in
[`revision-supervisor.md`](revision-supervisor.md) when testing v3 sidecars.
The shell helper is development orchestration; a later appliance/session
manager must own the same lifecycle without granting generated code process
authority.

The reproducible Debian 13 guest definition, provisioning command, and nested
acceptance gate are in [`linux-vm.md`](linux-vm.md).
The authenticated Smithay shell, compatibility-surface policy, input fence,
crash recovery, and compositor-owned submit evidence are in
[`linux-compositor.md`](linux-compositor.md).

The executable link on Debian/Ubuntu requires the XKB development libraries in
addition to the usual Rust/GPUI Linux prerequisites:

```sh
sudo apt-get install libxkbcommon-dev libxkbcommon-x11-dev
```

## Local evidence

The first nested proof ran on ARM64 Ubuntu 24.04 with Weston 13.0.0. Weston ran
with its X11 backend inside Xvfb to supply a real `wl_seat`; its headless backend
has no seat, and the pinned GPUI Wayland client currently unwraps that global at
startup. Rendering fell back to Mesa software paths because the user could not
open the host render node. That is sufficient functional evidence, not a GPU or
latency result.

After rebasing onto Scene ABI v2, the real host booted revision `ff63f61d…` and
emitted a GPUI next-frame event in PID 1527912. It prepared and committed
`daily-flow.luau` as revision `bc81479e…` in the same PID (116 us queue,
1,194 us compile, 646 us render, 1,848 us worker total). It then activated the
richer `android-exit-agent.luau` scene as revision `99ba2162…`, exercising
layers, shaped glyphs, gestures, and a revision SVG in that same process (26 us
queue, 1,936 us compile, 665 us render, 2,608 us total). The deliberately
infinite revision `628cb7a7…` was interrupted and rejected while `99ba2162…`
remained accepted in PID 1527912. Killing that host made the supervisor boot the
exact committed revision in PID 1528477 and report `HostRestarted`.

The v3 integration reran the nested proof in the same environment. The host
booted `f174e726…` in PID 1606742, then prepared and presented sidecar-backed
revision `728f905e…` in that same PID (119 us queue, 874 us compile, 157 us
render, 1,039 us worker total). Its 986-byte source
`tests/fixtures/sidecar-image.luau` (SHA-256 `3ec9aa6d…`) references the checked-in
4,021-byte PNG `mipmap-mdpi/ic_launcher.png` (SHA-256 `11ddafaa…`) through the
supervisor manifest rather than embedding it in Luau. Infinite revision
`632ce86e…` was rejected on its Luau time budget while `728f905e…` and PID
1606742 remained accepted. Killing the host restarted that exact committed
sidecar revision in PID 1607073 with a 1,089 us worker initialization and a new
GPUI frame.

The Linux host unit suite also starts the real provider/state daemon on a local
Unix socket, commits state plus one typed notes attachment, shuts the service
down, restarts it from the durable authority file, and reads the same attachment
back through the socket. The combined workspace suite additionally proves that
each prepared worker candidate receives its own sidecar set and that the Linux
manifest loader carries verified sidecars into the runtime boundary.

The coordinated executable rerun used an isolated Xvfb/Weston seat and store.
It bootstrapped authority revision `f174e726…`, booted the real host in PID
1661845, staged transaction `linux-activate-1-552f0696…`, and activated revision
`552f0696…` in that same PID. The worker reported 34 us queue, 1,162 us compile,
654 us render, and 1,825 us total. After activation the durable authority and
supervisor `current` pointer both named the full candidate revision. `linux-stop`
shut down both top-level services, and the exact disposable read-only store was
made owner-writable and removed. This was still the ARM64 Ubuntu nested setup,
not the Debian VM gate.

The reproducible VM gate then passed in an ARM64 Debian 13.6 guest on kernel
`6.12.100+deb13-arm64`, provisioned from the verified official generic image.
Weston 14.0.2 ran inside Xvfb 21.1.16 with Mesa Vulkan 25.0.7. The real host
booted `f174e726…` and initialized its worker in 3,635 us, then activated
`552f0696…` through `linux-activate-1-552f0696…` in unchanged PID 3874. That
candidate measured 374 us queue, 1,147 us compile, 651 us render, and 1,807 us
worker total. The exact authority revision and supervisor pointer matched after
the GPUI frame and confirmation. This completes the functional client-host VM
gate, not a GPU, compositor-presentation, or latency gate.

## Honest boundaries and next gate

- An ordinary `linux-run` still uses GPUI's next-frame callback. The nested
  `sos-compositor` path owns shell-commit/backend-submit evidence but its outer
  compositor can still delay or discard that buffer. The direct Debian VM path
  now waits for the matching DRM VBlank; only physical hardware can turn that
  into a device timing claim.
- The Linux text-input node is display-only. Native editing, selection, marked
  text, clipboard, and Wayland input-method integration remain open.
- Scene semantics remain the source of truth, but Linux does not yet adapt them
  to a native accessibility protocol.
- Conventional GPUI mouse input now emits the v3 single-pointer shape with a
  stable synthetic pointer ID. Native Wayland touch, multi-pointer transforms,
  pressure, and explicit capture policy remain compositor/input-adapter work;
  Android's raw NDK router is intentionally platform-specific.
- The model still comes from `providers-fake`; only state/effect authority uses
  the local service path.
- Linux and Android now share the Luau runtime worker, Scene ABI, revision asset
  and font registry, retained paint/glyph/layer surface, bounded layout-program
  mapping, validation rules, and activation semantics. Platform lifecycle,
  layout/content details, raw pointer collection, text editing, and
  accessibility remain separate adapters.
- Both compositor backends quiesce forwarded input from arming through their
  evidence boundary. Direct libinput is wired to the shared router, but cursor,
  touch/multi-pointer policy and injected input evidence remain open.
- The direct path is an SSH-launched seatd session in a VM. It does not prove
  boot-to-SOS, logind/virtual-terminal handoff, physical touch, GPU performance,
  thermals, suspend/resume, or any hardware/latency gate.

The nested and direct Smithay functional slices are complete. The next
architectural slice packages the direct backend as the Debian VM's boot session:
systemd/logind active-VT ownership, ordered services, protected credential
delivery, and recovery without SSH. Physical hardware comes only after that
session is stable.
