# Milestone 1: Luau → IR → GPUI vertical slice

> Historical gate: this report describes the original catalog-shaped UI IR.
> The active contract is the breaking, node-type-free
> [Scene ABI v3](experience-api.md); the measurements below remain historical
> evidence for the mutation loop.

## Result

The first vertical slice is **confirmed on the Samsung SM-A336B**, with a
deliberately small scope. One stable ARM64 APK can accept a new Luau experience,
validate and display it without restarting, retain synthetic provider and local
state, reject hostile candidates, and restore the previous accepted source.

This proves the source-mutation loop. It does not yet prove production-grade
sandboxing, 1,000 consecutive swaps, a complete UI vocabulary, or an embedded
agent.

## Boundary

```text
fake Rust providers ─┐
persistent JSON state ├─→ sandboxed Luau ─→ bounded UI IR ─→ stable GPUI host
typed UI event ──────┘          │                                │
                               └──── accept / reject / rollback ─┘
```

The code is split across:

- `crates/experience-ir`: provider model, events, UI nodes, and structural
  validation.
- `crates/providers-fake`: synthetic weather, calendar, notes, and music.
- `crates/runtime-luau`: source compilation, sandboxing, memory/time budgets,
  bounded table decoding, and update/render calls.
- `apps/experience`: the permanent Android GPUI host and transactional source
  store.
- `experiences`: the accepted baseline and a nontrivial time-flow alternative.

Luau receives serialized model/state/event values. It does not receive GPUI
contexts, entities, raw pointers, provider clients, filesystem access, network
access, `require`, or native userdata.

## Device run: 2026-08-08

| Check | Result |
| --- | --- |
| Initial APK | Rendered all four synthetic domains through GPUI/Vulkan and scrolled correctly. |
| Source-only transformation | `experiences/timeflow.luau` replaced the light card layout with a dark continuous time spine in the same PID and APK. Candidate compile/evaluate/decode/swap took 6.571 ms. |
| Script behavior | Tapping the visible music control emitted `toggle_music`, persisted `{"playing":false}`, rerendered, and removed music controls. |
| Persistence | A later `adb install -r` and cold launch preserved both the state bytes and active-source SHA-256 exactly. |
| Runtime timeout | An infinite `render` was interrupted and rejected; the accepted tree remained visible and the process stayed alive. |
| Memory limit | A candidate allocating repeated 1 MiB strings returned a recoverable Luau memory error and was rejected in the same PID. |
| Rollback | The previous accepted source restored in 2.215 ms without rolling back `playing=false`. |
| Lifecycle | Ten home/resume cycles produced ten pause and ten resume events, kept PID `28680`, retained state, and logged no fatal error. |
| Packaging | Final unwind-enabled APK is about 33 MB, versus the 26 MB Milestone 0 example. |

The first infinite-loop test uncovered an Android release-profile defect:
`panic = "abort"` caused Luau's internally thrown `lua_exception` to escape its
protected-call path and abort the process. Retaining unwind tables fixed the
same test. The accepted source and persistent state survived even that crash,
which also exercised the separation between data and disposable UI state.

## Commands

One command builds, installs with data preservation, launches, and tails logs:

```sh
./tools/sosctl m1-run
```

Source-only mutation and rollback are separate fast-path commands:

```sh
./tools/sosctl script experiences/timeflow.luau
./tools/sosctl rollback
```

The host keeps `experience.active.luau`, `experience.previous.luau`, and the
JSON state in app-internal storage. A rejected candidate is retained as
`experience.rejected.luau` for diagnosis and never replaces the active source.

## Remaining gates

- Move candidate compilation/evaluation off the GPUI event-loop thread. The
  current 20 ms render budget bounds VM execution, but source compilation is
  only source-size-bounded and a valid slow candidate can still consume a frame.
- Measure source-to-visible-frame p50/p95 rather than only the internal swap
  span, then run 1,000 swaps and 20 lifecycle cycles with memory telemetry.
- Reduce the roughly 7 MB APK delta. Unwind metadata and `libc++_shared.so` are
  correctness requirements in this build, so size work must preserve the
  hostile-candidate tests.
- Add native text-input, image, animation, focus, accessibility, and bounded
  path primitives to the IR as concrete experiences demand them.
- Add explicit state-schema migration before permitting revisions that change
  the meaning of persisted local state.
