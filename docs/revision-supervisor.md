# Stable-host revision supervisor

Date: 2026-08-08

This is the Linux prototype of SOS revision activation after removing native
experience binaries from the experience contract. Generated revisions are Luau
bundles consumed by one permanent Rust/GPUI host. Updating that host is a
separate system-software operation, not an experience mutation tier.

## Owned invariants

The `revision-supervisor` crate now owns these operations:

- install a content-addressed revision containing Luau source, durable state,
  state schema, required experience-API version, and bounded sidecar assets;
- verify every file's byte length and SHA-256 before activation;
- keep the active host process independent from revision contents;
- ask that stable host to prepare a candidate while the accepted scene remains
  active;
- activate only after the host reports that the candidate was presented;
- atomically replace the relative `current` symlink;
- restart the permanent host on its committed current revision if the host
  process exits.

The on-disk shape is:

```text
ROOT/
  current -> revisions/<revision-id>
  revisions/<revision-id>/
    manifest.json
    source.luau
    state.json
    assets/
      <sha256>.svg|png|jpg|webp|font|wgsl
  run/
    supervisor.sock
```

There is deliberately no `experience` executable or per-revision argument list.
Format version 3 hashes the state schema, experience-API version, source,
state, and sorted asset ID/kind/file identities. Installation writes and `fsync`s a private staging
directory, changes its files to read-only, renames it into `revisions/`, and
`fsync`s the parent. `current` is replaced with an atomic relative-symlink
rename followed by a directory `fsync`.

Scene ABI v3 revisions may retain small SVG declarations inside `source.luau`
or use individually hashed `svg`, `png`, `jpeg`, `webp`, `font`, and WGSL
`shader` sidecars. Installation enforces 64 assets, 4 MiB per asset, and 16 MiB
total; rejects active/external SVG content and malformed file signatures; and
makes the files and asset directory read-only. The runtime re-verifies the
manifest hashes and formats before a fresh candidate VM can reference an image
or load a font. Shader bytes do not acquire GPU authority until the permanent
host exposes a separate safe paint operation.

## Permanent host protocol

The supervisor is configured once with `--host-executable`; this executable is
not copied into or identified by an experience revision. It remains alive while
many revisions are activated. The current Linux adapter uses newline-delimited
JSON over the child's stdin/stdout. The wire types live in the small
platform-neutral `experience-host-protocol` crate so host adapters do not depend
on the supervisor implementation:

| Supervisor request | Required host behavior |
| --- | --- |
| `boot` | Load the committed revision and report `presented` |
| `prepare` | Create a fresh VM, migrate state, validate capabilities, and prepare a retained scene without replacing the active scene |
| `present` | Switch to the prepared scene at a frame boundary and report `presented` |
| `confirm` | Prove the host event loop is still alive after presentation and before pointer commit |
| `discard` | Destroy an unaccepted candidate VM/scene |
| `shutdown` | Terminate the permanent host cleanly |

Every request and response carries a request ID; revision operations also carry
the content-addressed revision ID. `boot` and `prepare` include the verified,
read-only revision directory and required experience-API version. A production
AOSP integration may replace pipes with a privileged IPC transport, but must
preserve these semantics.

Candidate syntax, migration, capability, timeout, and render-preparation errors
therefore reject a Luau revision without launching a new native process or
surface. A successful activation keeps the same host PID. A host crash is a
permanent-layer failure: the supervisor restarts the host on `current`; it does
not roll back durable provider state by moving only the source pointer.

## CLI

```sh
cargo build -p revision-supervisor --bins
SUPERVISOR=target/debug/sos-revision-supervisor
ROOT=/tmp/sos-revisions

$SUPERVISOR install --root "$ROOT" --source experience.luau \
  --state state.json --schema 1 --api 3 \
  --asset hero:png:hero.png --asset display:font:Display.otf
$SUPERVISOR bootstrap --root "$ROOT" --revision <initial-revision-id>
$SUPERVISOR serve --root "$ROOT" --host-executable /usr/libexec/sos-experience-host
$SUPERVISOR activate --root "$ROOT" --revision <candidate-revision-id>
```

`bootstrap` refuses an initialized store. All later pointer movement belongs to
the daemon. Coordinated mode additionally requires a stable state transaction
ID; see [`coordinated-activation.md`](coordinated-activation.md).

## Evidence

`cargo test -p revision-supervisor --all-targets` runs the supervisor and
coordinator integration suites. Supervisor tests cover executable-free revision identity, API-version
binding, read-only storage, concurrent atomic pointer reads, candidate
rejection, preparation timeout, failures before and immediately after
presentation, same-PID activation, host restart on the committed revision, and
the external daemon. Ten
coordinator tests retain state/source/schema binding and crash-journal cases.

This is Linux process/filesystem evidence. The corresponding Android harness
uses one GPUI process and in-process worker activation. Its physical-device
activation, rejection, recovery, platform regression, typed-effect, and
10,000-swap measurements are recorded in
[`stable-host-device-gate.md`](stable-host-device-gate.md). The AOSP adapter
still needs to join that real GPUI host to this external supervisor protocol.

## Remaining boundaries

- The current external host binary used by integration tests is a protocol
  probe. The AOSP GPUI shell still needs to implement this transport around its
  real Luau worker and compositor frame callback.
- The manifest and journal remain unsigned.
- The Android harness consumes the same runtime asset set, but the AOSP adapter
  must still carry the supervisor-provided revision directory through its
  production IPC instead of the current in-process activation harness.
- Host process descendants are not yet in a cgroup or capability sandbox.
- An actual compositor-present fence must replace the prototype host's
  `presented` assertion.
- The recovery interface and A/B permanent-host update mechanism remain to be
  built.
- Real-data isolation requires moving the Luau VM behind a constrained worker
  process or equivalently strong boundary; the current in-process Android VM is
  not a production trust boundary.
