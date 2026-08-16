# SOS Compat and SOS Core

Date: 2026-08-16

SOS has two product families and six explicit ownership stages over one
hardware, service, and revision base:

```text
shared a33x hardware + SOS services + revision format
├── SOS Compat
│   ├── Compat 0: SOS enforced as HOME; Android ceremonies remain
│   └── Compat 1: SOS chrome, attention broker, and APK workspace
└── SOS Core
    ├── Shadow:  manual native probe; Android remains recovery owner
    ├── Core 0A: native shell/input/watchdog; Android UI installed behind it
    ├── Core 0B: no visible Android UI; headless framework bridge retained
    └── Core 1:  no Zygote or APK process
```

The separate build products make the ownership decision reviewable in the OTA
itself. The detailed package, boot, credential, and recovery contracts are in
[`android-ui-ownership-stages.md`](android-ui-ownership-stages.md).

## Product boundary

| Stage | Build product | UI owner and boundary |
| --- | --- | --- |
| Compat 0 | `lineage_sos_compat0_a33x` | A persistent platform policy reasserts SOS as HOME. Android still draws system and compatibility ceremonies. |
| Compat 1 | `lineage_sos_compat_a33x` | SOS owns HOME, persistent task chrome, and durable typed attention. Launcher3 is absent; Android apps and ceremonies remain. |
| Shadow | `lineage_sos_core_a33x` | Android remains owner until a disabled native probe or GPUI supervisor is started manually. |
| Core 0A | `lineage_sos_core0a_a33x` | Android performs CE unlock, then init automatically gives the top display layer and exclusive touch/volume input to native SOS. Android UI remains installed behind it for failure escape. |
| Core 0B | `lineage_sos_core0b_a33x` | A fixed native lock surface starts before CE. Principal Android UI APKs are removed and the framework aborts all Activity starts; Zygote and `system_server` remain for headless services and a no-Activity LockSettings bridge. |
| Core 1 | `lineage_sos_core1_a33x` | AOSP's no-Zygote init is selected. The native host exposes an honest locked/recovery surface until synthetic-password unlock and framework services have native owners. |

Both inherit the same Samsung a33x device/vendor graph, init, SELinux, Binder,
SurfaceFlinger, Hardware Composer, audio services, Keystore, Gatekeeper, vendor
HALs, on-device SOS authority, and revision format 3. The split does not fork
hardware enablement or experience artifacts.

Both Compat stages package the platform-signed `SosShell` NativeActivity. Its
HOME intent filter has priority 1000, while a persistent direct-boot policy
uses the system role API to repair ownership after boot, package replacement,
or an external role change. Compat 0 retains Launcher3 only to prove that the
policy defeats that candidate. Compat 1 removes Launcher3 from the product;
the earlier Launcher3 ownership was accidental, not a desired fallback.

Core never packages the SOS Activity. It packages two native bring-up
programs:

- `sos-core-surface-probe`, the original one-frame SurfaceComposer/EGL probe;
- `sos-core-host`, a fixed signed supervisor that creates the top-level
  `ANativeWindow`, loads the standalone GPUI/wgpu experience, reads raw touch
  and hardware-key events, and owns a CPU-rendered recovery surface if the GPUI
  child fails.

The host has no Activity, JNI lifecycle, or APK data directory. It uses a
dedicated SELinux domain and `/data/misc/sos/core`, while the SOS revision,
authority, and provider services remain shared. Shadow is manually triggered;
Core 0A auto-starts only after Android reports CE available; Core 0B and Core 1
auto-start after SurfaceFlinger. Every child is supervised by fixed signed
native code. Failure presents a CPU-rendered recovery surface, and the locked
stages accept Volume Up+Down as a direct Recovery reboot chord.

## Why Zygote remains at Core 0

SystemUI, Launcher, Settings, and LatinIME are visible Android products, but
Zygote and `system_server` also host or coordinate working telephony, network,
Bluetooth, NFC, permissions, storage, and credential services. Removing them is
therefore a service migration, not a UI cleanup.

Core advances through two Core 0 gates before no-Zygote:

1. **Core 0A — native ownership with Android recovery.** Android UI stays
   installed but becomes inert behind the native top layer after trusted CE
   unlock. This isolates shell, input, watchdog, and recovery failures.
2. **Core 0B — no Android-rendered ownership.** Principal UI packages are
   overridden out and an immutable product policy aborts every Activity start,
   including starts from preserved `/data/app` packages on no-wipe upgrades.
   The remaining SOS Java process is a direct-boot, system-UID bridge with no
   Activity; it delegates bounded PIN verification to LockSettings.
   Phone/network/Bluetooth/NFC framework services remain.
3. **Core 1 — no Zygote/APK runtime.** AOSP's no-Zygote init is selected. The
   initial target deliberately remains locked because native Android
   synthetic-password unwrap and the displaced services are not yet complete.

Core 1 therefore proves the process and native recovery boundary; it does not
pretend that removing Zygote somehow replaces the services Zygote hosted.

## Ownership cutover gate

Each surface moves through the same sequence:

1. Build the signed native replacement.
2. Run it alongside Android without changing boot ownership.
3. Switch one explicit product flag or property.
4. Prove normal operation, process restart, safe-mode/recovery entry, and
   failure before and after first presentation on the physical SM-A336B.
5. Remove the superseded Android package from Core only.

For the permanent shell, the gate additionally requires:

- GPUI/wgpu renders through the SurfaceComposer-created `ANativeWindow` on the
  Mali/Exynos display path;
- touchscreen, power, and volume input are owned and quiesced across revision
  activation;
- a fixed signed recovery surface remains usable when the host or revision
  fails;
- a fixed signed lockscreen preserves FBE unlock, Gatekeeper, fingerprint
  lockout, and authentication-bound Keystore semantics;
- suspend/resume, rotation/display power, audio/call warnings, and thermal
  behavior pass on the phone;
- the Compat OTA remains installable as the no-wipe rollback target.

Desktop parsing, compilation, or Cuttlefish cannot complete this gate.

Current physical status on the SM-A336B:

| Gate | Status |
| --- | --- |
| Compat 0 ownership | Passed on `sos.compat0.0805cf6bd0b4.db36ed79bb16`: SOS reclaimed HOME while Launcher3 and Android ceremonies remained available. |
| Compat 1 ownership | Passed on `sos.compat1.0805cf6bd0b4.616ac2404a79`: Launcher3 was absent, SOS chrome remained above Files and Settings, the attention journal survived process restart, and explicit APK selection returned through SOS-owned controls. |
| Shadow display and failure boundary | Passed on `sos.shadow.0805cf6bd0b4.1aad692518b8`: both the one-frame probe and GPUI rendered through SurfaceComposer/Samsung HWC, injected failure reached fixed recovery, Retry relaunched GPUI, and Android escape exposed the intact framework UI. |
| Core 0A ownership | Passed on `sos.core0a.0805cf6bd0b4.1b0c9edec481`: GPUI started after CE unlock, raw input was acquired, user install was rejected, and Android remained an explicit failure escape. |
| Core 0B headless framework | Passed on `sos.core0b.0805cf6bd0b4.4341fa73391c`: native lock was visible before CE, headless boot reached `RUNNING_UNLOCKED`, native provider/revision IPC used confined Unix sockets, no Android Activity or UI surface rendered, and phone/Bluetooth/NFC framework processes remained live. |
| Core 1 no-Zygote boundary | Passed on `sos.core1.0805cf6bd0b4.1f3cd4b232c2`: `ro.zygote=no_zygote`, Zygote/system_server/APK processes were absent, CE stayed locked, and the native locked/recovery surface survived a watchdog/retry cycle. |
| Fixed native recovery after child failure | Passed in Shadow, Core 0A, Core 0B, and Core 1 through injected `SIGABRT`; the supervisor remained alive and Retry launched a clean child. Core 0B correctly refused the Android UI action. |
| Recovery and rollback | Passed: Core 1's ADB/recovery path accepted the preserved Compat 1 OTA without a wipe, and the handset was restored to the exact accepted `616ac2404a79` build. |
| Suspend/resume while native UI owns the display | Passed for one earlier Shadow doze/resume cycle; not yet repeated for every accepted stage revision. |
| Raw input acquisition | Passed in logs: Core exclusively grabbed `sec_touchscreen` and `gpio_keys` and observed `sec-pmic-key` while Android remained the display-power owner. |
| Physical touch dispatch and volume chord | Pending owner interaction; opening the devices is not gesture evidence. |
| Trusted lockscreen/FBE/Gatekeeper/Keystore ceremony | Implementation exists for a bounded PIN bridge in Core 0B, but the test handset has `CredentialType: NONE`; no real PIN, Gatekeeper throttle, fingerprint, or authentication-bound key release was exercised. Core 1 therefore remains honestly locked. |
| JNI-free Core provider baseline | Passed for native provider/revision Unix IPC, read-only network state, deterministic native-agent status/candidates, and a bounded semantic document. No Java VM fallback was used for the Core UI. |
| Full native framework bridge | Pending for Wi-Fi scan/SSID/validation/mutations, live-agent credential ceremony, assistive-service delivery/actions, phone, Bluetooth, NFC, and other framework state. |
| Trusted urgent attention | Pending for calls, alarms, security, battery, thermal, and recovery warnings. |
| Removal of Android Java UI and user-install surface | Passed for Core 0B presentation ownership: principal UI APKs are absent, PackageInstaller sessions are rejected, and all Activity starts are blocked. PackageInstaller remains as a non-rendering bootstrap invariant because PackageManager requires exactly one installer. |

## Replacement map

| Android surface | SOS owner |
| --- | --- |
| Navigation and Recents | Core has no traditional navigation. A permanent gesture or hardware chord opens agent/recovery controls. Compat supplies SOS-owned Back/Home/Exit chrome around Android applications. |
| Notification shade | Durable typed attention broker; trusted native code owns calls, alarms, and security warnings. |
| Quick Settings | Typed network, audio, display, battery, and power provider actions. |
| Status bar | Experience-owned presentation over trusted system facts. |
| Lockscreen | Fixed signed native experience; generated code cannot handle PIN, fingerprint, unlock state, or lockout. |
| Android IME | Retained on Compat. Core requires a native composition-aware keyboard, with fixed trusted PIN entry. |
| Settings | Generated provider surfaces for ordinary changes and fixed native confirmation for credentials, permissions, destructive actions, and recovery. |
| APKs | Explicit compatibility workspace on Compat. Core 0B blocks user installation and all Activity rendering; Core 1 has no Zygote or APK process. Inherited, non-executable system APK payloads remain image-size debt until the later pruning pass. |

Android notifications can first enter the Compat attention broker through
[`NotificationListenerService`](https://developer.android.com/reference/android/service/notification/NotificationListenerService).
The initial Core presentation path follows the same native SurfaceComposer
route as AOSP
[`bootanimation`](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/android16-release/cmds/bootanimation/),
while retaining
[`SurfaceFlinger` and Hardware Composer](https://source.android.com/docs/core/graphics/surfaceflinger-windowmanager).
The Core 1 validation product selects AOSP's
[`init.no_zygote.rc`](https://android.googlesource.com/platform/system/core/+/refs/heads/android16-release/rootdir/init.no_zygote.rc)
and remains locked until the service migration gate passes.

## Build and evidence commands

```sh
./tools/a33xctl build-compat0
./tools/a33xctl inspect-compat0
./tools/a33xctl build-compat1
./tools/a33xctl inspect-compat1

./tools/a33xctl build-core-shadow
./tools/a33xctl inspect-core
./tools/a33xctl build-core0a
./tools/a33xctl inspect-core0a
./tools/a33xctl build-core0b
./tools/a33xctl inspect-core0b
./tools/a33xctl build-core1
./tools/a33xctl inspect-core1
```

`build-compat`/`build-sos` remain aliases for Compat 1, and `build-core`
remains an alias for Shadow. Each inspector verifies the stage property,
autostart trigger, package presence/absence, native recovery and watchdog
markers, and APK/Zygote boundary appropriate to that one product. All Core
inspectors reject an accidentally packaged `SosShell` APK.

The shadow-stage native host remains disabled at boot. On an authorized
development device, start and stop it explicitly with:

```sh
adb shell setprop debug.sos.core.host 1
adb shell setprop debug.sos.core.host 0
```

The property is transient and deliberately uses Android's shell-writable
`debug` namespace. A successful start logs `native_gpui_start`,
`sos_experience_host role=core-native`, and one readiness line for each raw
input device. Test the supervisor without root by changing the fault property
from false to true:

```sh
adb shell setprop debug.sos.core.fault 0
adb shell setprop debug.sos.core.fault 1
```

The child receives `SIGABRT`; the signed supervisor must remain alive and show
`SOS Fixed Recovery`. On the device, Volume Up retries SOS and Volume Down
returns to Android. For unattended instrumentation only, make an edge change
to `debug.sos.core.recovery=retry` or `=android`; repeating the same value is
intentionally inert. Setting `debug.sos.core.host=0` sends SIGTERM and removes
either native surface.

The original one-frame probe is still available with
`debug.sos.core.surface_probe`, but new shell work should use
`debug.sos.core.host`.

The six products share one AOSP `out/target/product/a33x` directory. Switching
products triggers Android's install-clean and may remove the preceding
profile's install ZIP, so keep each build/inspect pair together and copy exact
accepted artifacts to the ignored evidence directory before switching.
Overlay staging uses content comparison and does not preserve source
timestamps; this prevents Ninja from silently retaining an older native object
after a same-timestamp source edit. Core build identity includes its compiled
policy inputs, the audited framework policy, and the shared-device no-Zygote
selection patch. Native Core provider and revision traffic uses
`/data/misc/sos/provider.sock` and `/data/misc/sos/revision.sock`; Android
loopback TCP is retained only for Compat.
