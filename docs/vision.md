# SOS product vision: an agent-native operating experience

This document defines the intended SOS product. Experiment reports say what has
been proven. When a prototype shortcut conflicts with this design, the shortcut
is temporary.

The idea originated in the
[initial design conversation](https://chatgpt.com/share/6a765af1-5328-83eb-83a8-ea844b42c865).

## One-sentence thesis

> SOS is an operating environment in which the user directs an agent that
> continuously writes, installs, and evolves the complete native operating
> experience, while independently installed providers retain authority over
> data and actions.

The user is not configuring a launcher, selecting widgets, prompting a chat
overlay, or asking a model to serialize a predetermined component tree. They are
directing an autonomous software engineer that can replace the implementation
of the visible environment.

For example:

> "Create a morning space around my first appointment, when I need to leave,
> family messages, and quiet music. Hide work information before 08:30."

The result becomes a durable, versioned space. Provider events refresh its data
without involving a model in every frame. The user can later reshape it:

> "Remove the cards. Make time flow upward, bend it toward appointments that
> require travel, and let me drag a note onto an appointment."

That request may require a new layout algorithm, geometry, hit testing,
animation, state transitions, and provider operations. SOS must permit the
agent to implement those concepts even when no "bent timeline" component exists.

## The change to the application model

Traditional applications own both domain capabilities and a fixed presentation.
In SOS, installed software remains an identity, update, execution, and authority
boundary, but it does not have to remain a user-visible container.

A provider package can expose:

| Interface | Examples |
| --- | --- |
| Resources | Messages, songs, contacts, events, documents |
| Queries | Search, filtering, aggregation |
| Actions | Send, play, purchase, schedule, edit, delete |
| Events | Message received, playback changed, event updated |
| Domain logic | Encryption, synchronization, DRM, banking invariants |
| Optional surfaces | Specialized, protected, or high-performance UI |
| Fallback | A deterministic experience that works without a model |

The provider remains authoritative over its data and operations. SOS owns the
cross-provider composition and presentation. In the intended experience there
may be no visible Calendar, Maps, Messages, or Music app, only a space assembled
around the user's current goal from those separately installed providers.

## What remains fixed and what remains generative

The permanent layer should contain only machinery that generated experiences
cannot safely or practically replace while they are running:

- a recovery supervisor that can stage, activate, observe, and recover
  revisions;
- display surfaces, frame scheduling, graphics/text primitives, input, and
  clocks;
- provider and system-service transport;
- persistent state that outlives any generated component graph or process;
- immutable revision storage, diagnostics, experience evaluation, and a
  separate permanent-host update service;
- a small recovery interface that remains usable when every candidate fails.

Everything normally visible above that boundary is eligible to be generated:

- component types and composition;
- layout and navigation models;
- geometry, drawing, hit testing, gestures, and animation;
- state machines, asynchronous behavior, and provider workflows;
- element-equivalent layout/paint/hit-test behavior expressed through the
  experience API, including validated shader modules where evidence justifies
  them;
- most or all of the visible system shell.

Convenience libraries are welcome. A closed component catalog is not the
architectural boundary. The agent may use a stock list, button, flex layout, or
text field, but must be able to replace them or descend to lower-level
primitives when the requested experience demands it.

## The canonical artifact

The prompt is not the product artifact. A generated experience is a durable,
inspectable revision containing at least:

```text
experience revision
├── source
├── assets and optional shaders
├── provider bindings
├── published exports and resolved experience dependencies
├── persistent-state schema and migrations
├── build/runtime metadata
├── originating user request and agent rationale
├── screenshots, logs, and acceptance telemetry
└── exact derivation parents for diff, rollback, branching, and remix
```

The user must be able to pin a space, modify it conversationally, inspect which
providers supply it, compare versions, undo changes, disable adaptation, and
export its implementation without exporting private provider data.

## Derivation and live composition

SOS distinguishes derivation from live composition. A fork or remix reads one
or more exact revisions and produces a new, self-contained experience. A live
composition mounts a declared export from one experience inside another while
each keeps separate code, state, provider grants, activation, and failure
lifecycle. Opening independent windows is coexistence, and revision-local
modules are code reuse.

The parent of a live composition owns layout space and passes only declared,
bounded values. A host-owned mount clips and composites the child, routes input
and accessibility, validates typed child events, and keeps the child scene and
state outside the parent VM. Deeply integrated cross-provider behavior should
normally become a remix. Independently useful behavior and state should remain
behind a live mount.

The versioned identity, authority, appearance, update, authoring, and
acceptance rules are defined in
[`experience-composition.md`](experience-composition.md). Package and
Experience API v4 implement them in the shared runtime and the Linux and
Android hosts. Non-v4 packages are rejected before runtime preparation. The
complete live-composition campaign has passed on both platforms; fresh rebuilt
artifacts remain to be accepted after the final reader removal.

## One experience language, not a permanent IR ceiling

The current GPUI host interprets Experience API v4 Luau into bounded retained
scene nodes. Scene
nodes have orthogonal layout, content, paint, interaction, animation, and
semantics facets instead of a catalog-defining node `type`. This is the base of
the long-lived execution path, not the full thesis; each facet must continue to
grow toward low-level capabilities rather than quietly becoming a renamed
widget schema.

The current integration executes nested clips, affine transforms, opacity
layers, host-shaped glyph runs, responsive retained layout programs,
multi-pointer capture/transform events, and content-addressed image/font
sidecars. Its semantic tree
is host-owned: Android exposes it as virtual accessibility nodes today, while a
future native SOS screen reader, switch controller, voice layer, focus engine,
or automation service can consume the same roles, actions, hierarchy, and
bounds without preserving an APK or Android UI toolkit.

SOS uses one generated-experience path:

```text
request → agent edits Luau/modules/assets/migrations
        → fresh-VM evaluation and capability validation
        → prepare retained scene in the permanent GPUI host
        → commit state/effects → present at a frame boundary → activate or reject
```

The experience API grows toward script-defined components over low-level
layout, paint, hit-test, event, animation, semantics, text-editing, asset, and
provider capabilities. Luau produces retained, host-owned structures; Rust/GPUI
executes frame-critical paint, input, text, and platform work. Experience code
never receives a GPUI `Context`, raw device handle, provider object, filesystem,
or socket.

There is no successful-Luau-to-native graduation step. If an experience cannot
express a request, SOS extends the versioned permanent-host capability API or
adds a validated revision asset kind. Replacing the Rust/GPUI host itself is a
rare signed A/B system update with its own recovery path, never the normal
conversational mutation loop and never part of an experience artifact.

Luau remains an implementation choice that can be revisited, but the product
contract has one safe experience tier. The invariant is stronger than the
language choice: normal requests must not be limited to a catalog of
preconceived visual components or interaction patterns.

## Target architecture

```text
User intent
    ↓
Agent engineering service
source inspection │ screenshot │ traces │ compiler/runtime feedback
    ↓
Versioned experience source + state migration
    ↓
Fresh Luau VM → capability validation → retained candidate scene
    ↓
Recovery/revision supervisor + permanent Rust/GPUI host
old scene stays usable → frame-boundary activation/recovery
    ├───────────────────────────────┐
    ↓                               ↓
Persistent state service      Provider broker
                              resources/actions/events
    └───────────────────────────────┤
                                    ↓
                       Native system and hardware services
                       display │ input │ audio │ storage │ GPU
```

The agent is central to composition but does not need to be in the frame loop.
Native or interpreted deterministic code handles touch, scrolling, layout,
animation, text input, and provider event updates after a revision is installed.

## What "moving off Android" means

It does **not** mean porting the prototype to an ordinary desktop Linux app.
It means promoting SOS from an application hosted by Android into the phone's
primary operating environment.

The transition is incremental, and multiple stages remain runnable as
regression and recovery targets:

1. **Android laboratory.** Passed and retained as a harness. The GPUI Mobile
   APK proved generated interaction, transactional revision activation,
   provider/state independence, and sustained device viability.
2. **Privileged Android/AOSP shell.** This is the current physical baseline. SOS boots as
   the complete visible shell. Compat may present explicitly selected
   non-system Android applications inside an SOS-owned boundary; Core presents
   no Android Activity UI.
3. **SOS system services.** Work remains in progress. Revision supervision, durable state,
   provider transport, the resident agent, evaluation, native input, and scene
   activation have first-class implementations. Phone, credential, network,
   accessibility, urgent-attention, and other retained Android services still
   need bounded native brokers or replacements.
4. **Thin hardware base.** The architecture gate passed, but migration is not
   complete. Core 1 boots the fixed native locked/recovery surface without
   Zygote or any APK process. It deliberately does not unlock CE storage or
   claim that displaced framework services have been replaced.

"Off Android" therefore does not require rewriting a kernel, GPU driver, modem
stack, camera HAL, or every vendor service. It means that Android's application
and SystemUI model no longer defines the shell, provider model, revision model,
or interaction experience.

The physical a33x implementation is split into
[`SOS Compat` and `SOS Core`](android-product-split.md). Compat is the native SOS
system with an Android application-runtime island: SOS owns every system
surface, while explicitly selected non-system Android app contents may appear
as compatibility windows. Android ceremonies are not part of Compat. Core
removes Android Activity presentation entirely while initially retaining
proven native Android infrastructure. The physical campaign has now passed the
historical Shadow, Core 0A, and Core 0B stage-specific gates, including native
display and input ownership, fixed recovery, a pre-unlock native lock surface,
and a headless-framework boot. Core 0A is now archived and Core 0B is a frozen
opt-in migration oracle. Core 1 is the sole active Core target and separately
proves the no-Zygote process and recovery boundary, but remains honestly locked
until native synthetic-password/FBE unlock and the displaced system services exist. The
exact ownership boundaries and accepted revisions are recorded in
[`android-product-split.md`](android-product-split.md) and
[`android-ui-ownership-stages.md`](android-ui-ownership-stages.md).

## Historical Android laboratory exit gate

The project was to leave the APK laboratory after proving the novel property,
not after polishing an Android application. The following were the required
gates; they remain useful regression criteria.

### A. Generative depth beyond the current IR

An agent must implement the canonical three-step demonstration against
synthetic data:

1. Center the experience on what is next and show music only when relevant.
2. Remove cards and invent a spatial time flow without selecting a predefined
   timeline component.
3. Bend the time axis according to travel, then implement dragging a note onto
   an appointment using new geometry, hit testing, state, and a typed provider
   action.

The third request was the decisive one. The original `UiNode` catalog could not
express it. The retained scene ABI lets generated paint operations and hit regions
coexist on an ordinary retained node. The first trial required an operator
layout correction; a later curated single-shot agent output completed the
interaction untouched through the low-level Luau capability API and closed the
prototype-scope gate.

### B. A genuine single-shot agent loop

For the Android exit decision, a request could be submitted from the
development workstation and had to drive one unattended generation attempt:

```text
request → inspect current source/state/provider schemas
        → patch → validate/check → run candidate
        → present diff → accept or rollback
```

The passing run used headless Codex with `gpt-5.6-luna` at medium reasoning and
recorded first-pass success, failed evaluations, time to visible candidate, and
rollback. Screenshot/log inspection and autonomous self-correction were
deliberately not part of this gate. A human manually writing the candidate did
not count as completion.

### C. Complete revision activation and recovery

Prove immutable source and assets, a persistent current pointer, fresh-VM
candidate preparation while the old experience remains interactive,
frame-boundary scene activation, rejected-candidate recovery, and permanent-host
restart from the fixed recovery supervisor. The host PID must remain stable
across ordinary experience revisions. Real-data deployments should isolate the
Luau worker without turning each experience into native code.

### D. Provider and state independence

Move calendar, notes, weather, and media behind a typed resource/action/event
transport rather than linking their data directly into the experience. At least
one interaction must cross that boundary. State must survive component, worker,
and permanent-host replacement, and incompatible revisions must carry explicit
migrations. Inject failure before, during, and after migration/activation and
durable service commit.

### E. Sustained device viability

Retain the already-proven touch, scroll, input, animation, frame latency,
suspend/resume, and rollback behavior while running the deeper generated
experience. Measure a longer thermal/memory soak, but do not block the AOSP
transition on production-quality Android accessibility, polished IME behavior,
real personal data, or a final security model.

The prototype deliberately uses synthetic data and disposable identities while
generated code remains prototype-sandboxed. Security, provider certification,
and protected system surfaces become mandatory before real credentials or
consequential actions, not before testing whether the generative experience is
worth building.

## Exit condition in one paragraph

The project was ready to begin the privileged AOSP/system-services phase when a
user could make the increasingly unconventional requests above, an agent could
implement the decisive revision in an unattended single shot, the phone could
activate working revisions without freezing or losing state, the old revision
could survive every failed candidate, and the final drag operation crossed a
typed provider boundary. The prototype-scope evidence met that condition;
remaining inside a normal APK would have constrained the research more than it
de-risked it.

## Current phase decision

The original 2026-08-08 Android laboratory gate passed at prototype scope. A
curated single-shot Luna revision completed the canonical low-level
drag/provider interaction on the phone; the then-current disposable-process
prototype survived pre/post-frame native crash probes; source, state, schema and
effects shared one commit decision; and 10,000 swaps completed with 20.7 ms
visible p95 and no rejection. This is historical evidence, not validation of the
new stable-host contract. The measurements and limitations are recorded in
[`android-exit-verdict.md`](android-exit-verdict.md).

The replacement stable-host contract has now also passed its APK regression
gate on the same phone. Accepted and rejected Luau revisions, rollback, worker
and process restart, IME editing, the coarse semantics bridge, a typed provider
effect, durable authority recovery, and 10,000 frame-paced swaps all ran through
one GPUI experience process. The new run's visible p95 was 92.708 ms with zero
rejections; full evidence is in
[`stable-host-device-gate.md`](stable-host-device-gate.md).

The project did leave the APK laboratory. Package format and Experience API v4
now own every built-in, authoring, activation, and rollback path. Exact graphs,
live mounts, independent state and grants, typed appearance, failure
containment, and fork/remix lineage passed desktop tests and physical campaigns
on Android and Linux. Android 17 Cuttlefish boots SOS as HOME with an
init-supervised on-device authority. On the SM-A336B, v4-only Compat and Core 1
artifacts passed composition, independent recovery, authoring, and rollback.
Core used a debug-only automation service for its repeatable campaign, so that
result does not replace the separately recorded physical Samsung input gate.
On the Framework Laptop 12, the exact post-cutover graph passed direct DRM,
composition, built-in keyboard, touchpad, touchscreen, authoring, rollback, and
clean GDM recovery from a mutable development-live image. That is diagnostic
hardware evidence, not Linux release promotion.

The current phase is service migration, security hardening, and product
acceptance. Compat 1 revision `sos.compat1.19d8a653fbd7.220e268c228f` remains
the broad physical fallback because its application-island and trusted-lock
campaign covered behavior not repeated by the narrower v4 composition runs.
Core 0B is the frozen headless-framework comparison target. Core 1 is the
active, intentionally locked no-Zygote target. Native CE unlock, real
credentials, assistive operation, urgent attention, data containment, the rest
of Android service replacement, and installed or immutable Linux release gates
remain open. The APK continues as a regression harness rather than the product
architecture.

## Explicit non-goals for the current phase

- Rebuilding the Linux kernel or vendor hardware stack.
- Replacing every Android service before the interaction thesis is proven.
- Production security for generated code using real identities or data.
- A final permission, provider-certification, or model-isolation architecture.
- Shipping real banking, private messaging, identity, DRM, or payment flows.
- Treating the current bounded IR as the final experience API.
- Treating Luau, GPUI, wgpu, or Android as irreversible product choices.
