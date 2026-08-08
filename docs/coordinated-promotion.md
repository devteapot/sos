# Coordinated revision and state promotion journal

Date: 2026-08-08

This slice joins the standalone revision supervisor in
[`revision-supervisor.md`](revision-supervisor.md) to the durable provider/state
authority in [`provider-state-service.md`](provider-state-service.md). It solves
the cross-process crash-recovery problem without pretending that two filesystem
objects and a service can be changed by one kernel-atomic operation.

## Commit decision and ordering

The durable service transaction is the commit decision. `current` never points
at a candidate whose service transaction is merely staged.

1. Verify that the service's current revision ID equals the supervisor's
   `current` revision.
2. Load the staged transaction and verify its revision ID, source SHA-256,
   schema, and complete JSON state against the immutable revision directory.
3. Durably write `promotion-journal.json` in phase `intent`.
4. Launch the candidate while the accepted process remains alive; wait for its
   first-frame event.
5. Promote the service transaction. Ambiguous during/after responses are
   reconciled by the stable transaction ID.
6. Durably advance the journal to `service_committed`.
7. Recheck that the prepared child is alive, atomically swap `current`, and
   retire the old child.
8. Durably advance the journal to `pointer_committed`, then remove and `fsync`
   the journal directory entry.

The service can therefore be briefly newer than the visible accepted revision,
but the visible revision is never newer than durable state/effects. A future
compositor should pause old-revision input across steps 5–7; the Linux prototype
serializes promotion control but cannot own application input routing.

## Recovery matrix

On startup the supervisor boots the revision named by the atomic pointer, then
reads the journal and asks the service for the stable transaction. Asking also
causes the service to finish a durable `committing` outbox left by its own crash.

| Service transaction | Pointer | Recovery decision |
| --- | --- | --- |
| `staged` or `aborted` | previous | Abort if needed, keep previous, clear journal |
| `staged` or `aborted` | candidate | Relaunch/switch previous, abort if needed, clear journal |
| `committed` | previous | Launch candidate to first frame, switch pointer, clear journal |
| `committed` | candidate | Keep candidate, clear journal |

An unknown transaction, unavailable service, invalid journal, or mismatch
between the transaction and immutable revision is not guessed through. The
journal remains for explicit recovery after the dependency or data is repaired.

## Candidate lifecycle race

Candidate readiness and pointer commitment are separate moments. A candidate
can render its first frame and then die while the service is committing. The
supervisor now checks the prepared child immediately before pointer replacement.
If it is already dead, `current` remains unchanged. Because the service decision
and journal are durable by then, recovery launches a fresh process from the same
immutable candidate before switching.

After coordinated promotion, an accepted-process crash relaunches that same
committed immutable revision rather than applying the standalone supervisor's
previous-revision rollback policy. Rolling back only the pointer would split it
from the already committed service authority. A future semantic rollback must
be a new forward service transaction targeting the previous revision, or an
independent fixed recovery experience that does not claim the old state is
current.

## Daemon use

Standalone behavior remains available when the supervisor is started without a
service socket. Coordinated mode is enabled explicitly:

```sh
sos-revision-supervisor serve \
  --root /var/lib/sos/revisions \
  --service-socket /run/sos/provider-state.sock

sos-revision-supervisor promote \
  --root /var/lib/sos/revisions \
  --revision <revision-sha256> \
  --transaction <stable-transaction-id>
```

The same control socket and process monitor are used in both modes. Coordinated
mode rejects a promotion without a transaction ID; standalone mode rejects an
unexpected transaction ID.

## Evidence

Ten coordinator integration tests use real candidate child binaries and a real
Unix-socket service. They cover normal coordinated promotion, mismatched
immutable state, candidate crash before first frame, service failure before its
commit, ambiguous service failure during commit, supervisor crash after journal
intent, supervisor crash after service commit, supervisor crash after pointer
commit, committed-revision relaunch after an accepted crash, and the external
long-lived daemon control path. The final workspace run completed this suite in
9.03 seconds.

The existing supervisor suite now has ten tests and adds the first-frame-to-
pointer death race. It passed in 26.26 seconds; the increased time comes mostly
from repeatedly hashing the larger debug candidate binary, not promotion
latency. These are Linux process/filesystem tests, not surface or latency
measurements. The complete workspace passes 53 tests.

No additional phone run was performed for this slice. The preceding device test
already proved the state daemon's ARM64 binary, abstract Unix IPC, durable middle
phase, and restart behavior. The new code coordinates Linux child processes and
filesystem pointers; meaningful phone evidence now requires the AOSP-owned
process/surface location rather than another disconnected Android shell probe.

## Remaining boundary

- Journal files are durable but unsigned; the same trust boundary as the
  revision store applies.
- Service peer credentials and administrative fault controls are not protected.
- The supervisor owns direct children, not process groups/cgroups or descendant
  cleanup.
- The service event log needs compaction/snapshots.
- Compositor surface promotion and input quiescing remain AOSP work.
- A recovery UI/multi-level fallback is still required when both candidate and
  previous revision cannot reach first frame.
