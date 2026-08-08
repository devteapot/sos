# SOS

SOS is a research prototype for an agent-native operating experience: the user
directs an agent that continuously writes and installs the visible experience,
while separately installed providers remain authoritative over data and
actions. It is not intended to become a scriptable Android application or a
fixed catalog of generated widgets. The authoritative end goal and the meaning
of “moving off Android” are defined in [`docs/vision.md`](docs/vision.md).

The project begins inside Android only as a hardware and interaction laboratory.
Milestone 0 intentionally contains no agent, provider protocol, or operating-
system architecture.

**Status:** Milestone 0 is confirmed on a physical Samsung SM-A336B. Milestone
1 is now a GPUI host with a sandboxed Luau experience layer. See the hardware
checks in [`docs/experiment.md`](docs/experiment.md) and the runtime decision in
[`docs/runtime-evaluation.md`](docs/runtime-evaluation.md).
The verified end-to-end results are in
[`docs/vertical-slice.md`](docs/vertical-slice.md).
The worker-thread and 1,000-swap latency gate is in
[`docs/worker-stress-gate.md`](docs/worker-stress-gate.md).
The stateful generated-experience gate, including native input and the
10,000-swap device soak, is in
[`docs/stateful-experience-gate.md`](docs/stateful-experience-gate.md).
The current five-assumption Android-exit audit is in
[`docs/android-exit-gate.md`](docs/android-exit-gate.md).
The stable-host Linux revision supervisor and its Luau activation ABI are in
[`docs/revision-supervisor.md`](docs/revision-supervisor.md).
The real GPUI/Wayland host, same-PID activation evidence, developer commands,
and remaining compositor boundaries are in
[`docs/linux-stable-host.md`](docs/linux-stable-host.md).
The durable typed provider/state Unix-socket protocol and authority are in
[`docs/provider-state-service.md`](docs/provider-state-service.md).
Their supervisor-owned activation journal is in
[`docs/coordinated-activation.md`](docs/coordinated-activation.md).
Ongoing experiments, failures, measurements, and decisions are indexed in the
living [`docs/progress.md`](docs/progress.md) ledger.

Scene ABI v3 lets any retained node combine responsive host layout, content,
nested clips/transforms/layers, host-shaped glyph runs, raw multi-pointer
routing, animation, and semantics without selecting a widget type. Android
adapts the host semantic tree to real per-element accessibility nodes and a
complete composing text bridge; supervisor revisions can carry bounded,
content-addressed image/font/shader sidecars.
The original Android-exit audit passed at prototype scope. SOS has also removed
native experience binaries from the revision format: one permanent Rust/GPUI
host prepares and activates Luau scenes. The stable-host change has both Linux
protocol evidence and a 10,000-swap physical-device gate; see
[`docs/stable-host-device-gate.md`](docs/stable-host-device-gate.md).

## Milestone 0

Milestone 0 fetched an unmodified pinned GPUI Mobile source into `.cache/`.
The permanent host now vendors that same commit under `vendor/gpui-mobile` with
a narrow pre-compatibility raw-touch observation hook, because Android's
`NativeActivity` input stream bypasses Java `dispatchTouchEvent`. The wrapper builds an optimized ARM64 Rust library, packages
it in a debug-signed APK, installs it on a connected phone, launches it, and
follows the process logs:

```sh
./tools/sosctl run
```

Useful individual commands:

```sh
./tools/sosctl doctor
./tools/sosctl sync
./tools/sosctl build
./tools/sosctl install
./tools/sosctl launch
./tools/sosctl logs
```

Use `./tools/sosctl run --no-follow` when a non-blocking command is needed in
automation. Build products are placed in `artifacts/` and are intentionally not
tracked.

The experiment contract and current device results are recorded in
[`docs/experiment.md`](docs/experiment.md).

## Linux stable host

On Linux, SOS can now run the same generated Scene ABI v3 Luau experience in
one permanent native GPUI process inside an existing Wayland session. Android
and Linux share the retained paint/gesture surface, bounded layout-program
mapping, and revision image/font registry:

```sh
./tools/sosctl linux-run --windowed
```

In a second terminal, activate another content-addressed source revision
without replacing the process or native window:

```sh
./tools/sosctl linux-script experiences/daily-flow.luau
./tools/sosctl linux-status
./tools/sosctl linux-stop
```

This is the client-host gate, not yet a complete SOS session. Presentation is
acknowledged by GPUI's next-frame callback; native Linux text editing, provider
session orchestration, compositor-owned presentation evidence, compatibility
surfaces, boot-to-SOS, and direct hardware remain subsequent gates. See
[`docs/linux-stable-host.md`](docs/linux-stable-host.md) for the exact evidence
and limitations.

## Milestone 1 vertical slice

The first end-to-end experience contains synthetic calendar, notes, music, and
weather data. Rust owns GPUI, fake providers, persistence, validation, and
rollback. [`experiences/default.luau`](experiences/default.luau) owns the screen
composition and local behavior.

Build, install, and launch the ARM64 APK:

```sh
./tools/sosctl m1-run
```

Replace only the experience source while the same process and APK stay alive:

```sh
./tools/sosctl script experiences/timeflow.luau
./tools/sosctl validate experiences/daily-flow-agent.luau
./tools/sosctl agent-apply experiences/daily-flow-agent.luau
./tools/sosctl rollback
./tools/sosctl worker-restart
./tools/sosctl stress 10000
```

A candidate must declare Scene ABI v3, compile, finish within its time budget,
decode to a bounded scene, and render against the current model and state before it replaces the
accepted revision. A rejected candidate leaves the current experience live.
