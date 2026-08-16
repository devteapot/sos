# Android UI ownership stages

Date: 2026-08-16

This document defines the concrete a33x build profiles used to move visible
ownership from Android to SOS. Each stage is a separately inspectable and
flashable product; a runtime property does not turn one image into another.

```text
shared a33x hardware, vendor HALs, SOS services, and revision format
├── Compat 0  lineage_sos_compat0_a33x  SOS is enforced HOME
├── Compat 1  lineage_sos_compat_a33x   SOS chrome/attention/APK workspace
├── Shadow    lineage_sos_core_a33x     manual native probe, Android recovery
├── Core 0A   lineage_sos_core0a_a33x  native shell after Android CE unlock
├── Core 0B   lineage_sos_core0b_a33x  native lock/UI, headless framework
└── Core 1    lineage_sos_core1_a33x   no Zygote; native locked/recovery gate
```

## Implemented boundaries

| Stage | Boot and UI owner | Android runtime | Recovery boundary |
| --- | --- | --- | --- |
| Compat 0 | A platform-signed persistent SOS process audits and reasserts `android.app.role.HOME`; Android still draws keyguard, status/navigation, permissions, calls, and other ceremonies. | Full. Launcher3 remains installed but is not an accepted steady-state HOME. | Android UI and Recovery. |
| Compat 1 | SOS remains enforced HOME. Launcher3 is removed from this product. A trusted overlay supplies time/network/battery status plus Back, Apps, Attention, and Exit above selected Android tasks; stock status/expand/navigation content is suppressed while the chrome service owns it. | Full. The workspace enumerates exported launcher activities and opens only an explicit selection. This is task containment, not a separate Android user or data sandbox. | Android credential/permission/call ceremonies and Recovery. |
| Shadow | Android boots normally. The disabled init services expose a one-frame SurfaceComposer probe and the supervised native GPUI shell for manual tests. | Full; no SOS APK is packaged. | Fixed SOS recovery can retry or expose the still-live Android UI. |
| Core 0A | Android performs its existing credential ceremony. When `sys.user.0.ce_available=true`, init starts the native GPUI supervisor, which owns the top display layer and grabs touch/volume input. | Full framework remains installed behind the native layer, but PackageInstaller rejects non-system installation callers. A direct-boot framework bridge is packaged for forward compatibility but is not the unlock owner. | Fixed SOS recovery can retry or expose Android. |
| Core 0B | Init starts the fixed native lock surface once SurfaceFlinger is available. A direct-boot, persistent system-UID process has no Activity and exposes only status and bounded PIN verification over an abstract Unix socket. After LockSettings releases CE storage, GPUI replaces the lock layer. | Zygote and `system_server` remain for headless phone/network/Bluetooth/NFC, LockSettings, Gatekeeper, Keystore, and vendor-backed services. The inherited launcher, SystemUI, settings, chooser, file picker, IME, media/PIM apps, setup/provisioning UI, and other ordinary UI APKs are overridden out. Mixed service/UI packages required by retained headless frameworks stay installed: notably, PackageManager requires exactly one installer package during bootstrap, so `PackageInstaller` remains present while session policy rejects user installs and the immutable Activity policy prevents its UI from rendering. The opaque native/fixed-recovery layer remains presentation owner. | Fixed SOS recovery retries the native host; Android is not offered as a UI fallback. Holding Volume Up+Down asks init to reboot into Recovery. |
| Core 1 | AOSP `core_no_zygote.mk` selects `init.no_zygote.rc`. The native host presents a fixed locked/recovery screen and does not claim CE unlock. | No Zygote, `system_server`, APK process, or Java framework bridge can run. | Native fixed screen; holding Volume Up+Down asks init to reboot into Recovery. |

Core products set an immutable read-only install-policy property. A small
`frameworks/base` patch rejects PackageInstaller sessions from every caller
except system/root when that property is true, including ADB shell and an APK
installer UI. System-owned rollback/update work remains possible. Compat does
not set the property. The policy prevents new user APK installation; preserving
userdata across stage OTAs means already-installed `/data/app` packages remain
on disk. Core 0B's separate Activity-start policy prevents those packages from
presenting an Activity; background components and data still remain until an
explicit migration policy is approved.

## Trusted attention and compatibility space

Compat 1 converts posted Android notifications into a bounded, durable JSONL
journal in credential-protected SOS storage. The adapter classifies call, alarm,
security, media, message, background, and general events. Calls, alarms, and
security events receive a fixed urgent classification. The signed Attention
Activity is the first trusted renderer; generated experiences do not decide
whether those events are security-critical. Notification title/body content is
not copied into device-protected storage; before first unlock Android retains
ownership of its existing call/alarm/security ceremonies.

The compatibility workspace is intentionally explicit: an application does
not become part of SOS merely because it is installed. The user selects one
exported launcher activity, Android creates its normal task, and persistent SOS
chrome remains above it. This stage does not isolate app files, permissions, or
UIDs. A later security-containment gate must choose a separate Android user,
managed profile, or virtualization boundary before the workspace can be called
a data sandbox.

## Credential limitations

Core 0B deliberately accepts only an ASCII PIN of 4–64 digits. The native host
grabs the physical touchscreen before accepting input, never logs the PIN, and
zeroes both native and Java buffers. The bridge checks the peer UID and
delegates verification to `LockPatternUtils`/LockSettings rather than reading
credential material itself. Password, pattern, fingerprint presentation,
biometric lockout messaging, emergency calling, and accessibility remain
gates. A device using a non-PIN primary credential must not be advanced to
Core 0B.

Core 1 is a no-Zygote architecture and recovery validation target, not a fake
unlock implementation. Android synthetic-password enrollment and unwrap bind
Gatekeeper, Weaver where present, vold CE keys, Keystore authentication tokens,
rollback/lockout state, and credential migration. Until a reviewed native
service owns that protocol, Core 1 must remain visibly locked and direct the
operator to Recovery. It must never set `sys.user.0.ce_available` or expose CE
data based on local PIN comparison.

For non-secret bridge instrumentation, `debug.sos.core.bridge_probe=1` starts a
one-shot native client that logs only the credential type and current unlocked
boolean. It has no command for submitting a credential. A successful probe is
evidence for the socket, peer-UID, SELinux, and LockSettings status path; it is
not evidence that PIN verification or CE-key release passed.

## Build and inspection

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

Every device campaign proceeds in that order. Each exact OTA is inspected
before sideload, installed without formatting data, and tested before the next
stage. Core 1 is followed by a Recovery sideload of the latest passing Compat 1
archive so the handset is not left on the intentionally locked validation
target.

## 2026-08-16 physical campaign result

The ordered campaign passed all six boundaries on the connected SM-A336B:

| Stage | Accepted revision | Physical result |
| --- | --- | --- |
| Compat 0 | `db36ed79bb16` | SOS reclaimed HOME with Android UI and user installation retained. |
| Compat 1 | `616ac2404a79` | Launcher3 absent; SOS HOME, chrome, durable attention, and explicit APK workspace passed. |
| Shadow | `1aad692518b8` | Native probe/GPUI, raw-input acquisition, fixed recovery, Retry, and Android escape passed. |
| Core 0A | `1b0c9edec481` | Post-unlock native ownership, install denial, bridge status, fixed recovery, and Android escape passed. |
| Core 0B | `4341fa73391c` | Pre-unlock native lock, direct headless boot completion, Unix provider/revision IPC, absence of Android UI/focus, Activity/install blocking, retained headless phone/Bluetooth/NFC, and no-Android watchdog recovery passed. |
| Core 1 | `1f3cd4b232c2` | No Zygote/system_server/APK process, CE locked, fixed native blocker, watchdog Retry, and Recovery rollback passed. |

Core 0B keeps `PackageInstaller.apk` because `system_server` refuses to boot
unless PackageManager finds exactly one installer. Its session API is denied by
product policy and its Activity cannot render. The framework also completes
boot directly for this headless product because no HOME Activity can report an
idle transition. Native Core provider and revision requests use SELinux-
confined filesystem Unix sockets; the rejected TCP candidates demonstrated
that a Core domain must not depend on Android loopback networking.

The handset's primary credential is `NONE`. The campaign therefore proves the
bridge status path and automatic no-credential CE transition, but not PIN
verification, Gatekeeper throttling, fingerprint, or authentication-bound
Keystore release. No credential was enrolled for testing. Physical touch
dispatch and the actual Volume Up+Down chord also remain owner-operated gates;
ADB fault/recovery properties prove process behavior, not finger input.

After Core 1, Recovery installed the preserved Compat 1 archive with no wipe.
The phone is left on exact revision `616ac2404a79`, unlocked on SOS HOME, with
Launcher3 absent and the SOS compatibility chrome visible.
