# System Providers v1 + Stock Base v0

## Scope

This milestone replaces the Android system product's seeded HOME model with a
live, versioned system-provider plane. The first vertical slice is clock,
power/thermal, connectivity/Wi-Fi, audio/media, compatible applications, and
attention. Display, session, telephony, alarm, and personal-data providers are
later slices.

Safety-critical credential, lock, permission, emergency, trusted power
confirmation, and Recovery surfaces remain fixed native code. A Luau revision
may eventually request one of those ceremonies only when the authority grants
its capability. It never implements the ceremony.

## Ownership and flow

The init-owned `sos-authority` remains the canonical registry. It reads facts
that are meaningful without Android directly (wall clock and bounded public
sysfs network/thermal state). Android battery and charging facts deliberately
come through the typed framework/health bridge: the platform coredomain does
not bypass the vendor-private battery-health boundary. The direct-boot
`SosFrameworkBridge` supplies a JSON document containing only facts that
require framework services. The two system-UID processes authenticate through
peer credentials on the existing abstract local socket.

The bridge resolves Android-only objects internally:

- `AudioManager`, media sessions, connectivity, and saved Wi-Fi selections;
- exported launcher Activities from eligible, non-system applications;
- active notification attention records and cancellation keys;
- locale/time-zone presentation and framework battery/thermal status.

Only bounded scalar values, visible labels, and opaque selection IDs cross the
socket. Binder objects, Intents, package/Activity names, notification keys,
credentials, and permission tokens do not. The Rust authority rejects a bridge
document whose `abi_version` is not `1`, intersects capabilities with its own
fixed allowlist, and supplies native facts with no action capabilities when the
bridge is unavailable.

Luau receives the merged value at `model.providers`; the complete field and
effect contract is in [`experience-api.md`](experience-api.md). Android HOME
polls the authority and refreshes the accepted VM from that canonical snapshot.
The resident agent remains a separate local adapter and is preserved across
system-provider refreshes.

## Typed actions

Luau still emits the Scene ABI's serializable `{ provider, action, payload }`
envelope. The authority immediately converts that envelope into a closed Rust
action enum, checks payload bounds and opaque-ID syntax, and requires the
matching capability from a fresh bridge snapshot. Only the typed enum reaches
the bridge. Stock and generated revisions use the same path.

The v1 bridge grants bounded volume/mute, media, saved-Wi-Fi,
compatible-application launch, and attention-acknowledgement actions when their
underlying resource is present. Lock, restart, and shutdown variants exist in
the authority type boundary but are intentionally absent from the granted
allowlist until the fixed trusted-confirmation surface exists.

## Stock trust and fallback

[`default.luau`](../experiences/default.luau) is Stock Base v0. The product
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

## Verification status and remaining gates

Desktop unit tests cover ABI merging, the absence of package/Activity fields,
typed payload bounds, missing-capability rejection, coordinated revision/state
fallback, and Luau compilation/rendering. `javac` against the Android 34 and
Lineage framework header jars covers the complete framework bridge source.
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
