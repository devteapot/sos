# SOS

SOS is a research prototype for an agent-native operating experience: the user
directs an agent that continuously writes and installs the native interface,
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
The first standalone Linux revision supervisor and its candidate ABI are in
[`docs/revision-supervisor.md`](docs/revision-supervisor.md).
The durable typed provider/state Unix-socket protocol and authority are in
[`docs/provider-state-service.md`](docs/provider-state-service.md).
Their supervisor-owned cross-process promotion journal is in
[`docs/coordinated-promotion.md`](docs/coordinated-promotion.md).
Ongoing experiments, failures, measurements, and decisions are indexed in the
living [`docs/progress.md`](docs/progress.md) ledger.

The current bounded Luau UI IR now includes custom canvas geometry and drag hit
testing, but the first Android-exit audit remains incomplete. Before moving to
a privileged AOSP shell, SOS must prove reliable single-shot task completion,
crash-safe whole-revision promotion, provider/state independence with
migrations, and sustained sub-100-ms presentation on the deeper experience.

## Milestone 0

The upstream GPUI Mobile source is fetched at a pinned commit into `.cache/` and
is never patched. The wrapper builds an optimized ARM64 Rust library, packages
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

A candidate must compile, finish within its time budget, decode to the bounded
UI IR, and render against the current model and state before it replaces the
accepted revision. A rejected candidate leaves the current experience live.
