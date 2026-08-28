# SOS

SOS is a research prototype for an agent-native operating experience: the user
directs an agent that writes and evolves the visible environment, while
separately installed providers remain authoritative over data and actions. It
is not a launcher, a scriptable Android application, or a fixed catalog of
generated widgets. The product direction is defined in
[`docs/vision.md`](docs/vision.md).

> [!WARNING]
> SOS is experimental software, not a daily-driver phone OS. The physical-phone
> work uses an unlocked Galaxy A33 5G, custom recovery, community device trees,
> and prototype security boundaries. Bootloader unlocking wipes the device and
> permanently trips Samsung Knox. Do not flash a device without reading
> [`docs/samsung-sm-a336b.md`](docs/samsung-sm-a336b.md) and preparing an exact
> stock rollback path.

## Current status

As of 2026-08-28, SOS has moved beyond its original Android application
laboratory into the privileged system and native-ownership phase.

| Track | Current evidence |
| --- | --- |
| Generated experience | Package format and Experience API v4 are the only built-in, authoring, activation, and rollback path. Named exports, exact fork/remix lineage, host-owned live mounts, isolated child VMs, state and grants, typed appearance, locked or tracked graphs, and transactional activation run on Linux and Android. The Agenda, Media, Dashboard, and self-contained remix gates pass desktop tests, v4-only Compat and Core SM-A336B campaigns, and the Framework development-live campaign. The composition milestones are closed; Linux release promotion and wider product hardening remain separate gates. |
| Android APK harness | The physical SM-A336B passed the stable-host regression, typed provider effect, durable state/authority recovery, and a 10,000-swap device soak. This remains a regression harness, not the product boundary. |
| Linux | A permanent GPUI/Wayland host, durable provider/state service, revision supervisor, resident Pi authoring agent, authenticated Smithay compositor, selectable GDM session, and Debian direct-DRM VM gate are implemented. The exact Framework development-live composition and integrated-input campaign passes; installed-product promotion remains open. |
| AOSP Cuttlefish | Pristine Android 17, SOS-as-HOME, and an init-supervised on-device authority passed in x86-64 Cuttlefish. |
| Samsung a33x | The historical six-stage campaign was completed on physical hardware. Compat 1 remains the accepted usable fallback and later passed live System Providers v1. Fresh v4-only Compat and Core 1 artifacts passed composition, containment, recovery, authoring, and rollback. Core input in that repeatable campaign used a debug-only uinput service and is not a new physical-touch claim. Core 1 is still locked, and its native provider slice has build parity rather than physical acceptance. Core 0A is archived and Core 0B is a frozen, opt-in migration oracle. |
| Resident agent | Pi runs on Linux and as native ARM64/Bionic Node on the phone. A subscription-backed Codex flow produced and activated a live generated revision on-device without bypassing trusted validation. |

The accepted physical fallback is Compat 1 revision
`sos.compat1.19d8a653fbd7.220e268c228f`. It passed full-frame SOS
HOME/workspace/attention, selected-application containment, redirected Android
system-Activity blocking, HOME restart, native side-button lock/wake, and
owner-confirmed touchscreen ENTER unlock on a no-credential test device. Core
1 intentionally remains locked: native synthetic-password/FBE unlock and the
system-service replacements needed to remove Android safely do not exist yet.
Provider-focused revision `sos.compat1.a3f3bae010bf.b093c3a0b50a` subsequently
passed live clock/power/Wi-Fi/audio/attention facts, reversible typed actions,
generated-revision failure recovery to signed stock, reboot persistence, and a
125-second refresh smoke soak. It does not supersede the broader fallback
revision because successful media/application actions and physical ENTER were
not repeated in that campaign.

The later v4-only composition artifacts are Compat 1
`sos.compat1.7c973a4bce2c.5ef843a208f6` and Core 1
`sos.core1.e21d0fcb4e31.41406fcf892b`. They close the cross-platform
composition milestone, but they do not replace the broader Compat fallback
claim or close Core native unlock and provider-service migration.

The concise chronological record is [`docs/progress.md`](docs/progress.md).
The product boundary and exact physical results are in
[`docs/android-product-split.md`](docs/android-product-split.md) and
[`docs/android-ui-ownership-stages.md`](docs/android-ui-ownership-stages.md).

## Architecture

The normal mutation unit is a content-addressed experience revision, not a new
APK or native binary:

```text
user request
    ↓
bounded resident authoring agent
    ↓
Luau source + assets + migrations
    ↓
fresh-VM evaluation and capability validation
    ↓
permanent Rust/GPUI host prepares and presents a retained scene
    ↓
supervisor commits revision, provider effects, and durable state, or rolls back
```

Generated code does not receive a GPUI context, device handle, provider
object, filesystem, or arbitrary socket. Fixed trusted code owns candidate
validation, frame-boundary activation, recovery, credentials, system facts,
and provider actions. Providers remain authoritative over resources, actions,
and events; the generated experience owns their composition and presentation.

On the physical a33x target, SOS is split into two product families over the
same hardware, services, host, and revision format:

```text
SOS Compat
├── Compat 0  historical SOS-as-HOME bring-up with Android ceremonies
└── Compat 1  native SOS presentation + selected Android applications

SOS Core
├── Shadow    manual diagnostic probe with Android recovery UI
├── Core 0A   archived historical stage; no build product
├── Core 0B   frozen legacy migration oracle; explicit opt-in only
└── Core 1    active no-Zygote target; fixed locked/recovery surface
```

The historical rows remain evidence, not an obligation to maintain every
intermediate as a current product. A runtime property cannot turn one ownership
stage into another.

## Choose a development path

### Android application harness

The quickest physical-device path is still the non-system Android harness. It
requires the Android SDK/NDK, Java, the Rust Android target, and an authorized
ARM64 device:

```sh
./tools/sosctl doctor
./tools/sosctl m1-check
./tools/sosctl m1-run
```

`m1-check` exercises the Android compilation path without requiring a connected
device. The other commands use `ADB_SERIAL` when more than one device is
attached.

`m1-run` builds the APK, starts the workstation provider/state daemon, creates
`adb reverse tcp:47777 tcp:47777`, installs the APK, and launches it. Use
`m1-run --no-follow` to leave the managed daemon running, and stop the complete
session with:

```sh
./tools/sosctl m1-stop
```

Validate a v4 Experience or stage a Stock-compatible v4 edit while the same
process and APK remain alive:

```sh
./tools/sosctl validate experiences/android-exit-agent.luau
./tools/sosctl agent-generate "make the Stock workspace calmer"
./tools/sosctl agent-apply .cache/agent-candidates/android-stock-agent.luau
./tools/sosctl rollback
./tools/sosctl worker-restart
```

The source delivery command edits the active Stock Experience and therefore
keeps its v4 `main` export and agent composer. Ordinary Experiences retain
their own registry identities and launch as independent top-level graphs; the
signed Agenda, Media, Dashboard, and remix packages exercise that boundary.

The original unmodified GPUI Mobile hardware spike remains available through
`./tools/sosctl run`; see [`docs/experiment.md`](docs/experiment.md).

### Linux stable host

Inside an existing Wayland session, run the authority, coordinator, permanent
host, and one generated experience:

```sh
./tools/sosctl linux-run --windowed
```

From another terminal, inspect or replace the active revision without
replacing the process or window:

```sh
./tools/sosctl linux-script tests/fixtures/stock-authoring-v4.luau
./tools/sosctl linux-status
./tools/sosctl linux-stop
```

Install the API v4 composition reference package into an isolated store:

```sh
demo_root=$(mktemp -d)
cargo run --locked -p revision-supervisor --bin sos-revision-supervisor -- \
  install-composition-demo --root "$demo_root"
```

The JSON result names the Agenda, Media, Dashboard, and remix revisions plus
the resolved Dashboard graph. See
[`docs/experience-composition.md`](docs/experience-composition.md) for graph
activation and acceptance status.

Run the resident-agent path deterministically without a model call:

```sh
./tools/sosctl linux-agent-test
./tools/sosctl linux-agent-run --fake tests/fixtures/stock-authoring-v4.luau
```

For a subscription-backed live model, authenticate with Pi's headless Codex
device flow before starting the agent:

```sh
export SOS_AGENT_PROVIDER=openai-codex
export SOS_AGENT_MODEL=gpt-5.6-sol
unset SOS_AGENT_FAKE_SOURCE
./tools/sosctl linux-agent-login
./tools/sosctl linux-agent-run
```

The nested compositor gate is safe to run from a workstation Wayland session:

```sh
./tools/linux-compositor/verify-nested
```

To install SOS as a selectable GDM session without removing the existing
desktop or changing the default boot target:

```sh
./tools/install-linux-login-session doctor
./tools/install-linux-login-session install
```

For the first physical Linux gate, use the deterministic offline agent so
display, input, and revision activation do not depend on credentials or a live
model:

```sh
./tools/install-linux-login-session install --offline
./tools/linux-hardware-gate prepare \
  --expect-product 'Laptop 12' \
  --evidence-dir artifacts/framework12-first-gate
# Select SOS in GDM, complete the printed physical interactions, and return.
./tools/linux-hardware-gate collect \
  --evidence-dir artifacts/framework12-first-gate
```

The same offline install can be baked into a checksum-pinned Fedora Workstation
`development-live` environment so the Framework 12 loop does not touch its
internal disk. Its SSH service accepts either the development password or an
explicitly baked developer Ed25519 public key. Key-authenticated images disable
remote password login while retaining the password for local recovery. An
optional development-only Wi-Fi autoconnect profile and mutable overlay allow
changed SOS binaries to be deployed with `tools/linux-live-deploy` without
rebuilding the ISO. Embedded Wi-Fi credentials or a developer public key make
that private development ISO unsuitable for sharing or release. The bake
accepts only Fedora's flat
EROFS rootfs format and performs a privileged metadata-preserving copy,
policy-based SELinux relabel, and repack so Linux ownership, ACLs,
capabilities, and security metadata are preserved.
Development-live diagnostics always record `promotion_eligible=false`; only a
future immutable release image can own release promotion.
It is not an installed product. See
[`docs/linux-live-image.md`](docs/linux-live-image.md).

`./tools/install-linux-login-session uninstall` removes the installed SOS
session while preserving user state and the existing display-manager/default
boot configuration.

The direct-DRM acceptance command targets the disposable reference Debian VM:

```sh
./tools/linux-vm/verify-direct-session
```

See [`docs/linux-stable-host.md`](docs/linux-stable-host.md),
[`docs/linux-compositor.md`](docs/linux-compositor.md),
[`docs/linux-vm.md`](docs/linux-vm.md),
[`docs/linux-hardware-gate.md`](docs/linux-hardware-gate.md),
[`docs/linux-live-image.md`](docs/linux-live-image.md), and
[`docs/sos-agent.md`](docs/sos-agent.md) for prerequisites and evidence limits.

### AOSP Cuttlefish

The Android 17 Cuttlefish track uses a separate checkout, by default
`~/dev/aosp-sos`, while the small SOS product overlay remains in this
repository:

```sh
./tools/aospctl image
./tools/aospctl doctor
./tools/aospctl init
./tools/aospctl sync
./tools/aospctl build-pristine
./tools/aospctl boot pristine
./tools/aospctl verify-pristine
./tools/aospctl stop

./tools/aospctl build-sos
./tools/aospctl boot sos
./tools/aospctl verify-sos
./tools/aospctl stop
```

The SOS image packages an x86-64 HOME APK and an init-supervised on-device
provider/state/revision authority. Its verifier removes ADB reverse as a hidden
dependency, kills the GPUI process, and requires HOME plus the durable revision
to recover. See [`docs/aosp-cuttlefish.md`](docs/aosp-cuttlefish.md).

### Samsung SM-A336B images

The physical-image build is intentionally separate from Cuttlefish. By
default, `tools/a33xctl` uses `~/dev/lineage-a33x`, a pinned LineageOS 23 /
Android 16 graph, and an Ubuntu 24.04 Podman image. The host gate requires
x86-64, at least 300 GiB free below `~/dev`, and at least 60 GB RAM.

```sh
./tools/a33xctl image
./tools/a33xctl doctor
./tools/a33xctl init
./tools/a33xctl sync
```

Build and inspect one explicit ownership stage:

```sh
./tools/a33xctl build-compat1
./tools/a33xctl inspect-compat1

./tools/a33xctl build-core1
./tools/a33xctl inspect-core1
```

The complete profile matrix is:

| Stage | Lifecycle | Build | Inspect |
| --- | --- | --- | --- |
| Compat 0 | Historical Compat bring-up | `build-compat0` | `inspect-compat0` |
| Compat 1 | Active fallback/application island | `build-compat1` | `inspect-compat1` |
| Shadow | Diagnostic probe | `build-core-shadow` | `inspect-core` |
| Core 0A | Archived; product removed | None | None |
| Core 0B | Frozen legacy migration oracle | `SOS_ENABLE_LEGACY_CORE0B_BUILD=1 ./tools/a33xctl build-core0b` | `inspect-core0b` |
| Core 1 | Active Core target | `build-core1` | `inspect-core1` |

`build-compat` and `build-sos` are aliases for Compat 1;
`build-core` is an alias for Shadow. Building or inspecting an OTA does not
authorize flashing it. The recovery, rollback, exact-device, and irreversible-
risk procedure is in [`docs/samsung-sm-a336b.md`](docs/samsung-sm-a336b.md).

## Repository map

| Path | Purpose |
| --- | --- |
| `apps/experience/` | Permanent Rust/GPUI host and Android/Linux adapters |
| `crates/runtime-luau/` | Bounded Luau evaluation and Scene ABI decoding |
| `crates/revision-supervisor/` | Revision preparation, activation, and recovery |
| `crates/provider-state-service/` | Durable typed provider/state authority |
| `crates/sos-compositor/` | Authenticated Smithay compositor |
| `services/sos-agent/` | Resident Pi authoring service and Android runner |
| `experiences/` | Generated-experience examples and regression fixtures |
| `aosp/device/sos/` | Cuttlefish and a33x product overlays |
| `aosp/device/sos/a33x/core/platform_adapter.cpp` | Core 1 native System Providers v1 platform adapter |
| `packaging/` | Linux systemd and GDM session integration |
| `tools/` | Reproducible build, run, install, and verification entry points |
| `docs/` | Architecture decisions, gate reports, and chronological evidence |

Build products and raw evidence belong in `artifacts/`, `.cache/`, or the
documented external evidence directories and are intentionally not tracked.

## Documentation map

[`docs/README.md`](docs/README.md) indexes every current architecture guide,
platform runbook, and historical gate report. The main entry points are:

- [`docs/vision.md`](docs/vision.md) defines the intended product and permanent
  versus generative boundary.
- [`docs/android-product-split.md`](docs/android-product-split.md) defines SOS
  Compat, SOS Core, and the current physical gate matrix.
- [`docs/android-ui-ownership-stages.md`](docs/android-ui-ownership-stages.md)
  records the exact six-image ownership campaign.
- [`docs/samsung-sm-a336b.md`](docs/samsung-sm-a336b.md) covers the physical
  device, reproducible build, rollback risk, and hardware evidence.
- [`docs/experience-api.md`](docs/experience-api.md) documents the Luau-facing
  capability API.
- [`docs/experience-composition.md`](docs/experience-composition.md) defines
  fork, remix, live mounting, experience boundaries, and shared appearance.
- [`docs/runtime-evaluation.md`](docs/runtime-evaluation.md) records why Luau
  was selected for the current experience runtime.
- [`docs/stable-host-device-gate.md`](docs/stable-host-device-gate.md) and
  [`docs/stateful-experience-gate.md`](docs/stateful-experience-gate.md) contain
  the physical stable-host and stateful-swap evidence.
- [`docs/linux-stable-host.md`](docs/linux-stable-host.md),
  [`docs/linux-compositor.md`](docs/linux-compositor.md),
  [`docs/linux-vm.md`](docs/linux-vm.md),
  [`docs/linux-hardware-gate.md`](docs/linux-hardware-gate.md), and
  [`docs/linux-live-image.md`](docs/linux-live-image.md) cover the Linux
  path from virtual acceptance through the first physical campaign.
- [`docs/aosp-cuttlefish.md`](docs/aosp-cuttlefish.md) covers the reproducible
  Android 17 system spike.
- [`docs/progress.md`](docs/progress.md) is the chronological index of material
  experiments, failures, artifacts, decisions, and next gates.

## Known limitations

- The physical evidence is from one unlocked Samsung SM-A336B development
  handset. Desktop, VM, or Cuttlefish results are never treated as equivalent
  to a phone hardware gate.
- Compat 1 constrains visible Android presentation but is not yet a data
  sandbox for selected applications. Permission, chooser, IME, emergency/call,
  alarm, ANR, accessibility, and security containment brokers remain gates.
- The test handset has credential type `NONE`; real PIN/Gatekeeper throttling,
  fingerprint, authentication-bound Keystore release, and the physical
  Volume-Up+Volume-Down Recovery chord remain unproven.
- Core 1 proves the no-Zygote process and recovery boundary and has passed the
  v4 composition, recovery, authoring, and rollback campaign while remaining
  deliberately locked. Its native System Providers v1 adapter has build/ABI
  parity, not physical provider acceptance: saved-Wi-Fi provisioning, validated
  reachability, media/app owners, attention producers, and provider restart/
  soak evidence remain open alongside native CE unlock.
- Core 0A is historical evidence only. Core 0B is retained solely as an
  opt-in comparison target until Core 1 owns native unlock and the displaced
  unlock, provider hardware behavior, call/alarm, session, update, and recovery
  services.
- Speaker, earpiece, Bluetooth/call audio, cellular calls/data, and longer
  suspend, thermal, and soak campaigns are not complete across all accepted
  ownership stages.
- Generated experiences and providers are still research-grade; do not use
  real personal data or consequential credentials beyond the explicitly
  documented, revocable test setup.
