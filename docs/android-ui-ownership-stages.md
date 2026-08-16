# Android UI ownership stages

Date: 2026-08-16

This document records the concrete a33x stages used to move visible ownership
from Android to SOS. Historical evidence remains here after an intermediate
product retires; a runtime property does not turn one image into another.

```text
shared a33x hardware, vendor HALs, SOS services, and revision format
├── Compat 0  lineage_sos_compat0_a33x  historical Android-visible bring-up
├── Compat 1  lineage_sos_compat_a33x   native SOS + Android app runtime
├── Shadow    lineage_sos_core_a33x     manual native probe, Android recovery
├── Core 0A   archived                  historical post-CE native shell
├── Core 0B   legacy opt-in             native lock/UI, headless framework
└── Core 1    active                    no Zygote; native locked/recovery gate
```

Core 1 is the sole active Core development target. Core 0A has no product or
CLI entry. Core 0B remains registered only as an explicitly enabled migration
oracle; it is not part of normal builds or release support.

## Source boundaries and target ownership

| Stage | Boot and UI owner | Android runtime | Recovery boundary |
| --- | --- | --- | --- |
| Compat 0 | Historical bring-up evidence only. A platform-signed persistent SOS process owns HOME while Android still draws keyguard, status/navigation, permissions, calls, and other ceremonies. It is not the intended Compat experience. | Full, including Android UI. | Android UI and Recovery. |
| Compat 1 | SOS owns every system surface. Source combines the shared fixed native boot/runtime lock with a restartable, full-frame Rust/GPUI HOME and trusted SOS controls; only the contents of an explicitly selected compatible non-system Android application may appear. Revision `sos.compat1.19d8a653fbd7.220e268c228f` passed the rebuilt exact-image hardware gate, including owner-confirmed side-button lock/wake and native touchscreen ENTER. | Zygote, `system_server`, PackageManager, WindowManager, and app processes remain. SystemUI, Launcher, Settings, chooser/file-picker/IME, setup, dialer, and the other inherited UI packages are removed. Framework policy aborts both initially resolved and framework-redirected system-package Activity launches, suppresses crash/ANR UI, and makes system-UID system windows non-presenting and non-interactive. | The persistent native supervisor first restarts a missing HOME through the headless bridge and owns fixed Recovery only if that bounded restart cannot restore the heartbeat. The GPUI NativeActivity is deliberately restartable and unavailable before CE unlock. Android is an application runtime, never a presentation fallback. |
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

Compat converts posted Android notifications into a bounded, durable JSONL
journal in credential-protected SOS storage. The adapter classifies call, alarm,
security, media, message, background, and general events. Calls, alarms, and
security events receive a fixed urgent classification. The signed Attention
Activity is the first trusted renderer; generated experiences do not decide
whether those events are security-critical. Notification title/body content is
not copied into device-protected storage. Android never receives presentation
ownership as a fallback: before SOS has native handling for an event or
capability, the operation must remain unavailable or reach fixed Recovery.

The compatibility workspace is intentionally explicit: an application does
not become part of SOS merely because it is installed. A launcher-intent
`<queries>` declaration exposes only exported launcher Activities; the adapter
then excludes SOS itself, system/updated-system packages, and legacy targets
that require Android's permission-review ceremony. The user selects one
remaining Activity, Android creates its normal task, and persistent SOS controls
remain above it. Android app content is allowed; Android system content is not.
A request that would normally open PermissionController, PackageInstaller,
Settings, DocumentsUI, a chooser, an IME, keyguard, or another system Activity
is blocked after final framework resolution until SOS has a native broker for
that capability. This stage does not isolate app files, permissions, or UIDs. A
later security-containment gate must choose a separate Android user, managed
profile, or virtualization boundary before the workspace can be called a data
sandbox.

“Native” here is a presentation and trust boundary, not a claim that Android's
task machinery has disappeared. The unlocked GPUI HOME is hosted by one fixed,
platform-signed NativeActivity so WindowManager can compose selected Android
application tasks. That hosting detail may not introduce Android chrome,
widgets, navigation, lockscreen, dialogs, or recovery UI.

Core and Compat do not fork the experience implementation. Both build the same
Rust `ExperienceHost`; Core supplies a standalone SurfaceComposer/input adapter
and Compat supplies the NativeActivity/task adapter. Shared product fragments
select the same native host, runtime, autostart property, and UI-removal marker
for Compat, Core 0B, and Core 1 so package policy cannot drift independently.

The former platform `Button`/`TextView` Compat prototypes are removed. The
workspace, attention journal, and permanent app controls use one small fixed
SOS Canvas renderer plus one headless Android app/task adapter. Those classes
are deliberately outside the generative experience and contain no platform-
default widgets. The Activity opts out of Android decor fitting, hides system
bar insets, draws through the display cutout, and disables bar contrast so SOS
owns the complete physical frame. These remain Android-hosted adapter surfaces,
so pixel parity,
accessibility virtual nodes, touch, and transition behavior are still physical
acceptance gates; they are not a second shell implementation.

All fixed Compat Activities inherit the same full-frame window/focus policy.
The workspace and attention broker pass package-derived strings through a
visible-identity boundary: installed non-system packages become their
user-facing application label, the platform package becomes `SOS RUNTIME`, and
unknown package-shaped strings become `COMPATIBILITY APP`. The task adapter and
chrome controls synchronously hide the overlay before requesting a transition;
destination focus reveals the complete software-text layer after 250 ms for an
SOS Activity or 750 ms for a foreign application. The service therefore never
publishes a stale rectangle-only or partially clipped chrome frame between
tasks.

## Credential limitations

Compat reuses the Core 0B direct SurfaceComposer lock layer and non-rendering
LockSettings bridge during boot. The host releases exclusive touch only after
the trusted unlock transition, then hands off to SOS HOME without exiting its
supervisor. A protected screen-off fact re-enters that same native lock, and a
readiness acknowledgement holds the broadcast until its fixed surface and
exclusive input grabs exist. A signature-protected heartbeat requires both
GPUI HOME and trusted controls to remain ready. Missing heartbeat enters fixed
native Recovery; Retry asks the
headless bridge to restart HOME. Side-button wake/display power, real
credentials, fingerprint, emergency calling, and authentication-bound Keystore
release remain mandatory physical gates. The legacy Android keyguard is not an
allowed substitute.

Credential type `NONE` is a valid LockSettings result (`-1`), not a bridge
failure sentinel. The native host records bridge readiness separately, so it
queries status once and can retain the fixed enter-to-unlock runtime ceremony
without a busy polling loop.

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
./tools/a33xctl build-core1
./tools/a33xctl inspect-core1

SOS_ENABLE_LEGACY_CORE0B_BUILD=1 ./tools/a33xctl build-core0b
./tools/a33xctl inspect-core0b
```

The first six-stage device campaign proceeded in the historical order below;
new campaigns do not rebuild retired stages. Each exact OTA that is selected
for a device is still inspected before sideload and installed without
formatting data. Core 1 may be followed only by the inspected native-Compat archive
`sos.compat1.19d8a653fbd7.220e268c228f` or another explicitly approved
recovery image; the rejected Android-visible Compat archive is development
recovery evidence, not the rollback target.

## 2026-08-16 physical campaign and native-Compat rerun

The historical ordered campaign exercised all six images on the connected SM-A336B. The
later lock report and clarified product definition reclassified the first
Compat 1 image as rejected. A no-wipe rebuilt-image rerun then exercised the
native Compat boundary; the other rows retain their original stage-specific
results:

| Stage | Accepted revision | Physical result |
| --- | --- | --- |
| Compat 0 | `db36ed79bb16` | SOS reclaimed HOME with Android UI and user installation retained. |
| Compat 1 | `220e268c228f` | The prior `616ac2404a79` was rejected for visible Android SystemUI/keyguard. The rebuilt image passed full-frame SOS HOME/workspace/attention, atomic chrome handoffs, explicit modern-app launch, legacy permission-review blocking, restart-first HOME recovery, side-button wake to the native lock, and owner-confirmed touchscreen ENTER return with no Android system surface. |
| Shadow | `1aad692518b8` | Native probe/GPUI, raw-input acquisition, fixed recovery, Retry, and Android escape passed. |
| Core 0A | `1b0c9edec481` | Post-unlock native ownership, install denial, bridge status, fixed recovery, and Android escape passed. |
| Core 0B | `4341fa73391c` | Pre-unlock native lock, direct headless boot completion, Unix provider/revision IPC, absence of Android UI/focus, Activity/install blocking, retained headless phone/Bluetooth/NFC, and no-Android watchdog recovery passed. |
| Core 1 | `1f3cd4b232c2` | No Zygote/system_server/APK process, CE locked, fixed native blocker, watchdog Retry, and Recovery rollback passed. |

The accepted results remain reproducible evidence, not a current support
matrix. Core 0A was retired because Compat 1 and Core 0B cover its useful
boundaries more directly. Core 0B is frozen until Core 1 passes native
synthetic-password/FBE unlock; native power, network, audio, attention,
call/alarm, session, update, and recovery ownership; and the associated
hardware soak. After those gates, Core 0B's product and commands can be removed
without deleting this record or its hashed artifacts.

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

After Core 1, Recovery first installed the preserved Android-visible Compat
archive with no wipe. The later rebuilt campaign installed exact revision
`sos.compat1.19d8a653fbd7.220e268c228f`, also without a wipe. The owner then
completed repeated physical side-button lock/wake and native ENTER cycles; the
host logged eight `native_runtime_unlock_complete credential=none` events and
the final focused surface was the SOS application workspace with no trusted
lock layer. The screen, WindowManager, SurfaceFlinger, and log evidence prove
that only SOS or explicitly selected non-system application contents rendered
throughout the rerun. Real credentials, fingerprint, emergency calling, the
Recovery volume chord, and the remaining service brokers continue as separate
physical/security gates.
