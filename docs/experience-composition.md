# Experience derivation and composition

Date: 2026-08-26

Status: implementation in progress. Package format v4, Experience API v4, the
registry and graph resolver, isolated graph runtime, authority-owned
appearance, graph state and activation transactions, derivation and
composition authoring, and the Linux host path are implemented. Stock and
Timeflow are v4 packages, and a Linux migration imports legacy Stock state
without changing the legacy pointer during the rollback window. API v3 is now
a legacy activation reader, not the target for checked-in experiences or new
authoring. Tracked updates activate every affected top-level graph atomically.
Physical Linux acceptance, complete Android host integration, and final
compatibility removal remain open.

## Decision

SOS supports two ways to combine experiences.

1. **Derivation** creates a new, self-contained experience from one or more
   exact source revisions and a user request. A one-parent derivation is a
   fork. A multi-parent derivation is a remix.
2. **Live composition** mounts a declared export from one independently owned
   experience inside another through a host-enforced contract. Parent and
   child retain separate code, state, grants, activation, and failure
   lifecycles.

These operations solve different problems. A remix is appropriate when the
result needs one information architecture, shared state transitions, or a new
cross-provider interaction. A live mount is appropriate when the child should
retain its identity, state, behavior, or independent update path.

Opening two independent windows is coexistence. Packaging helper functions in
a revision-local Luau module is code reuse. Neither operation is experience
composition.

## Terms and identities

| Term | Meaning |
| --- | --- |
| Experience ID | Stable identity that groups revision history, durable UI state, published exports, and reviewed grant decisions. Possessing the ID does not grant authority. |
| Revision ID | Immutable identity for exact source, modules, assets, state, schemas, dependencies, state-migration record, and provenance. |
| Instance ID | Opaque runtime identity for one mounted or top-level execution. It never grants authority. |
| Export ID | Stable name for one child entry point, such as `main` or `summary`, within a versioned experience contract. |
| Dependency alias | Revision-local name by which parent source refers to one resolved child export. |
| Contract digest | Content identity of the child's exported schemas and presentation requirements. |

The registry, supervisor, provider broker, and compositor may use the stable
identity internally. Luau receives only revision-bound dependency aliases and
host-issued opaque selections required for typed actions. It never discovers
or opens arbitrary installed experiences.

## Derivation

Derivation is an authoring operation, not a runtime import mechanism:

```text
Experience A revision 4 ─┐
                         ├── user request + agent synthesis ──> Experience C revision 1
Experience B revision 9 ─┘
```

The authoring service reads only the exact parent revisions selected by the
user or calling trusted interface. It produces a complete candidate package.
The normal compiler, runtime decoder, scenario validator, capability checker,
state coordinator, presenter, and revision supervisor then accept or reject
that candidate.

The resulting experience has these properties:

- its source, modules, assets, state schema, provider bindings, and validation
  scenarios are complete without either parent at runtime;
- its derivation record names every exact parent revision and preserves the
  originating request and agent rationale;
- it receives its own provider grants instead of inheriting the union of its
  parents' grants;
- it begins with new UI state unless the package contains an explicit,
  validated migration from selected parent state;
- it does not change when a parent later activates another revision; and
- another refresh from changed parents produces a new candidate and passes the
  same validation and activation transaction.

Durable domain data should remain provider-owned. Calendar events, notes,
messages, and media state therefore do not need to be copied from parent UI
state into a remix. A migration may carry deliberate user-interface state,
such as a chosen layout or pinned filter, but it must not merge arbitrary JSON
tables or transfer provider-scoped opaque IDs.

Every derived or composed authoring request selects either fresh state or one
exact current-target/selected-parent revision. Revision-backed migration reads
the authority's retained state for that exact revision. The immutable package
records the source Experience ID, Revision ID, schema version, source-state
digest, target schema version, and migrated-state digest; installation rejects
a package whose declared result differs from its durable state.

Revision history and derivation lineage are related but different:

```text
A:r4 + B:r9 → C:r1       derivation lineage
C:r1 → C:r2 → C:r3       later revisions of C
```

Revision-manifest format v4 hashes an immutable derivation record
alongside the executable package. Changing parent IDs, dependency bindings,
the originating request, or rationale therefore creates a different revision
identity even when emitted Luau bytes happen to match.

## Live composition

Live composition uses a host-owned mount between separate experience
executions:

```text
Parent VM                                  Child VM
    │                                          ▲
    │ dependency alias + bounded properties   │ bounded viewport + system context
    ▼                                          │
             host-owned experience mount
    ▲                                          │
    │ declared child events                    │ retained child scene
    │                                          ▼
parent update                           child update and state
```

The parent chooses where the mount participates in its layout. The host
measures that slot, gives the child the resulting logical viewport, clips child
rendering to the slot, translates input coordinates, and composites the child
output. The parent never receives or mutates the child's scene tree. The child
never receives the parent scene, parent state, native surface identity, or
unbounded drawing authority.

The contract does not require a particular process or graphics
implementation. A prototype may host isolated VMs in one permanent process.
A stronger deployment may use separate workers or compositor-managed buffers.
Both must preserve the same identity, state, authority, input, accessibility,
failure, and activation rules.

### Published exports

An experience may publish zero or more entry points. Exports are authored
views, not members of a host-defined widget catalog. `main`, `summary`, or any
other name has meaning only within that experience's contract.

Package format v4 carries a host-validated contract equivalent to:

```text
contract_version: 1
exports:
  - id: summary
    properties: <closed bounded schema>
    events: <closed bounded schema>
    min_width: 240
    min_height: 160
    max_width: 1920
    max_height: 1080
    appearance_abi: 1
    accepts_container_appearance: false
```

Property and event schemas admit only bounded JSON-like values. They contain no
code, callbacks, provider handles, credentials, filesystem paths, native
objects, or unscoped opaque selections. The host caps schema depth, field
count, encoded value size, exports per revision, and events per update. The
implementation fixes those limits at 16 exports and dependencies, 64 schema
fields or events, schema depth 8, 256 list items, and 16 KiB per serialized
boundary value. A resolved graph is limited to depth 4 and 8 runtime
instances.

Every export renders and handles events through the same sandboxed experience
runtime as a top-level scene. Publishing an export does not add provider
authority. The provider broker still resolves the child revision's own grants
for every resource and action.

### Resolved dependencies

The composing revision binds each child through immutable package metadata
equivalent to:

```text
dependencies:
  - alias: agenda
    experience_id: <stable identity>
    revision_id: <exact immutable revision>
    export_id: summary
    contract_digest: <exact digest>
    policy: locked
    grant:
      properties: [title]
      events: [open]
```

Parent Luau refers only to `agenda`. The package resolver verifies the child
identity, revision, export, contract digest, provenance, and installation
policy before candidate evaluation. A missing or mismatched dependency rejects
the candidate. The Luau sandbox does not gain package discovery or a
cross-revision `require` path.

The `locked` policy pins an exact child revision. The `tracked` policy follows
a stable experience identity only during an explicit tracked refresh. A tracked
update must keep a compatible export contract and pass preparation and
validation for every affected composition before activation. Matching a schema
alone is insufficient because the child may change geometry, semantics, input,
or visible behavior.

### Scene and event shape

Experience API v4 adds this content kind:

```luau
type ExperienceMountContent = {
    kind: "experience_mount",
    dependency: string,
    properties: JsonValue,
    container_appearance: ContainerAppearance?,
}
```

The node's ordinary `layout` determines its viewport. Its stable node ID
namespaces the mounted instance within the parent. Child output reaches the
parent as an API v4 `ExperienceEvent` carrying the dependency alias, declared
event name, and schema-validated payload. The host never converts child action
strings into parent actions or provider effects.

All values crossing the boundary count as an explicit data flow between two
experience identities. The composition package must declare that flow, and
the authority must allow it. This prevents a parent with access to one private
provider from using a child's unrelated action grant as a confused deputy.
Provider-scoped opaque IDs cannot cross as ordinary strings. Provider and
cross-experience data-flow reviews are versioned authority resources keyed by
stable Experience ID. A later revision may use only the intersection of its
package-declared requests and that stable reviewed grant. Forks and remixes
create new Experience IDs by default and therefore do not inherit parent
grants. A later brokered selection-transfer contract would need to revalidate
both identities and the fresh provider snapshot.

### Runtime containment

For every mount, the host must enforce these rules:

- child layout and paint are clipped to the measured viewport;
- pointer coordinates are translated into child-local logical coordinates;
- keyboard, focus, text composition, pointer capture, and accessibility focus
  have one host-resolved owner at a time;
- child node, semantic, action, state, and asset names remain in the child's
  namespace;
- the accessibility bridge attaches the child's semantic root beneath the
  mount without exposing mutable child nodes to the parent;
- a child failure produces a bounded unavailable result while the parent and
  sibling mounts remain usable;
- child provider effects use only the child's current grants;
- shell-only content such as `window_space` and `shell_overlay` remains
  unavailable to ordinary mounted experiences; and
- the complete dependency graph is acyclic and bounded in depth, total VMs,
  retained nodes, assets, memory, render time, and event traffic.

Nested composition is allowed only within those graph limits. A dependency
cycle rejects the candidate before presentation.

## Appearance and styles

Appearance crosses experience boundaries as typed data. Styles remain
revision-local code.

The system authority publishes one versioned appearance snapshot to the
shell, top-level experiences, and mounted children. It contains accessibility
preferences and semantic tokens such as background, surface, text, accent,
spacing, radius, and type roles. It does not contain executable style rules or
revision-owned asset references.

Only a caller holding the separately provisioned `appearance-write`
capability may advance that snapshot. The authority persists only the
capability digest, rejects missing or mismatched credentials, and still
requires the next exact generation. Read access to appearance and ordinary
experience-state grants do not imply write access.

The parent is not the source of global appearance. It may offer a bounded
container appearance override only when the child export declares
`accepts_container_appearance`. Resolution follows this order:

```text
system accessibility requirements
    ↓
system design profile
    ↓
accepted container override
    ↓
child-local choices
```

An experience may ignore optional stock design tokens and implement a custom
visual system. It may not bypass system-enforced accessibility or trusted
ceremony rules. Fixed credentials, permission, confirmation, lock, and
Recovery interfaces consume only trusted system appearance values, never
untrusted experience assets or style code.

Revision-local theme and style modules remain content-addressed with their
owning experience. They may turn the appearance snapshot into ordinary Scene
ABI nodes, but they do not become a global host module or a fixed component
catalog.

A declarative contribution follows a simpler rule. The receiving experience
renders the data in its own style. A status contribution may publish a label,
value, and typed selection, but it cannot inject child code, paint, or a
callback into the shell.

## Authoring and activation

The resident authoring surface emits only API v4 packages for active edits,
derivations, and live compositions. API v3 remains readable only for bounded
rollback of retained artifacts. None of the authoring flows gives the model
filesystem, registry, activation, or grant-review authority.

The trusted authoring broker must:

1. resolve an explicit bounded list of user-selected parent or dependency
   revisions;
2. return their complete inspectable source packages and contracts to the
   authoring model;
3. bind validation to the exact candidate source, modules, assets, state
   schema and migration record, provider requests, derivation parents,
   dependencies, and contracts;
4. validate every exported entry point at representative bounded viewports and
   supported appearance states;
5. install and return the exact resolved graph that passed validation without
   advancing an existing target's registry pointer; and
6. let the graph supervisor prepare, present, commit, and finalize that graph
   while the accepted composition stays active.

Forking or remixing produces a new experience identity unless the user
explicitly chooses to replace an existing target. Editing a mounted child
activates a revision in that child's slot. Editing the parent does not replace
the child. A global agent entry point therefore needs an explicit authoring
target such as the shell, the focused experience, a selected mounted child, or
a new remix.

Ordinary child revision updates commit independently under `locked`
dependencies because the parent remains pinned. A tracked update is a
coordinated graph activation and must retain the old accepted graph until the
new child and every affected parent have presented successfully.

## Current implementation boundary

The implementation is split so the contracts do not depend on GPUI or Linux:

- `experience-package` owns canonical identities, closed schemas, contracts,
  dependency grants and policies, derivation records, appearance data, graph
  limits, canonical JSON, and content digests;
- revision manifest v4 hashes `package.json`, while the experience registry
  keeps stable per-experience current and previous pointers;
- the graph resolver rejects missing exports, changed contract digests,
  undeclared grants, shell children, cycles, excessive depth, and excessive VM
  counts, then stores the exact canonical graph snapshot;
- `GraphRuntime` owns one sandboxed Luau VM and state namespace per graph node,
  validates mount values and child events, shares state across repeated
  instances of one stable experience, contains child failure, and rolls back a
  partially failed event cascade;
- provider/state protocol v2 commits all changed experience states as one
  durable graph transaction and retains revision-specific state for locked
  graphs after a newer child becomes current;
- the authority keeps separately capability-protected, versioned appearance
  and stable-Experience grant resources; the Linux graph host intersects each
  revision's declared provider requests with that Experience's reviewed grant;
- the Linux graph host prepares off the GPUI thread, while the graph supervisor
  presents and confirms the complete graph, advances registry and graph
  pointers under a durable journal, then explicitly finalizes the host switch.
  A normal failure before durable graph commit rolls back the presented graph;
  crash recovery selects the journaled side of the commit;
- the Linux host namespaces nodes, text state, assets, provider surfaces, input,
  accessibility, and provider subscriptions by graph owner; and
- the trusted authoring broker exposes bounded context, validate, and submit
  flows for self-contained fork/remix candidates and live compositions.

The checked-in Agenda, Media, Dashboard, and Agenda-Media Remix sources are a
reference package set. `sos-revision-supervisor install-composition-demo
--root DIR` installs their revisions, registry records, resolved Dashboard
graph, and lineage metadata.

The shared package, resolver, runtime, and authority code is platform-neutral.
The Android GPUI host still boots the legacy single-revision authority path and
does not yet consume a resolved graph. No Android composition claim is made.

## Rejected shortcuts

- **Raw scene injection.** A parent must not receive child nodes, action
  strings, callbacks, or mutable state. That collapses validation, authority,
  focus, accessibility, and failure ownership.
- **Cross-revision source loading.** Runtime `require` stays revision-local.
  Sharing executable code across a live trust boundary would make activation
  and rollback nondeterministic.
- **Permission union.** A remix receives new grants. A mounted child retains
  its own grants. Composition never combines authority implicitly.
- **Schema-only tracked updates.** Matching properties and events does not
  prove that changed geometry, semantics, or behavior fits an accepted parent.
- **Windows as the only composition mechanism.** Native windows provide
  coexistence and lifecycle isolation, but cannot express an experience
  embedded in a parent layout.
- **Remix as the only composition mechanism.** Flattening every combination
  destroys independent identity, state, updates, and failure isolation when
  the user wanted to preserve them.
- **A global style engine.** Shared appearance is data. Styles stay replaceable
  experience code.

## Acceptance status

Desktop tests and the explicit Linux VM campaign complete the non-physical
parts of the first gate:

1. Agenda and Media publish independent `main` and `summary` exports.
2. Dashboard mounts both summaries through exact locked aliases.
3. Separate VM/state ownership, grant enforcement, child-event routing,
   shell-content containment, failure rollback, and host activation recovery
   are covered by unit and integration tests.
4. An authority appearance generation rerenders the inheriting Agenda while
   the custom Media result remains unchanged, without revision activation.
5. Locked resolution stays on the old Agenda after its registry pointer moves.
6. Explicit tracked refresh resolves the compatible new Agenda, presents the
   complete graph, finalizes it, and boots the same graph after restart.
7. The trusted derivation flow creates a self-contained Agenda-Media remix with
   exact parent lineage, new identity, no dependencies, and no inherited grant.
8. Graph pointers, revision-specific states, appearance, and provenance all
   survive authority or supervisor reopen tests.

`tools/linux-compositor/verify-composition-nested` additionally installed the
reference packages into a disposable Debian 13 graph store and presented graph
`f09068511e1c9d2c160fcc55583e9d347024fbf4a6ca2fa53ff2492a983ab287`
through the real Linux host and Smithay nested compositor. The accepted run
proved the Dashboard root and both mounted child semantics, namespaced Agenda
input and `agenda.open` event routing, appearance generation 1 in the
inheriting child, the custom Media result, unchanged host PID 13140 across
activation, host recovery in PID 13305, and compositor-owned
`nested_backend_submit` evidence. It completed in 1.544 seconds.

The broader nested gate then passed pointer/text and accessibility routing,
clipping, conditional shell auxiliary-window lifecycle, compatibility-window
coexistence, exact presentation fences, and host recovery. The direct-DRM VM
gate passed the corresponding page-flip boundary, and the cold-boot gate passed
resident-agent authoring, VT pause/resume, `s2idle` freezer recovery, output
hotplug, separated identities, process recovery, and reboot restoration.

The final host and compositor binaries have since run on the Framework 12. A
2026-08-27 development-live diagnostic proved the recovery view and two
single-revision shells through physical DRM page flips, one permanent host
launch, resident authoring, durable authority agreement, and clean return to
GDM. It did not boot the reference graph: supervisor status recorded
`active_graph: null` before and after the authoring transaction. Its pointer and
touch observations also came from hot-added uinput devices, so the corrected
auditor rejects the physical-input claim.

Physical live-composition acceptance therefore remains open. It must boot the
Dashboard graph on the Framework, observe both independently owned mounted
children, dispatch a namespaced child event, apply appearance data, and retain
the graph across a physical host restart without synthetic input standing in
for integrated hardware. Panel latency, suspend, GPU recovery, thermals, and
power evidence remain separate gates. Android graph loading, rendering,
authoring, and physical-device recovery remain a separate platform milestone.
