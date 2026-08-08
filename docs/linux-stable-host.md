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

`experience-host-protocol` owns the transport ABI shared by the supervisor and
host. Host stdout is reserved for newline-delimited protocol events; all GPUI,
runtime, timing, and recovery diagnostics go to stderr. A candidate is compiled
and rendered in a fresh Luau VM while the accepted scene remains active. On
`present`, the worker commits that prepared VM and the same GPUI entity renders
the new scene. A GPUI next-frame callback emits `presented`, and a later
`confirm` request proves that the event loop is still responsive before the
supervisor advances `current`.

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
starts the candidate with `{}`. Set `SOS_LINUX_REVISION_ROOT` to use a disposable
or isolated revision store. The current convenience command intentionally runs
the supervisor in standalone mode and installs source-only candidates. Use the
supervisor's `install --asset ID:KIND:FILE` interface described in
[`revision-supervisor.md`](revision-supervisor.md) when testing v3 sidecars.
Coordinated service startup and authority bootstrap will become the
appliance/session manager's job.

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

## Honest boundaries and next gate

- `presented` currently means GPUI's next-frame callback, not compositor-owned
  proof that a particular buffer reached an output. `sos-compositor` must own
  that stronger acknowledgment.
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
- Input is rejected while a host operation or action is active, but the custom
  compositor must explicitly quiesce input across the coordinated authority
  commit and scene switch.
- This is a nested desktop result. It does not prove boot-to-SOS, direct
  DRM/KMS, virtual-terminal ownership, touch, GPU performance, thermals,
  suspend/resume, or any hardware/latency gate.

The next architectural slice is a small Smithay compositor running nested in a
normal Wayland session. It should authenticate the SOS shell, own surface order
and focus, place one compatibility client, and turn the shell's exact buffer
presentation into the activation fence. Direct VM boot and physical hardware
come only after that nested policy is stable.
