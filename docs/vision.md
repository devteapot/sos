# SOS north star: an agent-native operating experience

This document is the architectural north star for SOS. Experiment reports say
what has been proven; this document says what the project is trying to become.
When a prototype shortcut conflicts with this vision, the shortcut is temporary.

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

> “Create a morning space around my first appointment, when I need to leave,
> family messages, and quiet music. Hide work information before 08:30.”

The result becomes a durable, versioned space. Provider events refresh its data
without involving a model in every frame. The user can later reshape it:

> “Remove the cards. Make time flow upward, bend it toward appointments that
> require travel, and let me drag a note onto an appointment.”

That request may require a new layout algorithm, geometry, hit testing,
animation, state transitions, and provider operations. SOS must permit the
agent to implement those concepts even when no “bent timeline” component exists.

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
may be no visible Calendar, Maps, Messages, or Music app—only a space assembled
around the user's current goal from those separately installed providers.

## What remains fixed and what remains generative

The permanent layer should contain only machinery that generated experiences
cannot safely or practically replace while they are running:

- a recovery supervisor that can launch, observe, promote, and roll back
  revisions;
- display surfaces, frame scheduling, graphics/text primitives, input, and
  clocks;
- provider and system-service transport;
- persistent state that outlives any generated component graph or process;
- immutable revision storage, diagnostics, and a build/evaluation service;
- a small recovery interface that remains usable when every candidate fails.

Everything normally visible above that boundary is eligible to be generated:

- component types and composition;
- layout and navigation models;
- geometry, drawing, hit testing, gestures, and animation;
- state machines, asynchronous behavior, and provider workflows;
- custom GPUI `Element` implementations, raw drawing, and eventually shaders or
  other GPU escape hatches;
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
├── persistent-state schema and migrations
├── build/runtime metadata
├── originating user request and agent rationale
├── screenshots, logs, and acceptance telemetry
└── parent revision for diff, rollback, and branching
```

The user must be able to pin a space, modify it conversationally, inspect which
providers supply it, compare versions, undo changes, disable adaptation, and
export its implementation without exporting private provider data.

## Execution tiers, not a permanent Luau ceiling

The current GPUI host interprets Luau into a bounded `UiNode` tree. This is a
successful latency experiment and a useful rapid-execution tier; it is not the
full thesis and must not quietly become a fixed widget schema.

SOS can ultimately use two complementary paths:

```text
Immediate path
request → agent edits Luau → on-device evaluation → GPUI frame

Unrestricted path
request → agent edits native experience source → build candidate
        → launch candidate process/surface → first frame → promote or reject
```

The immediate path should grow toward script-defined components over low-level
layout, paint, hit-test, event, animation, and provider APIs. The unrestricted
path permits arbitrary GPUI Rust views and custom elements when script/IR
boundaries are insufficient. A successful dynamic implementation may later be
promoted to native Rust; a native build must never block or destroy the current
experience.

The exact language split remains an experiment. The invariant is stronger than
the implementation choice: normal requests must not be limited to a catalog of
preconceived visual components or interaction patterns.

## Target architecture

```text
User intent
    ↓
Agent engineering service
source inspection │ screenshot │ traces │ compiler/runtime feedback
    ↓
Versioned experience source + state migration
    ├──────────── immediate Luau candidate ────────────┐
    └──────────── native GPUI build candidate ─────────┤
                                                       ↓
                                         Recovery/revision supervisor
                                         old revision stays usable
                                         first-frame promotion/rollback
                                                       ↓
                     ┌─────────────────────────────────┴──────────────┐
                     ↓                                                ↓
          Persistent state service                         Provider broker
                                                         resources/actions/events
                     └─────────────────────────────────┬──────────────┘
                                                       ↓
                                  Native system and hardware services
                                  display │ input │ audio │ storage │ GPU
```

The agent is central to composition but does not need to be in the frame loop.
Native or interpreted deterministic code handles touch, scrolling, layout,
animation, text input, and provider event updates after a revision is installed.

## What “moving off Android” means

It does **not** mean porting the prototype to an ordinary desktop Linux app.
It means promoting SOS from an application hosted by Android into the phone's
primary operating environment.

The intended transition is incremental:

1. **Android laboratory — current.** SOS is an APK using GPUI Mobile, Android
   lifecycle/input/surfaces, synthetic providers, and workstation-driven tools.
2. **Privileged Android/AOSP shell.** SOS becomes the boot-to-home experience
   and owns the visible shell. Android applications may temporarily appear as
   compatibility providers or embedded surfaces.
3. **SOS system services.** Revision supervision, state, providers, agent,
   build/evaluation, input routing, and surface promotion become first-class
   services rather than APK-local mechanisms.
4. **Thin hardware substrate.** SOS runs over the Linux kernel, device drivers,
   vendor HALs, graphics/audio stacks, and whichever AOSP services remain useful.
   The Android application framework is an optional compatibility island, not
   the owner of the user experience.

“Off Android” therefore does not require rewriting a kernel, GPU driver, modem
stack, camera HAL, or every vendor service. It means that Android's application
and SystemUI model no longer defines the shell, provider model, revision model,
or interaction experience.

## Android exit gate

The project should leave the APK laboratory after it proves the novel property,
not after it polishes an Android application. The following are the required
gates.

### A. Generative depth beyond the current IR

An agent must implement the canonical three-step demonstration against
synthetic data:

1. Center the experience on what is next and show music only when relevant.
2. Remove cards and invent a spatial time flow without selecting a predefined
   timeline component.
3. Bend the time axis according to travel, then implement dragging a note onto
   an appointment using new geometry, hit testing, state, and a typed provider
   action.

The third request is the decisive one. The original `UiNode` catalog could not
express it; the prototype now has bounded low-level canvas geometry and hit
regions, but the first agent trial still required an operator layout correction
on the physical phone. Passing requires an untouched agent output to complete
the interaction through this low-level Luau API, generated GPUI Rust, or a
combination of both.

### B. A genuine single-shot agent loop

For the Android exit decision, a request may be submitted from the development
Mac and must drive one unattended generation attempt:

```text
request → inspect current source/state/provider schemas
        → patch → validate/check → run candidate
        → present diff → accept or rollback
```

Use headless Codex with `gpt-5.6-luna` at medium reasoning for the initial cheap
model test. Record first-pass success, failed evaluations, time to visible
candidate, and rollback. Screenshot/log inspection and autonomous
self-correction remain important future work, but are deliberately not part of
this gate. A human manually writing the candidate is not completion.

### C. Complete revision replacement and recovery

Prove immutable source and artifacts, a persistent current/previous pointer,
candidate launch while the old experience remains interactive, first-frame
promotion, crash detection, and rollback from the fixed recovery supervisor.
For native revisions, use a process boundary rather than a Rust plugin ABI.

### D. Provider and state independence

Move calendar, notes, weather, and media behind a typed resource/action/event
transport rather than linking their data directly into the experience. At least
one interaction must cross that boundary. State must survive component and
process replacement, and incompatible revisions must carry explicit migrations.
Inject failure before, during, and after migration/promotion.

### E. Sustained device viability

Retain the already-proven touch, scroll, input, animation, frame latency,
suspend/resume, and rollback behavior while running the deeper generated
experience. Measure a longer thermal/memory soak, but do not block the AOSP
transition on production-quality Android accessibility, polished IME behavior,
real personal data, or a final security model.

The prototype deliberately uses synthetic data and disposable identities while
generated code remains unrestricted. Security, provider certification, and
protected system surfaces become mandatory before real credentials or
consequential actions—not before testing whether the generative experience is
worth building.

## Exit condition in one paragraph

We are ready to begin the privileged AOSP/system-services phase when a user can
make the three increasingly unconventional requests above; an agent implements
them in an unattended single shot; the phone promotes each working revision without
freezing or losing state; the old revision survives every failed candidate; and
the final drag operation crosses a typed provider boundary. At that point SOS
has demonstrated that the interface is genuinely being programmed around the
user rather than configured from a catalog. Remaining inside a normal APK would
then constrain the research more than it de-risks it.

## Explicit non-goals for the current phase

- Rebuilding the Linux kernel or vendor hardware stack.
- Replacing every Android service before the interaction thesis is proven.
- Production security for unrestricted generated code using synthetic data.
- A final permission, provider-certification, or model-isolation architecture.
- Shipping real banking, private messaging, identity, DRM, or payment flows.
- Polishing the current bounded IR into a universal component framework.
- Treating Luau, GPUI, wgpu, or Android as irreversible product choices.
