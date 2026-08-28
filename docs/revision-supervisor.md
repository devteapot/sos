# Stable-host revision supervisor

Date: 2026-08-08 (updated 2026-08-28)

This is the Linux prototype of SOS revision activation after removing native
experience binaries from the experience contract. Generated revisions are Luau
bundles consumed by one permanent Rust/GPUI host. Updating that host is a
separate system-software operation, not an experience mutation tier.

## Owned invariants

The `revision-supervisor` crate now owns these operations:

- install a content-addressed revision containing Luau source, durable state,
  state schema, required experience-API version, and bounded sidecar assets;
- verify every file's byte length and SHA-256 before activation;
- keep Experience histories and current/previous pointers in the registry;
- resolve immutable, content-addressed graphs before runtime preparation;
- ask the stable host to prepare every graph instance while accepted graphs
  remain active;
- activate registry pointers, graph pointers, and authority state as one
  journaled transaction after presentation; and
- restart hosts on committed graphs if a host process exits.

The on-disk shape is:

```text
ROOT/
  revisions/<revision-id>/
    manifest.json
    manifest.hmac-sha256  # optional development integrity mode
    package.json
    source.luau
    state.json
    assets/
      <sha256>.svg|png|jpg|webp|font|wgsl
  experience-registry.json
  graphs/<graph-id>.json
  graph-pointers/<experience-id>.json
  reverse-dependencies.json
  graph-activation-journal.json  # present only during recovery
  run/
    supervisor.sock
```

There is deliberately no `experience` executable or per-revision argument list.
Format version 4 hashes the complete immutable package, state schema,
Experience API version, source, state, and sorted asset ID/kind/file
identities. Installation writes and `fsync`s a private staging directory,
changes its files to read-only, renames it into `revisions/`, and `fsync`s the
parent. Registry and graph pointer files use atomic rename plus directory
`fsync`; the activation journal binds every pointer transition.

When `SOS_REVISION_SIGNING_KEY_FILE` is provisioned, installation writes a
detached HMAC-SHA-256 over the exact manifest bytes. When
`SOS_REVISION_VERIFY_KEY_FILE` is provisioned, every verification requires that
file and checks it in constant time before parsing the manifest. This is a
useful keyed-integrity mode for controlled deployments, but it is optional and
symmetric. It is not an asymmetric release signature or a system-owned stock
recovery pin.

Experience API v4 revisions may retain small SVG declarations inside
`source.luau` or use individually hashed `svg`, `png`, `jpeg`, `webp`, `font`, and WGSL
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
| `boot_graph` | Load the committed resolved graph and report composed presentation evidence |
| `prepare_graph` | Create fresh per-instance VMs, migrate state, validate contracts and capabilities, and prepare a composed scene without replacing the active graph |
| `present_graph` | Switch every prepared root at a frame boundary and report composed presentation evidence |
| `confirm_graph` | Prove the host event loop and exact graph remain alive before pointer commit |
| `discard_graph` | Destroy every unaccepted candidate VM/scene |
| `shutdown` | Terminate the permanent host cleanly |

Every request and response carries a request ID; graph operations also carry
the content-addressed graph ID and verified revision root. A production
AOSP integration may replace pipes with a privileged IPC transport, but must
preserve these semantics.

Candidate syntax, migration, capability, timeout, and render-preparation errors
therefore reject a Luau revision without launching a new native process or
surface. A successful activation keeps the same host PID. A host crash is a
permanent-layer failure: the supervisor restarts the host on the committed
graph; it does not roll back durable provider state by moving only one
revision pointer.

## CLI

```sh
cargo build -p revision-supervisor --bins
SUPERVISOR=target/debug/sos-revision-supervisor
ROOT=/tmp/sos-revisions

$SUPERVISOR install-package --root "$ROOT" --source experience.luau \
  --state state.json --schema 1 --package package.json \
  --asset theme:luau:theme.luau
$SUPERVISOR bootstrap-graph --root "$ROOT" \
  --experience <experience-id> --revision <initial-revision-id>
$SUPERVISOR serve --root "$ROOT" --host-executable /usr/libexec/sos-experience-host \
  --root-experience <experience-id>
$SUPERVISOR activate-graph --root "$ROOT" --graph <candidate-graph-id>
```

Use the `install-package` command shown above. Bare `install` authoring is
disabled. `bootstrap-graph` creates the initial registry and graph pointers;
all later pointer movement belongs to the graph daemon. A revision without a
format-v4 package is rejected before resolution.

## Evidence

`cargo test -p revision-supervisor --all-targets` runs registry, resolver,
graph-supervisor, reverse-index, and fault-injection suites. The tests cover
executable-free revision identity, API-version binding, read-only storage,
independent histories, contract resolution, candidate rejection, preparation
timeout, every activation-journal phase, multi-root atomicity, same-PID
activation, host restart on committed graphs, and the external daemon. The
protocol probe remains the deterministic supervisor integration-test child,
but it is no longer the only external host. `sos-experience-host` is a real
GPUI/Wayland client that loads immutable resolved graphs, implements every
request in the table above, activates within stable host processes, and
restarts on committed graph pointers. Its scoped evidence and known presentation/input gaps
are in [`linux-stable-host.md`](linux-stable-host.md).

This is Linux process/filesystem and nested-Wayland evidence. The corresponding
Android authority uses its platform-specific bounded graph IPC with the same
package, registry, graph, state, appearance, grant, activation, and recovery
semantics. Its physical-device evidence is indexed in
[`progress.md`](progress.md).

## Remaining boundaries

- The activation journal remains unsigned. Revision manifests are only
  conditionally HMAC-authenticated; production Linux still needs mandatory
  asymmetric verification rooted outside mutable user state.
- Host process descendants are not yet in a cgroup or capability sandbox.
- Android has an AVB/OTA-protected, authority-pinned stock fallback. Linux still
  needs the equivalent immutable stock pointer and fixed recovery request;
  HMAC-enabled ordinary revisions do not supply that provenance boundary.
- The A/B permanent-host update mechanism remains to be built.
- Android VM worker-process isolation remains optional defense in depth; the
  API and authority model do not depend on the present in-process deployment.
