# SOS Compat and SOS Core

Date: 2026-08-15

SOS now has two named a33x product profiles over one hardware, service, and
revision base:

```text
shared a33x hardware + SOS services + revision format
├── SOS Compat: Android runtime and APK compatibility
└── SOS Core:   native SOS UI, no Android Java UI
```

The destination is clear, but ownership moves only after its replacement is
working. The first Core build is therefore an explicit **shadow stage**, not a
claim that Core 0 has passed.

## Product boundary

| Profile | Build product | Current owner | Intended boundary |
| --- | --- | --- | --- |
| SOS Compat | `lineage_sos_compat_a33x` | SOS HOME plus Android SystemUI/framework | Zygote, ART, PackageManager, `system_server`, Android applications, and compatibility ceremonies remain available. |
| SOS Core | `lineage_sos_core_a33x` | Android recovery UI while the native probe is manual | A native init-launched permanent shell owns presentation; Android Java UI and user APK installation are absent at Core 0. |

Both inherit the same Samsung a33x device/vendor graph, init, SELinux, Binder,
SurfaceFlinger, Hardware Composer, audio services, Keystore, Gatekeeper, vendor
HALs, on-device SOS authority, and revision format 3. The split does not fork
hardware enablement or experience artifacts.

Compat packages the existing platform-signed `SosShell` NativeActivity and its
framework overlay. Core does not package that Activity. It instead packages
`sos-core-surface-probe`, a native C++ executable that obtains the primary
display through `SurfaceComposerClient`, creates a top-level surface, obtains
its `ANativeWindow`, and presents one EGL frame. Its init service is `disabled`
and never auto-starts. The probe establishes the native SurfaceFlinger route
needed by a future GPUI/wgpu host while Android remains available for recovery.

The Core image currently retains SystemUI, Launcher3QuickStep, Settings, and
LatinIME and declares `ro.sos.ui_owner=android-shadow`. This is deliberate.
Removing them before the replacement has input, lockscreen, and recovery would
create an unreviewable black-screen/security failure. `Core 0` is not complete
until that property becomes `native-sos` and the physical acceptance gate
below passes.

## Why Zygote remains at Core 0

SystemUI, Launcher, Settings, and LatinIME are visible Android products, but
Zygote and `system_server` also host or coordinate working telephony, network,
Bluetooth, NFC, permissions, storage, and credential services. Removing them is
therefore a service migration, not a UI cleanup.

Core advances in two stages:

1. **Core 0 — no Android Java UI.** Keep the Android native substrate and,
   temporarily, background framework services. Package no SystemUI, Launcher,
   Settings, ordinary user-facing applications, or SOS Activity. Disable user
   APK installation.
2. **Core 1 — no Zygote/APK runtime.** Select AOSP's no-Zygote init mechanism
   only after native replacements exist for every required phone, network,
   Bluetooth, NFC, input, credential, and recovery service.

Going directly to Core 1 would discard working services as a group and makes
fault attribution needlessly difficult.

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
| APKs | Explicit compatibility workspace on Compat; absent from Core. |

Android notifications can first enter the Compat attention broker through
[`NotificationListenerService`](https://developer.android.com/reference/android/service/notification/NotificationListenerService).
The initial Core presentation path follows the same native SurfaceComposer
route as AOSP
[`bootanimation`](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/android16-release/cmds/bootanimation/),
while retaining
[`SurfaceFlinger` and Hardware Composer](https://source.android.com/docs/core/graphics/surfaceflinger-windowmanager).
Core 1 can later use AOSP's
[`init.no_zygote.rc`](https://android.googlesource.com/platform/system/core/+/refs/heads/android16-release/rootdir/init.no_zygote.rc)
after the service migration gate passes.

## Build and evidence commands

```sh
./tools/a33xctl build-compat
./tools/a33xctl inspect-sos

./tools/a33xctl build-core
./tools/a33xctl inspect-core
```

`build-sos` remains an alias for `build-compat`. `inspect-core` intentionally
requires Android SystemUI and Launcher to remain in the shadow-stage image and
rejects an accidentally packaged `SosShell` APK. When Core 0 ownership is
ready, that inspector must change in the same commit as the product package
removal and physical acceptance procedure.

The shadow-stage native display probe remains disabled at boot. On an
authorized development device, start and stop it explicitly with:

```sh
adb shell setprop debug.sos.core.surface_probe 1
adb shell setprop debug.sos.core.surface_probe 0
```

The property is transient and deliberately uses Android's shell-writable
`debug` namespace. A successful start must log `native_surface_ready` with the
physical display dimensions, and setting it back to `0` must restore the
Android recovery UI.

The two products share one AOSP `out/target/product/a33x` directory. Switching
products triggers Android's install-clean and may remove the other profile's
install ZIP, so keep each build/inspect pair together in the order shown.
