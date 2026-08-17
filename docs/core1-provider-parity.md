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

## 2026-08-17 physical-device acceptance result: failed

### Artifact and completed prerequisites

The acceptance attempt used the fresh raw OTA below; it remains outside Git.
`./tools/a33xctl build-core1` and `./tools/a33xctl inspect-core1` passed before
installation. Inspection covered the signed ZIP, AVB chain, PIT ceilings,
ELF identity, product properties, SELinux policy, package contents, and
provider hashes.

| Artifact | Revision | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| `/home/carlid/dev/lineage-a33x/out/target/product/a33x/lineage-23.0-20260817-UNOFFICIAL-sos_core1_a33x.zip` | `sos.core1.40c433d4fb63.1a338e2f0fb5` | 1,022,100,245 | `6262aa874877aae00b46f60882d90cd286a9893010c138ff0bac6669a2942f52` |

One controlled Lineage Recovery sideload completed with exit 0 and reported
`Total xfer: 1.00x`. Android then booted the exact revision. These acceptance
prerequisites passed:

- revision `sos.core1.40c433d4fb63.1a338e2f0fb5`;
- `ro.sos.profile=core`, `ro.sos.providers=core-native`, and
  `ro.zygote=no_zygote`;
- the authority and host processes plus the provider and revision Unix
  sockets; and
- the native Recovery UI.

An empty `sys.boot_completed` is expected for this no-Zygote product and was
not counted as a failure.

### Earliest blocker

The provider acceptance gate failed because
`init.svc.sos_core_platform=restarting`; the daemon repeatedly exited with
status 1. The earliest and repeated AVC was the
`u:r:sos_core_platform:s0` source denied directory `{ search }` on the `sos`
path component with target context
`u:object_r:sos_authority_data_file:s0`. No secondary AVC class was observed.

The source file-context rules label `/data/misc/sos(/.*)` as authority data
and `/data/misc/sos/platform(/.*)` as platform data, while policy grants the
platform daemon access only to its own type. This evidence identifies missing
parent-directory traversal as the smallest justified hypothesis. A live
`ls -lZ` was permission-denied, so this report does not claim a runtime label
listing beyond the AVC and source policy.

The live provider snapshot consequently failed. Thermal, audio, network,
Supplicant, actions, applications, media, attention, on-device injected-
capability rejection, Wi-Fi/audio restoration, restart/fallback, Recovery
coexistence, and sustained soak tests were not run because the earliest
blocker prevented a valid snapshot. No provider action or other
acceptance-matrix device mutation was performed.

### Raw evidence

These generated artifacts remain outside Git:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/core1-provider-boot-1a338e2f0fb5.png` | 17,324 | `fd11466f534905543f75d6c896942cf633250f221ba324e7766890f751de7890` |
| `/tmp/core1-provider-avc-20260817.log` | 6,644 | `62c1caff40f37be960192256990bb4199a126f5a6bd859908562bf4b878cacd6` |

### Minimal fix prepared; later device re-acceptance passed

The source fix adds one auditable relationship:

```text
allow sos_core_platform sos_authority_data_file:dir search;
```

This permits traversal of the authority-labeled `/data/misc/sos` parent only
to reach the separately labeled platform subtree. It does not grant `open`,
`read`, `write`, `getattr`, creation, name mutation, or any file permission;
it does not relabel authority data or change authority/capability controls.
Core 0B remains unchanged and does not run the Core 1 platform daemon.

The Core 1 inspector now requires the compiled policy's complete direct set of
`sos_core_platform` to `sos_authority_data_file` allows to equal exactly
`(allow sos_core_platform sos_authority_data_file (dir (search)))`. This
rejects a missing rule and any silently broadened permission or object class.
Local source-rule, Bash syntax, and diff-format checks passed. At this point no
new AOSP build, compiled neverallow result, OTA, or device result existed, so
the failed acceptance result above remained in force. The final retest below
later passed this specific policy gate.

### Non-shipping live acceptance probe prepared

The existing `provider-state-probe` exercises the unrelated Linux provider
service; it cannot validate the Android authority or Core platform adapter.
A binary pushed under `/data/local/tmp` would run in the shell domain, which
does not have the Core host's DAC and SELinux access to
`/data/misc/sos/provider.sock`. Granting shell access, relabeling a transient
binary, or opening another socket was rejected.

The acceptance harness instead reuses the real `ProviderRequest`,
`ProviderResponse`, `ProviderEffect`, `SystemProviders`, and capability types.
It is compiled into `libsos_core_experience.so` only by the named
`core-provider-acceptance` feature. On Core 1, the existing init-owned
`sos_core_bridge_probe` service and `sos_core_host` domain load the optional
probe export and connect to the real authority Unix socket. No new executable,
product SELinux permission, authority endpoint, provider behavior, Wi-Fi
credential path, or Core 0B behavior is added.

Normal `build-core1` excludes the export, and normal `inspect-core1` rejects an
image that contains it. The explicitly non-shipping test OTA is built and
checked with:

```text
./tools/a33xctl build-core1-provider-probe
./tools/a33xctl inspect-core1-provider-probe
```

The probe has five isolated modes:

| Mode | Behavior |
| --- | --- |
| `snapshot` | Read-only redacted presence/count/status output for Health, thermal, audio/media, link, Supplicant saved networks, applications, attention, and the fixed capability names |
| `security` | Loads current state, injects a staged `power.request_restart` effect, requires capability rejection before staging, and aborts the stage if a regression accepts it |
| `unavailable` | Selects an absent safe capability and requires the authority's explicit not-granted error using only a reserved bogus opaque ID |
| `audio-restore` | Captures volume/mute, changes each available capability by one reversible step, observes it, restores the exact prior value, and observes restoration |
| `wifi-restore` | Uses only an already-saved opaque network ID; disconnects/reconnects an initial connection or connects/disconnects an initially offline device, with bounded observation and restoration |

Output never prints SSIDs, network/app/attention IDs, interface names, titles,
provider error payloads, or credentials. It reports deterministic `PASS`,
`FAIL`, or `SKIP` records and exits 0, 1, or 2 respectively. The audio and
Wi-Fi modes are deliberately separate and must not run without explicit
authorization for reversible device actions.

For each mode, the runner clears old probe logs, writes the mode to
`debug.sos.core.provider_probe`, toggles the existing
`debug.sos.core.bridge_probe` trigger from 0 to 1, and captures
`core_provider_probe` records. The exact single-mode shape is:

```text
adb logcat -c
adb shell setprop debug.sos.core.bridge_probe 0
adb shell setprop debug.sos.core.provider_probe snapshot
adb shell setprop debug.sos.core.bridge_probe 1
adb logcat -b all -d | grep -F core_provider_probe
adb shell setprop debug.sos.core.bridge_probe 0
adb shell setprop debug.sos.core.provider_probe ''
```

Replace `snapshot` with one other supported mode per invocation. Cleanup
returns the trigger to 0 and clears the mode property even after `FAIL` or
`SKIP`. Nothing is staged in writable storage. Six host tests cover
redaction, privileged-effect rejection and regression cleanup, explicit
unavailable semantics, and exact audio/Wi-Fi restoration requests.

### First probe OTA attempt failed; contract fix pending reflash

The first non-shipping probe OTA booted as revision
`sos.core1.40c433d4fb63.940ce909570c`, but it did not produce acceptance
evidence. `snapshot` exited 1 with `FAIL request_or_decode`; `security` exited
1 with `FAIL wrong_rejection`; and `unavailable` exited 1 with `FAIL snapshot`.
The audio/Wi-Fi modes and all later gates were not run. During the security
and unavailable attempts, `sos_core_platform` received signal 13 and restarted
from PIDs 1453 and 1484 to PID 1517; the captured log also records an earlier
PID 944 signal-13 restart. The aggregate raw log remains outside Git:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/core1-probe-matrix.log` | 1,791 | `271c6ca130736149dbd018db342cf3380730e0b84dedbd3b63a7e34e56f4d859` |

Cleanup passed after restoring normal revision
`sos.core1.40c433d4fb63.36d94625c31f`: the probe was absent, the services were
stable, and no AVC was observed.

Code comparison proved two harness contract defects. First, the probe called
the normal UI helper, which converts every `ok=false` authority response into
a generic client error. That erased the exact authority rejection needed to
distinguish expected injected-capability denial from transport failure.
Second, the probe inherited the UI's 500 ms socket deadline even though the
real authority may spend up to two seconds waiting for its nested platform
adapter. The probe could therefore close its authority connection before a
valid response was available. Signal 13 is consistent with a server writing
after its client peer has closed; because separate per-mode timing logs were
not retained, the aggregate evidence does not prove that the platform daemon
will remain stable after the harness fix.

The non-shipping path now consumes the raw `ProviderResponse`, including
`ok=false` and its exact rejection, and uses a five-second per-request
deadline. Shipping UI callers retain their 500 ms deadline and identical
error mapping. The authority and probe client now share the production
newline-delimited JSON framing helpers rather than maintaining two copies;
the authority/provider schema, capability checks, platform protocol, product
policy, and runtime behavior are otherwise unchanged. Probe output now
separates transport/framing failure, load-state rejection, wrong rejection,
and expected capability denial. Inspection requires the packaged test runtime
to expose its framing/deadline contract marker.

Host tests pass snapshot, expected injected-capability rejection, and
unavailable-action semantics through the real `AndroidSystemAuthority`
handler and shared framing. Separate Unix-stream tests cover EOF, truncated
JSON, and a delayed server write after a short-deadline client closes. The six
redaction/restoration tests and ten existing authority tests also pass, as do
focused Clippy and an ARM64/API 31 release feature check. These are local
contract results only; no corrected probe OTA has been built or installed.

### Corrected probe replies pass; platform peer-close hardening pending

The corrected probe contract was exercised on a later non-shipping test image.
Snapshot, security, and unavailable each received a complete `PASS` response
and exited 0. Snapshot completed at 11:05:54.833, followed by platform signal
13 at 11:05:58.262 for PID 938. Security completed at 11:05:57.999, at the
same recorded time as that signal. Unavailable completed at 11:06:03.278,
followed by signal 13 at 11:06:04.266 for PID 1550. Thus the corrected outer
probe framing and rejection mapping work, but platform service stability still
fails the physical gate.

The aggregate diagnostic remains outside Git:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/core1-probe-final-diagnostic.log` | 15,211 | `fd019793daa2261b4b3b0e9eebc1b8844c16976dbc3abd9726a2c7f806449e2d` |

After restoring clean revision `sos.core1.40c433d4fb63.57ac4b474afb`, the
probe was absent, the authority/platform services were stable, and no current
AVC was observed.

The platform adapter was still using plain `write()` for every response frame.
A host subprocess regression with default signal disposition proves that this
old primitive terminates on signal 13 when a Unix-stream peer closes. The
replacement loops over `send(..., MSG_NOSIGNAL)`, retries `EINTR`, completes
partial sends, and surfaces `EPIPE`/`ECONNRESET` to the response handler. The
handler emits one bounded warning and continues serving later connections;
successful response bytes and framing do not change. Tests cover a complete
large response, peer close before and during reply, simulated partial and
interrupted sends, survival with default `SIGPIPE` disposition, and a healthy
connection after a broken peer.

The signal class and unsafe old primitive are proven locally. The timing and
the fact that `sos-core-platform` is the process receiving signal 13 make its
reply helper the narrowest supported call-site hypothesis, but there is no
on-device stack trace proving the exact instruction. The ignored
`write_all()` results in the separate Linux accessibility service are not part
of the Core platform executable and were not changed. Process-wide
`SIGPIPE` masking and ignored send errors were rejected because they would
hide unrelated defects or lose the peer-close result.

At this stage, one secondary AVC was observed during the security probe:
`u:r:hal_wifi_supplicant_default:s0` was denied `efs_file:dir { search }` for
name `/`, device `sda2`, inode 2. It was then a single occurrence and was
absent on the clean boot; the final probe windows below later reproduced it.
No allow or relabel is justified without source attribution.

### Final 2026-08-17 hardware result: partial/open

The final non-shipping test image booted exact revision
`sos.core1.40c433d4fb63.f0ecbf1885d5`. The Core profile,
`ro.sos.providers=core-native`, `ro.zygote=no_zygote`, authority/host/platform
services, and provider/revision sockets all passed. The exact compiled-policy
guard and product inspection passed, confirming that the platform-to-authority
data relationship is only `dir search`. The parent-search AVC did not recur,
so the minimal SELinux fix physically passed without another permission,
relabel, or Core 0B change.

The final snapshot passed with exit 0. The earlier corrected security and
unavailable runs also passed with exit 0 while the platform PID remained
stable. After the product C++ peer-close fix, platform PID 945 stayed stable
through the final action observations; no signal-13 restart occurred. The
five-minute soak passed across five 60-second intervals with the same PID,
both sockets present, and no AVC, crash, or restart.

The action modes were valid skips, not action passes. `audio-restore` exited 2
because the capability/state was unavailable, and `wifi-restore` exited 2
because there was no saved network. Neither mode mutated device state. Active
application, media, and attention owners were absent. Named daemon
restart/fallback and Native Recovery/lock coexistence were not run because the
repository has no supported non-mutating named recipe for those gates.

Raw evidence remains outside Git:

| Artifact | Result | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| `/tmp/core1-final-snapshot-f0e.log` | snapshot PASS, exit 0 | 1,367 | `90d9bbc234b245a9b20bc729277c31722a7c222f4ae664374ca54ed5f5f497f8` |
| `/tmp/core1-final-audio-restore-f0e.log` | audio SKIP, exit 2; no mutation | 862 | `aef5bad22d0061d91317e7e0e2754c5bead6fc161f9166665f7b164955e856f4` |
| `/tmp/core1-final-wifi-restore-f0e.log` | Wi-Fi SKIP, exit 2; no mutation | 1,161 | `7b92a93435ef1eb001d4825bc8ad666965dfdcaf6f61885b2e6f3384524ffdfd` |
| `/tmp/core1-final-soak-f0e.log` | five-minute stability PASS | 1,005 | `a516b218803afafcc540fa95ee7fa41be25d4cd1b51e0f1b9285f78729564f07` |
| `/tmp/core1-final-probe-f0e.png` | final probe screenshot | 17,324 | `fd11466f534905543f75d6c896942cf633250f221ba324e7766890f751de7890` |

The Supplicant-to-EFS denial recurred during probe windows and was absent on
the clean boot. Its target is the `sec_efs` `/dev/block/sda2` filesystem, but
causation and functional impact remain unproven. It remains a vendor-owner
watch item only; no allow or relabel was added.

Final cleanup built and inspected normal Core 1 successfully. Normal
inspection rejected the non-shipping probe export as intended. The raw clean
OTA remains outside Git:

| Artifact | Revision | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| `/home/carlid/dev/lineage-a33x/out/target/product/a33x/lineage-23.0-20260817-UNOFFICIAL-sos_core1_a33x.zip` | `sos.core1.40c433d4fb63.9fcf8d492e9b` | 1,022,100,714 | `91bb35f1d258b10166076af1dbd4a165beabfb3f4ce502bb2ac2a6b244fafbda` |

Its controlled sideload exited 0 with `Total xfer: 1.00x`; this Lineage
Recovery flow automatically rebooted after sideload. The final boot reached
the exact normal revision with services and sockets present, no current AVC or
crash, and no probe. Manual selection of **Reboot system now** is not a
required step for this flow; earlier runbook assumptions to that effect are
superseded.

### Decision and next gate

The minimal SELinux parent traversal and product SIGPIPE fixes physically
pass. Snapshot, injected-capability rejection, unavailable-action semantics,
and five-minute stability pass. Physical-provider acceptance nevertheless
remains **partial/open**: audio and Wi-Fi actions were justified skips rather
than exercised restorations; active applications, media, attention,
calls/alarms, named restart/fallback, Native Recovery/lock coexistence, and a
longer hardware soak remain open. Obtain capability-bearing audio and saved
Wi-Fi state, add supported named recipes for the mutation-sensitive gates,
attribute any recurring vendor Supplicant/EFS denial, and rerun those items.
Do not call provider parity acceptance complete from the evidence above.
