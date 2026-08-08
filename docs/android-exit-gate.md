# Android exit gate: first five-assumption audit

Date: 2026-08-08

> Historical audit: later work first implemented and then retired the native
> candidate-process path. Current revisions are Luau-only and activate within a
> permanent host; see [`vision.md`](vision.md).

This audit tests the five gates in [`vision.md`](vision.md) before SOS starts a
privileged AOSP shell. It deliberately reduces the agent requirement to a
single unattended mutation. Screenshot inspection and autonomous repair are
future work, as requested; a human-authored correction is evidence about the
runtime, but is not credited as agent success.

## Environment and artifact

- Host: development Mac, `codex-cli 0.147.0`, Rust 1.94.0.
- Agent: headless Codex, `gpt-5.6-luna`, medium reasoning, ephemeral
  workspace-write sandbox. This follows the
  [official model guidance](https://developers.openai.com/api/docs/guides/latest-model),
  which positions Luna for efficient, high-volume work.
- Device: Samsung SM-A336B, Android API 35, ARM64.
- Runtime: pinned GPUI/Zed `5688167d224b`, GPUI Mobile `1d3ec2a1d14a`, Luau
  through `mlua` 0.12.0.
- Tested dirty APK: `sos-experience-ed7669d9f777-dirty.apk`, 37,242,259 bytes,
  SHA-256 `e74bb671f91b4597d1cfe095e25a3a43396e25516a44740191d5411a412f6c43`.

The raw agent transcript and screenshots are intentionally ignored by Git. The
trial transcript is `agent-20260808T014820Z-luna-medium.jsonl`; the attached
result screenshot is `sos-android-exit-attached.png` with SHA-256
`4fdb39057a179d5a7f4b101d7e6c2f0d41ae2a25900f46b91c4fd595362f2007`.

## What changed

The runtime gained a bounded low-level canvas with paths, quads, local pointer
coordinates, and source-defined hit regions. Luau owns the geometry and drag
state; Rust only validates, paints, hit-tests the declared source regions, and
routes events. Limits cover command, point, hit-region, effect, payload, VM
memory, and execution time.

Provider resources and actions can now cross a JSON request/response protocol.
For this experiment, `adb reverse tcp:47777 tcp:47777` connects the APK to a
separate `providerd` process on the Mac. The app fetched its initial model over
that transport and the generated drop emitted an allowlisted
`notes.attach_to_event` action. The provider returned a receipt that was
persisted with experience state. A provider error rejects the state transition.

GPUI Mobile has unusual drag semantics: Android `ACTION_DOWN` is withheld until
a gesture is known to be a tap, while a drag starts with `MouseMove` and ends
with `MouseUp`. The first bridge assumed desktop `MouseDown` semantics. It was
fixed to acquire a source hit region on the first move and suppress the
simultaneous scroll gesture over the canvas.

## Agent trial

The exact canonical request was sent through:

```sh
./tools/sosctl agent-generate \
  'Center the experience on what is next and show music only while playing. Remove cards and invent a spatial flow in which time bends toward events with travel. Let me drag the first note onto the Design review appointment; calculate the geometry and hit regions in Luau, show attached state, and emit the typed notes.attach_to_event provider action on a valid drop.'
```

The agent created one 8,265-byte source revision. It independently calculated a
travel-weighted bent spine, event and note rectangles, release-coordinate
destination testing, persistent drag/attached state, and the typed provider
effect. Its first validation passed with 17 nodes, one image, one canvas, and
four semantic nodes. There was no agent repair edit.

The first source became visible on the phone in 43.976 ms: 0.129 ms queue,
4.976 ms compile, 1.051 ms render, and 6.030 ms total worker time. Its source
SHA-256 was `6c93e35afbeb5d0e6ce57b438578fe1245aea59e755b849a623bcbe32d87871b`.

However, the agent placed the note at local y=438, mostly outside the canvas's
interactive bounds on this phone. A tap on the few reachable pixels generated
`note_press` and `note_drop`, but a real drag could not start reliably. This is
a failed single-shot task even though compilation and presentation succeeded.
The initial harness also ran validation twice—once inside the agent and once in
the wrapper. The wrapper now reserves validation for itself, ensuring one
authoritative attempt in future runs.

For substrate testing only, an operator changed the default note y-coordinate
to 330. The resulting 8,343-byte source hash was
`a3bde37f3583b727616786a86d09b6e843d8f16c671e0bc3eabc7e198c0bb53d`.
This corrected source became visible in 99.135 ms and is not credited as an
agent success.

## Physical-device evidence

With the corrected geometry and mobile drag bridge, this real device gesture
completed the intended path:

```sh
adb shell input swipe 260 1510 495 890 1200
```

Logs recorded one `note_press`, 29 `note_drag` updates, and one `note_drop`.
Worker update time stayed between 0.891 and 6.528 ms in the captured sequence.
The APK logged:

```text
provider_effect_completed provider=notes action=attach_to_event
```

The separate Mac process logged:

```text
provider_action request_id=2 provider=notes action=attach_to_event note_id=note-1 event_title=Design review
```

Persisted phone state contained `attached=true`, `drop_valid=true`, the moved
coordinates, and receipt `notes:note-1->Design review`.

A 1,000-swap run alternated the corrected generated canvas source and the
earlier time-flow experience, waiting for a GPUI post-render callback after
each commit:

```text
accepted=1000 rejected=0 duration_ms=64441
visible_p50_us=38765 visible_p95_us=127746 visible_p99_us=128782
visible_max_us=130350 worker_p95_us=7740
rss_start_kb=293676 rss_end_kb=297264 rss_peak_kb=297268 rss_delta_kb=3588
```

Twenty background/foreground cycles retained PID 11345, the exact active
source hash, and the full attached/provider-receipt state. In-process rollback
to source `6c93e35a…` took 119.112 ms without reverting state; rolling forward
again took 64.932 ms.

## Five-assumption verdict

| Gate | Verdict | Evidence and missing proof |
| --- | --- | --- |
| A. Generative depth | **Partial** | The agent invented low-level geometry, hit testing, state, and an effect; the corrected source completed the interaction. Its original geometry made the physical drag unreliable, so the canonical first-shot demonstration did not pass. |
| B. Single-shot agent loop | **Fail** | Headless Luna performed one unattended source mutation and passed validation/presentation, but failed the task at physical interaction. No self-correction was expected or used. The transcript also consumed 171,435 input tokens (147,456 cached), 4,712 output tokens, so context/cost discipline needs work even with the cheaper model. |
| C. Complete revision recovery | **Fail** | The current tree stays live during candidate evaluation, source/current/previous files persist, and rollback works. There is still one APK process and one GPUI surface—no fixed supervisor, candidate process, first-frame surface promotion, or crash rollback. |
| D. Provider/state independence | **Partial** | Snapshot and the decisive action crossed a typed TCP boundary, and state survived swaps/lifecycle/rollback. The APK still links a fake-provider fallback, there is no external state service or explicit schema migration, and promotion/migration fault injection is absent. |
| E. Sustained device viability | **Partial** | Drag, paint, state, 1,000 swaps, and 20 lifecycle cycles were stable. Worker p95 is 7.740 ms, but visible p95 regressed to 127.746 ms versus the 100 ms target; a 64-second run is not a thermal or long-leak soak. |

## Decision and next gate

Do **not** move to the privileged AOSP shell yet. The low-level Luau direction
is confirmed strongly enough to continue, and the experiment found no reason
to replace GPUI or Luau. But three properties still depend on the APK host:
reliable one-shot task completion, process/surface-level recovery, and durable
provider/state services with migrations.

The shortest path to an Android-exit decision is:

1. Document viewport/layout constraints in the experience API and run a fresh
   suite of untouched single-shot agent outputs; measure task-level first-pass
   success, not only validation.
2. Attribute and reduce the generated-canvas visible p95 below 100 ms, then run
   a longer memory/thermal soak.
3. Build a fixed Android supervisor with a separate candidate process/surface,
   first-frame promotion, forced crash, and rollback while the old surface
   remains interactive.
4. Move state and all fake-provider access behind services, remove the linked
   fallback for the gate build, add explicit state schema migrations, and inject
   failures before, during, and after migration/promotion.

Only after those pass would Android's application model be constraining the
research more than it is de-risking it.
