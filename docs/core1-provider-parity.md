# Core 1 native provider parity

Date: 2026-08-16

## Meaning of parity

Core 1 now implements the same System Providers ABI v1 boundary used by
Compat 1. Parity here means that stock and generated Luau revisions receive
the same typed provider document and emit the same typed actions on either
product. It does not mean that Core 1 secretly recreates `system_server`, or
that every resource is present on the current no-Zygote image.

The authority remains canonical on both products:

```text
Luau stock/generated revision
        |
        | bounded provider values and typed effects
        v
init-owned sos-authority
        |
        | peer-credential-checked abstract Unix socket, ABI 1 JSON
        +-- Compat 1 / legacy Core 0B: headless Java framework adapter
        `-- Core 1:                 native sos-core-platform adapter
```

Core 1 selects the second path with the immutable product property
`ro.sos.providers=core-native`. The authority rejects an ABI mismatch, applies
one fixed capability allowlist to snapshots and action authorization, and
falls back to native clock/link/thermal facts with no adapter actions if the
daemon is unavailable.

## Native ownership in this slice

| Provider | Core 1 owner | Facts and actions | Initial-image availability |
| --- | --- | --- | --- |
| Clock | `sos-authority` | libc wall clock plus locale/time-zone properties | Live |
| Power | `sos-core-platform` + authority thermal fallback | Stable Health AIDL battery/charging/temperature; public thermal zones | Built; hardware execution pending |
| Connectivity | Native adapter | Link state from public sysfs; saved-network IDs, SSIDs, RSSI, select, and disconnect through stable Supplicant AIDL | Built; saved inventory depends on a native provisioner |
| Audio | Native adapter | Music-stream volume and mute facts/actions through `AudioSystem`/audioserver | Built; hardware execution pending |
| Media | Native adapter | Bounded native media state plus fixed play/pause/next/previous datagram endpoint | Inactive until a native media owner registers state |
| Applications | Signed product manifest | Labels and opaque IDs from `/system_ext/etc/sos/core-apps.json`; launch through a fixed native target endpoint | Truthfully empty; Core 1 has no APK runtime or native app entries yet |
| Attention | Native adapter | Bounded durable journal, normalized opaque IDs, urgent count, and acknowledgement | Truthfully empty; native call/alarm/system producers are not registered yet |

Capabilities are resource-derived. Audio mutations appear only when
audioserver answers; media controls require an active media record; Wi-Fi
connect requires saved Supplicant networks and disconnect requires an actual
Wi-Fi link plus live Supplicant interface; launch and acknowledgement require
current entries. An empty inventory is represented as an empty inventory, not
as prototype content.

Core 1 starts the existing vendor `wpa_supplicant` only for the
`core-native` provider profile and talks to its stable AIDL service. It does
not read a vendor control socket or receive credentials. Modern Android saved
network configuration is normally reconstructed by the framework Wi-Fi owner;
there is not yet a native credential/provisioning owner to migrate that state.
Consequently a clean Core 1 boot may expose link facts while correctly
withholding saved-network actions.

## Trust boundary

The platform daemon runs as system UID in its own `sos_core_platform` SELinux
domain. Policy grants only:

- Health and Supplicant HAL client access;
- the native audio service calls needed by this slice;
- read-only public network sysfs;
- its private `/data/misc/sos/platform` state directory; and
- the fixed local endpoints for future native media/application owners.

Only the system-UID authority may connect to `@sos_core_platform`. The daemon
returns JSON scalars, labels, and opaque identifiers. Luau and the authority do
not receive Binder objects, Intent-like targets, package internals, Wi-Fi
credentials, or raw notification keys. Application targets stay inside the
signed manifest and are resolved only in the native adapter.

Lock, credential, permission, emergency, restart, shutdown, and Recovery
surfaces remain fixed native code. Their action variants exist in the closed
Rust enum, but the platform-adapter allowlist cannot grant them. The same
filter is applied both when publishing capabilities and when authorizing an
effect.

## Verification and open hardware gate

Host tests exercise live native fallback facts, complete ABI merging through a
generic adapter, typed payload bounds, missing-capability rejection, adapter
ABI mismatch, and attempted privileged-capability injection. Android arm64/API
31 cross-checking covers the authority. The exact Core 1 product build compiles
the C++ adapter against the Health v4 and Supplicant v4 stable NDK interfaces,
audioserver, init, VINTF, and the complete SELinux neverallow/compatibility
suite. `inspect-core1` verifies the produced AArch64 binary, stable interface
dependencies, product selector, app manifest, authority connection rule,
absence of direct battery-sysfs access, signed package, and AVB chain.

This is build and ABI parity, not a physical-provider acceptance result. The
next gate is an authorized A33 boot that records live daemon/authority
snapshots and reversible audio and Wi-Fi behavior, forces daemon and authority
restarts, exercises fallback to stock, and verifies lock/recovery coexistence.
Native Wi-Fi provisioning, media/app owners, attention producers, validated
Internet reachability, calls/alarms, and a sustained power/thermal/soak run
remain explicit follow-up work.
