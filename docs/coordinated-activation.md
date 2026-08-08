# Coordinated revision and state activation journal

Date: 2026-08-08

This contract joins the stable-host [`revision-supervisor.md`](revision-supervisor.md)
to the durable provider/state authority. “Activation” means making a prepared
Luau scene current. The provider service may retain `promote` as its internal
transaction verb, but there is no Luau-to-native experience promotion tier.

## Commit decision and ordering

The durable service transaction remains the commit decision. `current` never
points at a candidate whose state/effect transaction is merely staged.

1. Verify that the service's current revision ID equals the supervisor's
   `current` revision.
2. Verify the staged transaction's revision ID, source SHA-256, schema, and JSON
   state against the immutable Luau revision.
3. Durably write `activation-journal.json` in phase `intent`.
4. Ask the permanent host to prepare the candidate VM and retained scene while
   the accepted scene remains active.
5. Commit the service transaction, reconciling ambiguous responses by stable
   transaction ID.
6. Durably advance the journal to `service_committed`.
7. Ask the same host process to present the prepared scene, confirm its event
   loop remains alive, atomically replace `current`, and advance the journal to
   `pointer_committed`.
8. Remove the journal and `fsync` its directory entry.

The service may therefore be briefly newer than the visible scene, but the
visible scene is never newer than durable state/effects. The production host
must quiesce old-revision input across steps 5–7.

## Recovery matrix

On startup the supervisor boots the revision named by `current`, reads the
journal, and asks the service for the stable transaction. Querying also lets the
service finish a durable `committing` outbox left by its own crash.

| Service transaction | Pointer | Recovery decision |
| --- | --- | --- |
| `staged` or `aborted` | previous | Abort if needed, keep previous, clear journal |
| `staged` or `aborted` | candidate | Reactivate previous, abort if needed, clear journal |
| `committed` | previous | Prepare/present candidate, switch pointer, clear journal |
| `committed` | candidate | Keep candidate, clear journal |

An unavailable service, invalid journal, unknown transaction, or immutable
binding mismatch is not guessed through. After service commit, semantic rollback
must be a new forward transaction; moving only the pointer would split visible
source from authoritative state and effects.

## Daemon use

```sh
sos-revision-supervisor serve \
  --root /var/lib/sos/revisions \
  --host-executable /usr/libexec/sos-experience-host \
  --service-socket /run/sos/provider-state.sock

sos-revision-supervisor activate \
  --root /var/lib/sos/revisions \
  --revision <revision-sha256> \
  --transaction <stable-transaction-id>
```

## Evidence and remaining gate

Ten coordinator integration tests cover normal activation, immutable binding
mismatch, Luau candidate rejection, failures before and during service commit,
supervisor faults after each journal phase, committed-revision host restart, and
the external daemon. Together with the twelve base supervisor tests, all 22 pass
on Linux.

No hardware gate is claimed. The next evidence must bind `presented` to a real
GPUI/compositor frame on the phone, prove input quiescing, repeat rejected and
back-to-back Luau revisions in one stable PID, force a permanent-host restart,
and confirm state/effect recovery.
