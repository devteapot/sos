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
The passing reproducible Debian 13 VM gate and acceptance command are in
[`docs/linux-vm.md`](docs/linux-vm.md).
The authenticated Smithay compositor, nested and direct-DRM gates,
compatibility-surface policy, and compositor-owned activation fence are in
[`docs/linux-compositor.md`](docs/linux-compositor.md).
The durable typed provider/state Unix-socket protocol and authority are in
[`docs/provider-state-service.md`](docs/provider-state-service.md).
Their supervisor-owned activation journal is in
[`docs/coordinated-activation.md`](docs/coordinated-activation.md).
The bounded resident Pi authoring service and first Linux live-test procedure
are in [`docs/sos-agent.md`](docs/sos-agent.md).
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

The first resident-agent gate can be run deterministically without a model API
call, then repeated from the Luau-authored composer in the live GPUI window:

```sh
./tools/sosctl linux-agent-test
./tools/sosctl linux-agent-run --fake experiences/daily-flow.luau
```

Type “Turn this into a calm daily flow” in the experience's “Make it yours”
field and press Enter. `linux-agent-prompt` remains available as a protocol
diagnostic, but it is not the product interaction boundary.

For a subscription-backed live model, authenticate through Pi's headless
OpenAI Codex flow before starting the resident agent:

```sh
export SOS_AGENT_PROVIDER=openai-codex
export SOS_AGENT_MODEL=gpt-5.6-sol
unset SOS_AGENT_FAKE_SOURCE
./tools/sosctl linux-agent-login
./tools/sosctl linux-agent-run
```

The browser authorization can happen on another machine; the command prints
the device URL and code. See [`docs/sos-agent.md`](docs/sos-agent.md) for the
boot-service procedure and credential boundary.

This command remains the ordinary Wayland client-host gate and uses GPUI's
next-frame callback. SOS's own compositor has both a nested development backend
and a direct Debian-VM backend. The nested gate is safe to run on a workstation:

```sh
./tools/linux-compositor/verify-nested
```

To add **SOS** as a full-desktop choice in GDM without removing GNOME or changing
the default boot target:

```sh
./tools/install-linux-login-session install
```

Log out, choose SOS from GDM's session menu, and authenticate normally. SOS then
owns the Wayland session and display; `Ctrl+Alt+Backspace` logs out to GDM. This
selectable-session path shares the authenticated UID across its components and
does not provide the appliance session's separate service-identity boundary.
See [`docs/linux-stable-host.md`](docs/linux-stable-host.md) for installation,
state locations, and current evidence limits.

Inside the disposable reference VM, the direct gate temporarily releases GDM,
owns the VirtIO DRM output and libinput seat through libseat/seatd, and accepts
revisions only after the matching KMS page-flip event:

```sh
./tools/linux-vm/verify-direct-session
```

Native Linux text editing, accessibility, boot-to-SOS service packaging,
physical input injection, and physical hardware remain subsequent gates. See
[`docs/linux-stable-host.md`](docs/linux-stable-host.md) and
[`docs/linux-compositor.md`](docs/linux-compositor.md) for the exact evidence
and limitations.

## Milestone 1 vertical slice

The first end-to-end experience contains synthetic calendar, notes, music, and
weather data. Rust owns GPUI, fake providers, persistence, validation, and
rollback. [`experiences/default.luau`](experiences/default.luau) owns the screen
composition and local behavior.

Build, install, and launch the ARM64 APK:

```sh
./tools/sosctl m1-check
./tools/sosctl m1-run
```

`m1-check` needs the SDK, NDK, Java, and Rust Android target but not a connected
device. On an ARM64 Linux workstation it automatically combines native LLVM
with the NDK sysroot because Google's Linux NDK executables are x86-64; on
other hosts it uses `cargo-ndk`.

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
