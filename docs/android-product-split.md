# SOS Compat and SOS Core

Date: 2026-08-16

SOS has two product families over one hardware, service, and revision base. Six
historical ownership stages established the migration evidence; Core 1 is now
the sole active Core development target:

```text
shared a33x hardware + SOS services + revision format
├── SOS Compat
│   ├── Compat 0: SOS enforced as HOME; Android ceremonies remain
│   └── Compat 1: native SOS presentation; Android app runtime only
└── SOS Core
    ├── Shadow:  manual native probe; Android remains recovery owner
    ├── Core 0A: archived historical stage; product removed
    ├── Core 0B: frozen opt-in headless-framework migration oracle
    └── Core 1:  active no-Zygote target
```

Current build products make the ownership decision reviewable in the OTA
itself through `ro.sos.lifecycle`; archived evidence remains revision-pinned in
the documents. The detailed package, boot, credential, and recovery contracts are in
[`android-ui-ownership-stages.md`](android-ui-ownership-stages.md).

## Product boundary

| Stage | Build product | UI owner and boundary |
| --- | --- | --- |
| Compat 0 | `lineage_sos_compat0_a33x` | Historical bring-up only. A persistent platform policy reasserts SOS as HOME while Android still draws system ceremonies. It is not the product vision. |
| Compat 1 | `lineage_sos_compat_a33x` | SOS owns pre-unlock, runtime re-lock, HOME, task controls, attention, system facts/actions, and Recovery. Android retains only framework/app-runtime services and explicitly selected non-system application Activities. Exact revision `sos.compat1.19d8a653fbd7.220e268c228f` passed the rebuilt image, transition, application, policy, restart, native-lock, side-button cycle, and owner-confirmed touchscreen ENTER hardware gates. |
| Shadow | `lineage_sos_core_a33x` | Android remains owner until a disabled native probe or GPUI supervisor is started manually. |
| Core 0A | Archived; no product | Historical evidence only. Android performed CE unlock before native SOS acquired presentation/input ownership. |
| Core 0B | `lineage_sos_core0b_a33x` (`ro.sos.lifecycle=legacy`) | Frozen, explicit-opt-in migration oracle. A fixed native lock starts before CE while Zygote/`system_server` retain headless services and the no-Activity bridge. |
| Core 1 | `lineage_sos_core1_a33x` (`ro.sos.lifecycle=active`) | Sole active Core target. AOSP's no-Zygote init is selected; the native host exposes an honest locked/recovery surface, and System Providers v1 selects the native Health/Supplicant/audio/inventory adapter. Synthetic-password unlock and the remaining displaced services still need native owners. |

Both inherit the same Samsung a33x device/vendor graph, init, SELinux, Binder,
SurfaceFlinger, Hardware Composer, audio services, Keystore, Gatekeeper, vendor
HALs, on-device SOS authority, and revision format 3. The split does not fork
hardware enablement or experience artifacts.

Both Compat stages package the platform-signed `SosShell` NativeActivity. Its
HOME intent filter has priority 1000, while a direct-boot policy uses the system
role API to repair ownership after boot, package replacement, or an external
role change. Compat 0 retains its historical persistent Android-owned HOME
process. Compat 1 instead keeps the no-Activity framework bridge persistent and
makes the GPUI process restartable under the native host; a stale NativeActivity
thread can therefore be discarded after a renderer failure. The HOME and its
fixed workspaces are not direct-boot-aware and the bridge refuses to start them
before CE unlock.

Compat 1 packages the Core fixed pre-unlock host and non-rendering LockSettings
bridge, removes the inherited Android UI package set except for the LatinIME
input service, and blocks Activity launches from remaining system packages.
LatinIME is retained only as the framework IME implementation behind GPUI's
host-owned editor; its setup/settings Activities remain unreachable through
the same system-Activity policy. The framework applies that block both
to the caller's initially resolved target and to the final target after
interception, legacy permission review, or ephemeral-installer redirection. A
framework window membrane also makes system-UID system windows transparent,
non-focusable, and non-touchable without breaking framework progress callbacks.
It does not block ordinary non-system application Activities. The NativeActivity
is a trusted full-frame window host for GPUI, not permission to expose Android's
system experience.

The product split is composition, not an experience fork. Core and Compat build
the same Rust `ExperienceHost`. A standalone SurfaceComposer/raw-input adapter
hosts it in Core; a NativeActivity/task adapter hosts it in Compat. Shared make
fragments own the native host/runtime/autostart set, while each product
explicitly selects its package-removal marker: Compat retains LatinIME and the
Core profiles do not. Compat-only code is restricted to headless Android
framework/task facts and the fixed controls required around a selected app.

Compat's fixed Android-hosted surfaces share one full-frame window policy,
Canvas renderer, and Activity focus lifecycle. Workspace and attention labels
are derived from user-facing application labels; raw package identifiers and
the platform package name are never presentation strings. The permanent chrome
is hidden synchronously in the control or app-launch input event, then revealed
atomically after the destination has focus. SOS-owned destinations use the
short focus delay while foreign app tasks use a longer delay, so a stale or
partially rasterized control frame cannot survive a task handoff.

The native supervisor treats HOME as restartable. A lost HOME/chrome heartbeat
first asks the persistent headless bridge to launch a clean HOME process; fixed
native Recovery is entered only if that bounded restart request is unavailable
or fails to restore the heartbeat. This keeps the trusted supervisor and lock
surface shared with Core without turning ordinary NativeActivity process loss
into a sticky recovery screen.

The Compat visible-output invariant is exact: a frame may contain SOS or the
contents of one explicitly selected compatible non-system Android app, and
nothing else. Package visibility is limited to exported launcher Activities;
legacy targets that require Android's permission-review ceremony are excluded.
SOS HOME is the independent `sos.stock.mobile` v4 experience, with a
phone-native top bar, bottom navigation, touch-first launcher, and full-screen
root presentation. Android never boots or adapts Linux `sos.stock.shell`.
Stock keyguard, status/navigation bars, notification/quick-settings shade,
Settings, permission and install dialogs, chooser/file picker, IME settings or
setup Activities, setup, dialer/emergency UI, crash/ANR dialogs, and Recovery
are forbidden. The LatinIME keyboard window is the narrow exception: it is an
input service for the SOS-owned GPUI editor, not a system-surface fallback.
Missing SOS
replacements fail closed instead of opening the Android implementation.

Core never packages the SOS Activity. It packages two native bring-up
programs:

- `sos-core-surface-probe`, the original one-frame SurfaceComposer/EGL probe;
- `sos-core-host`, a fixed signed supervisor that creates the top-level
  `ANativeWindow`, loads the standalone GPUI/wgpu experience, reads raw touch
  and hardware-key events, and owns a CPU-rendered recovery surface if the GPUI
  child fails.

The host has no Activity, JNI lifecycle, or APK data directory. It uses a
dedicated SELinux domain and `/data/misc/sos/core`, while the SOS revision,
authority, and provider services remain shared. Shadow is manually triggered.
The retired Core 0A started only after Android reported CE available; frozen
Core 0B and active Core 1 start after SurfaceFlinger. Every child is supervised by fixed signed
native code. Failure presents a CPU-rendered recovery surface, and the locked
stages accept Volume Up+Down as a direct Recovery reboot chord.

## Why Zygote remains at Core 0

SystemUI, Launcher, and Settings are visible Android products, while LatinIME
combines a required keyboard service with setup/settings UI. Zygote and
`system_server` also host or coordinate working telephony, network,
Bluetooth, NFC, permissions, storage, and credential services. Removing them is
therefore a service migration, not a UI cleanup.

Core historically advanced through two Core 0 gates before no-Zygote:

1. **Core 0A — archived native ownership with Android recovery.** Android UI stayed
   installed but becomes inert behind the native top layer after trusted CE
   unlock. This isolates shell, input, watchdog, and recovery failures.
2. **Core 0B — frozen no-Android-rendered migration oracle.** Principal UI packages are
   overridden out and an immutable product policy aborts every Activity start,
   including starts from preserved `/data/app` packages on no-wipe upgrades.
   The remaining SOS Java process is a direct-boot, system-UID bridge with no
   Activity; it delegates bounded PIN verification to LockSettings.
   Phone/network/Bluetooth/NFC framework services remain.
3. **Core 1 — active no-Zygote/APK target.** AOSP's no-Zygote init is selected. The
   initial target deliberately remains locked because native Android
   synthetic-password unwrap and the displaced services are not yet complete.

Core 0A no longer has a product definition. Core 0B requires an explicit
legacy-build opt-in and receives no new product features. Core 1 therefore
proves the process and native recovery boundary; it does not
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
| Compat 1 ownership | The old `sos.compat1.0805cf6bd0b4.616ac2404a79` was rejected for visible SystemUI/keyguard. Rebuilt `sos.compat1.19d8a653fbd7.220e268c228f` passed exact-image hardware evidence for full-frame HOME/workspace/attention, selected-app containment, redirected-system-Activity blocking, HOME crash restart, side-button wake to `SOS Trusted Lock`, and owner-confirmed touchscreen ENTER return, with no Android system surface. |
| Shadow display and failure boundary | Passed on `sos.shadow.0805cf6bd0b4.1aad692518b8`: both the one-frame probe and GPUI rendered through SurfaceComposer/Samsung HWC, injected failure reached fixed recovery, Retry relaunched GPUI, and Android escape exposed the intact framework UI. |
| Core 0A ownership (archived) | Passed on `sos.core0a.0805cf6bd0b4.1b0c9edec481`: GPUI started after CE unlock, raw input was acquired, user install was rejected, and Android remained an explicit failure escape. |
| Core 0B headless framework (legacy) | Passed on `sos.core0b.0805cf6bd0b4.4341fa73391c`: native lock was visible before CE, headless boot reached `RUNNING_UNLOCKED`, native provider/revision IPC used confined Unix sockets, no Android Activity or UI surface rendered, and phone/Bluetooth/NFC framework processes remained live. |
| Core 1 no-Zygote boundary | Passed on `sos.core1.0805cf6bd0b4.1f3cd4b232c2`: `ro.zygote=no_zygote`, Zygote/system_server/APK processes were absent, CE stayed locked, and the native locked/recovery surface survived a watchdog/retry cycle. |
| Core 1 System Providers v1 build boundary | Passed exact product build/inspection on `sos.core1.f4d780007972.812bca990cc5`: the AArch64 native adapter, Health v4 and Supplicant v4 clients, audio actions, signed app manifest, media/attention state, authority socket policy, VINTF, SELinux, package signature, and AVB chain were present. No physical provider test was run, so hardware behavior is pending. |
| Fixed native recovery after child failure | Passed in Shadow, Core 0A, Core 0B, and Core 1 through injected `SIGABRT`; the supervisor remained alive and Retry launched a clean child. Core 0B correctly refused the Android UI action. |
| Recovery and rollback | Mechanism passed, and revision `220e268c228f` is now the inspected, sideloaded, native-Compat rollback artifact for the next Core campaign. Recovery accepted it without a wipe and reported `Total xfer: 1.00x`. |
| Suspend/resume while native UI owns the display | Passed for one earlier Shadow doze/resume cycle; not yet repeated for every accepted stage revision. |
| Raw input acquisition | Passed in logs: Core exclusively grabbed `sec_touchscreen` and `gpio_keys` and observed `sec-pmic-key` while Android remained the display-power owner. |
| Physical touch dispatch and volume chord | Compat physical touch dispatch passed: eight native no-credential unlock completions were observed across owner-operated lock/wake/ENTER cycles. The Volume Up+Down Recovery chord remains pending owner interaction. |
| Trusted lockscreen/FBE/Gatekeeper/Keystore ceremony | Implementation exists for a bounded PIN bridge in Core 0B, but the test handset has `CredentialType: NONE`; no real PIN, Gatekeeper throttle, fingerprint, or authentication-bound key release was exercised. Core 1 therefore remains honestly locked. |
| JNI-free Core provider baseline | Passed for native provider/revision Unix IPC, read-only network state, deterministic native-agent status/candidates, and a bounded semantic document. No Java VM fallback was used for the Core UI. |
| Native platform-service replacement | The first provider ABI slice now builds without Zygote: health power facts, Supplicant saved-network selection, native audio, signed application inventory, media/attention paths, and a fixed memory-only Rust/GPUI OpenRouter credential ceremony. Physical execution of that ceremony and a live `deepseek/deepseek-v4-flash-0731` prompt, native Wi-Fi provisioning and validation, active media/app/attention producers, assistive delivery/actions, phone, Bluetooth, NFC, and other displaced services remain pending. |
| Trusted urgent attention | Pending for calls, alarms, security, battery, thermal, and recovery warnings. |
| Removal of Android Java UI and user-install surface | Passed for Core 0B presentation ownership: principal UI APKs are absent, PackageInstaller sessions are rejected, and all Activity starts are blocked. PackageInstaller remains as a non-rendering bootstrap invariant because PackageManager requires exactly one installer. |

## Replacement map

| Android surface | SOS owner |
| --- | --- |
| Navigation and Recents | Core has no traditional navigation. A permanent gesture or hardware chord opens agent/recovery controls. Compat supplies SOS-owned Back/Home/Exit chrome around Android applications. |
| Notification shade | None. Durable typed attention is rendered by SOS; trusted SOS code owns calls, alarms, and security warnings. |
| Quick Settings | Typed network, audio, display, battery, and power provider actions. |
| Status bar | Experience-owned presentation over trusted system facts. |
| Lockscreen | Fixed signed native experience on Compat and Core; generated code cannot handle PIN, fingerprint, unlock state, or lockout. Android keyguard is never a visible fallback. |
| Android IME | Compat retains the direct-boot-aware LatinIME service as the keyboard behind the GPUI-owned composing editor; its Activities are blocked and SOS remains presentation owner around it. Frozen Core 0B and active no-Zygote Core 1 exclude LatinIME and use the SOS-native composition-aware keyboard, with fixed trusted PIN entry. |
| Settings | Generated provider surfaces for ordinary changes and fixed native confirmation for credentials, permissions, destructive actions, and recovery. |
| APKs | Explicit non-system application workspace on Compat, with SOS controls retained and system-package Activities blocked. Legacy Core 0B blocks all Activity rendering; active Core 1 has no Zygote or APK process. Inherited, non-executable system APK payloads remain image-size debt until the later pruning pass. |

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
./tools/a33xctl build-core1
./tools/a33xctl inspect-core1

SOS_ENABLE_LEGACY_CORE0B_BUILD=1 ./tools/a33xctl build-core0b
./tools/a33xctl inspect-core0b
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

The five remaining products share one AOSP `out/target/product/a33x` directory. Switching
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
