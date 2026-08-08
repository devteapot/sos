# Progress ledger

This is the chronological index of verified SOS work. Detailed reports remain
in focused documents, but every meaningful experiment should add an entry here
in the same commit as its implementation.

## Entry template

```markdown
## YYYY-MM-DD — Short milestone or experiment name

**Goal:** What are we trying to prove or disprove?

**Changed:** Code, architecture, device, or tooling changes.

**Evidence:** Commands, target environment, measurements, tests, and artifact
identity. Include failures—the path to the result matters.

**Decision:** Confirm, kill, or continue with constraints.

**Open risks / next gate:** What remains unknown and what test comes next?
```

## 2026-08-08 — Milestone 0: GPUI Mobile hardware gate

**Goal:** Determine whether the pinned community GPUI Mobile port is viable on
a physical ARM64 Android phone before designing the agent or provider system.

**Changed:** Added a reproducible wrapper around the unmodified upstream example
at GPUI Mobile commit `1d3ec2a1d14a` and Zed/GPUI commit `5688167d224b`.

**Evidence:** The example built, installed, and cold-launched on a Samsung
SM-A336B (API 35, Mali-G68 Vulkan). Touch, scrolling, IME text input, animation,
and ten pause/resume cycles passed. The 26 MB APK SHA-256 was
`4e470128b8e710c488063184fd6105ac60ae63356b6ce67acb6cef8c30c3736f`.
Full evidence is in [`experiment.md`](experiment.md).

**Decision:** Confirm GPUI Mobile for the next prototype gate, without treating
the pre-1.0 community port as production-ready.

**Open risks / next gate:** Native Rust rebuild/install latency remained too high
for conversational interface mutation. Compare interpreted runtimes and prove a
source-only change loop.

## 2026-08-08 — Runtime exploration: Luau, Rhai, and Flutter/Dart

**Goal:** Find an on-device execution layer that preserves GPUI as the native
host while avoiding a Rust/APK rebuild for ordinary interface changes.

**Changed:** Explored Luau through `mlua`, Rhai as a control, and Flutter Engine
+ Dart VM + Impeller as an alternate stack.

**Evidence:** On the target phone, a small Luau UI tree evaluated in about 88 µs.
The Rhai control compiled a deeper tree in about 127 µs and evaluated/validated
it in about 19 µs. Flutter hot reload remained development-host-driven and
Flutter would replace rather than script GPUI. A cached native Rust/package/
install/launch loop measured about 14.5 seconds. Detailed tradeoffs and primary
references are in [`runtime-evaluation.md`](runtime-evaluation.md).

**Decision:** Use Luau for the first vertical slice behind a runtime-neutral,
bounded UI IR. Keep Rhai as a credible fallback/control. Do not embed Flutter as
a GPUI scripting layer.

**Open risks / next gate:** Prove sandbox limits, transactional replacement,
state preservation, rollback, real touch behavior, and Android lifecycle safety.

## 2026-08-08 — Milestone 1: Luau → IR → GPUI vertical slice

**Goal:** Prove that one stable APK can accept a nontrivial experience revision
as source, interpret it on the phone, and safely accept or reject it.

**Changed:** Added the typed UI IR, fake weather/calendar/notes/music providers,
the bounded Luau runtime, a permanent GPUI Android host, app-internal accepted/
previous/rejected source storage, state persistence, and `sosctl` build, script,
and rollback commands. Added a light baseline and a dark card-free time-flow
experience.

**Evidence:** The time-flow source swapped into the running process in 6.571 ms.
Touch emitted `toggle_music`, persisted `{"playing":false}`, and removed the
playing-only control. Rollback completed in 2.215 ms without reverting state.
`adb install -r` preserved the exact source hash and state. Infinite-loop and
memory-bomb candidates were rejected while the accepted UI stayed live. Ten
pause/resume cycles retained the same PID. The final 34,567,587-byte APK for
commit `2314cff6d383` has SHA-256
`4c1943082fc5ec2c3680680e665b946b6097d90989fdc20a633427c4ee9216c8`.
Full results are in [`vertical-slice.md`](vertical-slice.md).

One important failure occurred: with `panic = "abort"`, interrupting Luau on
Android escaped as an uncaught C++ `lua_exception` and terminated the process.
Retaining unwind support made the same hostile candidate recoverable.

**Decision:** Confirm Luau + typed IR + stable GPUI host as the current prototype
architecture. The result proves source-only mutation, not a production sandbox.

**Open risks / next gate:** Move compile/evaluate work off the GPUI thread;
measure source-to-visible-frame p50/p95; run 1,000 swaps and 20 lifecycle cycles
with memory telemetry; reduce the roughly 7 MB APK delta; then extend the IR
only for concrete needs such as text input, images, animation, and accessibility.

## 2026-08-08 — Luau worker and 1,000-swap latency gate

**Goal:** Remove Luau compilation/evaluation from the GPUI thread and determine
whether repeated source mutation remains stable and conversationally fast on a
physical phone.

**Changed:** Added a dedicated worker that exclusively owns each Luau VM,
two-phase candidate prepare/commit messaging, asynchronous action handling,
per-stage timings, GPUI post-render confirmation, RSS sampling, and a
frame-paced `./tools/sosctl stress [count]` device harness.

**Evidence:** Android logs showed GPUI on `ThreadId(2)` and Luau on
`ThreadId(11)`. On the Samsung SM-A336B, 1,000/1,000 swaps completed in 18.150 s
with visible-frame p50/p95/p99 of 17.073/20.618/20.815 ms, a 29.803 ms maximum,
and 2.936 ms worker p95. RSS started at 274,016 KB, peaked at 274,080 KB, and
ended at 259,180 KB. A later infinite loop was rejected in the same PID without
changing the active-source hash, and 20 home/resume cycles kept that PID alive.
Seven runtime tests passed. The clean 34,678,451-byte APK from commit
`1c9cf464ed13` has SHA-256
`77265416cda78e12ec3b65ca6d9e912fbdb0d65bdb5a02fb8440916933b9b249`.
Full definitions, commands, caveats, and artifact identity are in
[`worker-stress-gate.md`](worker-stress-gate.md).

The first stress launch failed because an unescaped `&` in the deep-link URI was
interpreted by Android's remote shell. Escaping it produced a passing 10-swap
shakedown and then the full run.

**Decision:** Confirm the worker-owned Luau path. Source mutation no longer
executes Luau on GPUI's event-loop thread, and measured latency is below the
100 ms prototype threshold with substantial headroom.

**Open risks / next gate:** The visible signal is GPUI's post-render callback,
not an Android compositor timestamp; the 18-second RSS run cannot rule out slow
leaks; startup still waits for the initial worker tree. Extend the IR through
real text-input/IME, image, animation, focus, and accessibility experiences,
add longer soaks, and review process isolation before accepting untrusted
remote source.
