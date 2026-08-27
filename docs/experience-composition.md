# Experience derivation and composition

Date: 2026-08-26

Status: implementation closure and physical acceptance in progress. The Debian
direct-DRM v4 graph gate and an exact Framework development-live composition
diagnostic pass; installed Linux promotion and Android device acceptance remain
open.
Package format v4, Experience API v4, the registry and graph resolver, isolated graph runtime, authority-owned
appearance, graph state and activation transactions, derivation and
composition authoring, and the Linux and Android host paths are implemented.
Stock and the reference composition set are v4 packages. Rolling Linux and
Android migrations import
legacy Stock state without changing the legacy pointer during the rollback
window. API v3 is now a legacy activation reader, not the target for checked-in
experiences or new authoring. Tracked updates activate every affected top-level
graph atomically. Installed Linux promotion, physical Android composition
acceptance, and final compatibility removal remain open. The cross-platform
wire, deterministic property, complete durable-phase fault, and desktop
performance gates pass.

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

The shell keeps the canonical Linux semantic socket. Each independently
presented ordinary top-level Experience binds a separate bounded socket name
derived from its stable Experience ID; mounted children remain namespaced
inside their owning host's semantic tree. A second top-level host must never
reuse, replace, or disable the shell's semantic endpoint.

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

Only a caller holding the authority's `appearance-write` capability may
advance that snapshot. Linux persists only the separately provisioned
capability digest and rejects missing or mismatched credentials. Android binds
the equivalent `appearance_write` package request and reviewed decision to the
platform's stable pinned Stock identity (`sos.stock.mobile` on Android and
`sos.stock.shell` on Linux); an ordinary presented root cannot
acquire it. Both paths bind the exact current generation, and Android also
binds the exact presented graph. Android's revision port is an administrative
host boundary restricted by SELinux to the fixed Core host or the
platform-privileged SOS host; untrusted app domains cannot connect. Read access
to appearance and ordinary experience-state grants do not imply write access.

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
- retiring an ordinary Experience moves its complete registry record into the
  recoverable `retired-experiences` archive. Retired records disappear from the
  launch catalog without deleting revisions or derivation history. The pinned
  neither pinned platform Stock experience can be retired;
- fresh Linux v4 boot creates only Experience registry and graph pointers and
  bootstraps authority state per Experience. The singleton pointer exists only
  when an imported v3 rollback artifact already owns it;
- the graph resolver rejects missing exports, changed contract digests,
  undeclared grants, shell children, cycles, excessive depth, and excessive VM
  counts, then stores the exact canonical graph snapshot;
- `GraphRuntime` owns one sandboxed Luau VM and state namespace per graph node,
  validates mount values and child events, shares state across repeated
  instances of one stable experience, contains child failure, and rolls back a
  partially failed event cascade. The Linux host can run the complete graph
  runtime behind a bounded, length-framed subprocess channel. The installed
  session uses that mode, while `thread` remains an explicit deployment
  fallback. No Experience API depends on the choice;
- provider/state protocol v2 commits all changed experience states as one
  durable graph transaction and retains revision-specific state for locked
  graphs after a newer child becomes current;
- the authority keeps separately capability-protected, versioned appearance
  and stable-Experience grant resources; the Linux graph host intersects each
  revision's declared provider requests with that Experience's reviewed grant;
- Linux provider snapshots read only reviewed domains. Ungranted notes,
  calendar, media, system, legacy network, and provider surfaces enter Luau as
  empty typed values rather than failing graph startup or exposing Stock data;
- the Linux graph host prepares off the GPUI thread, while the graph supervisor
  presents and confirms the complete graph, advances registry and graph
  pointers under a durable journal, then explicitly finalizes the host switch.
  Compositor input remains quiesced after candidate presentation until that
  finalization. A normal failure before durable graph commit rearms the
  compositor for the accepted graph and completes discard only after its
  restored frame is physically presented; crash recovery selects the journaled
  side of the commit;
- one shared host transform attaches child scene roots to their declared mount,
  prefixes every scene node with the owning Instance ID, and leaves a failed
  child's mount empty for the host-owned fallback. Linux and both Android
  profiles use that exact transform instead of maintaining platform copies;
- the Linux host namespaces text state, assets, provider surfaces, input,
  accessibility, and provider subscriptions by graph owner; and
- the trusted authoring broker exposes bounded context, validate, and submit
  flows for self-contained fork/remix candidates and live compositions; and
- the Android authority and GPUI host use the same package, registry, graph,
  per-Experience state, appearance, grants, per-Instance VM and namespace,
  presentation, journal recovery, and rollback contract. Android Stock edits
  stage a complete candidate graph and move no pointer until a rendered-frame
  confirmation. The signed Android product also installs the four reference
  Experiences. Stock receives their bounded registry catalog, a
  `shell.present_experience` action stages the selected exact graph, and a
  frame confirmation durably selects that top-level Experience. Dismissal
  stages and confirms Stock through the same path. Because an independently
  presented root replaces the Stock scene, the Android host supplies a small
  fixed system-control strip: `Home` returns an ordinary root to Stock,
  `Theme` advances the Stock-authorized appearance resource, and `Rollback`
  stages the retained graph. These controls are host-owned administrative
  affordances, not nodes or authority available to ordinary Experience code.
  Fixed host IDs also publish them through the bounded accessibility bridge;
  and
- child update or render failures become a failed child Instance with no
  emitted effects or child events. The mount receives the host-owned
  unavailable placeholder while the root and siblings remain operational.

The checked-in Agenda, Media, Dashboard, and Agenda-Media Remix sources are a
reference package set. `sos-revision-supervisor install-composition-demo
--root DIR` installs their revisions, registry records, independently
launchable `main` graphs, the resolved Dashboard graph, and lineage metadata.
The reference Dashboard and Agenda also publish ordinary semantic controls for
the physical acceptance campaign: a parent liveness action, child update or
timeout failures, and a mounted Agenda text session. They travel through the
same namespaced pointer, text, IME, accessibility, and runtime paths as product
actions; no diagnostic backdoor enters the Experience API.
Android embeds the same source/package constructor in its signed authority and
installs it idempotently without resetting later registry revisions on
authority restart.

The shared package, resolver, runtime, authority, and scene-composition code is
platform-neutral. The scene regression uses duplicate raw node IDs in the
Dashboard and Agenda Instances, mounts the healthy Agenda child, withholds the
failed Media child, and requires every resulting ID to remain unique and
Instance-prefixed. Android has compile, unit, restart-fault, top-level
presentation, appearance, child-containment, and campaign-auditor evidence for
the graph path. It does not yet have a physical-device composition verdict.

`tools/a33xctl capture-v4-composition-stage` records the physical campaign in
this fixed order:

1. `stock` records the migrated v4 Stock graph.
2. `dashboard` records the three-Instance locked composition after opening the
   first Agenda item.
3. `appearance` records a live authority generation change without a revision
   change.
4. `child-failure` records the unavailable Agenda mount while Dashboard and
   Media remain live.
5. `child-timeout` records the same containment after parent-driven recovery
   and a bounded child deadline.
6. `recovered` records the second parent-driven recovery and durable child
   event state.
7. `ime-accessibility` records physical focus, text, IME, pointer, and semantic
   routing through the Instance-prefixed Agenda input.
8. `host-restart` records host recovery with the authority PID unchanged.
9. `authority-restart` records authority recovery with the host PID unchanged.
10. `authored` records Stock after its resident agent activates a different v4
    Stock revision.
11. `rollback` records the original v4 Stock revision restored through the
    host control.

Each checkpoint binds the exact product revision, device serial, artifact
path, byte size and SHA-256, monotonic host timing, SELinux state, processes,
surfaces, authority JSON, screenshot, complete logs, memory, and the
product-appropriate accessibility or readiness snapshot. The final
`audit-v4-composition-campaign` command rejects missing or reordered stages,
identity drift, stale state, merged grants, revision changes during appearance,
uncontained failures, missing recovery, non-namespaced IME evidence, coupled
process restarts, non-v4 authoring or rollback, and altered artifacts. A pass
creates and independently verifies a complete evidence manifest.

## Milestone closure matrix

The original implementation plan is judged at its stated exit gates, not by
the presence of a commit with a matching title.

| Milestone | Implementation status | Exit-gate status |
| --- | --- | --- |
| 0. Wire model | Closed. `experience-package` owns canonical identities, schemas, contracts, graph limits, and canonical JSON. | Closed by the shared Rust, Linux, Android-authority, and TypeScript fixture plus canonical/oversize rejection and deterministic mutation/property campaigns. |
| 1. Package format v4 | Closed. Complete package metadata, derivation, contracts, dependencies, hashes, and deterministic revision identity ship in the platform-neutral crate and revision store. | Closed by corruption, digest, signature, legacy-read, sidecar, deterministic identity, and state-migration tests. |
| 2. Experience registry | Closed. Stable records have independent current/previous pointers; Stock Shell and Stock Mobile are distinct reserved pinned identities; retirement is recoverable. | Closed by independent-history, both platform legacy-import targets, pinned-retirement rejection, restart, and atomic-pointer tests. |
| 3. State and appearance authority | Closed. Durable state is Experience-owned; appearance and grants are separately versioned capability resources; graph state promotes as one batch. | Closed by restart, locked-revision state, appearance-without-revision, grant, stale writer, idempotence, and all promotion-fault tests. |
| 4. Experience API v4 | Closed. Named exports, typed properties/events, viewports, appearance context, and `experience_mount` are the only authoring target. | Closed by export/scenario, schema, viewport, mount, appearance, and v3 rejection/rollback-reader tests. |
| 5. Dependency resolver | Closed. Exact aliases, revisions, exports, contract digests, roles, grants, limits, and content-addressed graphs are validated before preparation. | Closed by missing, stale, cyclic, incompatible, unreviewed-flow, depth, and aggregate-instance tests. |
| 6. Linux runtime graph | Closed. Each Instance owns a VM/runtime record and children render inside host-owned mounts with unavailable fallbacks. | Closed by the reference Dashboard integration, process worker, nested compositor, and exact Framework campaign. |
| 7. Boundary containment | Closed. Instance namespaces cover scenes, assets, state, semantics, input, text, provider data, focus, clipping, and failures. | Closed by runtime/host/compositor tests and the Framework child-failure, timeout, input, renderer-recovery, and semantic-control campaign. |
| 8. Graph activation and recovery | Closed. Graph prepare, present, confirm, discard/finalize, journal, authority promotion, and multi-root pointer movement are durable operations. | Closed by every durable cut-point, presentation rollback, power-loss simulation, multi-root fault, restart, and Framework recovery evidence. |
| 9. Fork, remix, and authoring | Closed. Explicit targets, exact parents, derivation provenance, candidate contracts, migration binding, and fresh grants are enforced. | Closed by self-contained remix, exact-parent, replacement, state-migration, provenance, and no-inherited-grant tests. |
| 10. Tracked dependencies | Closed. The persistent reverse index resolves every affected locked/tracked root into one activation set. | Closed by compatible tracked advance, locked pinning, inactive/presented multi-root, atomic state, rollback, and restart tests. |
| 11. Android parity | Implementation closed. Android authority and hosts use the shared package, registry, graph, state, appearance, grant, namespace, activation, system-control, and physical-campaign model. | **Physical gate open.** Compile/offline/restart-fault and campaign-auditor evidence passes, but the SM-A336B composition, input, IME, accessibility, appearance, containment, restart, authoring, and v4 rollback campaign has not run on final rebuilt candidates. |
| 12. Stock migration and hardening | Implementation closed. Stock uses semantic appearance tokens and v4 exports; registry launch replaces singleton ownership; the retired secondary product is absent; optional Linux process isolation works. | Code and offline exit conditions are closed. Installed Linux promotion and Android physical hardening remain product-acceptance gates, not missing v4 architecture. |

All active built-ins, signed references, resident-agent examples, and new
authoring are therefore v4. The v3 reader is deliberately retained only for
the reversible migration window. Removing it before the cleaned Android
artifacts have physically migrated and rolled back would violate the original
migration plan rather than complete it.

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

Desktop tests and the nested Linux VM campaign complete the non-physical parts
of the first gate:

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

The shared boundary campaign decodes the same canonical fixture in core Rust,
Linux, Android, and TypeScript and rejects non-canonical, unknown, or oversized
input in each implementation. Deterministic property tests cover 10,000
bounded schemas, 10,000 resolved graphs, and 10,000 byte-level package or graph
mutations. Graph activation fault injection now interrupts all five durable
cut points. Intent and presentation recover the complete old graph; authority,
registry, and graph commits recover the complete candidate.

The repeatable release-profile desktop probe measured a three-Instance
Dashboard graph on the development workstation. Package install and resolution
took 1.383 ms, cold start through all mounted scenes ready took 1.347 ms, a
child event reached the composed snapshot in 0.693 ms, appearance propagated
to the composed snapshot in 0.250 ms, graph prepare/present/commit took 0.809
ms, and committed-journal recovery took 0.610 ms. Process RSS increased by 772
KiB, or a coarse 257 KiB per Instance. These measurements exercise the
serialization, VM, and transaction boundaries. They are not compositor frame,
physical input, or device memory verdicts.

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
coexistence, exact presentation fences, and host recovery. An earlier
direct-DRM VM and cold-boot campaign passed page flips, resident-agent
authoring, VT pause/resume, `s2idle` freezer recovery, output hotplug, separated
identities, process recovery, and reboot restoration. Those runs predate the
v4-only boot rewrite and remain host evidence, not v4 composition acceptance.

The exact `4f93f50e9e55` host and compositor binaries have since completed that
missing Framework 12 campaign. Stock presented locked Dashboard with independent
Agenda and Media mounts. Namespaced child input, authority appearance generation
2, update-failure and timeout containment, Dashboard renderer recovery, durable
state, Stock v4 authoring, dismissal, physical DRM page flips, integrated
keyboard, touchpad motion and button, touchscreen contact, and clean GDM logout
all passed. The 87-file evidence manifest is independently verified and has
SHA-256 `0be17dc236149c9755c93b755314f8950e52a2199be20eb1cd08f9dcbbd7e800`.
Because the target was Fedora development-live, the verdict is deliberately
`DIAGNOSTIC_PASS promotion_eligible=false`; it closes the Framework composition
and integrated-input question but not an installed-product promotion gate.

Compat 1 and Core 1 candidates were built from exact cleanup source
`884ab4e0036e621f51ba0a3a6c147a5c591da81e`. Both offline inspectors pass the
v4 product, graph, signature, AVB, ownership, and retired-secondary-absence
gates. Their sealed OTAs have SHA-256
`1e438726c96dfbdfa067b43a7ae4662b7e6b91ff56f4f2693d5c95470f0406ec`
and `0b0ce039e306c42292cd67eb6be19f15ee753f7ab2ff6b4059247210af4f17fb`;
their deterministic target-files archives have SHA-256
`e8dc8d7d41ddbfe7d6459cc63d4f7290bc9f6e147914382aca1f504d0261a4f2`
and `49ba5c7ace4df8bcd22605f2334da0fa34229b7fd227bf1f657b7f8785a1b863`.
This superseded the earlier Android candidates that still carried the retired
secondary agent input. Their independently verified 17-file evidence manifest
has SHA-256
`a8f056b6d8c77d56b2ed6b4f297802b5b726a3f4200573ab43859114690f7b93`.
Those cleanup artifacts are themselves now superseded by final candidates
built from exact source
`cfe4ebb63eb3b7ffc9bf72c95a25f33152e1314c`, which contains the shared
Android scene transform, mounted-child IME fixture, host-owned navigation,
appearance and rollback controls, and the complete evidence campaign. Compat
build identity `sos.compat1.cfe4ebb63eb3.a6a42402ae5b` produced a
1,067,699,297-byte OTA with SHA-256
`3f70274838d07d2aedeeea820b1bab549f628ed9032cc956e31c1a5bb07e1144`
and a deterministic 2,173,677,658-byte target-files archive with SHA-256
`dd80b1332c37e3a385551e70da4b829d938ef1a3967dda0bc09582b76f97c631`.
Core build identity `sos.core1.cfe4ebb63eb3.4d984b84b044` produced a
1,022,859,688-byte OTA with SHA-256
`c183cba1d9cbc71d19ef91a960716c6a621b3272bb075ed3c016a498c868c454`
and a deterministic 2,077,042,185-byte target-files archive with SHA-256
`95dcb14c5287fc33efb0006431559a81ad107901042cb976bcc5e2e0b29103ae`.
Both offline inspectors pass and both archives contain Stock and its semantic
theme while excluding the retired secondary experience. The independently
verified 22-file evidence manifest is
`.cache/evidence/android-v4-cfe4ebb/manifest.tsv`, 2,129 bytes, with SHA-256
`697ae62cc3f66211eebb2e95e97decd4fef34ec4935fe7ede6784c5274939bc8`.
These are the exact artifacts reserved for the next physical gate, not a
physical-device verdict.

The remaining physical composition gate is the Samsung SM-A336B campaign for
Compat and Core: composition, input, IME, accessibility, appearance, grants,
child failure, restart, rollback, and exact evidence manifests. The device is
currently exposed only through Samsung Download Mode without an authorized ADB
or recovery transport, so no physical mutation has been attempted. Installed
Linux promotion, panel latency, suspend, GPU recovery, thermals, and power
evidence remain separate product gates.
