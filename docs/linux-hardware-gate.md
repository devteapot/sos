# First physical Linux hardware gate

Date: 2026-08-21

The first physical Linux gate uses the selectable GDM session. It does not
install the boot-owned appliance target, stop or reconfigure GDM, or change the
default systemd target. The initial target is a Framework Laptop 12 in normal
clamshell orientation. The same harness can later identify another bare-metal
Linux target explicitly.

The gate is an observed human-input campaign. The controller prepares an
evidence directory before logout, the operator selects SOS and performs the
bounded interactions below, and the controller collects finalized evidence
after SOS returns cleanly to GDM. It never injects input or infers physical
behavior from VM results.

## Prepare the target

Use a current Fedora Workstation installation with GDM on the Framework Laptop
12. Framework lists Fedora as an officially supported Linux distribution and
recommends a current kernel for this hardware. Keep the conventional GNOME
session, SSH access from another machine, and a working text console available.

Install the native build dependencies on Fedora:

```sh
sudo dnf install \
  @development-tools clang cmake curl git jq \
  alsa-lib-devel fontconfig-devel glib2-devel libgit2-devel \
  mesa-libgbm-devel libinput-devel libinput-utils libseat-devel systemd-devel \
  libva-devel sqlite-devel openssl-devel vulkan-loader-devel \
  wayland-devel libX11-devel libxkbcommon-devel \
  libxkbcommon-x11-devel libzstd-devel pipewire-devel
```

Rust 1.95.0 is the reference toolchain. The installer accepts Node 22.19 or
newer and records the exact Rust and Node versions it used. Check the direct
session prerequisites before starting a release build:

```sh
./tools/install-linux-login-session doctor
```

For the first hardware gate, install the deterministic offline resident agent.
This isolates DRM, GPUI, input, and revision activation from credentials,
network access, and model availability:

```sh
./tools/install-linux-login-session install --offline
```

The installer records its source revision, clean/dirty state, toolchain, agent
mode, and the byte size and SHA-256 of each installed gate artifact below
`/usr/share/doc/sos`. A physical campaign refuses dirty or revision-mismatched
installed artifacts.

The selectable session reads its persistent display configuration from
`${XDG_STATE_HOME:-$HOME/.local/state}/sos/output.json`. The initial `{}` uses
the preferred panel mode, scale 1.0, and rotation 0. A bounded override may set
`mode`, `scale`, or `rotation`, for example:

```json
{"scale": 1.25, "rotation": 0}
```

Automatic tablet rotation is not part of the first gate. Finalize this file
before preparing evidence; the harness records its exact contents.

## Run the clamshell smoke gate

Commit the feature branch, reinstall from that clean revision, and prepare a
new ignored or external evidence directory:

```sh
./tools/linux-hardware-gate prepare \
  --expect-product 'Laptop 12' \
  --evidence-dir artifacts/framework12-first-gate
```

The command proves that it is running on bare metal, checks the exact installed
manifest and source revision, records the OS, kernel, BIOS, CPU, GPU/driver,
DRM connectors and EDID hashes, libinput inventory, package/tool versions, and
the current journal cursor. It then prints the operator steps:

1. Log out and choose **SOS** from GDM.
2. Confirm the compositor recovery view and generated experience appear.
3. Exercise the physical keyboard, touchpad motion and click, and touchscreen.
4. Enter one prompt in the Luau composer and wait for the deterministic revision
   to activate visibly.
5. Press `Ctrl+Alt+Backspace`, confirm GDM returns, and log into GNOME.
6. Collect the campaign from the same user account.

```sh
./tools/linux-hardware-gate collect \
  --evidence-dir artifacts/framework12-first-gate
```

Collection reads only the prepared journal interval for the login UID plus the
kernel journal. It also captures durable revision/authority agreement and the
restored display-manager state. It finalizes `verdict.txt`, measures campaign
wall time from monotonic timestamps, generates `evidence-manifest.tsv`, and
independently verifies every path, byte size, and SHA-256.

## PASS contract

Every criterion is required; a missing observation is a FAIL rather than a
SKIP:

- the compositor's recovery view reaches a physical DRM page flip before the
  generated shell starts;
- the direct compositor and logind-owned session become ready with
  `evidence=drm_page_flip`;
- the configured offline or live resident agent starts;
- real libinput events are observed for keyboard, relative pointer, pointer
  button, and touchscreen;
- at least two distinct revisions reach compositor-owned DRM page flips,
  proving boot plus one transactional authoring activation, while the permanent
  experience host has exactly one launch and no restart;
- the committed revision and durable provider authority agree after logout;
- `Ctrl+Alt+Backspace` ends the SOS lifecycle cleanly and the display manager is
  active afterward;
- the campaign contains no SOS process failure, Rust panic, or matching kernel
  DRM/GPU hang/reset marker.

This first PASS establishes the physical panel, Intel DRM/KMS/GBM path,
keyboard, touchpad, touchscreen, deterministic resident authoring, durable
activation, and reversible GDM lifecycle for the exact evidence revision. It
does not establish stylus pressure/calibration, tablet rotation, suspend/resume,
external-display hotplug, host crash recovery, latency, memory pressure,
thermals, or soak. Run those as later focused gates without weakening this
baseline.

## Audit, recovery, and uninstall

The finalized evidence can be rechecked without rerunning hardware:

```sh
./tools/linux-hardware-gate audit \
  --evidence-dir artifacts/framework12-first-gate
./tools/linux-hardware-gate verify-manifest \
  --evidence-dir artifacts/framework12-first-gate
```

If SOS cannot exit normally, use the preserved SSH session or a text console to
terminate the user's graphical session and return to GDM. Do not promote a run
that required this recovery to PASS. After returning to GNOME, remove only the
installed SOS session and product files with:

```sh
./tools/install-linux-login-session uninstall
```

Uninstall preserves `${XDG_STATE_HOME:-$HOME/.local/state}/sos`, does not remove
packages, and does not change GDM or the default boot target.
