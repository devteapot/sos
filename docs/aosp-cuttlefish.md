# AOSP and Cuttlefish system spike

This spike keeps the large Android checkout only at `~/dev/aosp-sos`. SOS owns
the small product overlay in `aosp/device/sos/cuttlefish/` and stages it into
that checkout only after the pristine AOSP gate. Generated APKs, native
binaries, Cuttlefish images, instance state, and the resolved Repo manifest are
not committed to SOS.

## Reproducible host and source

The Fedora host uses a rootless Podman Ubuntu 24.04 image so the AOSP dependency
set is explicit and does not require changing the workstation packages. The
checkout follows Google's `android-latest-release` manifest and records an
immutable resolved manifest after sync:

```sh
./tools/aospctl image
./tools/aospctl doctor
./tools/aospctl init
./tools/aospctl sync
./tools/aospctl manifest-identity
```

`doctor` requires an x86-64 host, readable/writable KVM, 64 GB RAM, and 400 GiB
free below `~/dev`. Network transfer remains parallel during `repo sync`, while
worktree checkout is serialized. The latter avoids a Repo/XFS race observed
while several new project worktrees were initialized concurrently.

## AOSP-0: pristine platform

Build and boot before staging the SOS directory:

```sh
./tools/aospctl build-pristine
./tools/aospctl boot pristine
./tools/aospctl verify-pristine
./tools/aospctl stop
```

The verifier requires `sys.boot_completed=1`, records the build fingerprint and
resolved HOME activity, and rejects a supposedly pristine image if HOME belongs
to `dev.sos.experience`. Pristine and SOS products use AOSP's same
`vsoc_x86_64_only` output directory, so `boot` also checks the output's product
properties and refuses to launch the wrong image; rebuild the requested target
when switching products.

The 2026-08-15 gate used Android 17 build `CP2A.260605.016` from the resolved
`android-latest-release` manifest and booted to
`com.android.launcher3/.uioverrides.QuickstepLauncher`. The immutable source
identity and full evidence are recorded in [`progress.md`](progress.md).

## AOSP-1: SOS is HOME

`./tools/sosctl m1-build --abi x86_64 --home` builds only x86-64 JNI libraries,
enables the otherwise-disabled `SosHomeActivity` alias, and enables the
`aosp-system` client feature. Ordinary ARM64/physical-device builds retain the
LAUNCHER activity and do not advertise HOME.

The SOS product inherits the current 64-bit-only Cuttlefish phone,
platform-signs and preinstalls `SosShell`, and installs the authority binary
plus bootstrap Luau source in `system_ext`. SOS has HOME priority 1000 and is
therefore the resolved default. Quickstep remains installed as Android's
Recents provider; making SOS the default HOME does not imply that SOS already
implements the separate Quickstep/Recents API. A small framework overlay names
SOS as the secondary HOME package without clearing Android's inherited Recents
component:

```sh
./tools/aospctl build-sos
./tools/aospctl boot sos
./tools/aospctl verify-sos
./tools/aospctl stop
```

`stage-sos` is available as a diagnostic, but `build-sos` normally owns the
host Rust/Gradle builds and staging step.

## AOSP-2: device authority and recovery boundary

The product starts `sos-android-system-authority` as an init-owned `class main`
service. It listens only on device loopback:

- TCP 47777 retains the bounded typed provider/state protocol;
- TCP 47778 owns immutable revision installation, current-revision lookup, and
  presentation activation.

SELinux gives the daemon its own domain and durable `/data/misc/sos` label. A
separate platform-app domain is the only app domain allowed to connect to the
two labeled ports. There is no ADB transport in this product path.

The daemon stores verified, content-addressed revision directories and an
atomic `current` symlink. A candidate transaction is:

1. the disposable Luau worker validates and renders a candidate;
2. the daemon installs its immutable source/state/assets, while the provider
   service stages the matching state envelope;
3. GPUI switches to the prepared scene and schedules its next-frame callback;
4. only from that presentation callback does the daemon journal activation,
   promote provider state, persist it, and atomically switch `current`;
5. app-private source files are updated only as disposable caches.

If the GPUI process dies before step 4, the durable revision remains unchanged.
If the daemon dies in the state-first gap inside step 4, its journal reconciles
the pointer at restart. An activation error after presentation deliberately
terminates the experience process; Android's independently owned HOME lifecycle
then starts a fresh process from the daemon's durable current revision. Init,
not GPUI, restarts the authority itself.

`verify-sos` checks the x86-64 package ABI, exact HOME resolution, a live
authority PID, enforcing SELinux with the expected app/daemon domains, an empty
`adb reverse --list`, and the durable current pointer. It submits the tracked
Timeflow probe, requires presentation to change that pointer without changing
the permanent GPUI PID, kills the authority and requires init to recover it
without changing GPUI or the revision, then kills only the GPUI HOME PID.
Android must produce a different HOME PID while the recovered authority PID and
activated revision remain unchanged.

On the final clean 2026-08-15 run, `verify-sos` resolved
`dev.sos.experience/.SosHomeActivity`, reported ABI `x86_64`, enforcing
`sos_shell_app` and `sos_authority` domains, and `adb_reverse=none`. The
Timeflow revision changed the durable pointer without replacing GPUI; killing
the authority changed PID 781 to 5302 without replacing HOME, then killing HOME
changed PID 3446 to 5388 without replacing the recovered authority. A captured
720x1280 frame was also visually inspected as the rendered Timeflow HOME. Exact
revision and artifact identities remain in [`progress.md`](progress.md).

## Boundary and remaining gates

This is an Android system-product spike, not the final SOS compositor. Android
still owns PackageManager, ActivityManager, SurfaceFlinger, input, IME, and HOME
task recovery. The next architecture gate is compositor-owned staging and input
focus rather than Activity-managed recovery. Hardware, latency, thermal,
suspend, and soak claims still require the physical-device procedures; a
Cuttlefish pass cannot complete them. The separate physical-device `m1-run`
developer harness intentionally retains its workstation provider and ADB
reverse mapping; those are not present in the AOSP product path. Revision
directories are content-addressed, verified on read, and made read-only, but
this development product does not yet provision the store's optional manifest
signing and verification keys.
