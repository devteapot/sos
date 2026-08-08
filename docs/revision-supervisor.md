# Standalone revision supervisor prototype

Date: 2026-08-08

This is the first Linux implementation of the fixed supervisor described in
[`android-exit-verdict.md`](android-exit-verdict.md). It is deliberately outside
the Android application and every experience process. The prototype establishes
the process, revision, and promotion contract that a later privileged AOSP
service can adopt; it does not claim the AOSP surface or input gates are complete.

## Owned invariants

The `revision-supervisor` crate owns these operations:

- install a content-addressed revision containing source, durable state, schema,
  executable, arguments, and a manifest;
- verify every file's byte length and SHA-256 before launch or pointer change;
- verify that `state.json` names the manifest schema and exact source SHA-256;
- launch a candidate as a direct child and provide a per-launch Unix-socket
  readiness channel;
- promote only after an authenticated, revision-specific `first_frame` event;
- observe direct-child exit independently of the experience;
- atomically replace the relative `current` symlink;
- on post-promotion exit, restore the preceding pointer and launch that
  preceding immutable revision again.

The on-disk shape is:

```text
ROOT/
  current -> revisions/<revision-id>
  revisions/<revision-id>/
    manifest.json
    source.luau
    state.json
    experience
  run/
    supervisor.sock
```

`revision-id` is a SHA-256 over the format version, schema, the three payload
identities, and the executable arguments. Installation writes and `fsync`s a
private staging directory, changes payloads to read-only and the executable to
read/execute-only, then renames the complete directory into `revisions/` and
`fsync`s its parent. The prototype calls these directories immutable because
there is no update API and normal permissions reject writes. It does not yet
use a read-only mount, fs-verity, or Linux immutable attributes against a
privileged attacker.

`current` is replaced by creating a new relative symlink, renaming it over the
old name, and `fsync`ing the root directory. Concurrent readers therefore see
the complete old or complete new target.

## Candidate process ABI

The supervisor launches `experience` with the manifest arguments, its revision
directory as the working directory, and three environment variables:

| Variable | Meaning |
| --- | --- |
| `SOS_REVISION_ID` | Exact immutable revision being launched |
| `SOS_SUPERVISOR_SOCKET` | Per-launch Unix readiness socket |
| `SOS_SUPERVISOR_TOKEN` | Per-launch token required in the event |

After it has rendered its first frame, the candidate connects to that socket
and writes one newline-delimited JSON event:

```json
{"event":"first_frame","token":"<launch token>","revision_id":"<revision id>"}
```

Exit before that event or expiry of the configured timeout kills the candidate
and leaves both the old child and `current` intact. After the event, the
supervisor swaps `current`, retires the preceding child, and monitors the new
one. Any subsequent exit is currently classified as a crash: the supervisor
restores the preceding pointer and waits for a first frame from a fresh launch
of the preceding revision.

The long-lived daemon accepts typed newline-delimited JSON on
`run/supervisor.sock`: `promote`, `status`, and `shutdown`. The CLI is a client
for that socket. A minimal local exercise is:

```sh
cargo build -p revision-supervisor --bins
SUPERVISOR=target/debug/sos-revision-supervisor
ROOT=/tmp/sos-revisions

$SUPERVISOR install --root "$ROOT" --source experience.luau \
  --state state.json --schema 1 --executable ./experience
$SUPERVISOR bootstrap --root "$ROOT" --revision <first-revision-id>
$SUPERVISOR serve --root "$ROOT"
$SUPERVISOR promote --root "$ROOT" --revision <candidate-revision-id>
```

`bootstrap` refuses an initialized store. All later pointer movement belongs to
the running supervisor.

## Linux evidence

`cargo test -p revision-supervisor --all-targets` passes nine tests. They use a
real copied Rust candidate executable, real Unix sockets, and OS child exit
statuses. Covered cases are content identity and read-only modes; rejection of
state/source/schema drift and recomputation of the content-addressed directory
identity; 50 atomic pointer alternations with a concurrent reader; pre-frame
exit; first-frame timeout; relaunch of a crashing boot revision; post-frame exit
with pointer rollback and predecessor relaunch; and a separate daemon process
that survives the accepted candidate's crash and answers another control request
afterward.

The final nine-test suite took 17.96 seconds. This is desktop
process/filesystem evidence, not an Android hardware, surface, boot, or latency
result.

## Remaining boundaries

- The manifest is content-addressed but not signed. Signature policy belongs to
  the native revision format/security work.
- Direct children are owned, but descendants are not yet placed in a cgroup or
  killed as a process group. Resource, syscall, namespace, and capability limits
  are not implemented.
- `first_frame` proves the candidate contract event, not compositor presentation.
  A privileged surface owner must bind the event to an actual staged surface.
- State is consistent inside the immutable revision, but provider effects and
  durable service state do not yet share this supervisor's commit record. The
  next provider protocol should stage a transaction, let this supervisor make
  the sole promotion decision, and reconcile before/during/after-promotion
  failures by transaction/revision ID.
- If the restored predecessor cannot reach its first frame, the daemon reports
  the recovery failure and remains available, but it has no multi-level
  recovery/recovery-UI policy yet.
