# Durable provider and state service

Date: 2026-08-08 (updated 2026-08-28)

This is the Linux-first implementation of the provider/state authority called
for by [`android-exit-verdict.md`](android-exit-verdict.md). It replaces the
legacy daemon's in-memory stage IDs and staged effects with caller-stable,
durable transactions. The old TCP daemon remains unchanged for the APK
regression harness; the new service is the protocol intended for the system
service and revision-supervisor path.

The original single-Experience transaction and its measurements remain below.
The current v4 authority also accepts a `GraphPromotionDraft`, which binds one
or more Experience revisions, states, migrations, and effects to one durable
transaction. The graph supervisor stages that batch, presents the candidate
graph, commits the authority batch, advances registry and graph pointers, and
finalizes the host through the recovery journal described in
[`revision-supervisor.md`](revision-supervisor.md).

## Protocol

`service-protocol` defines versioned, newline-delimited JSON request and response
envelopes. Every request has a caller request ID and one typed method. Responses
repeat the request ID, protocol version, success status, and exactly one typed
payload or error.

The first protocol version contains:

| Category | Types |
| --- | --- |
| Resources | `experience_state`, `notes` |
| Actions | `notes.attach_to_event { note_id, event_title }` |
| Events | transaction staged/aborted, revision committed, action applied, transaction completed |
| Transactions | stage, promote, abort, get/reconcile |
| Faults | before/after stage and before/during/after promotion |

The service accepts filesystem Unix-socket paths on normal Linux and
abstract-namespace addresses written as `@name` on Linux or Android. Abstract
sockets are important on the connected Samsung: the Android shell SELinux
domain denied creation of a pathname socket under `shell_data_file`, while
`@sos-provider-state-probe` worked without weakening policy.

## Durable transaction

A `PromotionDraft` contains a stable transaction ID, expected authority
revision, immutable revision ID, source SHA-256, schema version, complete state,
optional migration proof, and typed provider actions. Staging the same ID with
identical content is idempotent; using the ID for different content is a
conflict. Competing drafts are checked against the current revision again at
promotion.

Promotion has two durable writes:

1. `staged -> committing`: atomically install the new state envelope and record
   the target revision, leaving actions in a durable outbox;
2. `committing -> committed`: apply typed actions to provider resources, record
   deterministic effect IDs `<transaction-id>:<index>`, append typed events, and
   mark the transaction complete in one atomic authority-file replacement.

Each write uses a same-directory temporary file, file `fsync`, rename, and
parent-directory `fsync`. In-memory authority state is replaced only after the
durable write succeeds.

If failure occurs in the middle, the file contains `committing`. Opening the
authority after a process restart completes that transaction. A live daemon
also reconciles pending commits before serving its next request. A retry after
an ambiguous completion returns the existing committed record. Provider state,
effect receipts, and action events therefore contain one application of each
effect ID.

This exactly-once property currently covers providers whose projection is in
the same authority file, including the prototype notes provider. A later
external provider must durably deduplicate the same effect ID; no protocol can
make an arbitrary non-idempotent external side effect exactly once by itself.

## Schema migration consistency

The experience runtime still executes migration code under its own resource
limits. The authority requires a `MigrationProof` whenever the schema increases.
The proof binds the old schema, new schema, and SHA-256 of the exact current JSON
state. Missing, stale, unnecessary, or backward migration proofs are rejected.
This prevents a migrated result from being promoted against a different source
state; it does not attest that arbitrary migration code is semantically correct.

## Fault semantics

| Fault | Durable observation |
| --- | --- |
| `before_stage` | No transaction exists |
| `after_stage` | Transaction is durably staged; identical retry/get reconciles |
| `before_promotion` | Current state and effects remain unchanged |
| `during_promotion` | New state and durable outbox exist as `committing`; restart/next request completes effects |
| `after_promotion` | Transaction and effects are committed; retry returns the same receipts |

Faults are one-shot. Configuration is currently available over the service
protocol for laboratory testing and will need a privileged administrative
boundary before production use.

## Evidence

`cargo test -p service-protocol -p provider-state-service --all-targets` passes
ten tests. The authority cases cover idempotent staging/promotion, typed
resources/actions/events, schema proof validation, every fault boundary,
restart recovery from `committing`, exact-once event/receipt behavior, competing
writers, abort, and event bounds. The daemon test uses a real separate process
and Unix socket and confirms it survives an ambiguous post-promotion response.
The full workspace passes 42 tests after adding the service.

The same service and probe were cross-built for ARM64 Android and run as shell
processes on the connected Samsung SM-A336B, API 35. A `during_promotion` fault
returned an ambiguous response; retry reconciled transaction `device-probe-1`
to revision 1 with one effect, one notes attachment, and four ordered events.
After killing and restarting the daemon against the same file, a read-only probe
reported the identical revision ID, attachment count, and event count.

The ignored device authority artifact
`/data/local/tmp/sos-provider-state-run/authority.json` was 2,452 bytes with
SHA-256 `36412cad9a42f5b5af4602134dbf449f9177fb1fa831295d492676ea8549f28d`.
The dirty `bde9daf169b4` service/probe binaries were 525,928/481,896 bytes with
SHA-256 `44901b17df06bee738c06066a39ad43025ac3e0d22dd72f9a86930241cbb6648` and
`6abcaaed2c789aa065180b7bb224ca14108caa38f9bf7d14e32e232c0707607c`.
They and the device test directory were removed after recording this evidence.

Two environment failures shaped the device path. Plain target Cargo used the
host linker and could not find Android `log`/`unwind`. `cargo-ndk` then attempted
to execute the installed x86-64-only NDK Clang on the ARM64 development host and
failed through the incomplete binfmt environment. `tools/android-clang-linker`
uses the native system Clang with the NDK 29 sysroot/runtime. The first on-device
server attempt then exposed the pathname-socket SELinux denial; adding abstract
Unix addresses fixed transport without changing device policy.

## Supervisor integration

The v4 graph supervisor binds authority transactions to exact Experience and
graph identities. Its journal covers presentation, authority commit, registry
commit, graph-pointer commit, host finalization, rollback, and restart. Android
uses an init-supervised authority and bounded platform graph IPC; Linux uses the
Unix-socket service. The retired single-revision ordering remains in
[`coordinated-activation.md`](coordinated-activation.md) only as historical
evidence.
