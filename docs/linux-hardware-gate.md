# First physical Linux hardware gate

Date: 2026-08-21
Updated: 2026-08-25

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

Two environments can exercise the criteria below, but they do not produce the
same verdict:

- **installed-workstation:** Fedora Workstation installed to disk, SOS
  installed from this checkout with `install --offline`.
- **development-live:** a mutable SOS-baked Fedora Workstation live remix,
  prepared and collected on the same live overlay boot. It produces diagnostic
  evidence only, is not an installed product, and is never promotion eligible.
  See [`linux-live-image.md`](linux-live-image.md).

Hardware (DRM, input, DMI) is the same silicon on both. Persistence, disk,
and bootloader differ. Stock Fedora live media without SOS baked in is not a
development image. A future immutable `release` image owns release promotion;
there is no acceptance-live artifact class.

## Prepare the target

### Installed Fedora Workstation

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
The same bounded file may set `"layout"` to `"mirror"` or `"extend"` and may
map at most 32 exact, printable libinput device names to printable connector
names through `"input_outputs"`. The gate validates the same keys and limits as
the direct compositor, including the 128-byte limit on each name.

### Development-live remix, same-boot diagnostics

The Framework development loop may boot the mutable Fedora Workstation remix.
It keeps GNOME, password-protected SSH, and the selectable SOS session. Rebuild
the base only when its environment changes; for ordinary SOS patches use
`tools/linux-live-deploy` from GNOME after logging out of SOS. Copy evidence
off the overlay before reboot. Verify Fedora's signed CHECKSUM before the base
bake; the bake requires that expected SHA-256.

```sh
./tools/linux-live-image bake \
  --source-iso /path/to/Fedora-Workstation-Live-x86_64-*.iso \
  --source-sha256 "$FEDORA_ISO_SHA256" \
  --output-dir artifacts/linux-live-image \
  --liveuser-password-file /path/to/private-password-file \
  --networkmanager-profile-file /path/to/private-development-wifi.nmconnection
# Boot the remixed ISO, then from the GNOME live session:
/usr/local/libexec/sos/linux-hardware-gate prepare \
  --expect-product 'Laptop 12' \
  --evidence-dir /home/liveuser/framework12-first-gate
```

The rootless Podman bake verifies that Fedora's `LiveOS/squashfs.img` is a flat
EROFS rootfs, uses user-namespace root to preserve owners, permissions, ACLs,
capabilities, and portable xattrs, applies the image's SELinux file-context
policy after staging SOS, and repacks without host `sudo`.
Prepare records
`boot_kind=development-live`, `not_installed_product=true`,
`promotion_eligible=false`, the exact kernel `boot_id`,
both matching `image-identity.env` records, and the mandatory live-media payload
byte size and SHA-256. It also verifies any incremental deployment manifest and
snapshots the current SOS bytes. Persistence is optional only because prepare
and collect prove the same boot ID. A live overlay without both SOS image
identities is refused. An install-to-disk of this remix is also refused: it is
neither development-live nor the installed-workstation campaign.

## Run the clamshell smoke gate

Commit the feature branch, reinstall from that clean revision, and prepare a
new ignored or external evidence directory:

```sh
./tools/linux-hardware-gate prepare \
  --expect-product 'Laptop 12' \
  --evidence-dir artifacts/framework12-first-gate
```

The command proves that it is running on bare metal, checks the exact installed
manifest and revision pin, records the OS, kernel, BIOS, CPU, GPU/driver,
DRM connectors and EDID hashes, libinput inventory, package/tool versions,
development-versus-installed image identity, and the current journal cursor.
The installed-workstation pin is a clean matching source worktree. The
development-live pin preserves the baked image identity but permits an exact,
hashed overlay deployment whose source dirty state is recorded. It then prints
the operator steps:

Preparation also starts the root-owned transient unit
`sos-linux-hardware-gate-awake.service`. Its logind block inhibitor covers
`idle`, `sleep`, and `handle-lid-switch` while the machine waits at GDM, which
is before the normal SOS-session inhibitor exists. Preparation refuses a
second active owner. Collection requires the recorded unit to remain active,
captures its exact inhibitor record, and stops it before finalizing evidence.
An early collection failure also stops the unit. This is gate lifecycle state,
not a change to the workstation's stored GNOME or GDM power settings.

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

Collection first requires the exact kernel boot ID recorded by preparation so
journal cursors and monotonic timestamps can never span a reboot. It then reads
only the prepared journal interval for the login UID plus the kernel journal,
captures durable revision/authority agreement and the restored display-manager
state, verifies and releases the prepared awake inhibitor, and records the
matching boot ID again. It finalizes `verdict.txt`,
measures campaign wall time from same-boot monotonic timestamps, generates
`evidence-manifest.tsv`, and independently verifies every path, byte size, and
SHA-256. Manifest paths use bytewise `C` ordering, independent of the locale on
the target or the audit machine.

On development-live, use the baked harness for collection:

```sh
/usr/local/libexec/sos/linux-hardware-gate collect \
  --evidence-dir /home/liveuser/framework12-first-gate
```

## Runtime criteria and verdicts

Every criterion is required; a missing observation fails the run rather than
becoming a SKIP:

- the compositor's recovery view reaches a physical DRM page flip before the
  generated shell starts;
- the direct compositor and logind-owned session become ready with
  `evidence=drm_page_flip`;
- the configured offline or live resident agent starts;
- preparation and collection come from the same kernel boot;
- real libinput events are observed for keyboard, relative pointer, pointer
  button, and touchscreen;
- every input device added during SOS was already present in the libinput
  inventory captured at preparation, so a hot-added uinput helper cannot satisfy
  a physical-input criterion;
- at least two distinct revisions reach compositor-owned DRM page flips,
  proving boot plus one transactional authoring activation, while the permanent
  experience host has exactly one launch and no restart;
- the committed revision and durable provider authority agree after logout;
- `Ctrl+Alt+Backspace` ends the SOS lifecycle cleanly and the display manager is
  active afterward;
- the campaign contains no SOS process failure, Rust panic, or matching kernel
  DRM/GPU hang/reset marker.

An installed-workstation PASS establishes the physical panel, Intel DRM/KMS/GBM path,
keyboard, touchpad, touchscreen, deterministic resident authoring, durable
activation, and reversible GDM lifecycle for the exact evidence revision. It
does not establish stylus pressure/calibration, tablet rotation, suspend/resume,
external-display hotplug, host crash recovery, latency, memory pressure,
thermals, or soak. Run those as later focused gates without weakening this
baseline. Development-live uses the same observations to diagnose the mutable
runtime, but emits `DIAGNOSTIC_PASS promotion_eligible=false` or
`DIAGNOSTIC_FAIL`; it can never emit the normal PASS line. Only the future
immutable `release` artifact and its artifact-matched gate may support release
promotion.

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
