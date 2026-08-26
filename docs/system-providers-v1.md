# System Providers v1 + Stock Shell

## Scope

This milestone replaces seeded HOME models with a live, versioned
system-provider plane on Android, Core, and Linux. The first vertical slice is
clock, power/thermal, connectivity/Wi-Fi, audio/media, compatible applications,
and attention. The Linux adapter also preserves the earlier typed notes,
calendar, display, and input snapshots while those domains are normalized into
the canonical provider document in later ABI slices. Session, richer display
control, telephony, alarm, clipboard, Bluetooth, and removable-storage
contracts remain later slices.

Safety-critical credential, lock, permission, emergency, trusted power
confirmation, and Recovery surfaces remain fixed native code. A Luau revision
may eventually request one of those ceremonies only when the authority grants
its capability. It never implements the ceremony.

## Ownership and flow

The init-owned `sos-authority` remains the canonical registry. It reads facts
that are meaningful without Android directly (wall clock and bounded public
sysfs network/thermal state). Battery and charging facts deliberately use a
typed platform adapter: the coredomain does not bypass the vendor-private
battery-health boundary. Compat 1 and frozen Core 0B select the direct-boot
`SosFrameworkBridge`; Core 1 selects the native `sos-core-platform` daemon with
`ro.sos.providers=core-native`. Both adapters return the same ABI 1 JSON
document over peer-credential-checked abstract local sockets.

The bridge resolves Android-only objects internally:

- `AudioManager`, media sessions, connectivity, and saved Wi-Fi selections;
- exported launcher Activities from eligible, non-system applications;
- active notification attention records and cancellation keys;
- locale/time-zone presentation and framework battery/thermal status.

Core 1 resolves the corresponding non-framework resources internally: Health
and Supplicant stable AIDL HALs, native audio services, a signed native
application manifest, and bounded media/attention state. Its initial app
manifest is empty and it has no registered media or attention producers, so
those inventories and capabilities remain truthfully absent.

Linux selects an in-process adapter in the isolated experience host. UPower,
NetworkManager, and MPRIS are called over typed D-Bus proxies. PipeWire volume
and desktop application launch use strict `wpctl` and `gio launch` argument
vectors because those are stable command surfaces owned by the corresponding
desktop projects. The adapter never invokes a shell. It inventories eligible
`.desktop` entries once per session and exposes only labels plus opaque IDs;
desktop paths, D-Bus object paths, MPRIS names, and executable command lines do
not cross into Luau. Missing daemons or tools remove facts/capabilities rather
than replacing them with guessed state.

Existing native applications are not rewritten. The applications provider
indexes eligible freedesktop desktop entries, exposes only a bounded label and
opaque selection ID, resolves that ID again inside the adapter, and launches
the entry with `gio launch`. The SOS login publishes a normal graphical-session
environment and identifies itself as `SOS:GNOME`, allowing launched clients to
use the user D-Bus and XDG desktop portals while their windows remain ordinary
surfaces under the SOS compositor. A portal or service integration that needs
compositor-specific protocol support can therefore fail independently without
granting Luau direct access to that service.

The applications document also reserves `status_widgets` for SOS-native
applications: a bounded contribution ID, visible label/value and an optional
opaque compatible-application selection. Stock may render up to four in its
top bar. A tap uses the existing capability-checked `apps.launch` action; the
application cannot inject code, a callback, a command line or a Wayland object
into the shell. Linux and Core currently publish an empty list because the
native-application registration broker is not implemented. Ordinary
freedesktop applications remain managed windows and do not receive a synthetic
status contribution.

Only bounded scalar values, visible labels, and opaque selection IDs cross the
socket. Binder objects, Intents, package/Activity names, notification keys,
credentials, and permission tokens do not. The Rust authority rejects an
adapter document whose `abi_version` is not `1`, intersects capabilities with
its own fixed allowlist for both publication and action authorization, and
supplies native facts with no action capabilities when the adapter is
unavailable.

Luau receives the merged value at `model.providers`; the complete field and
effect contract is in [`experience-api.md`](experience-api.md). Android HOME
polls the authority and refreshes the accepted VM from that canonical snapshot.
The resident agent remains a separate local adapter and is preserved across
system-provider refreshes.

## Typed actions

Luau still emits the Scene ABI's serializable `{ provider, action, payload }`
envelope. The authority immediately converts that envelope into a closed Rust
action enum, checks payload bounds and opaque-ID syntax, and requires the
matching capability from a fresh adapter snapshot. Only the typed enum reaches
the selected adapter. Stock and generated revisions use the same path.

The v1 adapters grant bounded absolute or atomic-relative volume, mute, media,
saved-Wi-Fi,
compatible-application launch, and attention-acknowledgement actions when their
underlying resource is present. Lock, restart, and shutdown variants exist in
the authority type boundary but are intentionally absent from the granted
allowlist until the fixed trusted-confirmation surface exists.

On mutable Linux development-live media, `sos-login-session` creates a private
`0600` wildcard grant manifest and explicitly enables the development wildcard
escape hatch. That permits generated revisions to exercise the provider set
while iterating without rebaking the ISO. Non-development sessions do not get
this wildcard; they must supply a private revision-keyed grant manifest.
The development-only `sos-linux-provider-probe` reads through the same
`ProviderHub` and grant manifest and emits a bounded JSON snapshot for SSH
acceptance evidence; it has no action mode.

## Stock trust and fallback

[`default.luau`](../experiences/default.luau) is the substantial Stock Shell
revision described in [`stock-experience.md`](stock-experience.md). Android
stages that exact source at `/system_ext/etc/sos/default.luau`; AVB and the
signed OTA protect it as system content. At every authority start, its
content-addressed revision is installed and pinned independently from the
mutable current pointer. Revision responses identify that pinned stock revision
and its trusted provenance.

If an active generated revision fails runtime validation during system HOME
startup, the host sends the exact failed revision ID. The authority accepts the
request only if that ID is still current, transactionally stages empty stock
state, journals the state/pointer transition, restores the pinned stock
revision, and then lets the supervisor restart the host. A failure of the stock
revision itself is not recursively retried; it escalates to the fixed native
Recovery path.

Linux currently installs the same source read-only and content-addresses the
activated user revision, but the development-live session does not provision a
system-owned stock pin or release verification key. Its optional manifest HMAC
mode is not equivalent to the Android release-signing boundary. A signed,
immutable Linux stock recovery revision therefore remains an explicit release
gate rather than an inferred property of this development overlay.

## Verification status and remaining gates

Desktop unit tests cover ABI merging, the absence of package/Activity fields,
typed payload bounds, missing-capability rejection, ABI-mismatch fail-closed
behavior, attempted privileged-capability injection, coordinated
revision/state fallback, and Luau compilation/rendering. `javac` against the
Android 34 and Lineage framework header jars covers the complete framework
adapter source.
Exact Compat 1 revision `sos.compat1.a3f3bae010bf.b093c3a0b50a` passed the
first physical A33x slice on 2026-08-16. It booted signed stock, matched live
Android clock/power/Wi-Fi/audio/attention facts, completed reversible
volume/mute and saved-Wi-Fi actions, acknowledged attention, automatically
recovered an invalid generated revision to pinned stock, preserved that state
across reboot, and completed 60 provider samples over 124.729 seconds with no
failures or process restarts. The corrected image produced no authority
battery-sysfs AVCs or provider errors during that run. The exact commands,
artifact hashes, failures, and screenshots are indexed in
[`progress.md`](progress.md).

No compatible third-party application or active media session was present, so
successful application launch/return and media-control actions remain hardware
gates; their absent capabilities and rejection paths did pass. The short smoke
soak does not close a long-duration or thermal-load gate. Display/rotation,
session/power facts, trusted power confirmation, calls, alarms, notification
actions beyond acknowledgement, and personal data remain later slices.

Core 1 now has build-level parity for this ABI through the native platform
adapter described in [`core1-provider-parity.md`](core1-provider-parity.md).
The exact no-Zygote product compiles and passes package, VINTF, SELinux,
stable-HAL-linkage, and AVB inspection. It has not yet run this provider slice
on physical hardware. Saved Wi-Fi provisioning, validated reachability,
native media/application owners, native attention producers, and the same
reversible action/restart/soak campaign remain its next acceptance gate.
