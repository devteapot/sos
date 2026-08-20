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

## 2026-08-08 — Stateful generated experience and 10,000-swap gate

**Goal:** Prove that an agent can structurally rewrite a useful phone-like
experience while native input, focus, state, image, animation, semantics, worker
lifecycle, rollback, and visible-frame latency remain sound on hardware.

**Changed:** Added bounded keyed text-input and allowlisted image nodes, native
GPUI animation declarations, accessibility metadata, a cached GPUI input entity
with Rust-owned state shadowing, asynchronous cold worker startup, explicit
worker recreation, two structurally different daily-flow experiences, local
candidate validation, an agent-apply command, and sampled 10,000-swap telemetry.
The accepted source remains Luau data; GPUI, input, rendering, persistence, and
validation remain in the stable Rust APK.

**Evidence:** On the Samsung SM-A336B (API 35, ARM64), the final agent rewrite
validated as 5,130 bytes/37 nodes and reached a GPUI post-render callback in
15.858 ms without changing PID or JSON state; typing continued with the Android
keyboard still open. Rollback was visible in 15.302 ms and retained data. A
final-code 10,000/10,000 swap run completed in 172.646 s with visible p50/p95/
p99/max of 16.997/18.092/23.062/35.533 ms, worker p95 3.816 ms, RSS
289,268→281,088 KB, peak 294,432 KB, and zero positive delta. Five injected
keyboard taps landed during that run and the final coalesced event completed.
Twenty home/resume cycles retained PID/state and editing worked afterward;
Android automatically redisplayed the IME on 1/20 cycles, while a field tap
restored it on all others. Infinite-loop and memory-bomb candidates were
rejected without changing the accepted hash. `uiautomator` saw all six semantic
records in one window description. Fifteen workspace tests, formatting, clippy,
ARM64 packaging, install, launch, touch, submit, scroll, and native animation
checks passed. The clean 37,036,955-byte APK from implementation commit
`8687339234c5` has SHA-256
`c57255a3f3f42c320726cf4361c1ffffed131b3295f5e24333764fb69e31a372`.
Full commands and caveats are in
[`stateful-experience-gate.md`](stateful-experience-gate.md).

Three failures materially shaped the result. GPUI Mobile bypassed the generic
GPUI input handler for its Android keyboard, requiring a narrow character-
callback bridge. The first structural swap lost that callback despite retaining
the cursor, so commit now restores the active stable ID. The first long-soak
implementation left the final coalesced text event pending; draining it when
stress clears fixed the bug and the 10,000-swap run was repeated.

**Decision:** Confirm the smallest complete thesis for a trusted prototype. An
external coding agent produced a nontrivial GPUI composition; the phone
interpreted, validated, displayed, preserved state through, and rolled it back
without rebuilding the APK.

**Open risks / next gate:** The GPUI Mobile callback does not prove full marked-
text/non-Latin composition, accessibility is one coarse window node rather than
individually navigable elements, and a three-minute RSS run is not long-term
leak proof. Decide the untrusted-code process boundary, sign/revision source
delivery, add explicit state-schema migration, run an hours-long attributed
heap soak, and upstream or isolate the required GPUI Mobile platform changes.

## 2026-08-08 — Reassert the agent-native OS north star

**Goal:** Prevent the successful Luau/Android vertical slice from narrowing the
project into a scriptable app or closed declarative component system, and define
what “moving off Android” means in the original research vision.

**Changed:** Added [`vision.md`](vision.md) as the authoritative project goal and
linked it from the README and runtime decision. It defines the user experience,
provider-based replacement for visible applications, minimal fixed runtime,
durable revision artifact, immediate Luau and unrestricted native execution
tiers, target service architecture, staged AOSP transition, and measurable
Android-exit gate. Corrected the stateful-gate next step to prioritize generative
depth and autonomous revision engineering before production hardening.

**Evidence:** Re-read the initial design conversation. The defining user choice
was to avoid a preconfigured component catalog and allow the agent to write
dynamic implementation code—including component types, layout, drawing, hit
testing, navigation, behavior, dependencies, and shaders. The decisive example
was not a card rearrangement; it was a bent time axis with drag-to-attach
behavior that requires new geometry, interaction state, and a provider action.
The conversation also defined the permanent base as a minimal recovery
supervisor plus graphics/input/state/provider/build machinery, with immutable
source/binary revisions and eventually a privileged shell on the thin Android
hardware substrate.

**Decision:** Luau + typed IR remains the fast experimental path, not the final
component vocabulary. “Off Android” means SOS owns boot-to-experience, revision
supervision, provider/state services, input/surface routing, and composition on
AOSP/vendor foundations; it does not mean an ordinary Linux desktop port or an
immediate rewrite of the kernel and hardware stack.

**Open risks / next gate:** The current IR cannot express the decisive custom
interaction, `agent-apply` is still manually orchestrated, fake providers are
linked rather than reached through IPC, and swaps replace an in-process tree
rather than promote a complete candidate process/surface. Prove those four
properties with synthetic data before starting the privileged AOSP shell phase.

## 2026-08-08 — First Android-exit gate audit

**Goal:** Verify the five assumptions in the Android-exit gate using a reduced
single-shot agent requirement. Autonomous screenshot diagnosis and repair are
explicitly deferred.

**Changed:** Added bounded canvas path/quad commands, hit regions, pointer
coordinates, Android drag routing, typed provider effects, a JSON/TCP fake
provider daemon, and a reproducible headless `gpt-5.6-luna` medium command.
Added [`experience-api.md`](experience-api.md) and the full audit in
[`android-exit-gate.md`](android-exit-gate.md).

**Evidence:** The 12-test runtime suite and ARM64 build passed. A one-shot Luna
revision validated and appeared on the Samsung SM-A336B in 43.976 ms, but its
note was mostly outside the canvas hit bounds, so task-level first-pass success
failed. After one clearly attributed operator geometry correction and a mobile
touch-semantics bridge fix, a real drag emitted 29 updates, persisted attached
state, and caused a separate Mac `providerd` process to receive
`notes.attach_to_event`. A deeper 1,000-swap run accepted 1,000/1,000 in
64.441 s with 38.765/127.746/128.782 ms visible p50/p95/p99, 7.740 ms worker
p95, and +3,588 KB RSS. Twenty lifecycle cycles retained PID, source, state,
and provider receipt. The 37,242,259-byte dirty APK
`sos-experience-ed7669d9f777-dirty.apk` has SHA-256
`e74bb671f91b4597d1cfe095e25a3a43396e25516a44740191d5411a412f6c43`.

Two failures shaped the result. The first agent/harness contract accidentally
validated the same unchanged source twice; the wrapper now owns the sole
validation attempt. More importantly, GPUI Mobile deliberately withholds
`MouseDown` for scrolling gestures, so a desktop-style drag state machine never
started. Acquiring the source on the first `MouseMove` and suppressing canvas
scroll propagation fixed real-device drag handling.

**Decision:** Continue with Luau + GPUI, but do not leave the Android APK
laboratory. Gate A is partial, B and C fail, and D and E are partial. The
experiment confirmed the execution substrate while rejecting the claim that
the complete five-assumption exit condition has been met.

**Open risks / next gate:** Run a fresh untouched single-shot suite after
documenting viewport constraints; diagnose the 127.746 ms visible p95; build a
real candidate-process/surface supervisor with forced-crash rollback; remove
the linked provider fallback, externalize persistent state, implement schema
migrations and fault injection; then run a longer thermal/memory soak.

## 2026-08-08 — Freeze the raw single-shot evaluation protocol

**Goal:** Establish a reproducible lower-bound measurement for Luna medium
generation before adding a tailored prompt, skill, curated retrieval, repair
loop, stronger model, or fine-tuning.

**Changed:** Added six fixed requests spanning composition, conditional state,
custom geometry, safe phone hit regions, and a reachable provider effect. Added
a headless runner and deterministic Luau grader, documented in
[`raw-agent-evaluation.md`](raw-agent-evaluation.md). Raw generations and Codex
transcripts remain ignored artifacts; source hashes, token use, latency, and
scores are the durable evidence.

**Evidence:** `cargo check --locked -p runtime-luau --example eval_grade`,
`cargo fmt --all -- --check`, and `bash -n tools/eval-single-shot` pass. The
runner requires a clean tracked worktree and stops if the single-shot agent
touches tracked files. The frozen manifest is
`evals/raw-single-shot/cases.json`; no model outputs were inspected while
defining its checks.

**Decision:** Treat this as an intentionally weak baseline, not a forecast of
the mature agent. A valid compile alone is not success, and the deterministic
grader is necessary but not a substitute for selected physical-device visual
and interaction checks.

**Open risks / next gate:** Run all six cases, record the ignored suite artifact
hashes and summary, and audit representative results on hardware. Later compare
prompt/skill/retrieval and model variants against the unchanged suite. Consider
fine-tuning only after there is a useful accepted/rejected corpus and after
confirming support for the intended base model.

The first attempted run, `20260808-luna-medium-raw-v1`, stopped after one case
because `codex exec` inherited and consumed the case loop's piped standard
input. Redirecting Codex stdin from `/dev/null` fixed the runner. The partial
artifact is retained but is not a suite result; its sole 10,498-byte source
scored 5/5 and has SHA-256
`bbdcda53d511fffaebb72f7ab86ced58a2c6f03a00a40fa6f534a436f2bf8ce4`.

## 2026-08-08 — Android-exit gates B–D follow-up

**Goal:** Measure the raw six-case agent baseline, attribute the generated-
canvas latency tail, prototype process/surface crash recovery, and externalize
versioned state with migration and fault injection.

**Changed:** Added segmented source-to-frame telemetry; an isolated Android
candidate process/surface plus reproducible crash probe; a durable optimistic
state service with stage/promote/abort and four injected fault points; remote
state boot/action promotion; and a bounded Luau state migration contract.
Expanded the authoring contract with exact accessibility roles and the audited
phone viewport constraint. Full evidence is in
[`android-exit-followup.md`](android-exit-followup.md).

**Evidence:** Raw Luna medium v2 compiled 3/6 and scored 17/40 in 454.403 s,
using 659,981 input and 22,630 output tokens. Three identical unsupported-role
errors explain all compile failures; the drag result reached the typed provider
effect but failed safe geometry. A confirming physical-device 1,000-swap run
accepted 1,000/1,000 with visible p95 79.390 ms, worker p95 8.086 ms,
worker-to-commit p95 51.600 ms, GPUI tree-build p95 0.272 ms, callback p95
26.124 ms, and +3,748 KB RSS. Separate candidate PIDs `16995` and `17048`
failed before/after first frame while accepted PID `16889`, source, state, and
surface survived. The remote state envelope persisted across daemon restart;
phone-side before-promotion failure rejected state, while an ambiguous
after-promotion failure was reconciled by exact reload. Twenty-one Rust tests,
Java compilation, ARM64 build/install, and both candidate probes passed.
The final 37,256,773-byte dirty APK has SHA-256
`aacdc9fc8545ee008c8a542c1f3ff7f2928a688d13071204796e3d2031e0aa44`;
an exact-source after-first-frame smoke probe also retained accepted PID
`17754` after candidate PID `17861` died.

Direct process killing originally caused an Android restart loop; same-task
exceptions returned to the launcher; and an unhandled separate-task exception
showed Samsung's crash dialog. Isolating the candidate task and installing its
own uncaught-exception cleanup fixed the controlled probe. The raw evaluator's
first attempt also consumed its case-loop stdin; `/dev/null` isolation fixed it
before the authoritative v2 run.

**Decision:** The <100 ms latency gate passes and Luau/GPUI remain confirmed.
Do not move to privileged AOSP yet. Process recovery and provider/state
independence are meaningful partials, not complete gates.

**Open risks / next gate:** Replace the Java probe surface with a complete
GPUI/Luau candidate; keep the accepted experience interactive during build;
handle native process death from a fixed supervisor; remove the linked/local
fallback in the gate build; atomically bind migration, source, state, provider
effects, and surface promotion; coalesce drag state; then run the longer
thermal/memory soak.

## 2026-08-08 — Pass the Android laboratory gate at prototype scope

**Goal:** Complete the five Android-exit follow-up steps: run the real GPUI/Luau
candidate in isolation, recover native process death, preserve the accepted
experience until first-frame promotion, bind source/migration/state/effects to
one authority transition, remove the linked gate fallback, stress the device,
and make one small curated single-shot comparison before shifting focus to the
low-level AOSP work.

**Changed:** Replaced the Java candidate drawing Activity with a second complete
GPUI/Luau NativeActivity in `:candidate`. The accepted Java process now watches
exact candidate PIDs, restores its surface after native abort, kills cached
candidate processes before replacement, and distinguishes intentional
replacement from failure. The Luau worker now runs bounded state migration
during startup and candidate preparation. `StateEnvelope` carries the source
SHA-256; the external service stages state and allowlisted effects together and
executes effects only after promotion. The candidate promotes that envelope at
its first GPUI frame. The Android gate build uses `gate-strict`, and its target
dependency tree omits `providers-fake`. Added the focused evidence and decision
in [`android-exit-verdict.md`](android-exit-verdict.md), updated the north star,
and added one compact curated authoring guide.

**Evidence:** `cargo test --workspace` passes 23 tests, including bounded
migration/worker ownership and exact-once effect behavior under an ambiguous
post-promotion fault. `cargo tree -p sos-experience --target
aarch64-linux-android --no-default-features --features gate-strict` contains no
`providers-fake`. Multiple ARM64 builds, `adb install -r`, and strict remote
boots passed on the SM-A336B. Final crash probes retained accepted PID `31437`:
candidate PID `31533` aborted before a frame and PID `31656` aborted after a
real GPUI frame; both restored the accepted surface. Back-to-back promotions
used fresh PIDs `32676` then `393`, with state authority and active source both
ending at hash `597c42a1…`.

One curated Luna-medium `drag_attach` attempt produced an untouched 8,834-byte
source (SHA-256 `597c42a1ae99002ff9ba5cf5f2e6cf0876f53832a7d5b5d9ad07040aebf881ec`)
that scored 8/8 versus the raw 6/8. It used 71,595 input tokens (57,344 cached)
and 3,822 output tokens. On the phone, a real swipe produced 33 drag updates and
one drop; external revision 102 promoted with one effect and `providerd` logged
`notes.attach_to_event` exactly once. The ignored before/after screenshots are
102,430/107,303 bytes with SHA-256 `ce4b1122…`/`91fb6839…`.

The physical 10,000-swap run accepted 10,000/10,000 in 185.146 s, with visible
p50/p95/p99 18.679/20.703/21.452 ms, worker p95 3.213 ms, maximum 40.092 ms,
and `/proc` RSS +5,520 KB. Android total RSS moved +12,572 KB and battery
temperature was 36.3→36.2 °C while USB powered. The final dirty APK
`sos-experience-bbfaa87c303f-dirty.apk` is 37,297,301 bytes with SHA-256
`38130723201745591a9e954122f5133148336c987ac1604fcde4708f96c13fbd`.

**Failures and fixes:** A persisted schema-2 probe envelope correctly rejected
all schema-1 experiences; the synthetic envelope was reset to schema 1 without
discarding its state. Post-promotion recovery initially required exactly
`expected + 1` and rolled back after a candidate interaction advanced farther;
it now accepts a later exact-source revision and reloads authoritative state.
Focus events from either process could race the staged promotion; both sides
now queue actions during handoff. Direct probe stage ID `0` was mistakenly
treated as a real stage and now parses as absent. Reusing a cached candidate
process exposed GPUI global-runtime reuse, so every candidate is fresh. Finally,
back-to-back source delivery exposed deletion/overwrite races; reloads now stay
pending while busy, cleanup and reconciliation are hash-conditional, and a
deferred render is explicitly requested. The curated agent also ignored the
artifact directory; the unchanged source was moved before the sole grade.

**Decision:** Gates A–E pass strongly enough to begin the privileged
AOSP/system-services phase. Stop optimizing agent prompting for now and keep
the APK as a regression harness. This is not a claim of Android independence:
the supervisor still dies with the accepted process, Android owns composition
and input, and the provider service is reached from the Mac through `adb
reverse`.

**Open risks / next gate:** Build a privileged shell/supervisor outside every
experience process, own candidate/accepted surface and input-focus promotion,
move revision/state/provider authority onto the device, and define signed
immutable revision directories plus an atomic current pointer. Retain the
canonical drag, crash probes, strict dependency check, back-to-back PID test,
and 10,000-swap run as regressions. A multi-hour attributed heap soak, drag
coalescing, arbitrary native/shader revisions, and production security remain
future gates.

## 2026-08-08 — Standalone Linux revision supervisor

**Goal:** Remove revision and crash authority from the accepted experience
process, while defining a filesystem and candidate ABI that can be developed
before the AOSP environment is ready.

**Changed:** Added the `revision-supervisor` crate and long-lived
`sos-revision-supervisor` daemon. It installs read-only content-addressed
revision directories, verifies source/state/schema/executable identity, launches
direct child candidates, accepts a token-bound first-frame event over a Unix
socket, atomically replaces the relative `current` symlink, monitors the
accepted child, and rolls back plus relaunches the preceding revision after
exit. The typed local control protocol supports promote, status, and shutdown.
The complete contract and usage are in
[`revision-supervisor.md`](revision-supervisor.md).

**Evidence:** `cargo test -p revision-supervisor --all-targets` passed nine
tests in 17.96 seconds on the Linux development host. Tests use real child
executables and Unix sockets. They cover immutable/content-addressed install,
state/source/schema drift rejection, directory-ID recomputation, 50 atomic
pointer swaps under a concurrent reader, pre-frame crash preservation,
first-frame timeout, accepted boot-process relaunch, post-frame crash rollback
with predecessor relaunch, and a separate daemon that remains alive and answers
a control request after the candidate dies. The initial 500-swap
atomic-pointer test was rejected because rehashing the copied debug executable
on every verified pointer write made it exceed 60 seconds; 50 swaps retain the
atomic-reader assertion without pretending this is a throughput benchmark. A
CLI helper initially failed compilation because its request/response types were
one-way serde types; removing the unused helper, then restoring it with explicit
bidirectional derives when control commands were added, fixed the boundary.

**Decision:** Keep this process/filesystem contract as the standalone revision
authority prototype. It directly closes the architectural coupling in which the
accepted Android process supervised its candidate, but it does not complete the
AOSP surface/input or production security gates.

**Open risks / next gate:** Bind first-frame readiness to compositor-owned
surface presentation; sign manifests; put revision processes in namespaces and
cgroups; add process-tree/resource enforcement and multi-level recovery. Next,
move provider/state authority behind a durable Unix-socket transaction protocol
whose transaction ID is committed consistently with this revision pointer, and
inject failures before, during, and after promotion. Repeat crash/promotion
evidence on the AOSP target; this desktop test does not complete a hardware or
latency gate.

## 2026-08-08 — Durable typed provider/state authority

**Goal:** Replace the workstation prototype's in-memory stage/effect bookkeeping
with a clean local protocol and durable authority that can reconcile schema,
state, and exactly-once provider effects across ambiguous promotion failures and
process restart.

**Changed:** Added `service-protocol` with versioned typed resources, actions,
events, transactions, errors, and fault points. Added the
`provider-state-service` Unix-socket daemon/client, atomic authority-file
persistence, caller-stable transaction IDs, migration proofs bound to the exact
prior state hash, durable `staged/committing/committed/aborted` records, typed
notes projection, deterministic effect receipts, and restart/live recovery.
Both pathname and Linux/Android abstract `@name` sockets are supported. Added an
ARM64 probe and the native-Clang NDK linker wrapper required by this ARM64 host.
The detailed protocol and fault semantics are in
[`provider-state-service.md`](provider-state-service.md).

**Evidence:** Ten targeted tests pass, covering all five injected fault points,
migration proof acceptance/rejection, stale competing writers, abort,
idempotency, bounded events, restart from the durable middle phase, and a real
daemon/client Unix-socket exchange. Strict clippy passes for both new crates.
The complete workspace passes 42 tests.
On the Samsung SM-A336B (API 35), transaction `device-probe-1` injected
`during_promotion`, reconciled the ambiguous result to revision 1, and reported
exactly one receipt, one notes attachment, and four ordered events. Killing and
restarting the daemon preserved revision ID `93f1aed4…` and the exact resource/
event counts. The 2,452-byte authority file SHA-256 was `36412cad…`; exact binary
sizes/hashes are recorded in the focused document. Device probe files were
removed afterward.

**Failures and fixes:** Plain Cargo selected the host linker and missed Android
libraries. `cargo-ndk` selected the installed x86-64 NDK linker, which cannot run
in this ARM64 container's incomplete binfmt setup. A small wrapper now drives
native system Clang against the NDK 29 Android sysroot and ARM64 unwind runtime.
The first device daemon then failed with an unlabelled `EACCES`; contextual I/O
errors identified Android SELinux denial at pathname Unix-socket bind. Abstract
namespace transport fixed it without relaxing policy. An initial 500-iteration
supervisor pointer test from the preceding slice remains intentionally reduced;
it is unrelated to this service's transaction evidence.

**Decision:** Adopt the new protocol/authority as the provider and durable-state
system-service prototype while retaining the legacy TCP daemon solely for the
current APK regression harness. The Android shell run proves the service binary,
durable file, and abstract IPC work on the device; it is not an AOSP service,
privileged integration, or latency gate.

**Open risks / next gate:** Add a supervisor-owned promotion journal binding the
atomic `current` pointer to the authority transaction ID, with crash injection
between intent, pointer swap, service commit, and journal cleanup. External
providers must durably deduplicate effect IDs. Add event compaction, peer
credentials/authorization, signed manifests, cgroup/process-tree limits, and an
AOSP-owned socket/service location before replacing the APK adapter.

## 2026-08-08 — Coordinate revision pointer and service transaction

**Goal:** Make immutable revision promotion and the durable provider/state
transaction converge to one decision after supervisor, candidate, or service
failure, without exposing a candidate whose state/effects are not committed.

**Changed:** Added `CoordinatedSupervisor` and an atomically persisted
`promotion-journal.json` with `intent`, `service_committed`, and
`pointer_committed` phases. Revision preparation now waits for first frame while
the accepted child stays alive; the service transaction is the commit decision;
the candidate is checked for death again before atomic pointer replacement.
Startup recovery compares journal, pointer, immutable bundled state, and typed
service transaction, then deterministically keeps/aborts the previous revision
or relaunches the committed candidate. The long-lived supervisor daemon accepts
`--service-socket`, and coordinated promote requires `--transaction`. Full
ordering and recovery rules are in
[`coordinated-promotion.md`](coordinated-promotion.md).

**Evidence:** Ten coordinator integration tests passed in 9.03 seconds using
real child executables and a real Unix-socket authority. They cover normal
promotion; exact state/source/schema binding; candidate pre-frame death; service
failure before and during commit; supervisor crashes after intent, service
commit, and pointer commit; restart reconciliation; committed-current relaunch
after an accepted crash; and the external daemon control path. The ten-test base
supervisor suite passed in 26.26 seconds and now
also rejects a child that dies after first frame but before pointer commitment.
The complete workspace passes 53 tests with strict workspace clippy.

**Failures and fixes:** The first serialized test run hung because Rust drops
struct fields in declaration order: `TempDir` removed the pathname socket before
the service harness could send shutdown, leaving its join blocked. Making the
service field drop first fixed ownership and the suite completes in parallel.
A lifecycle audit after the first green coordinator run found the post-frame/
pre-pointer child-death race; an explicit liveness check now preserves the old
pointer and leaves a committed journal for fresh-candidate recovery. A final
cross-authority audit found that the base supervisor's accepted-child rollback
would move only the pointer after a coordinated commit while durable service
state remained new. Coordinated crash handling now pins the committed `current`
revision and relaunches it instead, keeping both authorities aligned.

**Decision:** Use the service transaction as the durable commit decision and
the supervisor journal as the cross-authority recovery record. This closes the
Linux process/filesystem consistency slice: before service commit recovery keeps
the previous revision; after service commit recovery must install the candidate.
It is not a claim of kernel-atomic cross-service commit.

**Open risks / next gate:** A compositor must quiesce old-revision input during
the short service-commit-to-pointer interval and bind `first_frame` to an actual
staged surface. Add signed revision/journal verification, peer credentials,
cgroups/process-tree enforcement, event compaction, and multi-level recovery.
Those can be partly prototyped on Linux; surface/input ownership requires AOSP.

## 2026-08-08 — Remove native experience promotion; adopt stable-host activation

**Goal:** Make Luau the sole generated-experience language, remove executable-
per-revision and Luau-to-native graduation contracts, and retain the atomic
source/state/effect/recovery guarantees as revision activation.

**Changed:** Revision format 2 now contains `source.luau`, `state.json`, schema,
and a required experience-API version; it no longer accepts, hashes, stores, or
launches an experience executable or arguments. `revision-supervisor` now starts
one configured permanent host and speaks a typed `boot`, `prepare`, `present`,
post-presentation `confirm`, `discard`, and `shutdown` protocol. The CLI and
public API use `activate`, the
journal is `activation-journal.json`, successful swaps retain the host PID, and
host/protocol failures restart `current` without rolling back already committed
service state. Candidate validation rejection remains non-destructive.

The Android harness removed `CandidateGpuiActivity`, the `:candidate` manifest
process, Java watchdog/broadcast code, process-role JNI, and `candidate-probe`.
A regular edit now prepares a fresh Luau VM on the existing worker, stages and
commits authoritative state, swaps the prepared runtime/tree in the permanent
GPUI host, and uses the existing next-frame callback as visibility evidence.
The candidate source remains a short crash journal; startup reconciles it by the
authoritative source hash if the host dies between service commit and local
source-pointer persistence. Provider transaction internals retain their staged
commit/reconciliation behavior. The north star, runtime decision, experience
API, supervisor docs, README, and historical gate notices now distinguish
experience activation from rare permanent-host A/B updates. Detailed ordering
is in [`coordinated-activation.md`](coordinated-activation.md).

**Evidence:** `cargo test --workspace --all-targets` passes 55 tests. The revised
supervisor contributes 22 integration tests: 12 base cases cover executable-free
content identity, API-version rejection, atomic pointers, same-PID activation,
candidate rejection/timeout/presentation failure, protocol recovery, permanent-
host restart, and daemon control; ten coordinator cases cover immutable
state/source binding and all journal/service fault phases. Both
`cargo clippy --workspace --all-targets -- -D warnings` and strict ARM64 Android
clippy pass. The latter used NDK 29 with `tools/android-clang-linker`, native
Clang/Clang++, and explicit Android sysroot flags. `:app:compileDebugJavaWithJavac`
also passed using a temporary writable SDK overlay because the system SDK is
read-only. No generated artifact was added to Git.

**Failures and fixes:** The first ARM64 check selected no cross C/C++ compiler
for `psm`; explicit `CC_aarch64_linux_android`, `CXX_aarch64_linux_android`,
archive tool, and sysroot flags fixed it. The first Gradle attempt tried to add
Build Tools 36 to read-only `/usr/lib/android-sdk`; a temporary SDK overlay let
Gradle install that tool without modifying the system SDK. Strict Android
clippy then exposed the existing eight-argument keyed text-input constructor;
the constructor now carries a focused allow with its immutable-boundary reason.
After protocol timeout recovery became eager, one presentation-failure test
still expected a later poll event; it was corrected to assert the immediate
replacement host PID and unchanged current revision.
An additional lifecycle audit found the remaining presented-event-to-pointer
race; a post-presentation `confirm` handshake and regression test now reject a
host that exits in that interval before `current` advances.

**Decision:** Native code is permanent substrate, not an experience artifact.
SOS will extend the versioned Luau-to-host capability layer or add validated
asset kinds when expressiveness is missing. Ordinary experience edits must not
compile Rust, replace the APK, or create a new GPUI process/surface. “Promotion”
remains only an internal legacy verb in the provider transaction service; the
product-level operation is revision activation.

**Open risks / next gate:** This is not a new hardware pass. The Linux host
executable is a protocol probe, while the Android harness mirrors the lifecycle
in-process but does not yet consume the external supervisor transport. Build and
run the changed APK on the SM-A336B; prove rejected and back-to-back revisions
in one PID, frame-bound activation, state/effect reconciliation, host restart,
IME/accessibility regressions, and latency/soak results. Then implement the AOSP
GPUI shell adapter, bind `presented` to a compositor frame, quiesce input across
the service-commit/scene-switch interval, isolate Luau for real data, and deepen
the retained layout/paint/hit-test/gesture/semantics/asset API beyond the current
bounded `UiNode` decoder.

## 2026-08-08 — Confirm stable-host activation on physical Android hardware

**Goal:** Close the preceding APK hardware gate for the Luau-only stable-host
contract: prove same-process activation and rejection, frame acknowledgement,
state/effect consistency, recovery, IME/accessibility regression behavior, and
latency/soak viability on the Samsung SM-A336B rather than inferring them from
Linux tests.

**Changed:** No runtime behavior changed in response to a device failure. The
device run did expose a reproducibility defect in `tools/sosctl`: Google's NDK
Clang and Maven `aapt2` were x86-64-only on the ARM64 Linux workstation, and the
system SDK was read-only. `m1-build` now selects native Clang/Clang++,
`llvm-ar-18`, the NDK sysroot, and `tools/android-clang-linker` on Linux ARM64;
it packages through a cached writable SDK overlay and native system `aapt2`.
The final documented command built and installed the tested APK. Detailed
measurements and artifacts are in
[`stable-host-device-gate.md`](stable-host-device-gate.md).

**Evidence:** The dirty `c710597f0296` APK is 36,649,277 bytes with SHA-256
`5a425e1f442c5670f2e90117c9cbdcec12ae75a3bb127d1a6f545d1659eb322d`.
On the SM-A336B/API 35, a structural Luau rewrite became visible in PID 28073 in
102.632 ms. An infinite render was interrupted in 20.313 ms without changing
PID, accepted-source hash, or authority-file hash. Rollback completed in the
same PID in 47.138 ms; worker recreation changed only the worker thread and took
7.153 ms. `uiautomator` retained the six-item coarse summary, the IME remained
shown, and an injected character updated the keyed Unicode draft and durable
authority. A real canvas drag committed `notes.attach_to_event` exactly once and
authority revision 46 contained the attachment and receipt.

The 1,000-swap smoke run accepted 1,000/1,000 with visible p95 80.191 ms and
maximum 82.777 ms. The full run accepted 10,000/10,000 in 428.631 seconds with
visible p50/p95/p99/max 40.197/92.708/93.846/97.376 ms, worker p95 5.352 ms,
RSS start/peak/end 296,476/306,292/306,268 KB, and a 9,792 KB positive delta.
Only one `dev.sos.experience` process existed. Restarting the external authority
and Android host recovered revision 46, the exact source and authority hashes,
Unicode draft, and effect receipt without an extra activation or write. The
final standard `./tools/sosctl m1-build`, `m1-install`, and `m1-launch` path then
repeated that recovery in PID 31416.

**Failures and fixes:** `doctor` first lacked exported SDK/JDK paths. The stock
`cargo-ndk` route then failed through the absent x86-64 glibc loader; the native
LLVM path fixed compilation. Gradle next failed while installing Build Tools 36
into the read-only SDK and then failed to start x86-64 Maven `aapt2`; the SDK
overlay and native `aapt2` override fixed full APK packaging. The existing APK
used a different debug key, so `adb install -r` was rejected. Five private
source/state files were streamed through `run-as` to an ignored local backup,
SHA-256 verified, and restored byte-for-byte around the one required uninstall.
The fresh package initially lacked its `files/` directory; creating it under
`run-as` before extraction completed the safe restore. Subsequent installation
of the final artifact succeeded with `adb install -r`.

**Decision:** The stable-host APK regression gate is confirmed at prototype
scope. Native experience promotion remains removed: accepted, rejected,
rollback, input, provider-effect, soak, and restart cases all worked through one
permanent GPUI experience process. The result replaces the previous “no new
hardware pass” status; it does not turn the APK into the permanent SOS system.

**Open risks / next gate:** Join the real AOSP-owned GPUI shell to the external
stable-host supervisor protocol, use an actual compositor-present fence,
quiesce old-scene input across authority commit, and inject crashes at each
on-device commit interstice. The gate still uses a workstation provider daemon
through `adb reverse`, began from a fresh external authority rather than
automatically migrating the legacy local state file, and retains the known
coarse TalkBack and incomplete marked-text bridges. Track the 9,792 KB RSS delta
in a longer thermal/leak soak, add signing and real-data isolation, and deepen
the Luau layout/paint/hit-test/gesture/semantics/asset API.

## 2026-08-08 — Break the widget catalog into Scene ABI v2

**Goal:** Make Luau the durable experience language without preserving the
closed `UiNode` catalog as either a compatibility constraint or a disguised
promotion trigger. Prove that the permanent Rust/GPUI layer can consume one
orthogonal retained-scene contract on the connected phone.

**Changed:** `experience-ir` now exposes `Scene`/`SceneNode` with independent
layout, content, paint, interaction, animation, and semantics facets.
`runtime-luau` requires `api_version = 2`, decodes those facets, and rejects
missing/version-1 modules; there is deliberately no catalog adapter. The GPUI
host now renders scene facets, and the former `native_canvas` implementation is
the generic `scene_surface`: paths/quads and hit regions may coexist on any
node. All five bundled experiences, hostile fixtures, validators, grading
tools, and current API/architecture documentation moved to v2. Source-local
Luau helpers still provide composition conventions without becoming host node
types.

**Evidence:** `cargo test --workspace --all-targets` passed 57 tests, including
the new explicit version-1 rejection and combined-facet validation;
`cargo clippy --workspace --all-targets -- -D warnings` passed. Every bundled
source passed the local validator. The `drag_attach` evaluator scored the
v2 spatial source 8/8: one low-level paint node, five paths, four quads, four
bounded hit regions, and a reachable typed provider effect. A strict ARM64
Android build completed through `ANDROID_SDK_ROOT=/usr/lib/android-sdk`, NDK 29,
Java 17, and `./tools/sosctl m1-build`.

The dirty `636295f714b8` APK
`artifacts/sos-experience-636295f714b8-dirty.apk` is 36,652,189 bytes with
SHA-256 `e6aab623ffc3c34bf3b51c8e191cbcd596ed8ede07c2b5d32197f9673237095d`.
It installed on the Samsung SM-A336B/API 35. The existing version-1 cached
source was rejected rather than translated, the embedded v2 source kept startup
usable, and applying `experiences/android-exit-agent.luau` committed v2 source
SHA-256 `edb83a0bd823f91ff676bde108e260a5af260ea6aa91f1dc4b3401ae4da42061`
in the same PID in 54.217 ms. The physical screenshot
`artifacts/sos-agent-latest.png` is 123,695 bytes with SHA-256
`eed26e00a10e3c4c58a5a0a6e87ea007cc3cb7d96ebf7cded76a3e040cc63be2`.
A 100-swap frame-paced smoke run accepted 100/100 with visible p50/p95/max
23.684/91.701/93.684 ms and worker p95 6.451 ms. A cold permanent-host restart
changed PID 534 to 1199, recovered authority revision 47 and the exact v2 source
hash, and published four semantic entries without a startup rejection.

**Failures and fixes:** A bare Android `cargo check` could not find the C
cross-compiler on the native ARM64 workstation; the repository's standard
`m1-build` path selected the native-Clang NDK wrapper and completed. The first
v2 launch found the intentionally incompatible cached v1 source. Existing
startup containment rejected it and selected the embedded v2 source; a normal
same-process v2 activation then advanced the authoritative source hash, so the
following cold launch recovered directly. No compatibility decoder or package
data deletion was added.

**Decision:** Scene ABI v2 is the only active experience contract. It is valid
to break this prototype ABI while the integration layer is still being
designed. Add expressiveness by extending orthogonal retained facets and typed
revision assets in the permanent host, not by adding widget types and not by
promoting individual experiences to Rust.

**Open risks / next gate:** Version 2 establishes the shape but still exposes
only GPUI flex/overlay layout, fill/path/quad paint, rectangular hit regions,
two animations, coarse window accessibility, and incomplete marked-text input.
Next add clips/transforms/layers and glyph runs, followed by retained custom
measure/arrange programs, richer gestures/pointer capture, real Android
accessibility nodes, and immutable revision-scoped assets. Keep VM, scene,
effect, and authority caps; tune them from device evidence rather than removing
the sandbox boundary.

## 2026-08-08 — Deepen Scene ABI v2 instead of adding native experience promotion

**Goal:** Test the hypothesis that the permanent Luau↔Rust↔GPUI integration
layer can absorb the next expressiveness gaps—layered paint, glyph shaping,
retained custom placement, richer gestures, accessibility, and assets—without
creating an experience-native Rust tier or rebuilding the APK for mutations.

**Changed:** `experience-ir` and `runtime-luau` now decode and recursively
validate clipped/opacity paint layers, affine translation/scale/rotation,
host-shaped multi-style glyph runs, layout min/max/aspect constraints, absolute
retained placement, clip bounds, tap/double-tap/long-press/swipe actions, and
phase/delta/velocity event fields. Paint depth is capped at 16 and glyph runs at
256. The Android `scene_surface` prepares text through GPUI's text system and
executes the retained display list through GPUI content masks, paint layers,
paths, quads, and glyphs. Luau stays out of frame-critical paint and layout.

Luau modules may declare up to 64 SVG assets of 256 KiB each. The runtime
rejects active/external SVG features, hashes accepted bytes, substitutes a
content-addressed host path, and transfers the asset set only when the worker
commits. The Android registry replaces the prior revision set atomically. This
is revision-scoped execution today because asset bytes live inside the hashed
Luau source; individually hashed supervisor sidecars remain a later manifest
format.

The host now builds a platform-neutral semantic tree from stable scene IDs,
roles, labels, values, hints, actions, editability, hierarchy, and GPUI-measured
bounds. `GpuiPlatformView` adapts it through an Android
`AccessibilityNodeProvider`, exposing virtual text, image, button, status, and
editable nodes with focus/click/set-text actions. JNI queues actions back to the
GPUI thread. Android is deliberately an adapter over SOS semantics, not the
durable abstraction. The bundled daily experiences gained stable semantic IDs,
and `android-exit-agent.luau` now exercises every new paint/layout/gesture/asset
facet.

**Evidence:** `cargo test --workspace --all-targets` passed 59 tests, including
new combined-facet, content-addressed asset, and active-SVG rejection cases;
`cargo clippy --workspace --all-targets -- -D warnings` passed. All five bundled
Luau experiences passed the local validator. The updated recursive paint grader
scored `drag_attach` 8/8 with five paths, five quads, four bounded regions, and
a reachable typed provider effect. A strict ARM64 build completed
with NDK 29 and Java 17, including Rust and Java conditional Android code.

On the Samsung SM-A336B/API 35, the expanded 12,350-byte spatial source
(SHA-256 `72097b1f05ac31931605948a77ea371907ab5f348ce4f0e19be7ff8a484efe9e`)
activated in the same PID in 74.872 ms during the final native-code cycle. GPUI drew
the nested clipped/transformed layer, naturally shaped complete calendar glyph
runs, the absolute ABI badge, and the revision-owned SVG badge. Two physical
taps on the same generated event region emitted `event_tap` followed by
`event_zoom`; the second worker transition completed in 4.602 ms. A separate
daily-flow activation exposed six virtual nodes including a real
`android.widget.EditText`. The final `uiautomator` dump exposed a full-screen
enabled host container and four independent descendants with distinct Android
classes, labels, clickability, and measured screen bounds; it carried no coarse
window description.

The final dirty APK
`artifacts/sos-experience-636295f714b8-dirty.apk` is 36,736,445 bytes with
SHA-256 `577bb4a00d71b659c40d6dde399c9bf0e2068e87b4841854171fac7e62714c85`.
The final screenshot `artifacts/sos-scene-abi-expanded-final.png` is 135,326
bytes with SHA-256
`a7bbce547f3c1470037ef8dd382c04fefe7cae17bb75b64391f698c2795d398a`.
The accessibility artifact `artifacts/sos-accessibility-tree.xml` is 1,868
bytes with SHA-256
`fc2dc7ecf1730d3151008b865606ed37ab037b6c3e45fd8eacebd4c0292a2650`.
Cold recovery preserved authority revision 61 and the exact expanded source in
PID 5867; no startup rejection, panic, or Android runtime crash was logged.

**Failures and fixes:** Requiring stable IDs for semantic nodes deliberately
rejected the previously cached v2 source on the first upgraded launch; startup
containment used the embedded scene, and a normal source-only activation
installed the migrated revision. The first device render spread glyphs across
the declared `max_width`: GPUI's `shape_line` fourth argument is fixed advance
per glyph, not a width constraint. Shaping with natural advances and applying
`max_width` as a content mask fixed the complete text on the phone. Android's
native click count remained one across two injected taps, so the host now
tracks a bounded 400 ms per-region tap history; the device then emitted the
double-tap action. The first virtual host node had a zero rectangle and repeated
the legacy summary; explicit decor bounds and a summary only when the semantic
tree is empty fixed both issues.

**Decision:** Keep native experience promotion removed. The durable contract is
a bounded retained scene plus a host-owned semantic tree and typed effects.
Android accessibility nodes are necessary while Android supplies system
accessibility, switch/voice input, focus, and automation, but they are not an
APK-compatibility commitment: a future native SOS environment replaces the
Android adapter and retains the semantic source of truth.

**Open risks / next gate:** Absolute retained placement lets Luau calculate
custom layouts now, but responsive algorithms still lack validated
host-executed measure/arrange programs over available size and child intrinsic
measurements. Add those before claiming arbitrary responsive layout. Physical
double-tap passed; long-press and swipe code paths need a real-touch gate because
ADB stationary-swipe synthesis resolved as taps on this input stack. Add
explicit capture policy and multi-pointer recognition. Accessibility still
needs scroll actions/visibility clipping, selection ranges, live-region event
coalescing, and real TalkBack/switch/Voice Access action testing. Finish marked
text/IME, upgrade the supervisor manifest for sidecar fonts/images/shaders, and
replace lexical SVG screening with a production asset compiler. The VM remains
prototype isolation, not a credential trust boundary.

## 2026-08-08 — Integrate Scene ABI v3 layout, pointer, platform, and asset depth

**Goal:** Close the remaining integration gaps without restoring native
experience promotion: responsive host-executed layout, explicit multi-pointer
capture, actionable scroll/edit semantics, complete Android composition, and
supervisor-owned image/font/shader sidecars.

**Changed:** Scene ABI v3 adds a bounded retained layout program whose measure
and arrange fractions are evaluated by GPUI/Taffy against the containing block;
Luau does not run during frame layout. Pointer events now carry stable Android
pointer IDs, count, pressure, phase, and per-pointer velocity. The Rust router
implements `none`, per-pointer, and whole-surface capture plus a bounded
two-pointer centroid/scale/rotation event. Because `NativeActivity` bypasses
Java `dispatchTouchEvent`, the repository now vendors GPUI Mobile commit
`1d3ec2a1d14a63b74d1f4269340441d4eeada27a` and observes the NDK stream before
its legacy mouse/scroll mapping. The host input queue is bounded and coalesces
only replaceable move/update samples.

Android semantics now publish scroll areas, offsets/ranges/viewports, moving
descendant bounds, UTF-16 selection and marked ranges. Virtual nodes implement
selection, set-text, copy/cut/paste, and forward/backward scroll actions. A
host-owned `InputConnection` implements committed and composing text/regions,
finish, deletion, selection, printable key events, editor submission, and
bounded complete-state delivery to the keyed GPUI editor. JNI queues explicitly
wake the host. IME insets extend the scroll content and focused editors are
revealed above the keyboard.

Supervisor manifest format 3 stores sorted asset ID/kind/file identities inside
the revision digest and content-addresses read-only SVG/PNG/JPEG/WebP/font/WGSL
sidecars. Limits are 64 assets, 4 MiB each, and 16 MiB total. Both supervisor
and runtime validate formats and SHA-256; the fresh candidate VM receives the
verified set, Android exposes images through its asset source, and GPUI loads
revision fonts for `font_family` glyph runs. WGSL remains inert data until a
separate safe shader paint operation exists. The API, vision, runtime,
supervisor, README, bundled Luau sources, fixtures, and evaluator guidance now
name v3. Both readers reject non-regular files and oversized metadata before
allocating sidecar bytes.

**Evidence:** `cargo test --workspace --all-targets --locked` passed the full
workspace suite; after adding a host-buildable router test, 91 tests pass,
including 26 vendored GPUI Mobile tests, the two-pointer 90-degree transform/
capture lifecycle, 15 supervisor tests, and an end-to-end supervisor-directory
to runtime image-sidecar test. `cargo clippy --workspace --all-targets --locked
-- -D warnings` passed. The 14,031-byte Android-exit source validates as 22
nodes, one text session, two images, one paint node, and six semantics. The
strict NDK 29/Java 17 `./tools/sosctl m1-build` and Gradle package completed.

The final installed APK
`artifacts/sos-experience-1ea2d963f8b7-dirty.apk` is 36,848,749 bytes with
SHA-256 `15d31d9a2025dd511b68fa6a7cdef61585146554d78e2916c5a1e196b2a4bd40`.
It cold-launched on the Samsung SM-A336B/API 35 in 1.467 seconds as PID 10742;
the Luau worker initialized in 15.059 ms and published five semantic nodes.
A physical tap on the revision asset emitted raw pointer down/up with ID 0,
count 1, and pressure 1.0/0.0, then committed both ordered events and left
`last_gesture = "Pointer up · id 0 · 1 active"`. The deterministic router test
supplies the two-pointer coverage that ADB cannot synthesize.

The final Samsung keyboard action emitted `kind=compose selection=16:16
marked=15:16`; the host persisted the changed multilingual draft immediately at
authority revision 93 with source SHA-256
`0763d941db8f90a0e262b65a2fa424de674b9bfb65743ae33f188b279526d82d`.
`dumpsys input_method` identified `GpuiImeBridge$BridgeConnection` as the served
connection. `uiautomator` exposed a real `android.widget.ScrollView` and the
focused editor at `[79,1262][1069,1408]`, above the IME after a measured 358.4
logical-pixel inset. The untracked capture `/tmp/sos-scene-abi-v3-ime.png` is
169,151 bytes with SHA-256
`e84d1314247b10b1ee882591225e867e2558b35d9be66ea66f431821c0141bdb`.

**Failures and fixes:** Java activity touch interception saw no native motion
because Android delivers this app's input through the NDK queue; a narrow
vendored pre-compatibility hook replaced that rejected approach. A scroll-wheel
handler on generic scene surfaces prevented the GPUI ancestor from scrolling;
removing its `prevent_default` restored physical scroll. Semantic bounds were
initially adjusted twice even though GPUI prepaint had already applied the
scroll transform; using measured child bounds directly fixed the tree. Samsung
printable keys and composition states reached JNI but remained queued until
another input; explicit main-thread frame requests fixed delivery. Finally, a
focus timer ran before the IME inset and the editor stayed obscured; observing
decor IME insets, adding a host spacer, subtracting the obscured viewport, and
retrying after layout produced the visible field.

**Decision:** Scene ABI v3 is the only active experience contract. These
capabilities belong to the versioned permanent host boundary; they do not
justify per-experience Rust promotion. Android accessibility nodes and its
`InputConnection` remain necessary while Android provides TalkBack, switch/
voice access, automation, and keyboards. A future native SOS system may replace
those adapters only by supplying equivalent system services, while keeping the
platform-neutral semantics and edit-session contracts.

**Open risks / next gate:** Run real TalkBack, Switch Access, and Voice Access
action conformance rather than relying only on the virtual-node tree and routed
actions. Exercise two physical fingers and more OEM IMEs in addition to the
deterministic router and Samsung composition gates. Replace lexical media/SVG/
WGSL admission with production decoders/compilers, sign manifests and journals,
and carry the supervisor revision directory through the eventual AOSP host IPC.
Layout v3 is responsive to containing-block size and composes with intrinsic
GPUI layout; add bounded child-intrinsic query/opcode forms only when an actual
agent layout cannot be expressed with the current retained constraints.

## 2026-08-08 — Isolate the platform-neutral host modules before Linux integration

**Goal:** Move the reusable host structure onto `main` before carrying the
Wayland client forward, so Android and future platform branches extend stable
shared boundaries instead of repeatedly moving Android-local files during
rebases.

**Changed:** Moved the revision image/font registry and retained paint/gesture
surface out of `android/` into shared experience modules. `SceneSurfaceHost`
owns the small adapter contract; Android implements it and registers prepaint
bounds with its raw-pointer router through a platform hook. Android behavior,
IME, accessibility, and pointer routing remain platform-specific. Moved the
supervisor request/event wire types into the platform-neutral
`experience-host-protocol` crate while preserving their serialized ABI and the
supervisor's public re-exports. Extended the runtime worker with explicit boot
and candidate sidecar entry points. Every prepared candidate now owns its asset
set instead of retaining or inheriting the boot revision's sidecars.

**Evidence:** `cargo test --workspace --all-targets --locked` passes all 93 main
tests: the existing 91 plus the extracted protocol wire-format test and a worker
test that prepares a candidate with a distinct image sidecar, discards it, and
then proves a source-only candidate cannot see the boot asset. Strict workspace
clippy, formatting, diff checks, and the NDK 29 AArch64 Android `cargo check`
pass. No Linux target dependency, host binary, command, fixture, or milestone
document is part of this commit.

**Failures and fixes:** The first locked test correctly refused to proceed after
adding a workspace member because `Cargo.lock` did not yet identify the new
crate; one unlocked workspace test regenerated the lock, after which the locked
suite passed. Generalizing the large glyph paint enum would have made every
variant carry the shaped-line payload size, so the shared representation boxes
that payload. Grouping the down-event position keeps the adapter trait within
the strict argument-count lint.

**Decision:** Treat protocol ownership, retained paint/gesture plumbing,
revision asset/font loading, and candidate asset isolation as permanent-host
infrastructure on `main`. Keep Wayland startup, Linux GPUI dependencies, the
Linux service adapter, synthetic pointer fallback, Linux tools, and nested
session evidence on the Linux feature branch.

**Open risks / next gate:** Shared layout/content rendering and full host
lifecycle are still duplicated or platform-local; extract them only where the
Android and Linux contracts are demonstrably identical. Rebase the Linux branch
onto this commit and verify its remaining diff is platform-scoped before using
this split for future Scene ABI work.

## 2026-08-08 — Run the stable host as a real Linux Wayland client

**Goal:** Use the Linux track while AOSP work is blocked to replace the
supervisor's protocol-only child with a real permanent GPUI/Luau presentation
host. Keep rebasing that work onto the active Scene ABI—now v3—rather than
preserving a forked presentation or revision contract. Prove boot, same-PID
scene activation, bounded rejection, host recovery, sidecar isolation, and the
typed provider/state boundary before beginning a custom compositor or
distribution.

**Changed:** Uses the small, platform-neutral `experience-host-protocol`
workspace crate for the supervisor's newline-JSON request/event ABI. Added the
feature-gated `sos-experience-host` Linux executable: it re-verifies immutable
revision manifests and files, starts the existing Luau runtime worker, renders
the bounded Scene ABI v3 through one GPUI Wayland surface,
implements `boot/prepare/present/confirm/discard/shutdown`, keeps protocol stdout
separate from diagnostics, and reports presentation from GPUI's next-frame
callback. Interaction effects use a background Unix-socket transaction, allow
only typed `notes.attach_to_event`, reconcile ambiguous promotion, and update
the visible scene only after authoritative state matches. Activation is rejected
while an interaction/state commit is active.

Rebasing onto `origin/main` `1ea2d963` required a semantic adaptation even
though Git found no code conflict: the Linux host now consumes `Scene`,
`SceneNode`, and `SceneEvent`, requires experience API 2, installs candidate
revision assets, and supports the new layout/content/paint/interaction/
animation facets. The Android asset registry and retained scene-surface painter
were lifted into shared modules, so both adapters use the same nested
clip/transform/layer, shaped-glyph, custom-hit-region, and rich-gesture code.
Platform lifecycle, layout/content rendering, Android accessibility/native text
input, and Linux host protocol integration remain thin platform-specific code.

The second rebase onto `origin/main` `d2844f23` integrated bounded responsive
layout programs, manifest-format-3 image/font/shader sidecars, the shared font
loader, and the new pointer event fields. The shared scene surface now exposes a
platform hook: Android registers prepaint bounds with its raw NDK multi-pointer
router, while Linux keeps that router out of its build and maps conventional
GPUI mouse input to the v3 single-pointer event shape. Linux candidate input now
uses the same bounded/coalescing 64-event queue policy as Android. The runtime
worker gained candidate-specific sidecar submission; it no longer reuses the
boot revision's captured asset set for later candidates. A focused fixture
[`sidecar-image.luau`](../tests/fixtures/sidecar-image.luau) exercises the real
supervisor-directory-to-Linux-host boundary.

The shared extraction was then isolated as mainline commit `b25accf`: protocol
ownership, revision assets/fonts, retained paint/gesture plumbing, the host
adapter hooks, and candidate-specific sidecars now precede this branch. After
rebasing, the shared Android adapter, protocol crate, runtime worker,
supervisor host, asset registry, and scene surface are byte-for-byte identical
to `main`. The Linux commit now contains only target dependencies and feature
wiring, the Linux host/binary, Linux fixtures and tools, and Linux-specific
documentation. Its synthetic pointer-event construction moved into `linux.rs`
and opts into the shared fallback hook, so the feature no longer patches the
shared renderer.

Added `linux-run`, `linux-script`, `linux-status`, and `linux-stop` to
`tools/sosctl`, an empty-state fixture, a real socket/service-restart effect
test, and the focused [`linux-stable-host.md`](linux-stable-host.md) report. The
default v3 developer store is ignored under `.cache/linux-revisions-v3`; the
incompatible v2 store is left untouched rather than rewritten or deleted. No
raw run artifact was retained or added to Git. The local ARM64 Ubuntu 24.04
environment gained Weston 13.0.0, Xvfb 21.1.12, and XKB development packages
1.6.0 so an actual executable could be linked and a nested Wayland seat
supplied.

**Evidence:**
`cargo test --workspace --all-targets --features sos-experience/linux-host`
passes all 98 tests with `--locked`, including 26 vendored GPUI Mobile tests, 25
supervisor/coordinator cases, 19 runtime tests, the shared wire format, and five
Linux-specific host tests. The provider test starts a real Unix daemon, commits
state plus one notes attachment, stops and restarts the service, and reads the
attachment back through its socket. `git diff --name-status main...HEAD` leaves
12 Linux-facing paths; explicit byte comparisons confirm the Android adapter,
shared assets and scene surface, protocol crate, runtime worker, and supervisor
host are identical to `main`. Strict workspace clippy with the same feature,
`cargo fmt --all -- --check`, `git diff --check`, and `bash -n tools/sosctl`
pass. The real binaries link with Rust 1.95.0 on ARM64 Linux 6.17. The existing
NDK 29/native-Clang command also passes the strict Android `cargo check` for
`aarch64-linux-android`, guarding the shared asset and scene-surface extraction
after the Cargo target split. The runtime validator also accepts all five
checked-in API-v3 experiences after the rebase.

In the earlier v2 run under Weston 13's X11 backend nested in Xvfb,
`./tools/sosctl linux-run --windowed` booted API-v2 revision `ff63f61d…` and
created the control socket in PID 1527912.
`linux-script experiences/daily-flow.luau` prepared revision `bc81479e…` with
116 us queue, 1,194 us compile, 646 us render, and 1,848 us worker total, then
reported the GPUI frame and supervisor confirmation in that same PID. Activating
`experiences/android-exit-agent.luau` as revision `99ba2162…` exercised nested
layers, shaped glyphs, gestures, and a revision SVG in the same process with
26 us queue, 1,936 us compile, 665 us render, and 2,608 us total. Infinite-render
revision `628cb7a7…` was interrupted and rejected while accepted revision
`99ba2162…` and PID 1527912 remained active. Sending `SIGKILL` to the host caused
the supervisor to boot the exact committed revision and report `HostRestarted`
in PID 1528477.

The v3 nested rerun booted revision `f174e726…` in PID 1606742 and activated
sidecar-backed revision `728f905e…` in that same process with 119 us queue,
874 us compile, 157 us render, and 1,039 us worker total. The installed inputs
were the 986-byte `tests/fixtures/sidecar-image.luau` at SHA-256
`3ec9aa6d0ff487b180dba62fa1dd91e9abeb7a87195e98e8d96a0a9a46342fef`
and the 4,021-byte checked-in `mipmap-mdpi/ic_launcher.png` sidecar at SHA-256
`11ddafaa7f09836b0576794c78ea208ebec67cd6b412578efabcfdea0c6a6183`.
Infinite-render revision `632ce86e…` was rejected while `728f905e…` and PID
1606742 stayed accepted. Killing the host restarted that exact sidecar-backed
revision in PID 1607073 with 1,089 us initialization and a new GPUI frame. The
disposable runtime and revision directories were removed after shutdown.

**Failures and fixes:** `cargo check` initially hid missing native linker names;
the executable build failed on `-lxkbcommon` and `-lxkbcommon-x11`, fixed by
installing their development packages. Weston's headless backend and an RDP
backend without a connected peer advertise no `wl_seat`; the pinned GPUI client
unwraps that global and panicked after Luau initialization. Xvfb plus Weston's
X11 backend supplied the required seat and became the nested automation setup.
The user lacked access to `/dev/dri/renderD128`, so Mesa rejected the hardware
driver and GPUI used a software path; the frame still completed. A first
standalone activation supplied `--transaction` and was correctly rejected
because transaction IDs belong to coordinated mode; retrying the standalone
request without it succeeded. The rebase itself conflicted only in this
chronological ledger, but main's ABI replacement made the auto-merged Linux code
fail conceptually until its old `UiNode`/canvas renderer was replaced. Strict
clippy then exposed an oversized shared glyph-paint enum; boxing the shaped line
kept the representation bounded without changing paint behavior. The first
Android cross-check exposed one missing shared-module import, which was added
before the target passed again. On the v3 rebase, textual conflicts appeared in
the Cargo feature/dependency split, shared scene-surface move, lockfile, and
progress ledger. More importantly, upstream's worker captured boot sidecars for
all future candidates; adding candidate-specific sidecars to the worker command
prevented cross-revision asset inheritance. The Android raw pointer registration
also could not remain a direct dependency of the shared surface, so a static
platform hook preserved Android routing without pulling GPUI Mobile into Linux.
Strict clippy exposed the shared font-registry type and expanded down-event
signature; a type alias and grouped position kept the common boundary warning
free.

**Decision:** Adopt the existing-session Wayland client as the first Linux
stable-host gate. Keep Wayland beneath GPUI and the generated IR, with all
platform handles in Rust. The Linux VM remains the primary next environment;
Raspberry Pi or other small hardware is a later portability/performance gate,
not a prerequisite. Do not begin a custom distro before the session and graphics
dependencies stabilize.

**Open risks / next gate:** This desktop/nested result is not a hardware,
latency, GPU, touch, direct-DRM, or boot-to-SOS pass. GPUI next-frame is not
compositor-owned presentation evidence; Linux text input is display-only; Linux
semantics are not yet exposed through a native accessibility adapter; and Linux
has only a synthetic single-pointer mouse bridge, not Wayland touch,
multi-pointer transforms, pressure, or explicit capture. The model is still
fake, and standalone developer startup does not orchestrate provider authority
bootstrap. Next, run this slice in a reproducible Debian Wayland VM, finish
native text input and coordinated service startup, then build a minimal nested
Smithay compositor that authenticates the shell, owns focus/surface ordering,
quiesces input across activation, places one compatibility client, and
acknowledges the exact presented shell buffer. Only after that should the same
compositor take over a VM DRM/input session.

## 2026-08-08 — Coordinate the Linux authority and define the Debian VM gate

**Goal:** Remove standalone provider startup from the Linux developer path and
make the next distro check reproducible. Prove that a real GPUI revision switch
advances the durable authority and supervisor pointer together before beginning
the Smithay compositor; do not count the existing Ubuntu nested run as Debian
VM evidence.

**Changed:** Added the Linux-only `sos-linux-session` crate. It re-verifies the
revision store, binds an empty authority to the immutable boot revision through
an idempotent durable transaction, refuses to overwrite an initialized or
unexplained mismatched authority, creates schema-bound candidate transactions,
and shuts the authority down through its typed protocol. The sole admitted
mismatch is an authority candidate and previous-revision pointer bound by the
durable activation journal, allowing the existing coordinator to finish crash
recovery. `linux-run` now builds and owns
the provider service, session helper, coordinated supervisor, and permanent
GPUI host. It passes one socket to both supervisor and host, monitors the two
top-level services, and tears the other down if either exits. `linux-script`
stages the installed revision and supplies its stable transaction ID to
coordinated activation.

Added `tools/linux-vm/create`, `start`, and `stop`, a cloud-init template, a
Debian-13-only package/Rust provisioner, and `tools/linux-vm/verify-session`.
The creator accepts only the host-matching official Debian 13 `generic` qcow2
filename and an explicit 128-hex SHA-512, verifies it before creating a 100 GiB
overlay, and starts an 8-vCPU/12-GiB UEFI VirtIO guest as a direct unprivileged
QEMU/KVM process. Networking is QEMU user mode with only loopback SSH port 2222;
the console is loopback VNC. `start` resumes the ignored overlay, while `stop`
targets only the recorded PID, preserves disks/logs, and refuses a force kill.
The verifier uses an isolated Xvfb/Weston seat and store, and reports the guest
OS so only `os=debian version=13` completes the VM gate. The full contract is in
[`linux-vm.md`](linux-vm.md).

**Evidence:** `cargo test -p sos-linux-session` passes the real-socket authority
test: it creates two verified revisions, bootstraps the first, proves bootstrap
idempotence, stages the schema-2 candidate with a proof bound to authoritative
state, confirms staging does not prematurely change current state, and refuses
to bootstrap across a deliberately promoted pointer/authority mismatch. It
then adds the exact durable journal binding and classifies that mismatch for
coordinator recovery without resetting either side. The
locked Linux-feature workspace passes all 99 tests. Strict workspace clippy,
the NDK 29 AArch64 Android cross-check, all five checked-in experience
validators, formatting, ShellCheck/Bash syntax, conflict-marker, and diff checks
pass. All four session executables link together under `cargo build --locked`.

`tools/linux-vm/verify-session` passes on the current ARM64 Ubuntu 24.04 host.
It bootstrapped authority revision `f174e726…`, booted the real GPUI host in PID
1661845, and activated revision `552f0696…` through transaction
`linux-activate-1-552f0696…` in that same PID. The worker measured 34 us queue,
1,162 us compile, 654 us render, and 1,825 us total. The exact 64-hex authority
revision and supervisor pointer matched after the GPUI frame/confirmation.
Shutdown removed both sockets; the exact disposable `/tmp/sos-linux-session.*`
tree was made owner-writable because immutable revision directories are mode
0555, then removed.

The same verifier passes inside the actual KVM guest with
`linux_nested_session_passed os=debian version=13 host_pid=3874
revision_id=552f06968bbc5c69de3db581454f60d4303289f304eaaf47a6e9dc3200297cdb`.
The guest is Debian 13.6 on `6.12.100+deb13-arm64`, with Rust 1.95.0, Weston
14.0.2, Xvfb 21.1.16, Wayland client 1.23.1, and Mesa Vulkan 25.0.7. It booted
revision `f174e726…` with 3,635 us worker initialization and activated
`552f0696…` through `linux-activate-1-552f0696…` in unchanged PID 3874. The
candidate measured 374 us queue, 1,147 us compile, 651 us render, and 1,807 us
worker total; the exact authority revision and supervisor pointer matched.

The immutable gate input was official
`debian-13-generic-arm64.qcow2`, retrieved 2026-08-08: 428,736,512 bytes,
SHA-256 `0e68f071dec0215f5d8c7e6f51898213951a6c1a4859f1b980fb4d479255e2bc`,
SHA-512
`e8ed94e83edded072c66b8871beff8243e0b846ac53980847e2ae44c6d47a8a55579181390b6c85939e85e2a821014ae87e9684930c0509a045212753c8d7916`.
The raw base, seed, and mutable overlay remain ignored under `.cache/linux-vm/`;
the overlay is reusable development state, not evidence. QEMU 8.2.2 cleanly
booted and resumed that overlay through the checked-in direct launcher.

**Failures and fixes:** The first session-helper compile treated
`state_sha256` as infallible; propagating its authority error fixed the migration
proof. The first hand-written nested run could not delete its disposable
revision files because the store correctly makes them read-only; cleanup now
validates the exact `mktemp` prefix and restores only owner write permission
before removal. An initial package-index update tried the host's foreign AMD64
architecture against ARM64-only Ubuntu ports and failed; installing the named
ARM64 virtualization packages directly succeeded. `virt-install` 4.1 first
rejected the proposed `portForward` network suboption. Moving the forward to
QEMU's monitor exposed the deeper problem: unprivileged libvirt advertised
only software `domain type=qemu` and rejected both host-model and
host-passthrough CPUs even though direct QEMU initialized KVM successfully.
The harness therefore rejected libvirt and now invokes QEMU directly.

The existing login did not acquire its newly added `kvm` supplementary group,
so this one run used `sg kvm`; a new login uses the normal group membership.
GDM/logind also replaced a temporary per-user KVM ACL, confirming that the
group is the durable fix. A clean guest poweroff made QEMU remove its PID file
before `stop` ran; `stop` is now idempotent when the file is absent and its live
PID path was tested against a disposable process. The preserved VM then
resumed and returned `cloud-init status --wait` successfully. Mesa software
rendering remains deliberate in the nested verifier, so no GPU or latency claim
changed.

**Decision:** Keep authority binding and process orchestration in a Linux-only
session layer; do not add Linux lifecycle policy to the platform-neutral
supervisor or expose it to Luau. An empty authority may be initialized from the
already committed immutable pointer. Any non-empty mismatch is an error, never
an implicit reset, unless the activation journal binds the exact candidate/
previous pair for coordinator recovery. Treat the official image digest and
matching guest architecture as gate inputs, while keeping all raw VM disks
outside Git.

**Open risks / next gate:** The Debian client-host gate is complete. Native
Linux text editing and accessibility remain open. Add the minimal nested
Smithay compositor for shell authentication, surface/focus policy, one
compatibility client, input quiescing, and an exact compositor-owned
presentation fence. No desktop or VM result completes direct DRM, physical
touch, hardware latency, thermals, or suspend/resume gates.

## 2026-08-09 — Fence Linux activation through a nested SOS compositor

**Goal:** Replace GPUI's client-side next-frame signal with evidence owned by a
minimal nested compositor, without exposing Wayland to Luau or disturbing the
Android path. Authenticate the permanent host, constrain shell/compatibility
surface policy, quiesce input across the visible scene handoff, survive a host
crash without replacing the compositor, and repeat the result inside the
reference Debian VM. Do not claim physical presentation from a nested backend.

**Changed:** Added the Linux-only `sos-compositor` crate pinned to Smithay 0.7.0
(crates.io checksum
`740cea6927892bc182d5bf70c8f79806c8bc9f68f2fb96e55a30be171b63af98`).
It uses Smithay's winit development backend, XDG shell, SHM, seat/data-device,
output, and presentation helpers. The retained state/input/winit setup was
reduced from Smithay's MIT `smallvil`/`anvil` examples at tag `v0.7.0`, commit
`a166cf4c94b5aedc332a65aa1dd753e8148829c3`; source comments and the upstream
license notice are retained. The compositor admits one fullscreen authenticated
shell and one fixed 720-by-520 compatibility toplevel, owns focus and ordering,
constrains popups, and forwards keyboard/pointer input only while no activation
fence is armed.

Added the platform-neutral `compositor-control-protocol` wire crate. Its
8-KiB-bounded newline JSON carries shell registration, presentation arming, and
registered/armed/presented/rejected events; tokens are bounded to 256 non-newline
bytes. The compositor binds a mode-0600 socket in the caller's private runtime
directory and requires both the launch token and the exact `SO_PEERCRED` PID.
The host registers before GPUI opens its Wayland connection, so that PID alone
receives the shell role and every other client receives the compatibility role.

The Linux experience host now uses that control channel when
`SOS_COMPOSITOR_CONTROL` and `SOS_COMPOSITOR_TOKEN` are both present and keeps
the existing GPUI next-frame path when neither is present. At `present`, it
first asks the worker to commit the prepared VM. In the resulting GPUI-thread
callback it arms the exact request/revision, installs the worker-confirmed
scene/state/schema/assets, and requests a frame. This avoids both certifying an
old animated frame during asynchronous worker commit and displaying a revision
whose active VM was not confirmed. Smithay tags the first later root shell
commit and emits evidence only after the shell render element participates in a
successful nested backend submit. The returned
commit and submit sequences are matched against the host's pending request
before the supervisor receives `presented`. An arm failure preserves the old
visible scene but follows the already committed worker by exiting for supervisor
recovery.

The compositor drops pending fences and releases input when control disconnects.
It records roles independently of live Wayland client lookup and replaces a
stale shell surface when the supervisor authenticates a recovery PID, allowing
the compositor to remain alive across permanent-host death. Added
`tools/linux-compositor/verify-nested`, deterministic ANSI-free logs, failure-log
reporting, the focused [`linux-compositor.md`](linux-compositor.md) contract,
and VM provisioner/README/stable-host updates. The verifier creates an isolated
Xvfb plus outer Weston session, runs `sos-compositor` nested, starts the existing
coordinated provider/supervisor/GPUI session inside it, activates
`daily-flow.luau`, kills the exact host PID, waits for recovery, and maps
`weston-simple-shm` as the separate compatibility client. All runtime stores
and logs are deleted after the gate; no raw artifact was added to Git.

**Evidence:** `./tools/linux-compositor/verify-nested` passes on the ARM64
Ubuntu 24.04 host. Revision `552f0696…` activated without replacing PID
1799559; after an exact `SIGKILL`, the supervisor booted that committed revision
in PID 1799841 without replacing the compositor. Boot, activation, and recovery
reported nested-backend submit sequences 1034, 1042, and 1124, and the
compatibility client mapped at `(280, 140)`.

The exact-worktree gate also passes in the retained ARM64 KVM guest: Debian
13.6, kernel `6.12.100+deb13-arm64`, Weston 14.0.2, and Mesa 25.0.7 software
rendering. It activated full revision
`552f06968bbc5c69de3db581454f60d4303289f304eaaf47a6e9dc3200297cdb` in
unchanged PID 11310, then recovered it in PID 11514. Compositor evidence was
commit/submit 1/928 for boot, 9/936 for activation, and 14/1009 for recovery.
The exact durable authority and supervisor pointer matched, no
`gpui_next_frame` fallback appeared, and `weston-simple-shm` mapped as the one
compatibility toplevel at `(280, 140)`.

`cargo test --workspace --all-features --lib --bins --tests --locked` passes all
104 non-documentation tests, including the new control-wire, compositor-policy,
and real Unix-socket fence-client cases. Strict all-feature/all-target workspace
clippy passes with warnings denied. The NDK 29 native-Clang
`aarch64-linux-android` release check passes with `gate-strict`, confirming the
Linux-only dependency/feature split did not affect Android. All five bundled
experiences pass `sosctl validate`; formatting, Bash syntax, ShellCheck,
conflict-marker, `git diff --check`, both local/guest compositor gates, and the
locked five-binary Linux build pass.

**Failures and fixes:** The first Smithay compile exposed version-specific
imports, Wayland trait requirements, winit callback return types, and calloop
errors that cannot enter `anyhow` because their sources are intentionally not
`Send`/`Sync`; using the 0.7.0 APIs and converting insertion failures at the
boundary fixed them. Reviewing the first host wiring exposed the old-frame race
before it became evidence; waiting for worker commit and arming at the actual
event-thread scene handoff fixed it. The first Debian verification had all
correct evidence but failed its log match because tracing emitted ANSI escapes;
machine logs are now explicitly ANSI-free. The recovery extension required
surface-role records and stale-shell replacement so delayed destruction of the
dead host cannot corrupt cardinality or reject its replacement.

The VM stopped when the host SSD previously filled and left only its exact stale
QEMU PID file. After space was freed, removing that verified-stale PID file and
resuming the preserved overlay restored the gate; Cargo detected and discarded
one corrupt incremental artifact from the abrupt stop. The first broad
`cargo test --workspace --all-features` also ran two pre-existing vendored GPUI
Mobile illustrative doctests that omit their surrounding imports/types and
failed to compile. The complete lib/bin/integration target suite above passes;
the unrelated vendor examples were not changed as part of this Linux slice.

**Decision:** Keep the compositor a trusted Linux platform layer beneath GPUI
and the generated Scene ABI. Use a separate bounded host/compositor protocol,
PID credentials, and Rust-owned surface policy; never give generated code
Wayland authority. Treat `nested_backend_submit` as valid functional activation
evidence for this development topology, but not as output presentation or
latency evidence. Preserve ordinary-compositor `linux-run` as the simpler host
gate and opt into the stronger fence only when the compositor environment is
present.

**Open risks / next gate:** The launch token plus private socket is a
development authenticator; production needs separated service identities or
system-managed credentials against same-UID inspection. The current compositor
has no direct DRM/GBM, udev/logind session, libinput backend, permanent recovery
view, cursor rendering policy, touch/multi-pointer route, text-input/IME,
clipboard, accessibility adapter, layer shell, XWayland, or general application
placement capability. Its successful submit can still be delayed or discarded
by the outer compositor. Next, carry the same policy into a direct Debian VM
session and bind acceptance to KMS/page-flip evidence. Only after that VM gate
should SOS attempt boot-to-session packaging or physical-device performance.

## 2026-08-09 — Present the SOS shell through direct DRM in the Debian VM

**Goal:** Carry the proven compositor policy into a direct session without
changing Luau, the Scene ABI, the permanent-host protocol, or Android. Acquire
the reference VM's real VirtIO DRM and input devices, keep a compositor-owned
recovery view alive independently of GPUI, and release each activation fence
only after the exact queued shell frame generates a DRM VBlank/page-flip event.
Do not infer physical-device timing or input behavior from the VM.

**Changed:** `sos-compositor` now selects `--backend nested|drm`; the nested
feature remains the default, while `direct-backend` opts into Smithay 0.7.0's
udev, libseat, libinput, DRM, GBM, EGL/GLES, and system-library features plus
`smithay-drm-extras` 0.1.0. The first direct slice intentionally accepts one
seat, one DRM device, and one connected output. It opens devices through
libseat, scans the output through udev/DRM, allocates scanout buffers through
GBM, composites Wayland surfaces with GLES, routes libinput through the shared
input policy, and continually owns a dark recovery clear even when no shell is
alive. Device removal stops the compositor; hot-add, multiple outputs, and a
rendered cursor are not silently approximated.

The activation policy now separates a successful queue from a presentation.
It attaches the request/revision/commit/submit identity to the DRM frame, keeps
input quiesced while that frame is outstanding, calls `frame_submitted` only
from the matching VBlank callback, and only then publishes the event and
releases input. The bounded control ABI now distinguishes
`nested_backend_submit` from `drm_page_flip`; direct evidence includes the
kernel output sequence, timestamp, and monotonic/realtime clock domain. Wayland
`wp_presentation` feedback is completed from that same callback. Realtime
drivers receive only the honest VSync feedback flag; monotonic DRM events also
carry hardware-clock/completion flags.

Added `tools/linux-vm/verify-direct-session`. It refuses bare metal and
non-Debian-13 guests, remembers GDM state, stops GDM, acquires `seat0` through
seatd, restores GDM on every exit path, and deletes only its exact disposable
store. Before starting GPUI it requires a direct KMS recovery page flip. It then
boots the coordinated session, activates `daily-flow.luau` without changing the
host PID, kills that exact PID, verifies supervisor recovery of the committed
revision, maps `weston-simple-shm` as the compatibility client, and rejects both
GPUI-next-frame and nested-submit evidence. The Debian provisioner now installs
GBM/libinput/libseat/udev development packages and seatd, enables the direct
compositor build, and installs rustfmt/Clippy for the pinned Rust 1.95.0 toolchain.

**Evidence:** `./tools/linux-vm/verify-direct-session` passes in the retained
ARM64 KVM guest on Debian 13.6/kernel `6.12.100+deb13-arm64`. Revision
`552f06968bbc5c69de3db581454f60d4303289f304eaaf47a6e9dc3200297cdb`
activated in unchanged PID 59723, and an exact `SIGKILL` recovered the committed
revision in PID 59849 without replacing the compositor. Boot, activation, and
recovered boot returned commit/submit pairs 1/3, 14/11, and 20/17. All three
events came from DRM VBlank callbacks with monotonic kernel timestamps. The
VirtIO driver reported output sequence zero, which is retained as driver
metadata rather than replaced by a synthetic value. Recovery frames were
observed before initial boot and between dead/restarted hosts; the compatibility
surface mapped at `(280, 140)`, and durable authority matched the supervisor
pointer.

The nested regression still passes on the ARM64 Ubuntu host:
`linux_nested_compositor_passed` activated in PID 1816195, recovered in PID
1816390, and produced `nested_backend_submit` pairs 1/983, 13/992, and 18/1020.
In the Debian VM,
`cargo test --workspace --all-features --lib --bins --tests --locked` passes all
105 non-documentation tests and strict all-feature/all-target workspace Clippy
passes with warnings denied. The NDK 29 native-Clang
`aarch64-linux-android` release check with `gate-strict` passes. Formatting,
ShellCheck, `git diff --check`, and locked direct/nested builds pass. No raw VM,
render, or log artifact was added to Git.

**Failures and fixes:** The first full verifier expired while `linux-run` was
still rebuilding GPUI; increasing only that bounded startup wait fixed the
harness. The next boot armed successfully but GPUI sent only registry/sync
requests and timed out: direct mode flushed Wayland clients only after a
damaged render, so the static recovery frame stranded protocol replies.
Flushing at the backend-independent event-loop boundary fixed initialization.
Boot then presented, but the first revision swap timed out after arming because
no-damage compositor cycles withheld Wayland frame callbacks and GPUI could not
schedule its next commit. Sending frame callbacks on those paced no-damage
cycles fixed activation without weakening the fence; acceptance still waits
for a later damaged frame's VBlank.

The VM software stack reports no `EGL_WL_bind_wayland_display`; the compositor
records that warning and continues through its advertised SHM path, which the
real GPUI host successfully uses. An attempted verifier assertion for early
QEMU keyboard/tablet `DeviceAdded` log messages was rejected: the backend and
seat initialize, but those callbacks were not emitted before the recovery
frame, and this gate injects no physical input. The code retains device logging,
while actual libinput event delivery remains an explicit later gate rather than
being inferred.

**Decision:** The direct Debian VM functional presentation gate is complete.
`drm_page_flip` is valid compositor-owned VM evidence and is materially stronger
than nested submission, but it is not a physical latency claim. Keep the direct
dependencies feature-gated so ordinary Linux/Android builds do not acquire
system seat/DRM requirements. Keep seat, KMS, and platform handles entirely in
trusted Rust; Luau continues to see only Scene and provider capabilities.

**Open risks / next gate:** Package this path as the Debian VM's boot session:
systemd ordering for compositor/provider/supervisor, logind active-VT ownership,
system-managed shell credentials, and recovery without an SSH-launched seatd
session. Add deterministic cursor rendering and injected keyboard/pointer/touch
evidence there. Native text editing/IME, clipboard, accessibility, hotplug,
multiple outputs, XWayland, suspend/resume, and physical-device performance
remain open; no VM result completes those gates.

## 2026-08-09 — Boot the direct compositor as the Debian system session

**Goal:** Replace the SSH-launched seatd proof with an unattended boot contract
in the disposable Debian 13 VM. Require a PAM/logind active session on tty1,
start the recovery compositor before provider/supervisor/host, deliver the shell
secret without putting its value in process arguments or environment variables,
recover component failures on the committed revision, and restore the VM to
GNOME after the evidence run. Do not turn the VM result into a physical-device
or latency claim.

**Changed:** Added `sos-linux-session run` as the single Rust lifecycle owner
for the direct session. It validates absolute executable/state/credential paths,
starts the compositor with `LIBSEAT_BACKEND=logind`, waits for Wayland/control
sockets and a page-flip readiness record, starts and bootstraps durable provider
authority, then starts the coordinated supervisor and permanent GPUI host. It
monitors every child, handles TERM/INT/HUP, shuts the supervisor and authority
down in order, reaps children, and fails the session if a component exits. On a
restart it removes only a Unix supervisor socket that first refused a connection;
a live listener or non-socket path remains fatal. GPU cache paths are explicitly
under `/var/lib/sos` because PAM intentionally supplies the login user's normal
home/runtime environment.

Added `packaging/systemd/sos-session.target`, `sos-session.service`, and the
`sysusers.d` declaration. The service conflicts with GDM/tty1 getty, opens a
`PAMName=login` session bound to `/dev/tty1`, creates private runtime/state
directories, requests logind rather than seatd, applies read-only system/home
and kernel hardening, and restarts after lifecycle failure. `pam_systemd` moves
the tree into active `session-N.scope`, so the Rust owner—not assumptions about
the now-empty service cgroup—owns child termination. The development VM uses its
existing `sos` account; creating the packaged system identity remains a
package-installation concern rather than evidence from this run.

The compositor and host now accept an exact bounded shell token from a file.
The packaged unit uses `LoadCredential=shell-token:/etc/sos/shell-token` and
passes `%d/shell-token`; the root source is mode `0400`. Shared parsing rejects
empty, newline-bearing, non-UTF-8, or greater-than-256-byte credentials. The
compositor writes an exclusive readiness file only after the first backend
presentation (`drm_page_flip` directly, backend submit when nested), so the
generated shell cannot race ahead of the recovery view. Direct page-flip logging
now keeps only recovery transitions and armed frames at info level and moves
unchanged flips to trace, avoiding unbounded journald noise.

Added host-side `tools/linux-vm/verify-boot-session`. It refuses non-virtual or
non-Debian-13 guests and any pre-existing product paths, synchronizes/builds the
current worktree, installs and verifies the disposable unit, seeds an immutable
boot revision, disables seatd, selects `sos-session.target`, and reboots. After
testing activation, host recovery, and a provider-triggered systemd restart, it
selects `graphical.target`, re-enables seatd, reboots into GDM, and deletes only
the exact unit, binary, credential, and state paths it previously proved absent.
The direct and boot verifiers were updated for the significant-page-flip log,
and the nested verifier's bounded supervisor wait was raised from 20 to 100
seconds so a clean GPUI rebuild is not mistaken for a runtime failure.

**Evidence:**
`SOS_LINUX_VM_GUEST_ROOT=/home/sos/sos-direct ./tools/linux-vm/verify-boot-session`
passes in the retained ARM64 Debian 13.6 KVM guest on kernel
`6.12.100+deb13-arm64`. seatd was disabled. logind reported
active Wayland session 1 on seat0/tty1 with lifecycle PID 770 as leader. The
boot host PID 883 activated revision
`552f06968bbc5c69de3db581454f60d4303289f304eaaf47a6e9dc3200297cdb`
without changing PID; boot and activation used commit/submit pairs 1/3 and
43/11. Killing PID 883 recovered the committed revision in PID 1089 with pair
50/19. Killing the exact provider made the lifecycle owner fail, systemd's
restart counter reach one, and a new lifecycle PID 1222 remove the refused stale
supervisor socket and boot host PID 1312 with another pair 1/3. Pointer and
authority matched after both recovery levels. The verifier confirmed the
credential source was `0400 root:root`, the token value appeared in none of the
lifecycle/compositor/supervisor/host command lines or environments, and the
host received only `/run/credentials/sos-session.service/shell-token`. It then
rebooted to GNOME with GDM and seatd active and left no installed SOS product
paths.

The updated SSH/seatd direct regression passes with activation PID 2656,
recovered PID 2794, and DRM pairs 1/3, 10/7, and 16/13. The ARM64 Ubuntu nested
regression passes with activation PID 1871851, recovered PID 1872036, fixed
compatibility placement, and nested pairs 1/936, 11/944, and 17/973. In the
Debian VM, `cargo test --workspace --all-features --lib --bins --tests --locked`
passes all 106 non-documentation tests and
`cargo clippy --workspace --all-features --all-targets --locked -- -D warnings`
passes. Formatting, ShellCheck, `git diff --check`, locked client/nested/direct
builds, and the unit's `systemd-analyze verify` pass. No VM image, journal, or
generated render artifact was added to Git.

**Failures and fixes:** A minimal transient PAM/tty1 probe first proved that
logind created an active seat0 session; a full transient compositor then opened
DRM/input through logind and produced a recovery VBlank. The first packaged run
was healthy but its verifier appeared stuck because every unchanged page flip
was logged at info and repeatedly scanning the growing journal was expensive;
transition/armed-only info logging fixed the operational issue. The next scan
looked under `_SYSTEMD_UNIT=sos-session.service`, but PAM correctly moved the
processes to `session-N.scope`; querying their inherited `sos-linux-session`
journal identifier fixed the evidence boundary. With shell `pipefail`, early
`grep -q` exits then surfaced journalctl's SIGPIPE as status 141; bounded full
consumption fixed that harness bug. The first service-restart assertion reused
the previous boot's ready line and raced the new compositor; comparing ready-line
counts now waits for the restarted host's own fence. PAM's protected-home setup
also exposed Mesa shader-cache warnings, fixed by setting child HOME/XDG cache
paths to the writable state directory. A clean nested rerun exceeded its old
20-second build-inclusive wait; the increased bounded wait passed without
changing runtime semantics.

An attempted `PrivatePIDs=yes` hardening was rejected. It would have made the
lifecycle owner PID 1 in a private namespace so the kernel could reap every
descendant after an uncatchable owner death, but systemd 257 failed at its
`NAMESPACE` exec step with this PAM/tty service before SOS ran. The passing unit
was restored. Component failure and graceful stop are proven; lifecycle-owner
`SIGKILL` recovery remains an explicit split-unit/reaper design gate.

An Android `cargo check --target aarch64-linux-android --release --locked
--no-default-features --features gate-strict` was attempted but could not reach
project code because this machine currently lacks `aarch64-linux-android-clang`
and the NDK sysroot; `psm` stopped in its build script with `ToolNotFound`. The
changed host fence is Linux-feature-gated, and the prior direct-slice Android
gate remains the latest successful Android evidence, but this run does not claim
a fresh Android regression.

**Decision:** The Debian VM boot-session ownership gate is complete. Keep one
PAM/logind session and one explicit Rust lifecycle owner; keep generated Luau
outside process, credential, seat, and DRM authority; and require the recovery
view's page flip before starting the permanent host. systemd credential delivery
is materially better than an inline token but does not yet establish mutually
distrusting same-UID services. VM VBlank remains functional evidence, not a
physical performance measurement.

**Open risks / next gate:** Render a deterministic compositor-owned cursor and
inject keyboard, pointer, and touch events through the booted VM, proving focus,
coordinates, touch lifecycle, and input quiescing across activation. Native
Linux text editing/IME, clipboard, accessibility, service-identity separation,
uncatchable lifecycle-owner recovery, hotplug, multiple outputs, XWayland,
suspend/resume, and physical GPU/touch performance remain open.

## 2026-08-09 — Repair the ARM64-host Android NDK compilation gate

**Goal:** Re-run the Android regression that the boot-session slice could not
reach, repair the local NDK path rather than relying on the earlier Android
result, and prove both target compilation and production of the packaged ARM64
library. The check must be usable without a phone or manually exported SDK,
NDK, or Java paths.

**Changed:** `tools/sosctl` now discovers the distro SDK at
`/usr/lib/android-sdk` and the active Linux Java installation when the Android
environment variables are unset. Added `./tools/sosctl m1-check`, which checks
the locked `sos-experience` Android release with `gate-strict` without requiring
ADB. On an ARM64 Linux host it uses native Clang/Clang++, `llvm-ar-18`, and the
NDK sysroot for C/C++ build scripts, then sends the final Rust link through the
existing Android linker wrapper. `m1-build` now calls the same helper so the
check and artifact paths cannot drift. The linker wrapper discovers the NDK's
Clang runtime-version directory instead of hard-coding version 21 and resolves
native Clang from `PATH` (or `SOS_ANDROID_CLANG`). The README records the new
device-independent command and why ARM64 Linux cannot execute the distributed
x86-64 NDK host binaries.

**Evidence:** With `ANDROID_HOME`, `ANDROID_SDK_ROOT`, `ANDROID_NDK_HOME`,
`ANDROID_NDK_ROOT`, and `JAVA_HOME` explicitly removed from the environment,
`./tools/sosctl m1-check` passes for `aarch64-linux-android` with the locked
release profile and `gate-strict`. A clean explicit cross-check compiled the
complete graph, including the formerly failing `psm` build script, in 11.09
seconds. A full locked release build then compiled and linked
`target/aarch64-linux-android/release/libsos_experience.so` in 36.12 seconds.
The result is a stripped 14,640,824-byte AArch64 ELF with SHA-256
`a6e124af13da86c3d765ba8dba9708b6fb9f30e37feb19dc0a77b3e4d1435379`;
its dynamic dependencies include Android `liblog`, `libandroid`, libc, and the
packaged `libc++_shared.so` rather than host libraries.

The same unset-environment invocation of `./tools/sosctl m1-build` identified
Rust 1.95.0, cargo-ndk 4.1.2, SDK `/usr/lib/android-sdk`, NDK r29
(`29.0.14206865`), Java 17, and the connected SM-A336B/API 35, then completed
Gradle `assembleDebug` successfully. The ignored
`artifacts/sos-experience.apk` is 36,849,469 bytes with SHA-256
`714d51afee7c133dc9676c473a3b3febd580b6446f2e342fcec27fa0f94123fa`.
The revision copy
`artifacts/sos-experience-4c966660fd93-dirty.apk` is byte-identical; the dirty
suffix records that the toolchain repair itself was not yet committed.
Archive inspection confirms its manifest, DEX, the exact 14,640,824-byte SOS
ARM64 library, and the 9,290,184-byte NDK `libc++_shared.so`. ShellCheck passes
for both changed scripts, and `apksigner verify --verbose` confirms one APK
Signature Scheme v2 signer. Generated JNI libraries, APKs, Gradle output, and
the writable SDK overlay remain ignored and were not added to Git.

**Failure and fix:** The earlier `ToolNotFound` was reported as a missing NDK
sysroot, but the NDK had become available at the system SDK path. The remaining
problem was discovery and execution: no Android variables pointed Cargo at it,
and Google's Linux NDK compiler is x86-64 while this workstation is ARM64.
Making distro SDK/JDK discovery explicit and applying the already-proven native
LLVM plus NDK-sysroot configuration to standalone checks fixed the failure. A
plain target Cargo invocation remains intentionally unsupported on this host
because it would again select a host compiler for C/C++ dependencies.

**Decision / next gate:** The current Linux changes have a fresh Android compile
and link regression, and the standard APK path is repaired. This is build
evidence, not new device-behavior evidence; no APK was installed or launched as
part of this gate. Resume the Linux input gate: deterministic compositor-owned
cursor rendering plus injected keyboard, pointer, and touch events across a
quiesced revision activation.

## 2026-08-09 — Route native Wayland editing and quiesce input before activation

**Goal:** Replace the Linux display-only text field with a native GPUI editing
session driven through the Wayland seat, and move input quiescing ahead of the
provider/state commit so no old-scene event can cross revision activation. The
functional gate must inject real nested-seat events and retain the existing
same-PID activation, compositor presentation, and crash-recovery guarantees. It
does not claim direct-libinput or physical-device behavior.

**Changed:** Added the Linux `NativeTextInput` entity and retained element with
focus, UTF-8/UTF-16 range conversion, grapheme cursor/deletion boundaries,
selection, marked text, bounded replacement, clipboard actions, mouse
selection, explicit Enter submission, and persisted `text_changed`,
`focus_changed`, and submit events. The compositor now advertises
`zwp_text_input_manager_v3`; ordinary keyboard character delivery remains the
native `wl_keyboard` path. Relative mouse motion now updates the same Smithay
pointer path as absolute tablet motion instead of being ignored.

Extended the host protocol with a revision-bound quiesce acknowledgement and
the compositor protocol with quiesce/resume requests and acknowledgements. The
coordinator order is now prepare candidate,
quiesce and await compositor acknowledgement, promote provider/state authority,
commit/present the retained VM, atomically advance the pointer, then clear the
journal. The compositor detaches keyboard/pointer focus, releases input the old
scene observed as pressed, intercepts backend events, suppresses the later
release of keys/buttons held across the boundary, and restores only a still-live
authenticated shell focus after the candidate frame is presented. The host
also clears its bounded queued-event and gesture epoch at acknowledgement.
Abort resumes the exact revision fence before candidate discard; control loss
clears quiescing without refocusing the stale shell. The supervisor test host
now rejects presentation unless the matching candidate was quiesced, so all
supervisor/coordinator integration tests enforce the new ordering.

`tools/linux-compositor/verify-nested` now uses XTest to inject through Xvfb,
outer Weston, the nested winit backend, Smithay, and the inner Wayland seat. It
establishes focus, injects F12 events across activation, requires a positive
compositor suppression count, then sends Ctrl+A, `wayland`, and Enter to the
autofocused candidate text session. It waits for both the exact durable draft
and submit state before continuing through host kill/recovery and compatibility
surface checks. Python and `libXtst` are harness-only; neither enters the SOS
runtime or product boundary.

**Evidence:** `./tools/linux-compositor/verify-nested` passed on the ARM64 Ubuntu
24.04 development host. Revision
`552f06968bbc5c69de3db581454f60d4303289f304eaaf47a6e9dc3200297cdb`
activated in unchanged PID 1961044, exactly 100 native keyboard events were
suppressed while the candidate fence was active, and the durable authority
recorded `draft="wayland"` plus the Enter-driven saved state. Killing that host
recovered the committed revision in newly authenticated PID 1961271 without
restarting the compositor. Boot, activation, and recovery used nested
commit/submit pairs 1/943, 17/954, and 63/1005; pointer and authority matched.

`cargo test --workspace --lib --bins --tests --locked --no-fail-fast` passes all
101 default-feature tests. The Linux-host focused run adds the native UTF-16
editing, compositor-control, and provider/socket tests and passes all 10 in that
crate configuration. `cargo clippy --locked --all-targets -p sos-experience
--features linux-host -p sos-compositor -p revision-supervisor -p
compositor-control-protocol -p experience-host-protocol -- -D warnings`,
`cargo fmt --all -- --check`, ShellCheck, and `git diff --check` pass.
`./tools/sosctl m1-check` also passes the locked ARM64 Android release check in
0.59 seconds. No raw render, disk, or input-capture artifact was added to Git.

**Failures and fixes:** The first two injected runs proved compositor suppression
(110 and 92 events) and exact text replacement, but printable `x` spam continued
briefly after the fence and saturated the host's intentional 64-event queue.
The first run therefore delayed the exact replacement; switching the fence
stimulus to non-editing F12 retained native delivery without manufacturing an
application backlog. The second run exposed that Linux Return was not mapped to
the text session's submit action; an explicit scoped Enter binding fixed it.
Review also found that held keys could otherwise deliver an unmatched release
after resume, a control disconnect could briefly restore focus to a stale shell,
and discarding the worker before compositor resume made a resume failure
unretryable. Suppressed-key tracking, no-restore disconnect cleanup, and
resume-before-discard ordering fixed those cases. A local `--all-features`
compositor check could not start because this development host lacks the
`libudev.pc` and `libseat.pc` development packages; default/nested compilation
passes here, while the prior direct Debian VM run remains the latest direct
build/runtime evidence.

**Decision / next gate:** The native Linux keyboard editing and nested
compositor-owned activation-quiesce functional gate is complete. Do not promote
this to direct-libinput, touch, physical keyboard, or IME evidence. Next render a
deterministic compositor cursor and inject relative pointer plus touch lifecycles
through the direct booted Debian session, including held input across both
successful and aborted activation. Then attach an input-method-v2 process and
prove non-Latin preedit/commit; clipboard and accessibility remain separate
gates.

## 2026-08-09 — Add the direct cursor render path and native Wayland touch lifecycle

**Goal:** Implement the source side of the next direct-input gate without
claiming desktop-only checks as hardware evidence: render a compositor-owned
cursor in the DRM graph, forward libinput touch slots through `wl_touch`, and
make touch cancellation obey the existing revision activation fence.

**Changed:** The Smithay seat now advertises touch and the shared backend input
router maps absolute libinput/winit touch coordinates into output space. It
forwards each slot as `wl_touch` down, motion, up, and frame events; Smithay
retains the down target per slot, providing Wayland capture, and a backend
cancel terminates the whole active sequence. A small `TouchLifecycle` owns the
active/suppressed sets. Entering quiesce sends one client cancel for all active
contacts, moves their slots to suppression, ignores held motion/release across
both the successful-presentation and abort-resume paths, and prevents contacts
that begin while quiesced from leaking into the next input epoch. A fresh down
explicitly starts a new contact even when libinput reuses a prior slot.

The direct DRM renderer now prepends a cursor element above Space content. It
honors a live Wayland cursor surface and its hotspot when the client supplies
one, hides it on an explicit hidden request, and otherwise uploads a stable
18x24 black/white compositor-owned fallback. Cursor motion participates in the
existing frame/damage path and does not change the shell presentation-fence
identity.

**Evidence:** `cargo test --workspace --lib --bins --tests --locked
--no-fail-fast` passes all 104 default-feature tests. The compositor's 8 tests
include three production `TouchLifecycle` cases for held contacts, slot reuse,
and a contact wholly contained by quiesce.
`PKG_CONFIG_PATH=/tmp/sos-pkgconfig cargo check -p sos-compositor --locked
--all-features --all-targets` compiles the direct render/touch code, and the
same environment with `cargo clippy -p sos-compositor --locked --all-features
--all-targets -- -D warnings` passes. `cargo fmt --all -- --check` and
`git diff --check` pass. The temporary pkg-config files supplied compile
metadata only for runtime libraries already present on this ARM64 host; they
are outside the repository and are not runtime or device evidence.

**Failures and fixes:** The normal direct-feature check initially stopped in
`libudev-sys` and `libseat-sys` because this host has versioned runtime shared
objects but no development `.pc` files. Temporary compile-only pkg-config
metadata allowed Rust type checking and exposed no direct-path errors. An
all-feature test link still cannot resolve the unversioned `-lgbm`, `-lseat`,
`-ludev`, and `-linput` development symlinks, so only the default-feature tests
ran here. The first lifecycle tests duplicated set operations rather than
exercising production code; extracting `TouchLifecycle` made those tests cover
the actual quiesce decisions.

**Decision / remaining risk / next gate:** Cursor and compositor-side
`wl_touch` implementation are ready for the Debian direct-session campaign,
but this gate is not complete. Run VM input devices through direct libinput and
capture visible cursor, relative pointer, physical-keyboard-shaped, multi-slot
touch, successful activation, and aborted activation evidence. Upstream GPUI's
current Wayland client does not bind `wl_touch`; after the compositor campaign,
add that client/host seam and reuse the bounded Scene raw-pointer router for
multi-touch, capture, cancellation, and gesture arbitration. Core `wl_touch`
has no pressure event, so pressure must use tablet-v2 or another explicitly
bounded native channel when hardware exposes it. Physical touch, latency,
thermals, and target-device behavior remain unproven.

## 2026-08-09 — Complete direct libinput and the Linux `wl_touch` Scene adapter

**Goal:** Close the direct-input gate in the disposable VM, including real
libinput classification, multi-touch delivery to Scene, and devices held across
both successful and aborted activation. Physical-device testing is explicitly
waived because no Linux touch target is available.

**Changed:** Pinned the GPUI Linux Wayland client in `vendor/gpui-linux`, bound
`wl_touch`, and forwarded down/move/up/cancel with stable slot IDs and pointer
counts into the shared bounded multi-pointer/capture/gesture router. Added
one-time compositor input-class evidence and `tools/linux-vm/inject-uinput`,
which creates kernel keyboard, relative-pointer, and direct two-slot
touchscreen devices, supplies distinct coordinates and evdev pressure, holds a
modifier/button/two contacts, and removes all three devices cleanly. The direct
verifier repeats that lifecycle through ordinary presentation and an injected
provider `before_promotion` failure.

**Evidence:** `tools/linux-vm/verify-direct-session` passed in the Debian 13.6
ARM64/KVM guest. Libinput added/removed all three `SOS Gate` devices and the
compositor recorded keyboard, relative-pointer, pointer-button, and touch
classes. Both successful and aborted activation logged `keys=1 buttons=1
touches=2`; later releases for the key, button, slot 0, and slot 1 were
suppressed. Static input revision
`250b157308407df4ed48c8e45351e69a0d82534ba001b6ff214c6a3348c0a326`
activated in unchanged PID 8641 and recovered after host kill in PID 8888.
Boot, activation, and recovery all used `drm_page_flip` evidence.

**Failures and fixes:** The first two-contact run crashed at GPUI's `RefCell
already mutably borrowed`: touch dispatch called the raw callback and
`window.frame()` while holding the Wayland client borrow. Building an
event/window list under the borrow and dispatching afterward fixed it. Held F12
and an animated fixture could manufacture a render/repeat storm and starve the
five-second acknowledgement; a non-repeating modifier and checked-in static
fixture isolate lifecycle behavior. The verifier's expected-failure command
also triggered its global `ERR` trap under `set +e`; an `if` condition retained
the assertion without aborting cleanup.

**Decision / remaining risk:** The VM direct-input gate is complete. Wayland
capture, cancellation, multi-touch, gesture routing, device hotplug, and held
success/abort behavior are proven through direct libinput. Core `wl_touch`
cannot carry pressure, so injected evdev pressure does not reach Scene; use
tablet-v2 or a bounded extension when required. Physical touch, panel latency,
vendor GPU, memory/thermal behavior, and actual-device rotation remain
unproven by explicit waiver.

## 2026-08-09 — Complete Linux IME, clipboard, and semantic accessibility

**Goal:** Turn native keyboard editing into a complete Linux text/semantic
slice and preserve it across activation and host recovery.

**Changed:** Added `sos-input-method` as the only executable admitted to the
compositor's input-method-v2 global. It implements bounded deterministic pinyin
composition/candidates, keyboard grab, candidate selection, CJK commit, dead
acute composition, popup rendering, and cursor rectangles. GPUI copy/cut/paste
now owns/consumes Wayland data sources/offers. Added an SOS-owned Unix semantic
service with snapshots/waits, hierarchy/focus traversal, activation, scrolling,
editable value/UTF-16 selection, clipboard actions, live status generations,
and refused stale-socket recovery.

**Evidence:** `tools/linux-compositor/verify-nested` observed cursor rectangle
`242,268 0x28`, pinyin candidates `["你好","你号"]`, selected/committed
`你号`, committed a dead acute sequence, and proved 7-byte Wayland copy, cut,
and paste ownership. The semantic snapshot exposed editable `note-draft`,
accepted focus/selection/copy, advanced generation, and returned after exact
host `SIGKILL` recovery. Three input-method tests and Linux host tests cover
composition cancellation, dead keys, candidates, multilingual UTF-16 offsets,
and semantic hierarchy.

**Failures and fixes:** Clipboard state changed correctly but the evidence grep
raced buffered stdout; operation evidence now uses stderr. The input-method
global originally admitted arbitrary clients; it now checks
`/proc/<peer-pid>/exe` for exact basename `sos-input-method`.

**Decision:** The prototype has an actual Linux IME, clipboard, and
automation-facing semantic service. It is SOS-owned rather than an AT-SPI
desktop bridge, which is intentional for the boot-owned environment.

## 2026-08-09 — Replace the Linux fixture model with capability-scoped providers

**Goal:** Supply real Linux data, live updates, and generated read/write actions
without allowing a candidate to inherit the accepted revision's authority.

**Changed:** Added Markdown notes, iCalendar, JSON/MPRIS music, network,
battery, and AC adapters. The host fingerprints the provider root every 250 ms
and rerenders the accepted VM without a revision install. Runtime effects now
allow `notes.write`, `calendar.append`, and `music.command` alongside durable
`notes.attach_to_event`. Writes use safe descendants and atomic rename; errors
distinguish denial, cancellation, invalid path, temporary unavailability, I/O,
and JSON failures.

Private manifests are at most 64 KiB and keyed by exact revision; wildcard
grants require `SOS_PROVIDER_DEVELOPMENT_GRANTS=1`. Candidate preparation loads
that candidate's snapshot before render. Commit switches subscription context,
and stale prior-revision events are dropped. Real effects are validated after
durable staging; capability/effect failure aborts before state promotion.

**Evidence:** Five provider tests prove three resource domains, live generation,
scoped/wildcard grants, denial/cancellation, path escape rejection, and typed
note/calendar writes. The Linux host integration combines durable attach with
an actual generated Markdown write and survives service restart. The nested
campaign mutates `notes/demo.md`, observes provider event/model refresh, and
proves the revision-frame count does not change.

**Failure and fix:** The initial adapter retained boot-revision grants for the
process lifetime. Candidate-specific snapshots, active-revision watcher state,
and revision-tagged events close that data/capability leak.

**Decision / remaining risk:** Real reads, live subscriptions, typed writes/
commands, cancellation/unavailability, and per-revision capabilities are in
place. File/calendar writes are idempotent by target name. MPRIS still needs
provider-owned durable idempotency receipts for production exactly-once crash
semantics.

## 2026-08-09 — Add user-operable recovery, lifecycle handling, and prototype security

**Goal:** Replace the solid fallback with permanent recovery controls, close VM
lifecycle behavior, and enforce a minimum authenticated revision/provider
boundary before real data is enabled.

**Changed:** The direct compositor now renders a 720x420 bitmap-font recovery
panel with current/previous revision, failure, progress, safe/provider flags,
and restart/rollback/safe-mode/provider-disable hit targets. The lifecycle
owner binds its datagram action channel, atomically publishes status, restarts
the host, coordinates rollback through provider authority, and persists flags.
`RevisionStore::set_current` maintains `previous`; the supervisor has explicit
restart control.

DRM rescans connectors on change, unmaps/reconnects its single output, and
supports configured mode, scale, and rotation. Existing libseat pause/activate
suspends/reactivates libinput and DRM managers. The systemd unit adds empty
capabilities, no-new-privileges, system/home/kernel protections, restricted
address families/syscalls, CPU/memory/task/FD limits, and accounting. Optional
HMAC-SHA256 detached manifests authenticate revisions using private bounded key
files.

**Evidence:** The final `tools/linux-vm/verify-boot-session` activated
`552f0696…` in PID 880, sent two recovery-socket rollbacks to move to the
previous revision and back in that same PID, recovered a host kill in PID 1237,
killed the provider to restart lifecycle PID 769 as 1377, booted host PID 1467
on durable authority, and restored GNOME/seatd. All five revision presentations
and the restarted session used DRM page flips. Recovery buttons and a standard
HMAC vector have unit tests; `systemd-analyze verify` passes in the campaign.

**Failures and fixes:** The strengthened campaign found `recovery.json` was
only published at boot/action and stale after ordinary activation. The owner
now detects current/previous/flag changes and republishes atomically. The first
verifier wait sampled the prior record; it now waits for the exact pair before
invoking rollback.

**Decision / remaining risk:** Recovery is user-operable, child/host failures
recover, input hotplug is exercised, and connector/suspend code is present. The
processes still share Unix UID `sos` despite separate PIDs/executables/protocol
identities and scoped credentials; mutually distrusting users remain production
hardening. HMAC is prototype authentication, not public-key update
distribution. Lifecycle-owner `SIGKILL`, output hotplug, real suspend/resume,
target GPU/touch, latency, memory pressure, and thermals are not claimed.

## 2026-08-09 — Linux integration prototype envelope decision

**Evidence:** The final matrix is `cargo test --workspace --lib --bins --tests
--locked --no-fail-fast`, strict workspace/all-target clippy, direct all-feature
compositor check/clippy with compile-only pkg-config metadata, `cargo fmt --all
-- --check`, ShellCheck, `git diff --check`, and passing nested, direct, and
boot-owned campaigns. Full workspace doctests additionally encounter two
pre-existing `vendor/gpui-mobile` examples that omit imports/types; ordinary
unit/integration targets are the documented green matrix.

**Decision:** The Linux integration prototype is complete within the explicit
single-display/virtual-device scope. Executable WGSL, video/protected/camera
surfaces, simultaneous multiple outputs, XWayland, and arbitrary legacy apps
remain subsequent milestones. With no Linux touch target and the user's device
waiver, do not promote VM evidence to physical touch, GPU, latency, suspend,
memory, or thermal evidence. The next credible gate requires target hardware.

## 2026-08-09 — Separate Linux service identities and recover lifecycle-owner death

**Goal:** Remove the shared development UID and make an uncatchable lifecycle-
owner death recover without accepting an abandoned PAM process tree.

**Changed:** The boot session now has distinct `sos-compositor`, `sos-provider`,
`sos-supervisor`, and `sos-host` Unix identities in shared `sos-ipc` group. The
PAM/logind session uses the compositor identity because logind authorizes DRM/
libinput to the active session UID. Only the lifecycle owner retains bounded
`CHOWN`, `SETUID`, `SETGID`, and `KILL` capabilities; each role child clears all
effective, permitted, inheritable, and ambient capabilities before exec.
Compositor and host receive separate owner-only runtime credential copies and
caches. A peer-credential-checked broker launches the host under its own UID
while preserving the supervisor's line protocol. Provider and supervisor set
permissions and recover sockets they own.

The lifecycle owner persists exact executable, PID, and `/proc` start-time
records. A replacement validates that registry, terminates only matching SOS
processes, and records whether it reaped survivors or logind/kernel cleanup had
already removed them. Supervisor readiness now requires a fresh socket inode,
not the abandoned path.

**Evidence:** `tools/linux-vm/verify-boot-session` passed on Debian 13 ARM64 KVM
with session 1, initial owner PID 768, same-process activation PID 882, isolated
host recovery PID 1380, provider-failure owner PID 1480/host PID 1549, and
lifecycle-owner-`SIGKILL` recovery PID 1724. `NRestarts=2`; revision
`552f0696…` and provider authority survived both full-session restarts, and all
accepted presentations used DRM page flips. The campaign asserted each Unix
owner, zero `CapEff` for compositor/provider/supervisor/host children, private
credential ownership, disappearance of every old-tree PID, and restoration of
GNOME/seatd. `cargo test -p revision-supervisor -p provider-state-service
--locked`, Linux-session tests, strict targeted clippy, ShellCheck, and
`git diff --check` pass.

**Failures and fixes:** A supervisor account could not traverse the controller's
`0700` home, so immutable inputs are staged in a supervisor-owned directory.
Logind rejected a compositor UID different from the PAM seat owner; the session
therefore uses the compositor identity. Ambient lifecycle capabilities leaked
to same-UID children until pre-exec capability clearing was added. The first
host broker buffered the line protocol and serialized accept handling; explicit
line flushing and per-connection workers fixed timeout and shutdown hangs.
Provider/supervisor socket chmod and stale unlink initially crossed ownership
boundaries; each daemon now owns those operations. A stale supervisor pathname
also produced false readiness; inode replacement closes that race.

**Decision / next gate:** The minimum prototype identity boundary and lifecycle-
owner crash gate are complete in the VM. This supersedes the shared-UID and
owner-`SIGKILL` risks in the preceding entries. Public-key update signing and
production sandbox policy remain later hardening. Continue with actual suspend/
resume and display/input hotplug lifecycle evidence; the overall envelope is
not complete yet.

## 2026-08-09 — Boot-session pause/resume and DRM connector lifecycle gate

**Goal / hypothesis:** The packaged PAM/logind-owned compositor must remain the
same process while its seat is deactivated, Linux freezes/resumes the session,
and the active display connector disappears and returns. The direct backend
must also tolerate complete DRM-device udev removal/addition even though the
available QEMU display model may not expose that operation.

**Changed:** `crates/sos-compositor/src/direct.rs` now records the libseat pause
state and suppresses its repaint timer until DRM managers have reactivated.
The direct backend can remove a DRM device, unregister its calloop notifier,
unmap its outputs, wait with no output, and construct a newly added DRM device
without restarting. `tools/linux-vm/start`, `qmp`, and `verify-boot-session`
provide deterministic VM control and a packaged-session lifecycle campaign.
The verifier switches away from tty1 to exercise the production libseat pause
boundary, runs Linux `pm_test=freezer`, returns to tty1, and writes `off`/`on`
to the virtio DRM connector status with a card uevent. It asserts pause,
activation, connector disconnect/reinitialization, and an unchanged systemd
MainPID before continuing the full activation/recovery campaign. Developer
`linux-run` now uses a 30-second cold-start host deadline (overridable through
`SOS_LINUX_HOST_TIMEOUT_MS`) rather than the brittle five-second daemon default.

**Evidence:**

- `PKG_CONFIG_PATH=/tmp/sos-pkgconfig cargo check -p sos-compositor --features direct-backend --all-targets --locked`
- `PKG_CONFIG_PATH=/tmp/sos-pkgconfig cargo clippy -p sos-compositor --features direct-backend --all-targets --locked -- -D warnings`
- `shellcheck tools/sosctl tools/linux-vm/start tools/linux-vm/stop tools/linux-vm/verify-boot-session tools/linux-vm/verify-direct-session tools/linux-vm/verify-lifecycle`
- `tools/linux-vm/verify-boot-session` completed with
  `linux_boot_session_passed session=1 main_pid=777 activation_pid=893 recovered_host_pid=1444 restarted_main_pid=1546 restarted_host_pid=1605 lifecycle_recovered_main_pid=1774 revision_id=552f06968bbc5c69de3db581454f60d4303289f304eaaf47a6e9dc3200297cdb nrestarts=2 identities=separated evidence=drm_page_flip`.
- The boot recorded `PM: suspend entry (s2idle)`, `PM: suspend debug: Waiting
  for 5 second(s).`, `PM: suspend exit`, `direct session paused`, `direct
  session activated`, `disconnected DRM output`, and a later `initialized
  direct KMS output` while the asserted lifecycle MainPID was unchanged.

**Failures / fixes / rejected evidence:** A raw `systemctl suspend` reached
`s2idle`, but this aarch64 QEMU `virt` machine has no supported platform wake
source; QMP explicitly returned `wake-up from suspend is not supported by this
guest`. An RTC alarm was also unsupported. `pm_test=devices` did return from the
kernel but QEMU's virtio-net receive path did not recover, so it is retained as
a VM-driver failure rather than passing evidence. The first production-ordered
pause exposed a real SOS bug: the periodic repaint ran against a paused DRM
surface, received `Device is currently paused`, and stopped the compositor.
Suppressing repaint while `session_paused` fixed it. Connector status writes
alone did not reach the udev DRM monitor; emitting `change` on the DRM card made
the disconnect/reconnect deterministic. Complete QMP GPU removal was rejected
by QEMU (`virtio-gpu-pci does not support hotplugging`), even behind a PCIe root
port, so only the code path is compiled here; it is not claimed as VM evidence.

**Decision / remaining risk / next gate:** Linux-owned pause/resume and output
connector hotplug are prototype-gated in the VM, and input add/remove remains
covered by the uinput direct-session campaign. Full platform sleep, DRM-device
replacement, mode/rotation changes on real KMS hardware, and thermal behavior
still require suitable hardware; none are relabeled as physical evidence.
Proceed to native pressure transport and the remaining render/surface
compatibility work.

## 2026-08-09 — Standard tablet-v2 pressure reaches Linux Scene input

**Goal:** Preserve native pressure where libinput exposes it instead of mapping
every Linux pointer sample to pressure `1.0`, while retaining the existing
multi-touch capture/cancel and activation-boundary behavior.

**Changed:** The compositor advertises `zwp_tablet_manager_v2`, creates tablet
and tool objects from libinput descriptors, and forwards proximity, tip,
motion, button, and normalized pressure/axis frames to the focused shell
surface. The vendored Linux GPUI platform binds tablet-v2, batches tool motion
and pressure until protocol `frame`, converts tool down/move/up/cancel into its
bounded raw-pointer callback, and marks ordinary `wl_touch` pressure as
unavailable. The Scene router now uses the supplied Linux pressure when present
and retains its `1.0` fallback only for standard `wl_touch`. The uinput gate
creates an independent pressure stylus with correct axis resolution and proves
nonzero normalized samples. Stylus input is sequenced after held touchscreen
contacts because libinput intentionally cancels touch for pen palm rejection.

**Evidence:** `tools/linux-vm/verify-direct-session` passed with activation PID
8236, recovered host PID 8533, revision `250b1573…`, and DRM page-flip evidence.
The asserted log included native keyboard, relative pointer/button, two active
touch slots across successful and aborted activation, tablet-pressure class,
and pressure values `0.621915…` and `0.874629…`; all four uinput devices were
then removed cleanly. Direct compositor and Linux-host check/clippy plus Python
syntax, ShellCheck, formatting, and `git diff --check` pass.

**Failure / fix:** The first stylus descriptor lacked X/Y resolution, so
libinput rejected it as missing tablet capabilities. `UI_ABS_SETUP` with
nonzero resolution fixed classification. Running the stylus concurrently with
the two-finger touchscreen caused two libinput `TouchCancel` events by design;
separating the pressure phase from the held-touch phase preserves both the palm
rejection behavior and the activation-boundary proof.

**Decision / risk / next gate:** Linux pressure is end-to-end through the
standard tablet-v2 path; finger pressure remains unavailable because neither
libinput touch events nor `wl_touch` define it. Actual pressure calibration and
touch/stylus coexistence remain physical-device work and are not claimed from
uinput. Continue with executable WGSL and specialized surface integration.

## 2026-08-09 — Revision WGSL becomes a bounded executable paint operation

**Goal:** Turn already packaged WGSL sidecars into a real Linux paint primitive
without granting a revision general GPU resource authority.

**Changed:** Scene ABI v3 now includes a bounded `shader` paint operation that
resolves only a shader declared by the same revision. Supervisor installation
and runtime activation both parse and validate WGSL with Naga, require
resource-free `vs_main` and `fs_main` entry points, and reject bindings and
compute. The Linux stable host caches execution results from a dedicated wgpu
device, draws one fullscreen triangle into a maximum 1024-by-1024 offscreen
RGBA target, reads the result into a GPUI `RenderImage`, and composites it with
the retained scene. Asset IDs, rather than arbitrary paths, remain the Luau
authority boundary.

**Evidence:** `cargo check -p sos-experience --features linux-host` passed.
`cargo test -p sos-experience --features linux-host
shader_paint::tests::executes_validated_fragment_shader_into_rgba_pixels`
executed the fragment program under Mesa software rendering and asserted all
256 output bytes for an 8-by-8 target. The runtime sidecar/paint decode test and
the supervisor content-addressed sidecar installation test also pass.

**Failure / fix:** The first pixel assertion treated the fragment's linear
green value as an unencoded byte; the `Rgba8UnormSrgb` target correctly encoded
linear `0.25` near byte 137. The test now asserts the sRGB result. Mesa also
reported and rejected an unavailable Freedreno render node before wgpu selected
a working fallback; that diagnostic is not counted as hardware-GPU evidence.

**Decision / risk / next gate:** Static resource-free custom shaders are now
executable without buffers, textures, network, or compute authority. Animated
uniforms and sampled revision images are intentionally absent from this first
safe contract. Proceed to explicit video/camera/protected-surface integration.

## 2026-08-09 — Simultaneous KMS outputs and bounded rootless XWayland

**Goal:** Close the optional display/legacy compatibility slice without
granting generated revisions raw window-system authority.

**Changed:** The direct compositor no longer rejects a second DRM device or
connected output. It owns a DRM manager/renderer/output set per device, maps
sorted outputs horizontally, updates aggregate shell geometry on every
connect/disconnect, and supplies a one-frame damage marker when a new head is
outside the shell's prior acknowledged size. QEMU exposes two VirtIO heads.
The compositor can also start an explicitly enabled rootless XWayland server,
publish its private display number, and map at most eight X11 windows under
bounded location/size/configure policy. X11 and XDG compatibility focus share
the compositor's trusted input policy.

**Evidence:** `tools/linux-vm/verify-direct-session` enabled Virtual-2, required
`empty=false` initial damage and a Virtual-2 page flip, disabled it again, and
completed activation/recovery with revision `250b1573…` in host PIDs 8324 and
8629 using only `drm_page_flip` evidence. The nested campaign started the real
`Xwayland` binary, mapped `xmessage`, and passed with activation PID 2237187,
recovery PID 2237457, and `nested_backend_submit` evidence. Direct/all-target
check and strict clippy passed with the compile-only pkg-config metadata.

**Failures / fixes / rejected evidence:** Smithay's validation commit left the
new head with no first damage when the existing shell still covered only head
one; resetting its swapchain and drawing the bounded initial marker produced a
real flip. Combining Virtual-2 hot-add after the kernel freezer or primary-head
reconnect was rejected: QEMU initialized head two but withheld its vblank in
those orderings. The independent direct multi-output gate is retained; the boot
campaign owns suspend and primary reconnect evidence.

**Decision / remaining risk:** Simultaneous outputs and bounded XWayland are
implemented at VM prototype scope. Physical multi-panel timing, another real
DRM device, arbitrary legacy applications, and production X11 isolation remain
hardware/compatibility work rather than claims from this gate.

## 2026-08-09 — Specialized surfaces and the final Linux platform adapters

**Goal:** Close the remaining product-visible media/system and lifecycle
contracts, then exercise them with the existing IME, accessibility, recovery,
activation, and isolation envelope.

**Changed:** Scene ABI v3 gained declared `video`/`camera` provider surfaces and
a `provider_surface` content facet. The Linux provider accepts bounded atomic
PNG/JPEG/WebP frames under separate video/camera grants, hashes them into the
host asset source, and pushes replacements through the live provider event
stream. Protected surfaces require their own grant but always report
`protected_unavailable`; bytes are never mapped because SOS has no secure
scanout path. Android renders an explicit unavailable placeholder.

The real model now also carries capability-scoped Unix time/timezone, online
interfaces, PipeWire default-sink volume/mute, battery/AC, connected DRM heads,
and input event devices. Their stable state participates in provider generation
fingerprints while wall-clock movement alone does not force a 4 Hz rerender.
The provider authority isolates a disconnected client write failure instead of
exiting. The direct compositor gained a 4 KiB, strict JSON output configuration
file and recreates KMS outputs on udev change when mode, rotation, or scale
changes. The semantic verifier now covers next/previous, activation, scroll,
focus, selection, and accessibility-originated copy/cut/paste.

**Evidence:** Seven `providers-linux` tests cover resource reads/actions,
grants, cancellation, paths, system parsing, ready/protected surfaces, and live
generation. The provider daemon test deliberately abandons a request before
its response, then completes a durable promotion and clean shutdown. The final
nested campaign generated a real 160x90 GStreamer test frame, rendered it,
atomically replaced it with a different frame/SHA-256 without revision
activation, exercised the full semantic/IME/clipboard path and XWayland, then
recovered the host. It passed at revision `b4718a74…` in PIDs 2294261/2294703
with three `nested_backend_submit` fences.

The direct VM campaign changed Virtual-1 live to 1024x768, scale 1.25, rotation
180, page-flipped Virtual-2, transported keyboard/pointer/two-touch/tablet
pressure across successful and aborted activation, and passed at revision
`250b1573…` in PIDs 2959/3268. The boot campaign then passed kernel freezer
suspend/resume, connector reconnect, two recovery rollbacks, host/provider/
lifecycle-owner failure, clean restoration, separated identities, and revision
`b4718a74…` in owner PID 771 and host PIDs 985/1607/1770; replacement owner PID
1945 retained durable authority. Every accepted direct presentation used
`drm_page_flip` evidence and systemd reported two service restarts.

**Failures / fixes:** The first live-frame verifier observed an older provider
refresh and checked before the replacement arrived; comparing the before/after
content hash made the gate causal. A semantic `cut` initially updated the host
recursively while GPUI was rendering it, causing a recoverable host panic.
Deferring native editor operations until the current GPUI effect cycle returns
removed the reentrancy, and the complete campaign then passed. The XWayland
campaign also exposed a disconnected provider client taking down authority;
scoping response-write errors to that connection fixed it and gained a daemon
regression.

**Decision / remaining risk:** All requested Linux prototype adapters are now
implemented and integrated. Provider media is a functional frame boundary, not
zero-copy decode/capture or protected playback. Physical touch calibration,
real GPU/panel latency, full platform sleep/wake, physical hotplug, memory
pressure, and thermal measurements remain unclaimed; physical touch verification
is explicitly waived because no device is available. Production asymmetric
signing, MAC policy, and broad legacy compatibility remain subsequent work.

## 2026-08-09 — Final Linux envelope regression matrix

**Goal:** Recheck the complete shared graph and both platform adapters after all
Linux-envelope changes, rather than accepting only focused campaign results.

**Evidence:** `cargo test --workspace --lib --bins --tests --locked
--no-fail-fast` passed every ordinary unit/integration target. The Linux-host
feature suite passed 12 tests, including real provider-state restart and actual
8x8 WGSL pixel execution. Strict workspace/all-target clippy, Linux-host
all-target clippy, and direct-compositor all-target clippy with
`PKG_CONFIG_PATH=/tmp/sos-pkgconfig` all passed with `-D warnings`.
`./tools/sosctl m1-check` cross-checked the shared ABI/Android fallback for
ARM64 Android. `cargo fmt --all -- --check`, ShellCheck for every changed shell
tool, Python bytecode validation for `qmp`/`inject-uinput`, and `git diff
--check` passed. The final nested, direct, and boot campaigns are the exact
passing runs recorded in the preceding entry.

**Failures / fixes / rejected evidence:** Strict clippy caught a collapsible
provider-surface validation match, unit-valued deferred calls, and an overly
complex shader-cache type; all were simplified. The Android cross-check caught
new model-refresh worker variants missing from its exhaustive result handler
and Linux-only shader/provider symbols producing target warnings. Android now
accepts model refresh results and renders provider surfaces as explicitly
unavailable, while Linux-only registries are target-gated. A local
`systemd-analyze verify` attempt was rejected because the development host
cannot read an unrelated system unit and does not install SOS under
`/usr/local/libexec`; the boot campaign's verify runs inside the disposable VM
after installing the exact unit and binaries and passed there.

**Decision:** The source, functional campaigns, strict lint, cross-platform
compile boundary, and chronological documentation agree. The Linux integration
prototype is complete under the explicit virtual-device scope and physical
touch-device waiver; no desktop/VM result is promoted to physical latency,
thermal, GPU-performance, or touch evidence.

## 2026-08-09 — Bounded resident Pi agent reaches transactional Linux activation

**Goal:** Establish the first honest agent-to-running-experience path: a user
prompt enters a resident model loop, produces complete Luau, and changes the
accepted Linux revision without giving the model shell, arbitrary filesystem,
or supervisor identity.

**Changed:** Added the Node service under `services/sos-agent`, pinned
`@earendil-works/pi-agent-core` and `@earendil-works/pi-ai` to 0.84.1, and
defined only `get_experience_context`, `validate_experience`, and
`submit_experience`. Added `sos-agent-authoring`, a Rust Unix broker that checks
`SO_PEERCRED`, bounds requests, preserves/migrates durable state, compiles and
renders candidates against the fake-provider snapshot, installs a
content-addressed revision, stages provider authority, and invokes the existing
coordinated supervisor. Pi runs under a separate identity and never receives a
revision-store path. Added developer commands, an exact faux-provider stack
gate, systemd/sysusers packaging, credential loading, and the live procedure in
`docs/sos-agent.md`. `docs/runtime-evaluation.md` records why low-level Pi was
selected over Sloppy and Prime Agent.

**Evidence:** `npm run check && npm test` passed the strict TypeScript check and
the Unix-socket Pi event-loop test, which observed the exact context, validate,
submit order and a completed streamed response. `cargo test -p
sos-linux-session authoring -- --nocapture` passed candidate acceptance and
invalid-render rejection. `./tools/sosctl linux-agent-test` built the locked
Rust/Node stack and drove Pi's faux provider through the public agent socket,
the authenticated authoring broker, real provider authority, staging, and
coordinated supervisor. It changed the accepted revision from
`f174e7261e67de25eaf84bf101aa93f62d61a2d6a906af2fb2eac8af0f8652c6` to
`b4718a74acad7caba8e7c7f72ed75ed6fa516d844aab5bfee788ab21f168588b`.
The full `tools/linux-vm/verify-boot-session` campaign then installed the agent
as separate `sos-agent`/`sos-supervisor` services and issued the same bounded
faux prompt after boot. The direct KMS host stayed at PID 913 while the agent
changed revision `f174e726…` to `d3a76a46…`; the compositor logged the matching
armed shell presentation with `evidence="drm_page_flip"`. Agent PID 856 and
broker PID 848 had the intended separate identities. The rest of the boot
recovery campaign still passed through final revision `b4718a74…`, replacement
owner PID 1967, and two service restarts. Debian provisioning downloaded
`node-v24.18.0-linux-arm64.tar.xz` (Node revision 24.18.0, 30,473,480 bytes,
SHA-256 `58c9520501f6ae2b52d5b210444e24b9d0c029a58c5011b797bc1fe7105886f6`)
and verified it before installation.
The complete `sos-linux-session` and `revision-supervisor` all-target suites,
strict `sos-linux-session` clippy, ShellCheck, Rust formatting, sysusers dry-run,
and `git diff --check` also passed.

**Failures / fixes / rejected evidence:** The first invalid-render fixture
returned an empty table, which is a valid default scene; a scalar result now
exercises the decoder rejection. The first exact stack run completed activation
but its cleanup could not delete immutable revision files, and a log-string
assertion did not match the supervisor display form. Cleanup now makes only its
validated `mktemp` root writable before removal, and the causal assertion uses
the supervisor's active revision. A shared writable `/run/sos` broker socket
was rejected during review; packaging now gives the broker a
supervisor-owned, group-traversable runtime directory the agent cannot modify.
The first image provisioning run exposed Cargo applying the added bin selector
to every package in the existing multi-package command; building the broker in
a separate locked invocation fixed it. The first boot prompt then raced
systemd's process-active state before the supervisor socket existed. Requiring
the existing direct-page-flip ready marker and the supervisor socket removed the
race. No Wayland socket was available in the outer development environment; the
visible evidence therefore comes only from the disposable direct-KMS VM and is
not a physical-display claim.

**Decision / remaining risk / next gate:** Pi is integrated for the first
bounded Linux test and the real SOS transaction changes revision under
deterministic orchestration. The next required evidence is the documented
credentialed live-model prompt in the booted distro. A trusted GPUI conversation
surface, visual screenshot feedback/repair, asset generation, transcript
compaction, and physical hardware behavior remain open. No desktop/VM result is
promoted to physical latency, GPU, thermal, or input evidence.

## 2026-08-09 — Pi becomes a Luau capability and passes the booted UI path

**Goal / hypothesis:** Make the resident Pi loop usable from an on-the-fly
experience without turning it into a fixed native GPUI panel. The hypothesis
was that Luau can own the visible conversation and composer while a narrow host
capability safely transports prompts and typed stream updates.

**Changed:** Scene model v3 now includes bounded `model.agent` availability,
activity, error, and user/assistant messages. Luau may emit one typed
`agent.prompt`; the Linux host commits the interaction state first, then bridges
that effect to Pi's Unix protocol and feeds text/tool/completion events back
through ordinary model refreshes. The default, daily-flow, and timeflow sources
render their own composer and conversation. Agent submission validation rejects
a replacement that removes every Luau `text_session` with
`submit_action = "agent_submit"`, so a generated experience may redesign the
entry surface but cannot strand the user. The system session passes only the
agent socket path to the isolated host. Linux semantic input gained event-driven
wake-up and a bounded text-session `submit` action for accessibility and UI
automation.

Activation now distinguishes immutable revision binding from mutable durable
interaction state: staging verifies revision ID, source digest, and schema, but
allows authority state to have evolved since installation. Pi validation and
installation use that current authoritative state. Provider refreshes preserve
agent state at ingestion, while queued agent refreshes retain their newer
conversation snapshot.

**Evidence:** `./tools/sosctl validate` accepted all three reference sources
with their Luau text sessions and semantic nodes. The runtime, Linux host, and
Linux-session suites passed 20, 13, and 4 tests respectively, including typed
`agent.prompt`, the real bounded Unix stream, authoritative effect commit,
composer-retention rejection, and the evolving-state activation regression.
The TypeScript check and Pi faux-provider test passed; Rust formatting,
ShellCheck, and `git diff --check` passed. The locked workspace
lib/bin/integration suite passed without failures, strict workspace and
Linux-host all-target Clippy passed with `-D warnings`, and
`./tools/sosctl m1-check` preserved the ARM64 Android compile boundary.

The complete `tools/linux-vm/verify-boot-session` campaign then entered “Turn
this into a spatial time flow” by setting and submitting the visible Luau
`agent-prompt`. It emitted:

`linux_boot_agent_luau_passed host_pid=906 initial_revision=b0d20599… active_revision=2cb2a047… agent_pid=848 broker_pid=836 input=text_session effect=agent.prompt tools=context,validate,submit evidence=drm_page_flip`

The assistant completion returned to the Luau semantic tree, and the same host
presented the agent-authored revision through a direct DRM page flip. The
remaining boot regression passed as
`linux_boot_session_passed session=1 main_pid=772 activation_pid=906
recovered_host_pid=1632 restarted_main_pid=1739 restarted_host_pid=1823
lifecycle_recovered_main_pid=2004 revision_id=93357d56… nrestarts=2
identities=separated evidence=drm_page_flip`.

**Failures / fixes / rejected evidence:** The first UI attempt queued semantic
input after GPUI became idle; delivering actions through an async host wake path
fixed it. The next attempt reached all Pi tools but submission failed because
staging treated legitimate composer state as revision corruption; separating
binding identity from evolving state fixed activation. A following run changed
the revision but an older provider refresh overwrote the queued assistant
completion; merging provider agent state only at ingestion preserved the latest
stream snapshot. One otherwise successful run used a systemd-unit journal
filter that omitted launcher descendants; the final gate uses the established
`sos-linux-session` identifier and requires the exact activated revision's page
flip. These failed runs are not counted as passing evidence.

**Decision / remaining risk / next gate:** The product boundary is Luau over a
typed Pi capability; GPUI remains the renderer and trusted transport owner, not
the conversation UI. The deterministic direct-KMS test is complete at virtual
hardware scope. The next gate is one credentialed live-model prompt entered in
the booted distro. Visual screenshot feedback/repair, asset generation,
transcript compaction, physical input/hardware behavior, latency, thermal, and
GPU measurements remain unverified.

## 2026-08-09 — Empty native text-field hit-test repair

**Goal / hypothesis:** Explain and repair the Linux experience-host panic seen
when clicking an empty agent prompt in a windowed Linux VM. The reported failure
was `end byte index 25 is out of bounds` for empty content at
`apps/experience/src/linux_input.rs:405` on revision `b0d20599…`.

**Changed:** Linux and Android native text input now constrain the shaped-line
hit-test index to the editable content length and a UTF-8 character boundary.
This prevents placeholder glyphs from creating a nonzero selection in empty
content. Added a Linux regression test for the observed placeholder index 25,
plus multibyte boundary and oversized-index cases.

**Evidence:** Source inspection showed that empty inputs shape their placeholder
but the Linux click path passed that display layout's index directly into the
content selection; the next GPUI UTF-16 selection query then sliced the empty
content at byte 25. The pinned GPUI input example already treats display and
editable offsets separately. The targeted regression passed as
`linux_input::tests::placeholder_layout_cannot_move_an_empty_input_selection`.
`cargo test -p sos-experience --features linux-host` passed all 14 tests;
`cargo clippy -p sos-experience --all-targets --features linux-host -- -D warnings`
passed; and `./tools/sosctl m1-check` passed the release ARM64 Android compile
boundary. The first targeted invocation used `--exact` without the module path
and selected zero tests; it was rejected and rerun with a matching selector.

**Decision / remaining risk / next gate:** Repair both native hosts because they
shared the faulty placeholder hit-test logic. These automated desktop and
cross-compile results do not establish VM interaction or Android hardware
behavior. The next gate is to rebuild/restart
`./tools/sosctl linux-run --windowed` on the macOS-hosted VM and click, type into,
select within, and clear the empty agent prompt without a host restart; physical
Android input remains a separate gate.

## 2026-08-09 — Pi-backed ChatGPT subscription authentication for SOS agent

**Goal / hypothesis:** Connect the resident SOS agent to OpenAI Codex through a
ChatGPT Plus/Pro subscription without introducing an SOS-owned OAuth protocol or
requiring an OpenAI API key. The hypothesis was that the pinned Pi 0.84.1
provider, login, refresh, and request-authentication APIs could remain the sole
provider implementation while SOS supplied only durable service-local storage
and a headless interaction adapter.

**Changed:** SOS now registers Pi's `openaiCodexProvider`, accepts the
`openai-codex` provider ID, calls `Models.login(..., "oauth", ...)`, and exposes
a `login --device-code` command. `./tools/sosctl linux-agent-login` drives that
flow and the resident runtime reads the same `.cache/linux-agent/auth.json`.
Added a mode-`0600`, atomic JSON `CredentialStore` with in-process queuing and a
process-visible mutation lock so Pi can persist rotated refresh credentials
without a lost update. The boot unit now uses
`/var/lib/sos-agent/auth.json` and no longer requires an API-key file; an
optional systemd drop-in retains the prior protected API-key path for `openai`
and `anthropic`. The reference faux-provider boot setup no longer creates a
dummy secret. Updated the live developer and booted-VM procedures in
`docs/sos-agent.md` and the README.

**Evidence:** `npm run check && npm test` passed four tests: atomic credential
persistence and `0600` permissions, secret-free credential listing/deletion,
serialized refresh-style updates across two store instances, Pi Codex provider
registration/model discovery, and the existing bounded faux-agent stream.
`shellcheck tools/sosctl tools/linux-vm/verify-boot-session` and `bash -n` for
both tools passed. `./tools/sosctl linux-agent-test` rebuilt the locked Rust and
Node stack, reported zero npm audit vulnerabilities, preserved the exact
`get_experience_context -> validate_experience -> submit_experience` tool order,
and activated revision
`93357d5643d00a4510a728743e04b0c35ce3439fb6a308c46b1c7f977a3ede84`
from
`b0d20599c81f62db31cfffd4883289e64a12ee9ada6f20a1c92ef518277e9be4`.
`git diff --check` passed.

**Failures / fixes / rejected evidence:** The first ShellCheck invocation ran
from `services/sos-agent` with repository-relative paths and therefore checked
nothing; it was rejected and rerun from the repository root. A local
`systemd-analyze verify` attempt was also rejected as packaging evidence because
the workstation lacks the packaged `/usr/local` executables and cannot read an
unrelated host unit. The existing disposable-VM verifier remains the valid unit
installation check and was updated, but was not rerun for this change. No OAuth
URL was requested and no live model call was made during automated tests, so the
provider factory test is not credentialed network evidence.

**Decision / remaining risk / next gate:** Reuse Pi for every provider-specific
step; SOS owns only credential durability and the user-facing/headless login
transport. The next gate is the intentionally manual live E2E: run device-code
authorization for the isolated `sos-agent` account in the booted distro, enter a
prompt through the Luau composer, observe Pi refresh/request authentication and
transactional revision activation, and record the model, revision IDs, service
logs, and DRM presentation evidence. This desktop/VM work makes no physical
hardware, latency, thermal, or GPU-performance claim.

## 2026-08-09 — Restart preserves evolved authority state and cleans up safely

**Goal / hypothesis:** Repair the live developer restart failure observed after
successful provider login. `linux-stop` followed by `linux-run --windowed`
reopened authority revision `b0d20599…`, but bootstrap rejected it as a
replacement by the identical revision and the EXIT trap then failed with
`supervisor_pid: unbound variable`. The hypothesis was that bootstrap was
incorrectly comparing mutable interaction state to the immutable installed
state even though the revision/schema/source binding still matched.

**Changed:** `bootstrap_authority` now treats a matching revision ID, schema,
and source digest as already bound while preserving the authority's newer
durable state. It still rejects an unexplained binding mismatch and still
requires an exact state match immediately after first initialization. The
developer runner's EXIT cleanup now reads task-specific PID slots that outlive
the `linux_run` function scope, rather than expanding local variables after Bash
has unwound the failed function.

**Evidence:** The authority integration test now commits a same-revision state
change, shuts down the provider process, reopens the durable authority file in a
new process, and receives `BootstrapOutcome::AlreadyBound`; it then continues to
stage the next revision and retains the mismatch/recovery checks. `cargo test -p
sos-linux-session --all-targets` passed four unit tests and the strengthened
integration test. Strict all-target Clippy, Rust formatting, ShellCheck, Bash
syntax validation, and `git diff --check` passed. The complete deterministic
`./tools/sosctl linux-agent-test` path also passed the bounded
`context -> validate -> submit` sequence and activated revision
`93357d5643d00a4510a728743e04b0c35ce3439fb6a308c46b1c7f977a3ede84`
from `b0d20599c81f62db31cfffd4883289e64a12ee9ada6f20a1c92ef518277e9be4`.

**Failures / fixes / rejected evidence:** The first trap repair intentionally
captured numeric PIDs when installing the trap, but ShellCheck rejected eager
trap expansion. Replacing that approach with persistent, narrowly named PID
slots made the lifetime explicit and passed strict checking. No Wayland socket
is attached to this worktree, so a local `linux-run --windowed` result was not
claimed; the durable-process restart test is the causal automated evidence.

**Decision / remaining risk / next gate:** Authority state may evolve within an
immutable revision binding and must survive ordinary service restarts. Retry the
original windowed stop/start sequence in the live VM, then start the
credentialed resident agent and issue one Luau-composer prompt. That VM result
will establish the developer restart and live-provider gate only, not physical
display, latency, thermal, or GPU behavior.

## 2026-08-09 — First credentialed subscription model changes the live experience

**Goal / hypothesis:** Exercise the newly wired Pi `openai-codex` subscription
path from the visible Luau composer after restarting the windowed VM session,
and determine whether a live model completion can author and render a new SOS
experience without an API key.

**Environment / evidence:** The user completed Pi's device-code login, ran the
windowed Linux session and resident agent, and entered “Let's add a table with
the running processes on this machine” through the Luau chat. The resulting
live frame shows a new “RUNNING PROCESSES” table above the preserved composer,
an assistant completion, and agent status `Ready`. The table truthfully reports
that process information is unavailable in the current provider snapshot rather
than inventing rows. The external evidence image
`Screenshot 2026-08-09 at 18.29.43.png` is 584,610 bytes, 2784x1904 RGBA PNG,
SHA-256 `35edc80055050c6e552027cfd295fa53c553d2f2aa0463d71f7634948e37ef60`;
the raw image remains outside Git.

**Decision / remaining risk / next gate:** The credentialed ChatGPT subscription
request, bounded authoring loop, completion return, and visible live redesign
work together in the VM. This is not evidence that SOS exposes arbitrary host
state: the resident author receives only the active source and typed provider
snapshot, and deliberately has no shell, process, filesystem, or general
network tool. The screenshot does not record the exact selected model, revision
IDs, service logs, or DRM page-flip evidence, so those remain unclaimed. If live
process data is a desired product capability, the next design gate is a trusted,
read-only, capability-scoped system/process provider with explicit schema,
redaction, refresh, and grant policy—not broader agent access.

## 2026-08-09 — Package SOS as a selectable GDM Wayland session

**Goal / hypothesis:** Make SOS available from the normal login screen as the
complete desktop GUI while preserving GNOME as a fallback. Reuse GDM's
authenticated PAM/logind session so the direct compositor owns the active seat,
without weakening or replacing the existing boot-owned multi-UID appliance
path.

**Changed:** Added `sos-linux-session run-user`, which rejects root and explicit
service-user overrides, requires every component identity to equal the current
effective UID/GID, and avoids unnecessary same-identity credential changes.
The isolated `run` command retains its four-distinct-UID requirement. Added the
GDM entry `packaging/wayland-sessions/sos.desktop`, the per-user
`packaging/libexec/sos-login-session` lifecycle/bootstrap launcher, and
`tools/install-linux-login-session`. The launcher creates private persistent
state and a shell token below `XDG_STATE_HOME`, allocates a new runtime directory
for every login, forces the logind libseat backend, and starts the direct
compositor as the authenticated user. A launcher-scoped
`SOS_ALLOW_SESSION_EXIT=1` enables `Ctrl+Alt+Backspace`; the compositor consumes
the chord, exits cleanly, and shared-user lifecycle mode treats that exit as a
logout rather than a failed appliance component. Updated the Linux host and
README documentation with the install procedure, identity boundary, escape
path, and evidence limits.

**Evidence:** `cargo test -p sos-compositor -p sos-linux-session --all-targets`
passes the compositor's prior nine tests, the Linux-session authority and
authoring suites, two new identity-policy tests, and the new scoped logout-chord
test. `cargo fmt --all -- --check`, `bash -n` and `shellcheck` on both new shell
files, `desktop-file-validate packaging/wayland-sessions/sos.desktop`, and
`git diff --check` pass. The exact release build from the installer advanced to
the direct-backend native dependencies, then failed in `libudev-sys` because
this development host has no `libudev.pc`; `pkg-config` also reports `gbm`,
`libinput`, and `libseat` absent. The installer now rejects these omissions
before starting a long Rust build and prints the Debian/Ubuntu package command;
an expected-failure installer invocation confirmed that preflight. The exact
five-binary `cargo build --locked --release` command then passed in 2 minutes 1
second in a source-only disposable `/tmp/sos-login-build.*` copy on the
provisioned Debian 13 ARM64 VM, where all seven required `pkg-config` modules are
present. The temporary tree and generated binaries were removed after the build
and are not retained as runtime evidence.

**Failures and fixes:** Reusing the appliance's distinct service UIDs from a GDM
login was rejected because logind grants DRM/input devices to the authenticated
session UID; a setuid launcher would have introduced an unnecessarily broad
privilege boundary. The first desktop entry included `DesktopNames`, which the
host's validator rejected, so the launcher now exports the SOS desktop variables
and the non-portable key was removed. ShellCheck could not infer trap callback
reachability; narrow annotations document those callbacks. The initial design
also lacked a path back to GDM, fixed with the selectable-session-only logout
chord and clean-exit handling. The first direct release-build attempt lacked an
early native-library check and failed late at `libudev-sys`; the new preflight
reports all missing direct-session modules and the packages that provide them.

**Decision / remaining risk / next gate:** Keep both modes: selectable GDM login
for reversible development and the systemd target for the isolated appliance.
This is desktop build/static packaging evidence, not an interactive GDM, DRM,
input, logout, suspend, latency, thermal, or physical-device result. The next
gate is to install it in the Debian reference VM, choose SOS through GDM, require
logind seat ownership plus a DRM page flip, inject the logout chord, confirm the
greeter returns, and then repeat on physical hardware without promoting VM
measurements to hardware evidence.

## 2026-08-09 — Start the resident agent with the selectable SOS login

**Goal / hypothesis:** A user who chooses SOS in GDM should receive the same
resident authoring interaction as the appliance session, with the agent ready in
the background and bound to that login's actual revision/provider authority.

**Changed:** Extended `tools/install-linux-login-session` to require Node
22.19+, build the locked TypeScript agent and release `sos-agent-authoring`
broker, install their runtime, API document, and two reference experiences, and
run a new per-user `sos-agent-login --if-needed` device-code helper. The helper
stores the `openai-codex` credential, exact model selection, and later message
history below `${XDG_STATE_HOME:-$HOME/.local/state}/sos/agent` with private
permissions. `sos-login-session` now gives the host a session-private agent
socket, waits for the provider and supervisor, starts the same-UID authoring
broker and resident Node agent, waits for both sockets, monitors all three
top-level lifecycles, and tears the background processes down on logout. If
credentials are absent, login refuses to start and names the repair helper; if
the agent or broker later exits, the session returns to GDM instead of silently
presenting a dead agent. Updated the README, Linux session guide, and agent guide
with configuration and ownership details.

**Evidence:** `bash -n` and `shellcheck` pass the installer, session launcher,
and new credential helper. `npm run check` and `npm test` pass all four agent
tests, including atomic credential persistence and the bounded
`context -> validate -> submit` flow. `cargo test -p sos-linux-session
--all-targets` passes six unit tests and the authority integration test. Rust
formatting and `git diff --check` pass. The exact release authoring-broker build
passes in 16.02 seconds, strict all-target Linux-session Clippy passes, and the
installer's exact `npm ci --ignore-scripts && npm run build` sequence installs
99 locked packages with zero reported vulnerabilities and compiles the agent.
An expected-failure installer invocation still stops before build/authentication
and lists this host's missing `gbm`, `libinput`, `libseat`, and `libudev`
development modules. Desktop-entry validation and the final combined
format/shell/diff checks pass. Generated release, `node_modules`, and `dist`
artifacts remain ignored and are not committed or used as runtime evidence.

**Failures and fixes:** Enabling the existing `sos-agent.target` was rejected:
its broker requires `sos-session.service` and hard-codes appliance paths under
`/var/lib/sos` and `/run/sos`, while every GDM login has a per-user revision
store and a fresh runtime directory. A system-wide agent would therefore point
at the wrong authority and cross the session ownership boundary. The first
provider override design also advertised API-key providers even though the new
helper deliberately drives a device-code flow; selectable-session login is now
restricted to `openai-codex`, with the existing credential/drop-in procedure
retained for appliance API-key configurations.

**Decision / remaining risk / next gate:** Keep the agent and broker as monitored
children of the authenticated graphical session, not as the incompatible
appliance system services. This proves builds, protocol tests, static lifecycle
ordering, and credential-path policy only. The next gate remains an interactive
GDM login in the Debian VM: require the DRM page flip, agent and broker sockets,
a successful composer prompt and activation, then inject logout and prove the
agent processes and private runtime paths disappear before GDM returns. No VM
result will complete the physical hardware, latency, suspend, thermal, or GPU
gate.

## 2026-08-09 — Direct SOS login completes a live experience rewrite in a local VM

**Goal / hypothesis:** Test the prototype as a user-visible end-to-end flow by
logging directly into SOS in a local VM hosted on a MacBook Pro, entering a
natural-language change through the experience itself, and observing the
resulting revision rather than driving the authoring path from a development
shell.

**Environment / evidence:** The user reports completing the direct SOS login in
the local VM and supplied a before/after capture. In the first frame, the live
single-column experience contains the request “i want the layout to be two
columns, the chat should be on the right col and the rest in the left one.” In
the second frame, SOS reports that it updated and activated the layout, the
dashboard content remains on the left, and the conversation and composer are
rendered in a new right column. The two external 3000x2122 RGBA PNG artifacts
remain outside Git:

- `Screenshot 2026-08-09 at 19.56.43.png`, user-supplied capture with no embedded
  source/build revision, 1,039,721 bytes, SHA-256
  `e61bd6a75f5c8dddeeed5bcac7524c37e98651c9e850371814cf6b9c72dc5790`;
- `Screenshot 2026-08-09 at 19.57.01.png`, user-supplied capture with no embedded
  source/build revision, 1,214,146 bytes, SHA-256
  `9f34a6dd8fcbd1b7beef98d8be965362354af4864ee43ccfc12b5110033ae1bc`.

**Decision / remaining risk / next gate:** Confirm the user-facing VM prototype
gate: a person can enter SOS directly, ask the resident experience to change its
own composition, and continue in the visibly activated result. This evidence
does not identify the exact model, source/build commit, old and new revision
IDs, activation latency, service logs, DRM page flips, or lifecycle cleanup;
the captures also do not independently show the GDM selection or logout path.
Repeat with timestamped supervisor/agent logs and revision IDs, exercise logout
back to GDM while checking process/runtime cleanup, and separately run the
physical-hardware, suspend, latency, thermal, and GPU gates.

## 2026-08-14 — Bootstrap a Fedora x86-64 host and smoke-test Android hardware

**Goal / hypothesis:** Prove that a newly provisioned Fedora 44 x86-64 machine
can reproduce the current strict Android build and render it on the physical
Samsung SM-A336B, while distinguishing a basic launch/input smoke test from the
existing hardware and latency gates.

**Environment changed:** The user expanded the Fedora root XFS logical volume
from 15 GiB to the full 928.9 GiB LVM physical volume. Fedora `android-tools`
37.0.0 and a targeted Samsung `04e8:6860` udev rule made ADB usable from the
remote `carlid` session. User-local tooling is JetBrains Runtime 21.0.11 through
SDKMAN, Rust/Cargo 1.97.1 through rustup, Android target
`aarch64-linux-android`, `cargo-ndk` 4.1.2, Android command-line tools 22.0,
Platform Tools 37.0.1, Platform 34 revision 3, Build Tools 34.0.0 and 36.0.0,
and NDK r29 (`29.0.14206865`). `~/.bashrc.d/android-sdk.sh` exports the
user-local SDK path. The device reported Android API 36 and
`arm64-v8a,armeabi-v7a,armeabi`.

**Evidence:** `./tools/sosctl doctor` passed with the physical phone.
`./tools/sosctl m1-check` completed the locked strict ARM64 Android release
check in 1 minute 4 seconds. `./tools/sosctl m1-build` linked the optimized
native host in 39.57 seconds and Gradle 9.4.1 assembled the APK in 38 seconds
under JBR 21. The clean source revision was `2a77c92fdc12`. Ignored artifact
`artifacts/sos-experience.apk` is 38,193,731 bytes with SHA-256
`e4233218ae9dd33ce77dfb016b7ea6a7904272e6a979528f608442e039d44a6f`;
APK Signature Scheme v2 verification passed with one signer, and archive
inspection found the SOS host, GPUI Mobile, and NDK C++ ARM64 libraries. After
provider startup and a cold relaunch, Android reported 917 ms total activity
launch time. PID 23607 logged remote provider snapshot, external state revision
0, Mali-G68 Vulkan selection, an 8,442 us runtime-worker initialization, and a
live SOS window. A controlled ADB-injected scroll on the physical device changed
the frame SHA-256 while retaining PID 23607 and the foreground activity, with no
panic, ANR, fatal exception, or script rejection. The visually inspected ignored
capture
`artifacts/sos-new-machine-launch-2026-08-14.png` is 179,208 bytes with SHA-256
`da2665e125f135fe5a98d040457c6c1e2c1fd2d27520477bf50fee4e698a83b0`.

**Failures and fixes:** The remote web/TTY login had no physical-seat ACL even
though udev tagged the phone for `uaccess`; assigning the specific Samsung USB
product to `wheel` fixed ADB's `no permissions`, after which the phone-side RSA
prompt fixed `unauthorized`. The phone contained an APK signed by the previous
machine, so Android could not replace it with the new debug signer. A verified
private-data backup was created first, then removed at the user's request before
the old app and its state were deliberately deleted. A standalone `apksigner`
invocation initially lacked Java in the non-interactive shell; initializing
SDKMAN fixed the invocation and signature verification passed. Gradle
automatically installed Build Tools 36.0.0 in addition to the repository's
doctor-required 34.0.0. Most importantly, the first strict launch selected
Vulkan but panicked at `apps/experience/src/android.rs:214` because no provider
snapshot service was listening. Starting locked `providerd` with ignored state
at `.cache/android-provider-state.json`, adding
`adb reverse tcp:47777 tcp:47777`, force-stopping the failed process, and cold
launching again fixed startup; the daemon logged two successful boot requests.

**Decision / remaining risk / next gate:** Confirm this new machine for a basic
strict-build, physical-device render, and scroll smoke test. Do not treat it as
a repeat of the lifecycle, soak, latency, thermal, or durable-state gates. The
documented `m1-run` path did not yet start `providerd` or establish its ADB
reverse mapping, so a fresh strict launch was not actually one-command
reproducible. The following entry addresses that developer-experience gate.

## 2026-08-14 — Make Android `m1-run` own its provider lifecycle

**Goal / hypothesis:** Make the documented strict Android run command complete
from a stopped state by building and starting the provider authority before the
APK, preserving it after `--no-follow`, and providing explicit, repeatable
cleanup without taking ownership of unrelated listeners.

**Code and environment changed:** `tools/sosctl` now builds locked
`providers-fake`, checks its snapshot protocol on TCP 47777, starts a managed
daemon with ignored state, PID, log, and unit metadata below `.cache/`, installs
the matching ADB reverse mapping, and unwinds only resources created by a
failed run. On a host with a user systemd manager, a transient user service
keeps the daemon alive after the invoking process exits; other hosts retain a
`nohup` fallback. A new idempotent `m1-stop` stops the local managed daemon even
when no phone is connected, and, when one is available, force-stops the app and
removes the reverse mapping. `README.md` documents the lifecycle.

**Evidence:** `bash -n tools/sosctl`, ShellCheck 0.11.0, `git diff --check`, and
`cargo fmt --all -- --check` passed. `cargo test --locked -p providers-fake
--bin providerd` passed both exactly-once/state-promotion tests. With an
independently started compatible provider on port 47777, `./tools/sosctl m1-run
--no-follow` exited 1 with an unmanaged-provider diagnostic and preserved that
process, the existing reverse mapping, and the installed app. From a stopped
state, the managed command completed in 6.61 seconds; after it returned, PID
83061 remained owned by the active user unit and served an independent snapshot
request, while the physical SM-A336B showed the app in the foreground with a
remote provider snapshot and a live window. A second invocation replaced PID
83061 with PID 83578, reused the mapping, and cold-launched the app. `m1-stop`
then removed the provider PID/unit metadata and reverse mapping and stopped the
app; invoking it again from that stopped state also succeeded.

The final acceptance command, `/usr/bin/time -p ./tools/sosctl m1-run
--no-follow`, completed in 6.26 seconds. After command exit, transient user unit
`sos-android-providerd-84497-21178.service` was active with provider PID 84838;
snapshot request 9001 returned `ok`, `adb reverse --list` contained the exact
47777 mapping, and Android reported app PID 25104 as the top resumed activity.
The rebuilt ignored artifact at dirty source revision `2a77c92fdc12` remains
`artifacts/sos-experience.apk`, 38,193,731 bytes, SHA-256
`e4233218ae9dd33ce77dfb016b7ea6a7904272e6a979528f608442e039d44a6f`.

**Failures and fixes:** Redirecting a bare Bash `exec` while probing the port
silenced later diagnostics in the same shell; scoping the file-descriptor open
kept probe failures quiet without muting the command. A first managed attempt
used `nohup`; the app booted, but this remote command supervisor reaped provider
PID 81840 when `m1-run` returned. Moving Linux persistence to the user systemd
manager made the daemon survive command completion. The compatible-listener
test also confirmed that the fix rejects an unmanaged authority rather than
killing or replacing it.

**Decision / remaining risk / next gate:** Accept the one-command strict Android
run and explicit-stop contract on this Fedora/systemd host and physical phone;
leave the final provider, reverse mapping, and foreground app running for
interactive use. This is lifecycle evidence, not a repeat of the latency,
thermal, suspend, durable-state, or soak gates. The non-systemd `nohup` fallback
has not been exercised on macOS, and the transient unit deliberately does not
restart a crashed provider. The next portability gate is a clean macOS run and
failure injection during install and launch; the next product gate remains the
full physical-device lifecycle and performance matrix.

## 2026-08-14 — Reproduce the Debian 13 VM gates on Fedora/QEMU 10

**Goal / hypothesis:** Recreate the reference Debian VM on the new Fedora
x86-64 workstation, provision the current Linux stack, and pass both nested and
direct compositor gates without treating virtual display evidence as physical
hardware evidence.

**Code and environment changed:** Fedora 44 provides KVM to the unprivileged
`carlid` user and runs QEMU 10.2.2 with OVMF. The host packages are
`qemu-system-x86-core`, `qemu-img`, `cloud-utils-cloud-localds`, `edk2-ovmf`,
`qemu-device-display-virtio-gpu`, its separate
`qemu-device-display-virtio-gpu-pci` wrapper, and `virt-viewer`; the Fedora
package split is now documented. `tools/linux-vm/create` checks for the exact
VirtIO PCI model before writing generated VM files. On x86-64,
`tools/linux-vm/start` disables QEMU's implicit legacy VGA so the explicit
VirtIO GPU is the sole DRM device. `tools/linux-vm/stop` tolerates QEMU removing
its own PID/control files during an otherwise successful shutdown.

**Immutable input and provisioning evidence:** The ignored base image is
`.cache/linux-vm-base/debian-13-generic-amd64.qcow2`, 436,404,224 bytes,
SHA-256 `d4e6f5d1e9f571c198a65b45ab1adae6c5734607614e72f9661d84ce5881e5fc`,
and official SHA-512
`f6978100d8031c266d55d7815ceea7fcdeacf28e1e5834fdb9c94ac96880a054a6e6f8681c2d3b0584e0057eaf3ef7353856b85212d04134744faa9b3bb1f24f`.
`tools/linux-vm/create` verified that digest before creating the 100 GiB
copy-on-write guest. Cloud-init completed without errors in 6 minutes 46
seconds; the guest reported Debian 13 amd64, kernel
`6.12.101+deb13-amd64`, 8 vCPUs, 12 GiB RAM, and a 99 GiB root filesystem.
The source worktree at `4a815401087eb9f943351483e45caf2b09549401` was
copied without Git metadata, caches, artifacts, or host build output.
`tools/linux-vm/provision-debian` installed the pinned dependencies, Rust
1.95.0, and Node 24.18.0, then linked both compositor backends and the Linux
session binaries and compiled the agent.

**Gate evidence:** `tools/linux-vm/verify-session` returned
`linux_nested_session_passed os=debian version=13` with unchanged host PID
52499 and active revision
`93357d5643d00a4510a728743e04b0c35ce3439fb6a308c46b1c7f977a3ede84`.
After correcting the display topology, `tools/linux-vm/verify-direct-session`
returned `linux_direct_session_passed`, kept activation PID 1801, recovered in
PID 2100, activated revision
`250b157308407df4ed48c8e45351e69a0d82534ba001b6ff214c6a3348c0a326`,
and recorded `drm_page_flip` for recovery, boot, activation, and recovered boot.
The verifier restored GDM, removed its temporary directory and processes, and
left no failed units. Bash syntax, ShellCheck 0.11.0, and `git diff --check`
passed for the three VM lifecycle scripts. A stop/stop/start sequence stopped
QEMU PID 100371, accepted the second idempotent stop, preserved the disk, and
started PID 101385; only `card0-Virtual-1` was connected afterward, optional
`Virtual-2` remained disconnected, and GDM was active.

**Failures and fixes:** Installing only Fedora's core QEMU package let creation
write an overlay and seed but then failed because `virtio-gpu-pci` was absent;
the partial generated directory was removed while the verified base image was
preserved, both split device packages were installed, and creation succeeded.
The first direct gate then found intended VirtIO outputs on `card0` plus an
implicit QEMU stdvga output on `card1-Virtual-3`. The legacy `1234:1111` device
rejected an atomic page flip with `EINVAL`, which ended the compositor before
the Linux host could connect. Adding x86-only `-vga none` removed that device
and the unchanged direct gate passed. During the required restart, QEMU removed
its PID file before `stop` did; plain `rm` turned the successful shutdown into a
false failure, so cleanup now uses idempotent removal.

**Decision / remaining risk / next gate:** Accept nested GPUI/Wayland and direct
VirtIO DRM lifecycle evidence for Fedora 44 hosting an x86-64 Debian 13 VM. No
GNOME user login or VM password is required for these automated gates. This
does not establish physical GPU, touch, suspend, latency, thermal, or soak
behavior. The x86 QEMU 10 topology fix leaves the existing ARM path unchanged,
but that path was not rerun here. The boot-session verifier also remains to be
repeated on this guest. The provisioned VM is retained and running for that
next gate.

## 2026-08-15 — AOSP-0: build and boot pristine Android 17

**Goal / hypothesis:** Establish a known-good AOSP toolchain and Cuttlefish
environment before allowing any SOS platform changes, using the workstation's
single active checkout at `~/dev/aosp-sos` rather than duplicating the roughly
hundreds-of-gigabytes tree under `~/upstream`.

**Code and environment changed:** Added the pinned Ubuntu 24.04 Podman build
environment and `tools/aospctl` orchestration for host checks, Repo init/sync,
resolved-manifest identity, pristine and SOS builds, boot, verification, and
cleanup. The host was Fedora 44 x86-64, kernel 6.19.10, Podman 5.8.4, 32 logical
CPUs, 65,091,260 kB RAM, 8,388,604 kB swap, and user-accessible KVM. The final
1,340,023,005-byte container image ID is
`48736568ce9e99753c1460420698b895799ff7160a253e19021bf989f87ea609`.
The source was Google's `android-latest-release`; manifest checkout revision
`ad156f32caaa06dae91c02d443f6a8fe210eaa54`, with ignored resolved manifest
`.repo/sos-resolved-manifest.xml`, 230,225 bytes, SHA-256
`a97bb91ebe99656ae59f87cfb2059932c477c679b1f27287651cdeadde892bc1`.

**Evidence:** `./tools/aospctl doctor`, `init`, `sync`, `build-pristine`, `boot
pristine`, and `verify-pristine` passed. The unmodified
`aosp_cf_x86_64_only_phone-aosp_current-userdebug` build completed 29,136
actions in 13 minutes 50.64 seconds. Cuttlefish returned
`sys.boot_completed=1`, resolved HOME to
`com.android.launcher3/.uioverrides.QuickstepLauncher`, and reported fingerprint
`generic/aosp_cf_x86_64_only_phone/vsoc_x86_64_only:17/CP2A.260605.016/eng.root:userdebug/test-keys`.
The pristine generated images were intentionally not retained after this boot
proof because both products share `out/target/product/vsoc_x86_64_only`; the
SOS build replaced that output. `aospctl boot` now checks `ro.sos.home` and
refuses to mislabel the shared output as the other product.

**Failures and fixes:** Parallel Repo worktree creation raced on XFS, so network
fetch remains parallel while checkout is serialized with `--no-interleaved`.
Podman's default 2,048 PID ceiling caused Metalava native-thread failure;
unbounded container PIDs fixed it. Trusty's nested nsjail could not mount its
`/proc` under the default masks and capabilities, fixed by unmasking procfs and
granting `SYS_ADMIN` to this trusted build container. The original image lacked
Cuttlefish host support and QEMU audio libraries, so the official pinned
`cuttlefish-base=1.55.1`, ALSA, and Pulse libraries were added. Default seccomp
also denied AF_VSOCK. Crosvm then stalled before its guest kernel in rootless
Podman, while `qemu_cli` booted with tap, sandbox, virtiofs, WebRTC, and host GPU
integration disabled. Finally, Cuttlefish advertised `0.0.0.0:6520`; explicitly
connecting `127.0.0.1:6520` on every retry prevented a connected physical USB
phone from being selected.

**Decision / remaining risk / next gate:** Accept the AOSP-0 toolchain and
pristine Cuttlefish boot. The rootless QEMU/SwiftShader path is deliberately a
virtual platform gate, not physical GPU, touch, suspend, latency, thermal, or
soak evidence. Proceed to a separate SOS product using the same checkout.

## 2026-08-15 — AOSP-1/2: SOS HOME with an on-device authority

**Goal / hypothesis:** Make the GPUI experience the default x86-64 HOME, move
provider/state/revision authority and restart supervision out of that process,
and prove the complete product without workstation transport or `adb reverse`.

**Code and architecture changed:** `sosctl` and Gradle now select either
`arm64-v8a` or `x86_64`; a build-only manifest placeholder enables a priority
1000 HOME alias while ordinary physical-device APKs remain LAUNCHER-only. The
new `aosp_sos_cf_x86_64_phone` product platform-signs the privileged APK,
retains Quickstep as the separate Recents provider, installs an RRO naming SOS
as secondary HOME, and installs a native init service and bootstrap source in
`system_ext`. Dedicated enforcing `sos_shell_app` and `sos_authority` SELinux
domains communicate with each other through labeled device-loopback ports
47777 and 47778. The authority owns the existing typed provider/state service,
immutable content-addressed revision store, staged-state lookup, fsynced
activation journal, and atomic current symlink below `/data/misc/sos`. GPUI
asks it to activate only after the candidate's frame-presented callback; init
restarts the authority, while Android independently restarts HOME. The physical
`m1-run` developer path retains its host provider and reverse mapping, but the
AOSP product does not use either.

**Evidence:** The first unconstrained SOS build reached 14,414 actions before
the kernel killed a SystemUI KSP JVM at roughly 4.9 GiB anonymous RSS while
several multi-GiB Java actions ran concurrently and swap was full. Retrying the
same build with eight workers completed the remaining 11,421 actions in 11
minutes 55 seconds; eight is now the safe default. Subsequent product, RRO,
SELinux neverallow, compatibility, contexts, APEX-policy, and image checks all
passed. `aapt2 dump xmltree` confirms an enabled priority-1000 HOME alias with
its native-library metadata, and APK inspection finds only x86-64 JNI entries.

Repeated clean Cuttlefish acceptance runs passed `./tools/aospctl verify-sos`. The
final run reported HOME `dev.sos.experience/.SosHomeActivity`, ABI `x86_64`,
SELinux `Enforcing`, app domain
`u:r:sos_shell_app:s0:c88,c256,c512,c768`, authority domain
`u:r:sos_authority:s0`, and `adb_reverse=none`. Presenting Timeflow changed the
durable pointer from
`revisions/b0d20599c81f62db31cfffd4883289e64a12ee9ada6f20a1c92ef518277e9be4`
to
`revisions/32fa86a739260e3b13a7bf7f4bc9639708a7d9517d852c6bfe71acb13a552f59`
without replacing HOME PID 3446. Killing authority PID 781 caused init to
recover PID 5302 with HOME and the revision unchanged; killing HOME then caused
Android to recover PID 5388 with authority PID 5302 and the revision unchanged.
The ignored `/tmp/sos-home.png` visual check was a 720x1280 RGBA rendering of
the activated Timeflow HOME, 90,409 bytes, SHA-256
`73586544f732f70cd8a8189a3f34fe7211d9d764028b6c54cb0c513b05608616`.

The ignored artifacts from dirty SOS source base `a44ce7ce82f9` are:

- `artifacts/sos-experience.apk`: 39,372,746 bytes, SHA-256
  `c5cbc319dd3e91a61c6212ab2c3a8ae007d9f2d674432c6a095a97f1aaa6635a`;
- `target/x86_64-linux-android/release/sos-android-system-authority`: 1,391,144
  bytes, SHA-256
  `0f14f2c6d001b5ab08ac14ba861821ac6508929c5cb7b9dbfbf4ec392df89de3`;
- `system.img`: 955,183,104 bytes, SHA-256
  `7bbe6643a9d706b0b51e0d99ebe5ea1ab0660fe6328aed29e343599f53f0dff8`;
- `system_ext.img`: 294,813,696 bytes, SHA-256
  `bc1511443ae3ecb0ac1f09412f75e9bdaa645bdaf6721ac34f54a629eb3d2315`;
- `product.img`: 292,134,912 bytes, SHA-256
  `38a8c4a7b5254bfff78f45857218165ded06b5e4e95e172634ea8cbd9633275c`;
- `vendor.img`: 287,932,416 bytes, SHA-256
  `edc322dd446fb90b40d5fd7c55718f23bf074bfd5363af882a6ee9f4fd93c104`;
- `boot.img`: 67,108,864 bytes, SHA-256
  `9a2c7a72de0dda17413dbb88df30fc7ed6abdb73354ca564690854628366409a`;
- `vendor_boot.img`: 67,108,864 bytes, SHA-256
  `0015a50cf9662724b5e69a3264acb9ec7706085848f79b4394f65905a7c786e9`.

`cargo fmt --all -- --check`; locked authority, provider, and revision-store
tests; strict authority Clippy; strict x86-64 and ARM64 Android checks; Bash
syntax; and `git diff --check` passed. Authority tests cover atomic
presentation activation, restart recovery from the state-first crash gap, and
rejection of state belonging to another source.

**Failures and fixes:** Emptying `config_recentsComponentName` made SystemUI's
`LauncherProxyService` dereference a null component and restart; Quickstep now
remains solely for Recents while SOS wins HOME resolution. The authority first
failed to create its atomic `current` symlink, fixed with the narrow labeled
`lnk_file` permission. SOS's custom domain initially omitted the standard
privileged-app service attribute and could not find ActivityManager; adding
`priv_app_domain` and the correct `privapp_data_file` label fixed it. HOME then
crashed because Activity aliases do not inherit the target Activity's
`android.app.lib_name`; the alias now declares it and the Java loader safely
falls back to concrete Activity metadata. The verifier also observed transient
FallbackHome resolution and display sleep during user unlock, so it wakes the
display and waits for SOS plus a live/active GPUI signal. Editing a running
shell invocation once produced an end-of-file parse error after a successful
build; the saved script was valid and later `bash -n` checks pass.

**Decision / remaining risk / next gate:** Accept AOSP-1 and AOSP-2 at
Cuttlefish product scope. Revision and provider authority now survive GPUI and
the product has no workstation transport. Android still owns boot services,
PackageManager, ActivityManager HOME recovery, SurfaceFlinger, input, IME, and
focus; Quickstep still owns Recents. The next architecture gate is SOS-owned
surface staging/promotion and input focus. Repeat functional, lifecycle,
latency, suspend, thermal, and long-soak gates on compatible physical x86-64 or
new ARM64 product hardware before making any hardware claim. Production
revision signing and verification-key provisioning also remain open; this
Cuttlefish product verifies content identities and read-only revision layout
but does not provision a signing key.

## 2026-08-15 — SM-A336B bootloader and platform viability gate

**Goal / hypothesis:** Determine whether the existing Galaxy A33 5G can safely
advance from ARM64 APK testing to a device-specific SOS system image, and
whether enough device, kernel, vendor, recovery, and restore material exists to
justify an ARM64 product port.

**Device, environment, and documentation changed:** Probed the already
authorized SM-A336B over ADB without rebooting, wiping, rooting, entering
Download Mode, or writing a partition. Developer Options was opened for a UI
dump and the phone was returned to HOME. Pulled its ignored `SecSettings.apk`
to `/tmp` for bytecode inspection, shallow-cloned the candidate community
device/common/kernel/vendor trees under `/tmp`, and cross-compiled the existing
authority for `arm64-v8a`. Removed the 842 MiB temporary audit tree after
recording its revisions and measurements. Added the focused
[`samsung-sm-a336b.md`](samsung-sm-a336b.md) assessment and linked it from the
AOSP report.

**Evidence:** `adb shell getprop` identified `SM-A336B` / `a33x`, Exynos 1280
`s5e8825`, Android 16 / One UI 8, stock build `A336BXXUEGYI8`, dynamic A-only
partitions, Treble, and rollback level 14. The phone remains locked and intact:
`flash.locked=1`, `vbmeta.device_state=locked`, verified boot `green`,
`other.locked=1`, Knox Guard `Completed`, and warranty bit `0`.
`ro.oem_unlock_supported` was absent and the OEM-unlock row was absent from the
top of Developer Options. `apkanalyzer dex code` confirmed this firmware's
`OemUnlockPreferenceController` requires `ro.oem_unlock_supported=1` to obtain
an `OemLockManager` and rejects `ro.boot.other.locked=1`. The ignored
114,799,707-byte `SecSettings-A336BXXUEGYI8.apk` had SHA-256
`b16a9bbd64d740f7482afe1fcd0c44b7ef8340f01bdb1950f73cf125fedd6a6c`.

The community source audit found an `a33x` device tree at
`a85c2a9652c…`, `s5e8825-common` at `33dd9c999786…`, a 5.10.239 kernel at
`0f885d194baa…`, device blobs at `a7efdd5712ec…`, common blobs at
`4a2275bfabd9…`, and a dependency manifest at `23b84e5f5dc0…`. The board and
kernel cover the observed ARM64 partition topology and device DTS, panel,
camera, touch, init, VINTF, SELinux, radio, and recovery surfaces. A separate
2026-08-08 unofficial LineageOS 23.2 release is 1,260,835,762 bytes with
published SHA-256
`54f38de8d898ba6ea6712fd69ce4853b99f8f9b336505baed6ee05368ff843b1`.
This establishes a credible Android 16 community basis, not a proven SOS or
Android 17 product.

`cargo ndk -t arm64-v8a -P 31 build -p android-system-authority --bin
sos-android-system-authority --release --locked` passed in 15.60 seconds. The
ignored stripped Android 31 ARM64 PIE from clean SOS revision
`b6da0369c29092b85720c4c20a8a56707afc2942` is 1,216,976 bytes, SHA-256
`d4e25d1e5be06b5c4b7cf2fd159f7a9f593788f37626e0b49b278faeaa2b8158`.

**Failures and rejected paths:** The device-tree blob update for One UI 8
explicitly pins the older `A336BXXSEFYH2` bootloader/TEE set because newer
`sboot.bin` adds an auto-lock property, consistent with the connected phone.
Official a33x TWRP exists, but its latest downloadable recovery is a 2024
Android-12 build with a prebuilt kernel and decryption disabled, so it is not a
recovery proof for this Android 16 phone. No exact stock firmware bundle or
validated Samsung flashing tool exists locally. Two discovered GitHub
"firmware preservation" repositories were empty. The apparent shared binary
revision `E` between the current build and pre-One-UI-8 `FYH2` makes downgrade
a donor-device hypothesis only, not a safe procedure. The community trees are
LineageOS 23.x / Android 16 and include unlicensed proprietary blobs; they
cannot simply replace the current Android 17 Cuttlefish device directory.

**Decision / remaining risk / next gate:** Keep the current SM-A336B as the
intact ARM64 APK test device and classify it as no-go for system flashing. The
device family remains a conditional candidate because a complete community
hardware basis exists and the SOS authority already builds for ARM64. Acquire
a separate pre-One-UI-8 SM-A336B with a visible OEM-unlock control, archive and
hash exact stock packages, and prove stock-to-stock restore on that donor
before accepting the factory reset and irreversible Knox warranty-bit risk.
Then reproduce and boot the pinned Android 16 baseline before layering the SOS
ARM64 product, init service, SELinux policy, and signing. No Samsung hardware,
latency, suspend, thermal, or soak gate was advanced by this non-destructive
audit.

### Sole-device rollback and official-TWRP clarification

**Goal / hypothesis:** Determine whether the only available SM-A336B could
recover the missing OEM-unlock path by returning from One UI 8 to One UI 7 and
then rely on the official a33x TWRP image.

**Device and environment changed:** No device state changed. A second
read-only `adb shell getprop` check confirmed EUX, build `A336BXXUEGYI8`,
`ro.boot.rp=14`, locked/green verified boot, `flash.locked=1`,
`other.locked=1`, and no `ro.oem_unlock_supported`. The phone was not rebooted
or placed in Download Mode. The published TeamWin prebuilt kernel was fetched
to a fresh `mktemp` directory, inspected, and deleted; the attempted direct
official TWRP image fetch returned an HTML download page rather than an image
and that temporary file was also deleted.

**Evidence:** Samsung's update history identifies `A336BXXSEFYH2` as Android
15 / One UI 7 dated 2025-08-27 and `A336BXXUEGYI8` as Android 16 / One UI 8
dated 2025-10-22. Firmware metadata lists an EUX 7.86 GB four-file FYH2 package
at binary revision `E`, matching the installed build and direct rollback level
14, with published MD5 `45d5e89f77e1dfa8ee45d62b9c376e93`; no package was
downloaded, so its size, hash, signature, and completeness remain unverified.
Samsung's FYH2 release note warns that its security-policy update prevents
downgrading to older software, without promising a later One UI 8 build can
return to FYH2.

The official TWRP listing still offers only `twrp-3.7.1_12-0-a33x` from
2024-02-18. `curl` plus `sha256sum`, `wc -c`, and `strings` measured the
TeamWin tree's `prebuilt/Image` at 31,461,888 bytes, SHA-256
`593ad8f97564fe067ca5dec37417e7eeac6b0b80f342c6407e4fa280c6fe606e`,
with embedded Linux version `5.10.66-Gabriel260BR-TWRP-ga0103aac9499` built
2023-01-01. `BoardConfig.mk` targets platform 12 and sets
`TW_INCLUDE_CRYPTO`, `TW_INCLUDE_CRYPTO_FBE`, and
`TW_INCLUDE_FBE_METADATA_DECRYPT` false. The device-specific TWRP changelog
does not state a compatible Samsung firmware baseline.

**Failures and rejected paths:** Matching binary revision `E` is necessary but
does not prove Samsung verified-boot/version-binding acceptance. A direct
download URL was deliberately not treated as an image after `file` identified
the 6,795-byte response as HTML. The old official TWRP image cannot be the sole
restore path: its kernel predates FYH2 by more than two years and it cannot
decrypt `/data`. Combining stock rollback, bootloader unlock, custom recovery,
and SOS in one flash session was rejected because it erases the intermediate
evidence and multiplies the only phone's failure modes.

**Decision / remaining risk / next gate:** A complete Samsung-signed
GYI8-to-FYH2 rollback is a plausible destructive experiment, not a safe or
simple migration. Keep APK testing as the default while this remains the only
phone. Proceed only if the phone is explicitly accepted as expendable, all
data and authentication material are backed up independently, exact EUX FYH2
and GYI8 stock packages are locally archived and verified, and a pinned
flashing/recovery host passes non-writing Download Mode detection. Roll back to
stock FYH2 and boot it as a separate gate; continue only if the OEM-unlock
control actually returns. Do not use the 2024 official TWRP build as the sole
recovery plan; build a recovery against the pinned contemporary a33x
kernel/device tree before any SOS system flash. No hardware gate advanced.

### Authorized sole-device rollback preparation

**Goal / hypothesis:** After the owner explicitly accepted loss of this
dedicated development phone and waived user-data backup, assemble a complete
stock restore path and advance only as far as non-writing Download Mode/PIT
validation before flashing FYH2.

**Device, environment, and artifacts changed:** No partition was written.
Verified the native Fedora 44 host had 572 GiB free, a direct USB path, and no
installed Samsung flasher; the phone was at 100% battery. Downloaded pinned
`samloader` 2.0.0 release archive (2,328,736 bytes, SHA-256
`7c6514028f20d5ea0eb57d6f872eee41b3a52336eabac6379b15a01a06ed7a79`)
and verified its extracted 8,470,752-byte static executable at SHA-256
`8a12712a530aa404df50df4fef0b16b7e0081b5362a3a34c752472d79c61f288`.
All tools and firmware remain outside Git under
`/home/carlid/sos-samsung-work`.

**Evidence:** `samloader check-update --model SM-A336B --region EUX --all`
returned the exact FUS versions for installed GYI8 and target FYH2. Direct
Samsung downloads produced `SM-A336B_EUX_A336BXXSEFYH2.zip`, 8,436,163,243
bytes, SHA-256
`71a9a3433400cd0002541020395b5680f8651c4b3bf47f0e7d895e94d7f959d6`,
and `SM-A336B_EUX_A336BXXUEGYI8.zip`, 8,086,643,743 bytes, SHA-256
`237d6567800569a7120474761643fd3571b1cfbb93a3d841e932495b65300bc3`.
`unzip -t` passed both; `samloader verify-md5` passed all ten extracted BL,
AP, CP, CSC, and HOME_CSC archives. Their filenames, sizes, and SHA-256 values
are recorded in `docs/samsung-sm-a336b.md`.

The package PIT files have distinct raw hashes but parse to identical 48-entry
layouts. `adb reboot download` then moved the still-locked device from USB
`04e8:6860` to Download Mode `04e8:685d`; `samloader detect --verbose`
returned `Device detected`.

**Failures and fixes:** A third-party published FYH2 outer MD5 did not match
the ZIP streamed directly from Samsung FUS, so it was rejected as the trust
anchor; ZIP CRC validation, locally recorded SHA-256, and every embedded Odin
MD5 all pass. The first live PIT dump stopped before protocol setup because
Fedora exposed the Download Mode node as `root:root` mode `0664`; the pinned
tool reported `Access denied (insufficient permissions)`. The normal-mode ADB
udev rule covers only product `6860`, not Download Mode product `685d`.

**Decision / remaining risk / next gate:** Package acquisition and offline
integrity gates pass. The phone is paused intact in Download Mode and no flash
has started. Obtain administrator authorization for an ephemeral write ACL on
the current USB node, dump the live PIT, compare it to the identical parsed
FYH2/GYI8 layout, and only then perform a complete BL/AP/CP/wiping-CSC FYH2
stock flash without repartition. Stock FYH2 boot and return of the OEM-unlock
control remain separate future gates; official TWRP remains rejected as the
sole recovery path. No hardware or unlock gate advanced.

**Continuation:** The owner applied an ephemeral ACL to the current Download
Mode node. A full Odin 5 session then succeeded and dumped the ignored live
PIT: `SM-A336B-live-before-FYH2.pit`, 8,192 bytes, SHA-256
`238552c2c4857cb7cf4a5e2c8033b324478bf5201ff14552f33f93cd15c2c53a`.
`samloader print-pit` produced an exact diff match against the FYH2 package's
48-entry layout. The PIT command sent its end-session/reboot request, but the
phone remained in Download Mode. The first flash invocation then revalidated
BL/AP/CP/CSC and failed during the initial `ODIN` handshake with bulk-transfer
timeouts; no Odin session or file transfer began. `usbreset 001/006` reset the
host USB port successfully without changing enumeration or the ACL, but a
second invocation failed at the same pre-session handshake. This confirms the
device-side Odin session, not package integrity or host USB permission, must
be reset by a real handset reboot. No partition write occurred. Install the
pinned tool's audited `TAG+="uaccess"` udev rule, physically reboot with Side +
Volume Down, re-enter Download Mode from the intact GYI8 system, and use the
fresh enumeration for one flash session without a preceding PIT command.

**Rollback flash result:** After the physical reboot, GYI8 booted intact and
the persistent `/etc/udev/rules.d/60-samloader.rules` rule exposed the fresh
Download Mode node with the expected per-user `uaccess` ACL. From that fresh
session, pinned `samloader` 2.0.0 verified and flashed the FYH2 `BL`, `AP`,
`CP`, and wiping `CSC_OXM` archives. No PIT argument, repartition, EFS clear,
or `HOME_CSC` was used. The Odin 5 handshake and its in-session PIT read
succeeded; `FLD`, `BOOTLOADER`, `UP_PARAM`, `LDFW`, `TZSW`, `TZAR`, `HARX`,
`KEYSTORAGE`, `UH`, `BOOT`, `VENDOR_BOOT`, `DTBO`, `RECOVERY`, `SUPER`,
`USERDATA`, `PRISM`, and `OPTICS` all returned successful upload responses.
The tool printed `Ending session...`, `Rebooting device...`, released the USB
interface, and exited 0 at 2026-08-15 03:51 CEST. There was no anti-rollback,
signature, size, or write failure.

The tool's automatic reboot request did not transition the handset: for at
least 38 seconds afterward it remained enumerated on the same USB node as
Samsung Download Mode `04e8:685d`. This is a post-flash reboot-boundary issue,
not evidence of a failed transfer. Stock FYH2 boot is not yet claimed. The
next gate is a physical Side + Volume Down restart into normal boot, completion
of the expected factory-reset setup, and read-back of build, rollback, lock,
verified-boot, and OEM-unlock state before any custom image or unlock action.
No SOS hardware, latency, or suspend gate advanced.

**Stock boot continuation:** The owner held Side + Volume Down after the host
had exited and no process held the Download Mode USB node. Download Mode
disappeared, the phone completed its first boot, and the owner reached the
Android welcome/setup screen. At 2026-08-15 03:58 CEST the host saw a fresh
normal-mode Samsung `04e8:6860` device in MTP mode. This passes the factory-boot
and USB re-enumeration portions of the rollback gate. Exact FYH2 properties,
verified-boot/lock state, and availability of OEM unlocking remain pending
until setup is complete and USB debugging is re-enabled.

**FYH2 read-back and unlock-control gate:** After setup and renewed ADB
authorization, the phone reported `ro.build.PDA=A336BXXSEFYH2`, Android 15 /
API 35, security patch `2025-08-01`, the FYH2 release-key fingerprint, and
`ro.bootloader=A336BXXSEFYH2`. It remains at rollback level 14 with
`flash.locked=1`, `vbmeta.device_state=locked`, verified boot `green`, and
Knox warranty bit `0`. A temporary `uiautomator` dump of Developer Options,
removed immediately after inspection, confirmed an enabled `OEM unlocking`
row with summary `Allow the bootloader to be unlocked`; its switch is
currently unchecked. The rollback goal therefore passes: the exact signed
FYH2 baseline boots and restores the unlock control without changing the lock
or Knox state. Bootloader unlock is a separate destructive gate and has not
yet been attempted.

**Unlock authorization staged:** With the phone at 100% on USB power, Android's
`automatic_system_updates` global setting was changed from enabled to `0` to
avoid an unattended FYH2 replacement on reboot. The owner then enabled the
visible `OEM unlocking` preference and accepted its device warning. A second
temporary `uiautomator` dump, removed immediately, showed the enabled switch
as `checked=true`; at that checkpoint `flash.locked` was still `1` and the
Knox warranty bit still `0`. This only authorizes a later bootloader unlock;
it did not itself unlock, wipe, or flash the phone.

**Bootloader unlock initiated:** From the powered-off A33 hardware-key entry,
the device presented `Continue`, `Device unlock mode`, and `Cancel`; the owner
long-pressed Volume Up, then confirmed the `Yes (may void warranty)` prompt.
Download Mode disappeared immediately. After roughly 48 seconds without a USB
device, the phone re-enumerated at 2026-08-15 04:15 CEST as normal Samsung MTP
`04e8:6860`, demonstrating that the unlock-triggered reset reached Android.
Final lock, AVB, and Knox properties remain pending until setup and renewed
ADB authorization; do not call the unlock gate complete from MTP alone.

**Bootloader unlock verified:** After setup and renewed ADB authorization,
FYH2 remained installed and the device reported `ro.boot.flash.locked=0`,
`ro.boot.vbmeta.device_state=unlocked`, and verified boot `orange`. Rollback
level remained 14. No custom image had been loaded, and the Knox warranty bit
remained `0`; Knox Guard changed from the pre-unlock `0x4` observation to
`ro.boot.kg=0x1`. The wipe cleared the update setting, so
`automatic_system_updates=0` was reapplied. A temporary UI dump, deleted after
inspection, showed the OEM control checked, disabled, and summarized as
`Bootloader already unlocked`. The stock rollback and bootloader-unlock gates
are complete. The next gate is a pinned contemporary a33x recovery/custom-ROM
baseline and a stock-restore procedure; the outdated official TWRP image is
still not accepted as the sole recovery path.

### Pinned a33x Android 16 recovery build and AVB gate

**Goal / hypothesis:** Reproduce a contemporary ARM64 recovery from the
audited LineageOS 23.0 a33x graph, prove its partition/layout compatibility
against exact FYH2 stock, and reject any flash set whose AVB relationships are
not internally consistent.

**Code and environment changed:** Added the exact-revision local manifest
`aosp/manifests/a33x-lineage-23.0.xml`, a dedicated Ubuntu 24.04 build image at
`tools/a33x/Containerfile`, and the `tools/a33xctl` init/sync/build/inspection
driver. The new checkout is isolated at `/home/carlid/dev/lineage-a33x`; the
Android 17 Cuttlefish tree was not changed. The container image ID is
`4e351528281b6b7085676140451e0f2cc531764963668a8f0f3016f2f82596dc`.
The phone remained booted on unlocked stock FYH2 throughout; no custom binary
was flashed and the last observed Knox warranty bit remains `0`.

**Evidence:** `./tools/a33xctl doctor`, `init`, and `sync` passed. The clean
1,150-project checkout produced the 280,889-byte resolved manifest
`.repo/sos-a33x-resolved-manifest.xml`, SHA-256
`91594f3ddcbeee8b87196d017cfedd8b5bff5b66622c6363b0228efa56d8d573`.
Direct HEAD checks matched the pinned a33x device
`a85c2a9652c93880a1c1474a098a72368d416e21`, s5e8825 common
`33dd9c99978647a44aa22089db4830f95bb91fb8`, kernel
`0f885d194baaed657ad05bc4ff0d8d5cd4a2f4e5`, a33x vendor
`a7efdd5712ece827ad3632fd38c93dd267f58b51`, and common vendor
`4a2275bfabd9fcce764bcf773a7d1e236ff67346` revisions. `repo status` reported
a clean worktree.

`./tools/a33xctl build-recovery` configured Android 16,
`lineage_a33x-userdebug`, ARM64 `cortex-a55`, compiled 11,704 actions including
the 5.10.239 s5e8825 kernel and A33 EU DTBOs, and completed in 10m19s. The
ignored `out/target/product/a33x/recovery.img` is exactly 100,663,296 bytes,
SHA-256
`9bbf7983feb5dbb0854dc34448690c18c037273821c0eb45a210ac50218b48e9`.
`file`, `unpack_bootimg`, and `avbtool info_image` report boot header v2,
4,096-byte pages, the stock load addresses, a valid `recovery` hash descriptor,
and a SHA256_RSA4096 AVB footer. Its signed content is 65,105,920 bytes.

Exact FYH2 `recovery.img.lz4`, 56,973,312 bytes and SHA-256
`cb7910d8ee1727ea6f2ba91ebf0f2daf818990ba3c57c6498e646905c574a442`,
was extracted from the already verified AP archive and decoded outside Git.
The decoded stock recovery is also exactly 100,663,296 bytes, SHA-256
`49b0745a746aaa45ccba806479d6e9c2cc7f74f756d03e2390bdc6ffb3f78712`,
and uses the same header version, page size, and load addresses.

The vendor tree labels the SM-A336B radio set `A336BXXSEFYH2`. A streaming
comparison used `tar -xOf <archive> <member>.lz4 | lz4 -d -c | sha1sum`
inside the pinned container and compared each result with `sha1sum` on the
pinned vendor file. All ten matched: `fld.bin` `1136f30e…`, `sboot.bin`
`ef119131…`, `ldfw.img`
`af59ba8a…`, `tzsw.img` `5d400680…`, `tzar.img` `3c3658ad…`, `harx.bin`
`535423b4…`, `keystorage.bin` `b443046c…`, `uh.bin` `b560c96e…`,
`modem.bin` `6d7d58bf…`, and `modem_debug.bin` `203a495a…`. This proves the
install package's firmware input is the exact FYH2 generation already running
on the handset, not merely a matching version label.

**Failures, fixes, and rejected paths:** The first source sync was lengthy
because partial-clone checkout fetched multi-gigabyte Clang and Rust packs;
process, network, disk, and file-growth checks showed continuous progress, and
the sync finished without retry. Containerized nsjail was unavailable, so the
Android build disabled sandboxing inside the already isolated Podman build
environment. Upstream optional-library, Python escape, deprecation, and depmod
messages were warnings; the build exited 0. A temporary layout-audit helper
was rejected before execution because it included recursive cleanup; the audit
was rerun into a persistent ignored evidence directory without deletion.

The decisive rejected path is flashing recovery alone. FYH2 `vbmeta.img`
chains `recovery` at rollback-index location 1 to Samsung public-key SHA-1
`557dab1a3e7a1b571d6d864f8414d0e39468f835`, which matches FYH2 recovery.
The built recovery uses Lineage test-key SHA-1
`2597c218aae470a130f61162feaae70afd97f011`. A generic
verification-disabled vbmeta was also rejected because it would bypass the
matched-chain proof and could strand stock Android.

**Decision / remaining risk / next gate:** The source, compilation, partition
size, header, DTB/DTBO, and recovery-footer gates pass, but the flash gate does
not. The pinned device releasetools intentionally update `dtbo`, `vbmeta`, and
`vendor_boot`. AOSP's non-A/B releasetools explicitly exclude recovery from the
generated top-level vbmeta descriptors and instead give recovery its own AVB
footer; Samsung stock currently does chain recovery from vbmeta, so the exact
generated package and bootstrap set still require inspection rather than an
assumption about bootloader behavior. `tools/a33xctl inspect-rom` now requires
the package's boot, DTBO, recovery, vbmeta, and vendor-boot entries and enforces
their exact live-PIT size ceilings. Build and inspect the complete Lineage
install package and its boot-chain images. Only after their hashes, ZIP
integrity, PIT fit, updater script, and AVB descriptors agree may the first
custom flash proceed directly into recovery and full ROM installation. No SOS
hardware, latency, thermal, suspend, or soak gate advanced.

**Full-ROM build continuation and camera-shim fix:** `./tools/a33xctl
build-rom` reused the pinned checkout and recovery output, then advanced through
123,295 of 147,270 Ninja actions before stopping after 1h39m05s. The phone
remained booted on unlocked FYH2 and no partition write occurred. `out/error.log`
identified one failed prebuilt-ELF gate: the FYH2 `libexynoscamera3.so` imports
`createScenarioOperator`, while the pinned `libepicoperator` shim exported
`createOperator`. `llvm-nm -D --undefined-only` independently confirmed the
blob import; the generated shim had no matching export.

The maintainers' later commit
`cb4ca128b0867d9cc92f22501430d0775018f5f1` contains exactly the required
one-line rename. Added its audited backport as
`aosp/patches/a33x-lineage-23.0/0001-s5e8825-fix-epicoperator-symbol.patch`,
SHA-256 `a8dea6c8c01f3c8572f952b8455560c690e700626df0332bd391abc04b175c61`,
and made `tools/a33xctl` idempotently apply it after sync and before either
build target. `git apply --check`, the resulting source diff, `bash -n`, and
`git diff --check` passed. Allowing arbitrary undefined symbols in the camera
blob was rejected because it would suppress the build-time proof without
providing the runtime symbol. Resume the cached full-ROM build, then run the
unchanged ZIP, PIT-size, updater, firmware, and AVB inspection gates. No
hardware or flash gate advanced.

**Second cached-build stop and RIL dependency fix:** The resumed graph rebuilt
the shim, exported `createScenarioOperator`, regenerated the successful camera
ELF-gate timestamp, and then stopped after 2m26s at 9,544 of 23,990 remaining
actions. `out/error.log` showed a distinct, precise mismatch: FYH2
`libsec-ril.so` has `DT_NEEDED libprotobuf-cpp-full-21.7.so`, but its generated
`Android.bp` declared generic `libprotobuf-cpp-full`. `llvm-readelf -d`
confirmed the binary SONAME dependency, and the pinned vendor tree already
contains and packages the `libprotobuf-cpp-full-21.7` module.

The maintainers corrected that dependency in later commit
`cf2678a02cedac743ddd00502fc390731a337301`. Added the applicable one-line
backport as
`aosp/patches/a33x-lineage-23.0/0002-s5e8825-match-libsec-ril-protobuf-soname.patch`,
SHA-256 `18517d33328c5ffdf101bf8e8b1f25d5383d96ee50cfde5463a6d036da543795`,
and extended the idempotent patch driver to map each patch to its exact source
project. Disabling ELF checking was rejected because the declared dependency
must match the blob's runtime loader contract. Patch application, source
diffs, `bash -n`, and `git diff --check` passed. Resume the cache again; no
device or flash gate advanced.

**Third cached-build stop and LFS hydration gate:** The RIL ELF dependency
check passed after the second backport. The next cached run stopped after
2m31s at 3,208 of 14,475 actions because the ARM64 WebView APK was a 134-byte
Git-LFS pointer, so `manifest_check`/`aapt2` correctly rejected it as an
invalid ZIP. The adjacent `depmod` and Ninja `restat` messages were not the
failed subcommand; `out/error.log` isolated WebView. A checkout-wide pointer
scan found only the four architecture-specific WebView APKs plus a Rust test
fixture; only ARM64 is in this product graph.

Added `tools/a33xctl hydrate-lfs`, called after sync and before both build
targets, to explicitly pull and verify the pinned ARM64 object while automatic
LFS smudging remains disabled. `git lfs pull --include=webview.apk` materialized
`external/chromium-webview/prebuilt/arm64/webview.apk` at 265,525,351 bytes,
SHA-256 `68fa550b7a76e39f0382308d93b235c0623d032c0aa6c4a56fc02eedfdbe6342`;
`unzip -tq` passed and the LFS-aware container reported a clean project. Resume
the cache; no phone or flash gate advanced.

**Full-ROM result and offline flash gate:** After LFS hydration, the final
cached run completed 11,275 actions in 8m05s. `check_target_files_vintf.py`
reported `COMPATIBLE`, the OTA generator signed the result, and Ninja exited
0. The ignored install artifact is
`lineage-23.0-20260815-UNOFFICIAL-a33x.zip`, 1,226,299,848 bytes, SHA-256
`765e4a9045bcece5fba8777f041d9f86d5d8569870ca63a183483086c3451e20`.
`unzip -tq` passed, and AOSP's `check_ota_package_signature.py` verified the
whole-file signature against the packaged test certificate.

Inspection found that late target-files image generation intentionally made
the ZIP's `dtbo`, `vbmeta`, and `vendor_boot` differ from stale top-level
product copies. The installer writes the ZIP copies, so `tools/a33xctl
inspect-rom` now extracts those authoritative files to the ignored
`/home/carlid/sos-samsung-work/lineage-a33x/lineage-23.0-20260815-UNOFFICIAL-a33x-bootstrap`
directory, requires byte identity with the target-files images, and verifies
that graph. The resulting bootstrap set is:

| Image | Bytes | SHA-256 |
| --- | ---: | --- |
| `boot.img` | 67,108,864 | `d27ea3e21a8643631f744616b1b98f5fe949f5f43914a599c2758720cc191d9a` |
| `dtbo.img` | 8,388,608 | `bb8b37acf0f6122228a203d9890ee2bcb45cf6860172f6fd5f86a0b032c99ba4` |
| `recovery.img` | 100,663,296 | `fe53c96b609dfd4c3a4121551bd8b965990d43784783ffbc54d2f52d82b50800` |
| `vbmeta.img` | 8,192 | `6061dd683af3d33a3ad01a2d4fc05e2e51a8f0d63aa4bc05d8bb7f1eb5e966b8` |
| `vendor_boot.img` | 33,554,432 | `053d9b6cab655ebdd89cb6895aae3667df7c44b33e020132e14ed03c77b2b82d` |

All five fit their exact live-PIT ceilings. `avbtool verify_image` passed each
footer. Recovery is independently SHA256_RSA4096-signed with key SHA-1
`2597c218aae470a130f61162feaae70afd97f011`; top `vbmeta` is algorithm
`NONE`, flags `0`, rollback index `1769904000`, intentionally omits recovery,
and binds the package's boot, DTBO, vendor boot, ODM, product, system,
system_ext, vendor, and vendor_dlkm images. Full graph verification passed all
three hash descriptors and all six dm-verity hashtrees.

The updater asserts `a33x`, patches the dynamic partitions, writes boot,
DTBO, vbmeta, and vendor boot, and does not write recovery. Its SM-A336B
firmware clause runs only when `ro.boot.bootloader` is not
`A336BXXSEFYH2`; the live property is exact FYH2, so it will skip that clause.
Independent SHA-1 reads of all ten firmware entries in the finished ZIP still
match the already verified stock FYH2 bytes. A final ADB preflight reported
`SM-A336B`, `A336BXXSEFYH2`, model property `SM-A336B`, unlocked boot state,
and 100% battery.

The offline build, package-signature, updater, firmware, PIT, and AVB gates
pass. The next gate is one no-repartition/no-PIT samloader session that writes
the five authoritative bootstrap images with automatic reboot disabled,
followed immediately by a hardware-key transition into Lineage recovery,
format-data, and sideload of the verified ZIP. This first custom write is
expected to irreversibly trip Knox; the owner already accepted that risk. No
physical Lineage or SOS hardware gate has advanced yet.

**First custom bootstrap flash and recovery boot:** Immediately before the
write, `sha256sum -c` revalidated all five authoritative bootstrap files; ADB
revalidated unlocked FYH2, `SM-A336B`, and sufficient battery, and the pinned
samloader binary remained SHA-256
`8a12712a530aa404df50df4fef0b16b7e0081b5362a3a34c752472d79c61f288`.
The phone entered a fresh Download Mode session. One samloader invocation used
explicit `BOOT`, `VENDOR_BOOT`, `DTBO`, `RECOVERY`, and final `VBMETA`
partition mappings with `--no-reboot`. It supplied no PIT, repartition,
skip-size-check, archive, EFS, or userdata option. The in-session live-PIT read
passed; all five uploads returned success; the tool ended the session and
exited 0.

The owner then held Side + Volume Down until black and immediately switched to
Side + Volume Up with USB connected. The handset booted the newly built
Lineage Recovery instead of stock Android. This passes the matched-bootstrap
write and first custom-recovery boot gates. Recovery's main menu did not expose
an ADB USB gadget, which is expected before selecting ADB sideload and is not a
boot failure. The next gate is recovery format-data, ADB sideload of the exact
verified ZIP, and first Lineage system boot. Knox state has not yet been read
back; no ROM or SOS hardware gate has advanced.

**Recovery USB/watchdog repair and first Lineage boot:** The owner completed
Recovery's factory-reset/format-data operation. Selecting `Apply update` →
`Apply from ADB` then exposed no USB device at all: repeated `adb devices` and
`lsusb` checks remained empty after cable, connector-orientation, and direct
host-port retries. The handset also eventually showed a green screen and
rebooted back into Recovery. These results rejected an ADB-server or Linux
permissions explanation and implicated the recovery image itself.

Offline ramdisk inspection reproduced the defect. Recovery's generic
`system/etc/init/hw/init.rc` imports `/init.recovery.${ro.hardware}.rc`, but
`cpio -it` proved the built ramdisk had no `init.recovery.s5e8825.rc`. The
pinned common tree instead packaged `init.s5e8825.recovery.rc` only under
`/vendor/etc/init`, which is unavailable to that early recovery import. That
file sets `sys.usb.configfs=1`, selects controller `13200000.dwc3`, and starts
`watchdogd`, accounting for both observed symptoms.

Added the local backport
`aosp/patches/a33x-lineage-23.0/0003-s5e8825-package-recovery-init.patch`,
SHA-256 `51557dbd9e58d1788b505cf87d267ea260c362f9311eef2d9c62d743e906b2d4`,
and extended `tools/a33xctl apply-patches` to install it idempotently. The
patch copies the existing source file to the exact imported recovery-ramdisk
path. The cached `recoveryimage` rebuild completed 42 Ninja actions in 2m34s.
The corrected ignored artifact is
`/home/carlid/sos-samsung-work/lineage-a33x/recovery-fix-20260815/recovery.img`,
100,663,296 bytes, SHA-256
`d751d08b12c80861a5e0e7800e7df5eb189a94f3d7fda31fdfdb36dce04c7a6c`.
It fits the exact live PIT, and `avbtool verify_image` passed its
SHA256_RSA4096 footer and payload hash. `unpack_bootimg`, `lz4`, `cpio`, and
`cmp` proved that `/init.recovery.s5e8825.rc` is present and byte-identical to
the intended source.

The first `samloader flash --wait` attempt rejected the absent device and
returned before a session or write; it was discarded. After an explicit USB
watcher detected fresh Download Mode `04e8:685d`, one no-auto-reboot samloader
session downloaded the live PIT and uploaded only `RECOVERY` successfully. It
used no PIT input, repartition, archive, size-check bypass, or other partition.
The corrected recovery booted and immediately enumerated as `18d1:d001`; ADB
reported serial `RFCT50EGFCN` and expected `unauthorized` state on the main
menu. Selecting sideload changed that same device to `sideload` with product
`a33xnsxx`, model `SM_A336B`, and device `a33x`, proving the USB repair on the
physical handset.

Immediately before transfer, SHA-256 revalidated the exact signed
1,226,299,848-byte Lineage ZIP. `adb sideload` served the package through the
usual displayed 47% boundary and exited normally with `Total xfer: 1.00x`;
Recovery returned to its main ADB state without a transport or installer
error. The owner then selected system boot and reported the handset booted
into LineageOS. Format-data, full-package sideload, and first Lineage system
boot gates therefore pass on the SM-A336B. The OTA intentionally did not write
recovery, so the corrected image remains installed. The already-built ZIP
still contains the earlier recovery entry; regenerate and reinspect the full
package before any later distribution or SOS image flash. Android property
read-back, Knox state, and Wi-Fi/Bluetooth/audio/camera/NFC/modem/UDFPS,
suspend, thermal, latency, and SOS hardware gates remain pending.

**Lineage read-back, hardware baseline, and flash-ready SOS product:** Normal
ADB read-back from the booted handset reports `SM-A336B` / `a33x`, Android 16
API 36, `lineage-23.0-20260815-UNOFFICIAL-a33x`, kernel
`5.10.239-android12-9`, FYH2 bootloader, rollback level 14, unlocked/orange
verified boot, Knox warranty bit `1`, SELinux `Enforcing`, and encrypted FBE.
The system patch is 2026-02-01 and retained vendor patch is 2025-10-01. The
stock-looking FYH2 `ro.build.fingerprint` is an intentional device-tree spoof;
partition build properties independently identify Android 16 and Lineage.
Battery was 99%, USB-powered, and 31.8 C during this read-back.

The baseline CameraService enumerated camera IDs `0`, `1`, `2`, `3`, and `60`.
Aperture opened both primary cameras without a HAL death: the rear preview
produced frames, while the front preview was visibly correct. The ignored
screenshots are `evidence-20260815/camera-rear.png`, SHA-256
`4ccb34a3dc0dff6d0efd1a64a39cc9e008dd2bdf63b4bb38c39b3c6cf9e590fb`,
and `camera-front.png`, SHA-256
`3d2f2beed41ca4f59ab2a12f1891a96c02b8064d3484f0bfd5f5982a6f8c2d0a`.
Fingerprint HAL sensor 0 enumerated with no HAL death, NFC was on, Bluetooth
was on as `Galaxy A33 5G` with no crash, and the FYH2 modem registered the Salt
SIM in slot 1 on LTE in Switzerland. Wi-Fi was enabled but not associated;
fingerprint enrollment, actual audio, calls/data, suspend, thermal, and soak
remain untested and are not advanced by this smoke check.

Added the ARM64 Samsung SOS product at `aosp/device/sos/a33x` and corresponding
`tools/a33xctl stage-sos`, `build-sos`, and `inspect-sos` gates. It inherits the
reproduced a33x product, retains Launcher3QuickStep for SystemUI Recents, and
adds the platform-signed privileged SOS HOME, ARM64 on-device authority,
bootstrap experience, init service, product properties, and dedicated
enforcing `sos_shell_app` / `sos_authority` policy. The staged input APK is
38,244,027 bytes, SHA-256
`0add171a0673f7f8bc840d15d1ea94a3c6790b44e3d54c5f5ff2e556ad356938`,
and contains only ARM64 native libraries with its priority-1000 HOME alias
enabled. The authority is 1,216,976 bytes, SHA-256
`d4e25d1e5be06b5c4b7cf2fd159f7a9f593788f37626e0b49b278faeaa2b8158`;
the 6,080-byte bootstrap is SHA-256
`a9bb30563d21d05912e9b58e24d8455088686f41c82058ec9c566a32758193f4`.

The first SOS build stopped safely at policy compilation because the Android
17 Cuttlefish policy used `priv_app_domain()`, a macro absent from LineageOS
23. Inspection of Lineage's policy model showed its custom privileged apps use
`app_domain` plus explicit grants. The port now follows that model; permissive
policy and broad allow rules were rejected. The resumed build compiled all
SELinux compatibility/context/APEX tests, completed 12,924 actions in 3m51s,
reported VINTF `COMPATIBLE`, and produced the signed ignored
`lineage-23.0-20260815-UNOFFICIAL-sos_a33x.zip`: 1,247,998,781 bytes,
SHA-256
`0fb4d1139475b4f53b64f555db34851ab4a55251f578db5efd804984f781cf2a`.

`./tools/a33xctl inspect-sos` passed ZIP integrity and the whole-package
signature, selected the SOS package by exact target suffix, proved package /
target-files byte identity for all five bootstrap images, enforced their live
PIT ceilings, and verified the complete AVB graph. It also unpacked the
package's 100,663,296-byte recovery and proved the repaired
`init.recovery.s5e8825.rc` is present. The installed APK is 41,536,658 bytes
after platform signing, SHA-256
`d5ad8b059b30931e30b3e0938b1a8d566937a849a9b35abb35fe4081db77a36d`;
it remains ARM64-only and HOME-enabled. The packaged authority and bootstrap
match their staged/source hashes exactly, both SOS product properties are
present, and compiled seapp/file contexts contain the intended domains and
labels. The offline SOS flash gate passes. Next, sideload this exact ZIP from
repaired recovery without formatting data, boot it, and require physical proof
of SOS as HOME, on-device activation with no `adb reverse`, enforcing runtime
domains, authority/app restart recovery, and preserved hardware services
before advancing the SOS hardware gate.

**First SOS boot, enforcing-policy failure, and corrected OTA:** Recovery
installed the 1,247,998,781-byte SOS OTA above with `Total xfer: 1.00x`; no
format-data operation was performed. After reboot the physical phone reached
`sys.boot_completed=1` as
`lineage-23.0-20260815-UNOFFICIAL-sos_a33x`, retained FYH2, unlocked/orange
AVB, warranty bit `1`, and SELinux `Enforcing`, and exposed both SOS product
properties. PackageManager found the system_ext privileged APK with
`primaryCpuAbi=arm64-v8a` and resolved priority-1000 HOME to
`dev.sos.experience/.SosHomeActivity`. Init kept `sos_authority` running as
PID 945 in `u:r:sos_authority:s0`; `adb reverse --list` was empty.

The HOME itself failed the runtime gate. Three launches died before attaching
to ActivityManager. Crash-buffer and kernel audit evidence showed the single
root cause: enforcing SELinux denied `service_manager find` on
`activity_service` from `u:r:sos_shell_app:s0`, after which ActivityThread
received a null ActivityManager binder and crashed. This was not a boot-chain,
APK-ABI, HOME-resolution, or authority failure.

Source inspection established why the offline policy compiler could not catch
the behavioral mismatch. LineageOS 23's `app_domain()` supplies only the base
app attribute and isolation neverallows; its complete framework, network,
Bluetooth, and privileged-service contract is attached to the concrete
canonical `priv_app` domain. Android 17's `priv_app_all` attribute, which
allows a package-specific domain to inherit that contract on Cuttlefish, is
absent. Iteratively copying framework allows, running permissive, and adding a
wildcard service rule were rejected. The package-specific seapp rule now maps
this platform-signed system_ext privileged package to Lineage's canonical
enforcing `priv_app` domain, while SOS policy adds only its two labeled TCP
ports.

The cached rebuild compiled 292 affected actions in 3m48s. All neverallow,
compatibility, context, APEX-policy, and VINTF checks passed. The corrected
signed OTA is 1,247,480,012 bytes, SHA-256
`6476f3a80556708491992b8a88b305d353e03d4a3390346f5062746d9b3f61ce`.
`./tools/a33xctl inspect-sos` again passed ZIP and whole-package signatures,
all PIT ceilings, package/target-files image identity, the complete AVB graph,
repaired recovery ramdisk, ARM64/HOME component checks, exact authority and
bootstrap identity, and compiled contexts. The new system_ext seapp contexts
are 525 bytes, SHA-256
`065dde02949dfcca65a27825b3f63a8da5bfadfc55b6218a1a28a99b7f481dde`,
and contain the exact package-to-`priv_app` assignment. This corrected artifact
is the only SOS ZIP approved and is the one installed below; the earlier hash
is retained solely as failed historical evidence.

**Corrected SOS physical runtime, live revision, and restart gate
(2026-08-15):** Recovery accepted the corrected 1,247,480,012-byte OTA above
with `adb sideload` reporting `Total xfer: 1.00x`; data was not formatted.
After reboot the SM-A336B reported
`lineage-23.0-20260815-UNOFFICIAL-sos_a33x`, Android 16 / API 36, FYH2,
unlocked/orange AVB, warranty bit `1`, encrypted FBE, and SELinux
`Enforcing`. Priority-1000 HOME resolved to
`dev.sos.experience/.SosHomeActivity`; `ro.sos.authority=on-device` and
`ro.sos.home=dev.sos.experience` were present. The authority ran as PID 938
in `u:r:sos_authority:s0`, and the application ran as PID 2061 in the
canonical enforcing `u:r:priv_app:s0:c154,c256,c512,c768` domain. The app
selected the Mali-G68 Vulkan adapter, opened its 1080x2400 window, logged a
healthy five-second event-loop heartbeat, and used its TCP provider transport
with an empty `adb reverse --list`. The ignored first healthy HOME screenshot
is `evidence-20260815/sos-home-corrected.png`, 171,681 bytes, SHA-256
`67ed68a8f883bb9c3fedfe27759b65b29c3c9d7e99c63f050baadfd733cc4364`.

The localhost authority `current` request established bootstrap revision
`b0d20599c81f62db31cfffd4883289e64a12ee9ada6f20a1c92ef518277e9be4`,
state revision `0`, and source SHA-256
`a9bb30563d21d05912e9b58e24d8455088686f41c82058ec9c566a32758193f4`.
The normal `run-as` candidate-copy helper could not traverse this package's
canonical `privapp_data_file` directory; audit evidence identified that as a
test-tooling limitation in Lineage's `runas` policy, not an SOS service
denial. Adding product policy solely for `run-as`, weakening the app domain,
or issuing authority mutations by hand were rejected. Rooted debugging was
temporarily enabled from Developer Options, `adb root` copied the candidate
with UID/GID 10154, mode 0600, and the exact
`privapp_data_file:s0:c154,c256,c512,c768` label, and the presentation path
remained the component that requested validation and activation.

The candidate was `experiences/timeflow.luau`, 5,678 bytes, SHA-256
`4983de6756ef4b21ba6a0eddaed9f2a01f4363b0ab18d0292f55987f49f7ceb9`.
`am start -W -a android.intent.action.VIEW -d sos://reload` returned a hot,
successful launch. Runtime evidence reported `candidate_validated` with
7,204 us compile, 5,493 us render, and 12,716 us worker-total time, followed
by `android_authority_revision_activated`. The authority returned new
revision
`32fa86a739260e3b13a7bf7f4bc9639708a7d9517d852c6bfe71acb13a552f59`,
state revision `1`, and the candidate source hash while the application
remained PID 2061. The visibly distinct Timeflow screenshot is
`evidence-20260815/sos-timeflow-activated.png`, 178,465 bytes, SHA-256
`ca07d68fabaf65412598dd4c5592d68416c2796a0119548ce2621eda0bef985d`.

Two independent forced-death tests passed on hardware. Killing authority PID
938 caused init to start PID 2946; application PID 2061 stayed alive and a
fresh localhost request returned the exact activated revision and state.
Killing application PID 2061 then produced PID 3000; authority PID 2946
stayed alive, SOS resumed as the focused HOME, selected Mali-G68 Vulkan,
reported its live window and runtime worker in 9,104 us, and reattached to the
same revision. The final screenshot after both restarts is
`evidence-20260815/sos-timeflow-after-restarts.png`, 173,926 bytes, SHA-256
`3bb3189d0602011ac94347dbdb8239cfcd24ec6765333940a22fcfb4261c7042`.

Rooted debugging was then switched off in Developer Options; UI automation
read `checked=false`, `adb root` was rejected with `ADB Root access is
disabled by system setting`, and `adb shell id` returned UID 2000 in
`u:r:shell:s0`. Final read-back still showed SOS HOME focused, the two
expected enforcing process labels, SELinux `Enforcing`, the activated
revision, and no reverse mapping. The post-restart log scan found no SOS AVC,
fatal exception, or ANR. CameraService still enumerated five devices,
fingerprint sensor 0 reported zero HAL deaths, NFC was on, Bluetooth was on,
and the Salt SIM remained registered for voice and data on LTE. This completes
the physical SOS HOME, on-device activation, persistence, enforcing-domain,
process-recovery, and service-preservation smoke gates. Actual audio,
fingerprint enrollment, Wi-Fi association, calls/data transfer,
suspend/resume, thermal behavior, and a longer soak remain open and must not
be inferred from this smoke test.

### 2026-08-15 — SM-A336B Wi-Fi association and microphone capture gates

**Hypothesis / goal:** Close the two first functional hardware gaps after the
corrected SOS HOME bring-up: prove that the physical phone can associate and
transfer data over Wi-Fi, and prove an actual primary-microphone capture with
measurable audio rather than inferring either result from service presence.
This is a hardware baseline only; it does not claim an SOS-authored network or
recording experience.

**Changed / environment:** No repository code, boot image, partition, root
state, or SELinux policy changed. The physical SM-A336B remained on the
corrected SOS Android 16 image at Git revision `245e427`, SELinux enforcing,
with ADB as UID 2000. The owner entered the Wi-Fi credential directly in
Android Settings; the SSID, BSSID, credential, and assigned address were not
copied into logs. Lineage Recorder was used for the microphone capture. Its
notification permission, initially denied during the test, was granted after
the application correctly refused to start a foreground recording without it.

**Evidence:** Read-only ADB checks found an IPv4 address on `wlan0`, a default
route through `wlan0`, and an Android connectivity record with both Wi-Fi and
`VALIDATED`. DNS resolution for `api.openai.com` succeeded. Toybox `nc`
connected to `api.openai.com:443`, and the system `curl` completed TLS to
`https://api.openai.com/v1/models` and received HTTP `401`, the expected
unauthenticated application response. A raw ICMP probe to `1.1.1.1` did not
reply; it was rejected as a connectivity criterion because the named-host,
TCP, TLS, and Android-validation checks all passed.

The owner spoke near the handset while Lineage Recorder captured 41.720 s.
MediaStore reported a 7,359,452-byte WAV. It was pulled outside Git as
`evidence-20260815/sos-microphone-baseline.wav`, SHA-256
`344f363a4d4c9c6a945ad2b8af3d10e098d52b08c486ec953a9385f4517a2218`.
`file` identified little-endian RIFF/WAVE, signed 16-bit stereo PCM at
44,100 Hz. A read-only PCM scan measured channel RMS levels of -40.00 and
-20.76 dBFS, peaks at 0.00 dBFS, 14 total clipped samples, and 231 of 418
100-ms windows above -45 dBFS. The artifact is not tracked by Git.

**Failures / fixes:** The first Recorder interaction produced a 5.260-second
capture and was rejected as too short. Denying the notification prompt caused
Recorder to explain that its required foreground-recording permissions were
unavailable; granting only the requested notification permission and
restarting Recorder repaired the path. The accepted second capture is the
artifact named above. No microphone playback, speaker, earpiece, Bluetooth
audio, or call-audio claim follows from this recording.

**Decision / next gate:** Physical Wi-Fi association/data transfer and an
actual microphone capture now pass on the corrected SOS image. The next gate
is an SOS-native network capability: Luau owns the visible setup journey and
emits bounded typed effects, while trusted Android code owns Wi-Fi authority
and keeps credentials out of Luau state, revision source, logs, screenshots,
and agent context. Audio playback and the remaining hardware matrix stay open.

### 2026-08-15 — SOS-native Wi-Fi capability, local implementation gate

**Hypothesis / goal:** Put ordinary Wi-Fi status, discovery, association, and
disconnect control inside the permanent SOS experience without giving Luau or
a future agent direct Android Wi-Fi authority and without allowing a password
to enter the revision/state protocol.

**Changed / environment:** `ExperienceModel` now has a serializable redacted
network snapshot and the Luau effect decoder admits only
`network.refresh`, `network.connect`, and `network.disconnect`. The default and
Timeflow experiences render the connection/validation state and a bounded
network list. The Android host verifies a connect selection against its most
recent trusted scan snapshot before calling the new `GpuiWifi` helper. That
platform-signed helper owns `WifiManager`, the native password dialog, and the
confirmation dialog for disconnect. Snapshots contain SSID, coarse signal,
security class, and saved/connected booleans, but omit BSSID, addresses, and
credentials. Password input disables save/autofill, is cleared on every exit,
never crosses JNI, and is not logged. The manifest requests the network-state,
Wi-Fi-state, Wi-Fi-change, and signature-level `NETWORK_SETTINGS` permissions;
the SOS APK is already installed as a platform-signed privileged system-ext
application.

**Evidence:** `cargo test --locked -p experience-ir -p providers-fake
-p runtime-luau` passed 4 experience-IR, 5 fake-provider/providerd, and 20
runtime-Luau tests plus doc tests. `./tools/sosctl validate` accepted both
`experiences/default.luau` (8,927 bytes, 67 nodes) and
`experiences/timeflow.luau` (8,547 bytes, 69 nodes). The final focused rerun
passed 21 runtime-Luau tests, including the bounded network-selection effect.
After correcting one JNI method-name conversion,
`./tools/sosctl m1-check --abi arm64-v8a` completed.
`./tools/sosctl m1-build --abi arm64-v8a --home` then compiled the release
ARM64 Rust library, compiled `GpuiWifi.java`, assembled all 36 Gradle tasks,
and produced ignored `artifacts/sos-experience.apk`, 38,303,919 bytes,
SHA-256 `c3d59a8468b24b47a2bf8f463bc102b5cd1ecd96fac72025ff69bcdf4bbfb165`.

**Failures / fixes:** Including the Linux `sos-experience` test target in the
first focused Cargo command reached the system linker but failed because this
host lacks `libxkbcommon` and `libxkbcommon-x11`; the portable crates were
rerun independently and passed, while the actual Android target and APK were
used for host-specific compilation. The first Android cross-check also found
that JNI 0.22 does not accept a dynamic Rust `&str` as a method identifier;
converting it to `JNIString` fixed the type error. Neither failure was a Luau
validation or Android source failure.

**Decision / remaining risk / next gate:** The architecture and local Android
build gate pass. Android remains the capability boundary; Luau owns policy and
presentation, and no Wi-Fi secret is part of mutable experience data. This is
not yet a hardware claim. The next gate is a full inspected a33x OTA followed
by on-device permission read-back, scan rendering, saved-network reconnect or
a deliberately entered native-dialog connection, validation, disconnect
confirmation, SELinux/ANR/fatal scans, and restoration of the working network.

### 2026-08-15 — Resident Android experience agent and credential boundary

**Hypothesis / goal:** Make the permanent phone capable of accepting an agent
request and transactionally changing its own Luau experience, first with a
deterministic offline provider and then with an explicitly configured OpenAI
provider. Keep model credentials outside Luau/JNI, retain the existing SOS
revision authority as the only activation path, and finish every software and
artifact gate that does not require touching the phone while its owner is away.

**Changed / environment:** The Android host now owns a resident-agent channel.
Both providers emit the same bounded tool/activity/candidate updates; the fake
provider deterministically alternates the complete Daily Flow and Timeflow
experiences, while the live provider calls the Responses API with
`gpt-5.6-luna`, one forced strict `propose_experience` function, parallel tool
calls disabled, and response storage disabled. A proposal is bounded to 256
KiB, compiled, rendered against the exact current redacted model, scene
validated, then submitted to the normal worker/state/revision/presentation
transaction. It cannot directly install a revision or bypass visible-frame
commit. Agent and network effects are removed before provider-authority commit
and handled only by trusted Android code.

The default, Timeflow, and Daily Flow revisions expose `OPENAI`, `FAKE`,
`REMOVE KEY`, and prompt controls. Daily Flow also gained the trusted network
surface, so the deterministic fake change cannot remove ordinary Wi-Fi access.
Luau can emit only `agent.prompt`, `agent.configure_openai`, `agent.use_fake`,
and `agent.clear_credential`; it never receives a credential. The native
password field disables save/autofill/suggestions, filters obscured touches,
sets `FLAG_SECURE`, and clears itself on exit. Java encrypts the API key with a
randomized AES-GCM key held under Android Keystore alias
`sos.openai.api-key.v1`, requires the device to be unlocked, and stores only
ciphertext/IV in no-backup app-private preferences. Plaintext stays inside the
trusted Java request bridge and is not logged. Only the prompt and complete
active Luau source are sent to OpenAI.

The supported embedded credential is a project API key, not reused Codex
consumer OAuth. The public OpenAI API contract documents API keys and
short-lived workload-identity bearer tokens; it does not document repurposing
Codex OAuth for an embedded third-party client. The same contract explicitly
warns against exposing long-lived keys in apps. This direct-key path is
therefore a device-owner prototype boundary, not the eventual fleet design.
A production deployment should exchange device identity through a controlled
relay for a short-lived, tightly scoped credential. Until then, use only a
dedicated low-spend, readily revocable project key.

The durable APK path changed from `assembleDebug` to a locally signed release
build with `debuggable=false` and JNI debugging disabled; the a33x import still
re-signs it with the platform certificate. Both the local APK builder and the
target-files inspector now reject a debuggable APK and require Android backup
to be disabled.

**Evidence:** `cargo fmt --all --check` passed. `cargo test --locked -p
experience-ir -p providers-fake -p runtime-luau` passed 4 experience-IR, 5
fake-provider/providerd, and 22 runtime-Luau tests plus doc tests, including
the three new bounded agent credential effects. `./tools/sosctl m1-check
--abi arm64-v8a` passed the actual Android cross-target. Exact final validators
reported default at 9,856 bytes / 74 nodes, Timeflow at 9,485 bytes / 76 nodes,
and Daily Flow at 13,612 bytes / 67 nodes. `./tools/sosctl m1-build --abi
arm64-v8a --home` compiled Rust and `GpuiAgent.java`, completed 44 release
Gradle tasks, and passed the new manifest gates. The ignored ARM64-only
`artifacts/sos-experience.apk` is 37,764,812 bytes, SHA-256
`5d2f0539bae49c4bdbe0081cf339cd481dfce69d017e77b467634557260ac661`.
`aapt2 dump xmltree` showed package `dev.sos.experience`, HOME alias
`SosHomeActivity`, `allowBackup=false`, and no `debuggable` attribute (the
Android default is false).

Before this combined implementation, the Wi-Fi-only host had also been staged
into an ignored OTA which passed every then-existing inspector:
`lineage-23.0-20260815-UNOFFICIAL-sos_a33x.zip`, 1,247,513,996 bytes,
SHA-256
`ab7d37334d70ca630f621ecff35512a6c919d9de0f956316109b35d41232aeec`.
ZIP, whole-package signature, live-PIT ceilings, package/target-files image
identity, complete AVB graph, recovery, VINTF, SOS contents, and policy gates
passed. Retrospective `aapt2` inspection then found
`android:debuggable=true` in its 41,598,822-byte target-files APK (SHA-256
`095fc8fb1ba829e6d913640078b210f64125e9a934eb68e98f1ec43bcb4e2747`).
That OTA is rejected, not approved for sideload, and is retained only as
historical evidence. It was never installed. The same inherited Gradle path
means the currently installed core SOS APK is also presumed debuggable; no
credential has ever been configured in it. The combined release OTA is a
required packaging-security correction as well as the feature update.

**Failures / fixes:** The first Android cross-build exposed a missing direct
`serde` dependency for the status envelope; adding the target dependency fixed
it. Inspecting the then-current APK and the Wi-Fi-only target-files exposed
`android:debuggable=true`; changing only native Rust optimization would not
repair that application boundary, so the complete Gradle packaging path and
both artifact inspectors were changed to release/non-debuggable gates. The
Linux application test target remains
unrunnable on this workstation because the system development link interfaces
for `libxkbcommon` and `libxkbcommon-x11` are absent; portable unit suites and
the real ARM64 Android build cover the changed code instead.

The phone accepted `adb reboot recovery` while still remotely reachable, but
Lineage Recovery's main menu intentionally presents an unauthorized ADB
interface until a person selects `Apply update` then `Apply from ADB`. No wipe,
install, key entry, or other mutation occurred after that reboot. This recovery
UI cannot be driven through its current unauthorized transport. On a later
Android session, test Lineage's `adb reboot sideload-auto-reboot` target as the
preferred no-touch OTA route rather than assuming support.

**Decision / remaining risk / next gate:** The deterministic and live agent
architectures, credential boundary, non-debuggable packaging, and local ARM64
gates pass. No on-device agent, Keystore, OpenAI, Wi-Fi UI, or revision-change
claim is made yet. Build and fully inspect one combined a33x OTA while the
phone remains parked in Recovery. Installation requires one physical menu
selection unless a previously authorized/automatic recovery entry can be
proven in a later session. After boot, run the deterministic fake end to end
first, then—with the owner present—configure a dedicated API key and test one
live experience change, followed by credential removal, SELinux/fatal/ANR
scans, process-restart persistence, and network restoration.

### 2026-08-15 — Combined Wi-Fi/agent OTA offline artifact gate

**Hypothesis / goal:** Produce one no-wipe a33x update containing the trusted
Wi-Fi surface, deterministic/live resident agent, and corrected non-debuggable
packaging. Prove every package, partition, signature, compatibility, and SOS
content property offline without attempting to drive the unauthorized Recovery
transport or requiring the absent owner.

**Changed / environment:** From clean SOS revision `a29d43d`,
`./tools/a33xctl build-sos` rebuilt the release ARM64 HOME, ARM64 authority,
bootstrap, affected system_ext image, recovery/boot family, target files, and
signed non-A/B OTA in the separate Lineage checkout. It did not contact or
write the handset. The exact final ignored artifacts are:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `artifacts/sos-experience.apk` (local release signature) | 37,764,812 | `beb8b73187121c9ef9ba7b4e5c90b896f65f6e153ddff23741bf7310282db2db` |
| target-files `SYSTEM_EXT/priv-app/SosShell/SosShell.apk` (platform signature) | 40,679,549 | `0335ff7d4e3f7a147759e9a4285c8d730d4ff1d1572c67592a45422263c87ae9` |
| target-files `SYSTEM_EXT/bin/sos-authority` | 1,219,472 | `36f11b3510fe59ce6b109c223a1d2ef10fb0e70eb3d9e5f1177baa795f4c669c` |
| target-files `SYSTEM_EXT/etc/sos/default.luau` | 9,856 | `b30ad9c8f1fb933a5385f04fb340a3c8d66c02f962389b68e47fd5bcc8c89aaa` |
| `lineage-23.0-20260815-UNOFFICIAL-sos_a33x.zip` | 1,247,268,938 | `b7b042c69795365408c9bf744e424e818486e0f76747004b56a8ae1df784e2d6` |

The local APK SHA differs from the earlier release-build record because the
debug-key ZIP signature metadata is regenerated on each Gradle packaging run;
the source revision, size, manifest gates, embedded library, and final artifact
identity are recorded explicitly here. AOSP then produced the separately
identified platform-signed APK above.

**Evidence:** The Lineage build completed 49 affected target actions in 3m29s,
reported VINTF `COMPATIBLE`, and signed the OTA successfully. `./tools/a33xctl
inspect-sos` exited 0. It passed compressed-data integrity and whole-package
signature verification; exact SOS target selection; package/target-files
image identity; the live-PIT ceilings for boot, dtbo, recovery, vendor_boot,
and vbmeta; each embedded AVB footer and the complete vbmeta hash/hashtree
graph; repaired recovery init identity; ARM64-only ABI and HOME alias; exact
authority/bootstrap identities; product properties; and compiled seapp/file
contexts. The new manifest assertions passed: the platform-signed APK has
`allowBackup=false` and no `debuggable` attribute. A read-only DEX string check
found `GpuiAgent`, the Keystore alias, `gpt-5.6-luna`, and the HTTPS Responses
endpoint in that exact platform-signed APK. No credential value exists in any
artifact.

`adb devices -l` after inspection still returned serial `RFCT50EGFCN` as
`unauthorized` on USB transport 19. This is the expected Lineage Recovery main
menu state and independently confirms that no sideload, boot, wipe, or Android
interaction was performed during the remote build.

**Failures / fixes:** There was no failed build or inspection gate. Incremental
kernel/target-file tooling printed its known non-fatal depmod, restat, missing
optional build-property, and ramdisk device-node warnings; the build completed,
VINTF passed, and the explicit recovery/PIT/AVB/content inspections all passed.
The previously built debuggable Wi-Fi-only OTA remains rejected and is
superseded by the exact combined hash above.

**Decision / remaining risk / next gate:** This combined OTA is the only
offline-approved next update. It is not installed and carries no physical
runtime claim. Current Recovery cannot accept shell or sideload commands until
a person selects `Apply update` then `Apply from ADB`; there is no safe host
automation around that authorization gate in the present state. Leave the
phone untouched. When physical access returns, select sideload once, transfer
only the exact SHA-256 above without formatting data, reboot, and execute the
deterministic agent, Wi-Fi, Keystore/live-agent, credential-removal, enforcing
SELinux, crash-scan, restart-persistence, and hardware-regression gates. From a
future healthy Android session, separately test `adb reboot
sideload-auto-reboot` so later approved OTAs may become genuinely unattended.
The exact checked-out Lineage source already contains that path:
`system/core/init/reboot.cpp` writes `--sideload_auto_reboot` into the bootloader
message, Recovery calls `ApplyFromAdb`, and then requests `INSTALL_REBOOT` even
on an install error; Lineage's own `eat` helper uses the same command followed
by `adb wait-for-sideload` and `adb sideload`. This passes the source-support
gate, but not the physical runtime gate on this device. It cannot change the
already-running unauthorized Recovery instance because writing the bootloader
message requires an authorized Android shell before the reboot.

### 2026-08-15 — First combined OTA install and stale PackageManager cache rejection

**Hypothesis / goal:** Install the offline-approved combined OTA without a
wipe, prove the release/non-debuggable package on hardware, then advance to the
Wi-Fi and resident-agent gates only if PackageManager and JDWP agree with the
audited manifest.

**Changed / environment:** The owner physically selected Recovery's `Apply
update` / `Apply from ADB` entry. The host revalidated the exact
1,247,268,938-byte OTA, SHA-256
`b7b042c69795365408c9bf744e424e818486e0f76747004b56a8ae1df784e2d6`,
and `adb devices -l` identified the expected `SM_A336B/a33x` sideload target.
`adb sideload` completed with `Total xfer: 1.00x`; no format-data action was
performed. The owner selected normal reboot. Android returned with ADB as UID
2000 shell and SELinux enforcing.

The image itself installed correctly. `/system_ext/priv-app/SosShell/
SosShell.apk` is the exact inspected 40,679,549-byte platform-signed file,
SHA-256
`0335ff7d4e3f7a147759e9a4285c8d730d4ff1d1572c67592a45422263c87ae9`.
HOME resolves to `dev.sos.experience/.SosHomeActivity`, the APK is ARM64, the
authority/app run in `sos_authority`/canonical `priv_app` enforcing domains,
and no ADB reverse exists. Revision
`32fa86a739260e3b13a7bf7f4bc9639708a7d9517d852c6bfe71acb13a552f59`
and state revision 1 survived exactly across the OTA.

**Evidence / rejected gate:** The exact installed APK's `aapt2` tree has
`allowBackup=false` and no `android:debuggable` attribute, and `ro.debuggable`
is `0`. PackageManager nevertheless reported `DEBUGGABLE`, and `adb jdwp`
listed the exact SOS PID 2015. This is a real failure, not a cosmetic dumpsys
artifact, so no API credential was entered and agent/Wi-Fi E2E did not begin.
PackageManager also reported version 1 and a fixed APK timestamp of
2009-01-01, while its package-cache directory was newer.

Lineage source inspection identified the deterministic cause. `PackageCacher`
keys entries by package filename, parse flags, and path hash, and considers an
entry current using only APK versus cache `st_mtime`; it does not compare APK
length or content. SOS's reproducible system image gives successive APKs the
same 2009 timestamp. `PackagePartitions.FINGERPRINT` would normally select a
new cache directory, but these same-day incremental builds reused
`ro.build.version.incremental=1786782878` and the stock-overridden partition
fingerprints, so the pre-OTA parsed debug manifest was reused for the new exact
APK bytes.

**Fix / local evidence:** The SOS APK is now version code 2 / version name
0.2.0. More importantly, `build-sos` supplies an explicit build number of the
form `sos.<12-char-SOS-revision>.<12-char-staged-APK-SHA>` to the Lineage
build. That changes `ro.build.version.incremental` and therefore the package
partition fingerprint/cache directory for every materially different staged
APK. `inspect-sos` now rejects an OTA unless both the APK version bump and the
unique incremental value are present. `bash -n`, `cargo fmt --all --check`,
the ARM64 `m1-check`, and a complete 44-task release APK build passed; `aapt2`
reported version 2, version name 0.2.0, backup disabled, and no debuggable
attribute.

**Decision / remaining risk / next gate:** The first combined OTA is rejected
for credential use even though its APK bytes are correct, because the running
PackageManager state remained debuggable. Build and fully inspect a cache-
invalidating replacement OTA, enter it from the now-authorized Android session
with `adb reboot sideload-auto-reboot`, and require version 2, absence of the
PackageManager debug flag, absence of the SOS PID from `adb jdwp`, exact new
build increment, and all original security/runtime checks before resuming the
functional gates. This also becomes the first hardware proof of unattended
Lineage sideload if it succeeds.

### 2026-08-15 — Cache-invalidating replacement OTA offline gate

**Hypothesis / goal:** Produce a replacement combined OTA whose content-derived
build identity forces PackageManager to discard the stale parsed manifest, and
approve it for unattended sideload only after the full offline artifact gate.

**Changed / environment:** Clean SOS revision `be793baf7c5d` staged the
37,764,812-byte release APK, SHA-256
`5b205cc28acb39c9a4e8c290e2718d0f52fa0c8d2c03651deb0c2e138f3ce01c`.
`./tools/a33xctl build-sos` supplied the resulting build increment
`sos.be793baf7c5d.5b205cc28acb` and completed the Lineage target-files and
signed non-A/B OTA build in 4m52s. No device write occurred during this build.
The exact ignored output approved by this gate is:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| target-files `SYSTEM_EXT/priv-app/SosShell/SosShell.apk` | 40,679,549 | `5ca45e90685f7f4c990f020a5584c92482992c55b48fb57a68789173405691f1` |
| `lineage-23.0-20260815-UNOFFICIAL-sos_a33x.zip` | 1,247,262,059 | `36e52b5384c2917e5003c7880d894c48cc925cd99a858ec3cf06c8bc787da38c` |

**Evidence:** `./tools/a33xctl inspect-sos` exited 0. ZIP integrity and the
whole-package signature passed; VINTF was `COMPATIBLE`; all live-PIT ceilings,
package/target-files identity, recovery init, embedded image AVB footers, and
the complete vbmeta hash/hashtree graph passed. The exact platform-signed APK
is ARM64-only, version code 2 / version name 0.2.0, disables backup, and has no
debuggable manifest attribute. Target files contain
`ro.system_ext.build.version.incremental=sos.be793baf7c5d.5b205cc28acb`; the
product properties, HOME alias, authority/bootstrap hashes, and compiled
SELinux contexts also passed their assertions.

**Failures / fixes:** The build emitted the known non-fatal optional-property,
ramdisk device-node, and depmod warnings, then completed successfully. There
was no failed artifact gate. This artifact supersedes both the rejected
Wi-Fi-only OTA and the first combined OTA for future installation.

**Decision / remaining risk / next gate:** The exact replacement hash above is
approved for no-wipe installation. It has not yet been installed and carries
no hardware claim. From the currently authorized Android session, use
`adb reboot sideload-auto-reboot`, transfer only this archive, wait for Android,
and require the exact build increment and installed APK hash, version 2, no
PackageManager `DEBUGGABLE` flag, and no SOS PID in JDWP before any credential
or agent test. If those gates pass, continue in order with deterministic agent,
trusted Wi-Fi, live OpenAI, restart persistence, and final enforcing/crash and
hardware-regression scans.

### 2026-08-15 — Native Android Pi, Codex subscription, and live rewrite gate

**Hypothesis / goal:** Replace the provisional Android-specific OpenAI request
client with the same Pi runtime used on Linux, run Pi directly in native
ARM64/Bionic Node without a WebView, support direct OpenAI and OpenRouter API
keys plus Codex subscription OAuth, and prove a real model can change the
physical phone's experience without crossing the trusted revision boundary.

**Changed / environment:** `tools/build-android-node` now reproducibly builds
official Node v24.19.0 source commit
`cdc1b38d40cb567b7ad0b39c86addf830a0af0ae` with Android NDK r29 for ARM64 /
API 31. The pinned patch under `aosp/patches/node-android-v24.19.0` separates
host and target tools, uses modern NDK ARM64 hardware-capability definitions,
fixes Android zlib configuration, and applies Node's bundled Android V8 trap-
handler setting. The ignored runtime artifacts are:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `artifacts/sos-node-android-arm64` | 91,280,288 | `e1e6cf7de807baea6fa1d2a81bd6da29d777ab08149645431ebbe283bda33607` |
| `artifacts/sos-node-libc++_shared.so` | 1,373,744 | `fa9e42dff4c2b14bd8dec3f302aef93fcac3827704053801268090b4caa377d0` |
| `services/sos-agent/dist/android-runner.cjs` | 2,001,174 | `7d9bee75d9912399ea87715c82a51cfb28a3020e244df58685e5cfd5e650157e` |

`services/sos-agent/src/android-runner.ts` is a single bounded stdin/stdout
request runner over real `@earendil-works/pi-agent-core` and `pi-ai`. It can
report its catalog, execute the faux provider self-test, perform Pi's
`openai-codex` device-code login, or run one live authoring prompt. It exposes
only the existing context, validate, and submit tools. A candidate remains
staged until Rust independently compiles, renders, validates, and commits it;
Node cannot directly mutate the revision store. OpenRouter is now registered
through Pi's provider implementation alongside OpenAI, Anthropic, and Codex.

The a33x product packages Node at `/system_ext/bin/sos-node`, its C++ runtime,
and the bundle plus exact authoring documents/examples. A dedicated immutable
`sos_node_exec` file label grants the named platform-signed HOME only read and
execute access. `GpuiAgent` launches native Node as a child in the existing
enforcing `priv_app` domain and passes exactly one JSON document over anonymous
pipes. No WebView, shell, general filesystem tool, writable code path, or ADB
transport participates.

The trusted Android surface now selects direct OpenAI (`gpt-5.6-luna`), direct
OpenRouter (`openai/gpt-5.4-mini`), Codex subscription
(`openai-codex` / `gpt-5.6-sol`), or the deterministic fake. Provider-scoped
credentials are AES-GCM encrypted under non-exportable, unlock-bound Android
Keystore alias `sos.agent.credentials.v2`; only ciphertext and IV persist.
Plaintext is passed to Pi only in the anonymous request pipe and never reaches
Luau, JNI, argv, environment variables, a credential file, a log, or a
screenshot. The default, Timeflow, and Daily Flow sources expose all provider
actions while preserving the required agent composer.

**Local and temporary-device evidence:** `npm test --prefix
services/sos-agent` passed all 5 tests, including Pi's Codex subscription and
OpenRouter registrations. The Android bundle's real catalog under native Node
reported `platform=android`, `arch=arm64`, Node `v24.19.0`, and available
OpenAI, Anthropic, Codex, and OpenRouter model entries. Native TLS completed.
The bundle self-test used the exact context, validate, submit order. `cargo
test -p runtime-luau --locked` passed 22 tests. Final validators accepted
default at 10,537 bytes / 79 nodes, Timeflow at 10,172 / 81, and Daily Flow at
14,447 / 72. `./tools/sosctl m1-build --abi arm64-v8a --home` completed all 44
release tasks. The first inspected native-Pi OTA was 1,277,021,382 bytes,
SHA-256
`a515008d154bf1d0e2599d45bd3fb5c7b08781bb8bb602af7bdc56150edbf974`,
with build increment `sos.dc0062a5a23d.171782b6d5ce`.

**Physical install and real Pi E2E:** That first OTA installed without a wipe
at `Total xfer: 1.00x`. Android returned boot-complete with enforcing SELinux,
SOS HOME/on-device authority, no reverse tunnel, UID-2000 ADB, and no SOS JDWP
process. Selecting Fake and submitting through the Luau composer ran the
deterministic path and activated updated Daily Flow revision
`3839f95c7dc6e44efad083bbd06cd41bf3efe57dedf70044b3d0ac9dd6d10c14`,
source SHA-256
`e6623bc23473a85d2d74f619e3f1c506a0155b761fef0a684e83448242458690`.
It reached visible commit in 81.410 ms with 1.844 ms compile, 3.104 ms render,
and 4.966 ms worker-total time, exposing the new provider controls.

The owner then completed Pi's official Codex device-code authorization. The
phone reported `Codex subscription ready · gpt-5.6-sol`; its OAuth document
was stored only through the Keystore boundary. Submitting the existing Daily
request through the phone's composer launched native `sos-node` PID 5134 under
the HOME app's enforcing MCS-labelled `priv_app` domain. Real Pi completed in
about 129 seconds and proposed a visibly different green Daily experience.
The trusted host logged request 67 with 3,022 us queue, 1,662 us compile,
2,370 us render, and 4,049 us worker-total time; source-to-visible after host
receipt was 104,393 us. It activated revision
`fe7e19b3e63572b6522dcafec548fb4022f44378a6b4ebe0155c0860263de8a4`,
source SHA-256
`9b0d8f6022e175b647a88f484247f3d4fcebe8359b8fe49cf11afce4cb64903e`.
The ignored first live screenshot `sos-pi-codex-live.png` is 181,961 bytes,
SHA-256
`4bca0a5adbfac3ac5789fa768fe3bc2c80232c72a0914978dd80882588cc4e37`.
An independent HOME death changed its PID while the authority PID and generated
revision remained stable; restart logged `provider=openai-codex
configured=true`.

**Failure / fix:** The first Codex login attempt uncovered a genuine lifecycle
bug. Native Node remained healthy for more than 45 seconds from an ADB shell
and indefinitely while HOME was foreground, but Android reaped the child as a
phantom process roughly ten seconds after the external browser backgrounded
HOME. Temporarily setting `settings_enable_monitor_phantom_procs=false`
isolated that cause; it was immediately deleted and final read-back is `null`.
Keeping the global mitigation was rejected. The durable change adds an
unexported `dataSync` `GpuiAgentService` and the two matching foreground-
service permissions. HOME starts it only for a live provider request or OAuth
login and stops it in every completion/cancellation path. The first Java build
failed because this inherited package has no generated `R` namespace; using
the platform notification icon fixed the release build. A glibc ARM64 Node
tarball, the old Node-18 nodejs-mobile runtime, a WebView execution model, and
an ESM bundle with dynamic YAML loading were also rejected: they respectively
do not target Bionic, miss Pi's Node version floor, weaken the runtime boundary,
or fail as a self-contained artifact. The CommonJS esbuild bundle is the
accepted packaging path.

**Final OTA and installed regression evidence:** The foreground-service build
completed in 3m35s with increment `sos.dc0062a5a23d.b69ae2204b16`.
`./tools/a33xctl inspect-sos` passed ZIP/signature integrity, VINTF, every live
PIT ceiling, package/image identity, the complete AVB graph, repaired recovery,
ARM64/HOME/nondebuggable/no-backup gates, exact Node/libc++/runner/doc/example
identity, manifest foreground-service assertions, properties, and compiled
SELinux contexts. The final artifacts are:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| platform-signed SOS APK | 40,704,817 | `f5e8ba397ada9e4614b9d02b96ca2ec135ae66dc7501004a801145789caf2252` |
| `lineage-23.0-20260815-UNOFFICIAL-sos_a33x.zip` | 1,276,639,235 | `b5da4fdc0a2b448ba989ce41cdac8e11308983addf7ae813256aeca08dff737c` |

From authorized Android, `adb reboot sideload-auto-reboot` entered Recovery's
automatic sideload target. The exact OTA transferred at `Total xfer: 1.00x`
and Recovery rebooted Android without touch or data formatting, closing the
unattended OTA gate. Installed read-back returned the exact increment,
`ro.debuggable=0`, no package `DEBUGGABLE` flag, no JDWP PID, enforcing
SELinux, priority-1000 SOS HOME, on-device authority properties, and an empty
reverse list.

The generated experience and Codex status survived the OTA. With the external
browser resumed for more than one minute, Node remained the same PID 2878 and
ActivityManager reported `GpuiAgentService` as foreground with dataSync type
`0x00000001`; no phantom-process or kill event appeared. Returning to HOME and
cancelling the fresh one-time flow stopped both Node and the service while the
prior Codex credential still reported ready. The physical OpenRouter and
OpenAI configuration buttons each opened a `password=true` field in a
`SECURE` activity window; both were cancelled without a secret.

A final independent HOME force-stop changed PID 2083 to 3312 while authority
PID 949 stayed fixed. Direct revision-authority read-back returned revision
`fe7e19...`, state revision 58, source SHA-256 `9b0d8f...`, 10,250 source
bytes, and no assets. HOME again logged the configured Codex provider. Fresh
SELinux/fatal/ANR scans were empty, and phantom monitoring remained at its
default unset value. The ignored final screenshot
`sos-pi-codex-after-final-home-restart.png` is 177,603 bytes, SHA-256
`6c8788c7f4eb369408037b2c77b0a1ae228d938d60d3b6418fe13aba6191bf8d`.

**Decision / remaining risk / next gate:** Native Pi on Android, bounded tool
use, Codex subscription login/refresh storage, one real model-authored
experience change, transactional activation, OTA and process persistence,
foreground browser survival, and unattended no-wipe OTA now pass on the
SM-A336B. WebView and SOS-specific provider transports are rejected for this
path. No direct OpenAI or OpenRouter key was entered, so their dialogs,
catalog, bundle, and security boundaries pass but their real API calls remain
unclaimed. Speaker/earpiece/Bluetooth/call audio, fingerprint enrollment,
cellular call/data transfer, suspend/resume, thermal, and soak testing also
remain open. No further reboot is required for this milestone.

## 2026-08-15 — Text-field backspace deadlock and horizontal overflow repair

**Goal / hypothesis:** Reproduce the reported SOS failure when backspace is
used in a text field and keep long single-line values inside their input bounds.
The hypothesis was that deletion exposed either an invalid UTF-8 selection or
an Android IME/GPUI synchronization fault, while the visual overflow came from
painting an unconstrained shaped line.

**Changed:** Android now releases the bounded IME-state queue mutex before it
requests a GPUI frame. The identical accessibility-action enqueue/wake ordering
was repaired at the same boundary. Android and Linux native text inputs now
retain a horizontal viewport offset, keep the focused caret visible, translate
pointer/IME coordinate queries through that offset, reset to the leading edge
when unfocused, and paint text, selection, and caret through the exact input
content mask. A focused Linux regression covers scrolling right, returning
toward the leading edge, and resetting when the full line fits.

**Evidence:** The pre-fix SM-A336B bug report captured the UI thread blocked in
`Java_dev_gpui_mobile_GpuiActivity_nativeOnImeState` from
`GpuiImeBridge$BridgeConnection.endBatchEdit`, while the GPUI `android_main`
thread was simultaneously blocked. The event log stopped immediately after
`nativeOnDeepLink: sos://ime` and Android reported an input-dispatch ANR. Source
inspection established the lock inversion: JNI retained `IME_STATES` while
`request_host_frame` waited on GPUI, and GPUI render waited on `IME_STATES`.

`./tools/sosctl m1-check`, Linux-host `cargo check --all-targets`, strict
Linux-host clippy, formatting, and `git diff --check` pass. With temporary
Fedora `libxkbcommon` development libraries supplied through `LIBRARY_PATH`
and `LD_LIBRARY_PATH`, the focused `linux_input::tests` suite passes all three
tests, including `horizontal_scroll_keeps_the_caret_inside_the_field`. The
release HOME build completed all 44 Gradle tasks. The platform-signed update
based on source revision `93cb6ba13ef308c515cf84a8eebdb9b3c340563f` plus
these dirty changes is
`sos-experience-platform-final.apk`, 37,813,431 bytes, SHA-256
`a6a80ee433628c54d86be95b65a69105cb3f0db694763c03ecdc7471fce8df61`.
Its certificate SHA-256 matched the installed product certificate before the
data-preserving update.

On the physical SM-A336B, an AOSP keyboard delete produced
`kind=delete`, updated and committed the text state, and was followed by three
successive five-second GPUI heartbeats with no ANR, fatal exception, or process
replacement. The restored accessibility tree contains the exact original
multilingual value `Caffè ☕️ – 明日のデザイン`. Visual inspection shows the
long agent request clipped at the right input edge and beginning at its leading
text while unfocused. The ignored screenshot `sos-final-overflow.png` was
195,925 bytes with SHA-256
`cec70dc752f09b9ba93eaacfd1ecea87de44fb82da6e0d3c5e06c7286ab7d1b3`;
it and the extracted bug report remained outside Git and were deleted after
inspection.

**Failures / fixes / rejected evidence:** The first focused Linux unit-test
invocation compiled the new code but could not link because this development
host lacks the unversioned `libxkbcommon` and `libxkbcommon-x11` development
libraries. Extracting the matching Fedora development RPMs outside the
repository and exposing those temporary link interfaces allowed the unchanged
test binary to link and all three focused tests to pass. The locally signed APK
was correctly rejected as update-incompatible; uninstalling or clearing user
data was rejected. Signing the already-built APK with the matching product
platform key allowed an in-place update. A diagnostic ADB text event was not
delivered to the hidden editor and the subsequent delete changed the coffee
emoji's presentation selector. A temporary UIAutomator accessibility action
restored the exact Unicode value, verified through the published tree, and the
helper was removed.

**Decision / remaining risk / next gate:** The causal Android lock inversion is
closed on physical hardware, and both native hosts share bounded single-line
painting and caret scrolling. Linux runtime interaction remains unclaimed;
perform a windowed Linux input interaction on a provisioned Linux host when
that platform gate is next exercised.

## 2026-08-15 — Repeated-backspace dispatcher self-deadlock follow-up

**Goal / hypothesis:** Reproduce the reported remaining failure under repeated
software-keyboard backspace and replace the earlier single-delete hardware gate
with a sustained IME deletion gate. The hypothesis was that releasing the SOS
IME queue lock closed one inversion but left another lock held while queued
foreground work executed.

**Changed:** `AndroidPlatform::flush_main_thread_tasks` now clones its dispatcher
out of the platform-state mutex before draining arbitrary foreground tasks. A
headless Android regression queues a task that probes the state mutex and
re-enters `primary_window`, proving that the task is not invoked while the
platform lock is retained.

**Evidence:** The SM-A336B reproduced a second input-dispatch ANR at 19:17:38.
Its UI thread was blocked in `nativeOnImeState` from
`GpuiImeBridge.BridgeConnection.setComposingRegion`, and `android_main` was also
in a futex wait. Source inspection identified the self-deadlock: the GPUI loop
held `AndroidPlatformState` across `dispatcher.flush_main_thread_tasks`; the
queued IME frame task then called `primary_window` and attempted to reacquire
that same mutex. The ignored diagnostic
`repeat-backspace-bugreport.zip` is 4,844,010 bytes with SHA-256
`b90f0089db18ba095166cfc6565d29cde1f380bcf7641f2a38210283549cb07b` and
contains `FS/data/anr/anr_2026-08-15-19-17-38-874`.

`./tools/sosctl m1-check`, formatting, and the release HOME build pass; the
build completed all 44 Gradle tasks. The platform-signed, data-preserving test
update is based on revision `a9fff8883185d4390223555d861242afd6dc6622` plus
the dispatcher change. `sos-repeat-backspace-fixed.apk` is 37,813,431 bytes
with SHA-256
`4c7b3904c50ccf4f43c60603a9e22dba42770bd215d026b744ba1551b0a0bb0a`;
its signing certificate SHA-256 is
`c8a2e9bccf597c2fb6dc66bee293fc13f2fc47ec77bc6b2b0d52c11f51192ab8`,
matching the installed product certificate.

On the physical phone, 40 software-keyboard delete presses at 80 ms spacing
removed all 16 prompt characters and delivered a further 24 `kind=delete`
states at selection `0:0`. PID 13747 remained unchanged, no new ANR, fatal
exception, or native signal appeared, and healthy GPUI heartbeats continued at
19:27:58, 19:28:03, and 19:28:08. A direct accessibility `ACTION_SET_TEXT`
restored the exact pre-test values `gpt-5.6-luna` and `I want dark mode`, both
verified through the final accessibility tree.

**Failures / fixes / rejected evidence:** The earlier single-delete test was
insufficient and is superseded by the repeated-delete/empty-boundary gate.
A direct cross-target Cargo command lacked cargo-ndk's compiler environment;
the normal strict Android gate passed. The corrected Android vendor test-target
check remains blocked before execution by an unrelated existing
`AndroidWindow::gpu_info` test that no longer compiles; it is not counted as a
passing regression test. Two coordinate-based diagnostic taps opened Codex and
OpenAI configuration dialogs because keyboard scrolling moved the virtual
bounds. No credential was saved or submitted; both dialogs were cancelled.
The restoration helper was changed from coordinate-based UIAutomator
`setText` to direct accessibility actions before restoring and verifying the
two affected values.

**Decision / remaining risk / next gate:** The remaining failure was the
generic Android foreground-dispatch lock boundary, not deletion or UTF-8
handling. The physical repeated-delete gate now passes across composing,
commit, ordinary delete, and empty-input delete states. Repair the stale vendor
test before counting the regression as an executed Android unit test; retain a
repeated IME edit burst in future device acceptance checks.

## 2026-08-15 — Split a33x into SOS Compat and Core shadow products

**Goal / hypothesis:** Start removing Android's visible ownership without
discarding its working telephony, network, Bluetooth, NFC, credential, display,
audio, and vendor infrastructure. The proposed boundary was one shared a33x
hardware/services/revision base with an Android-compatible product and a native
Core product, while treating removal of Java UI and removal of Zygote as
separate gates.

**Changed:** Replaced the single `lineage_sos_a33x` definition with shared
`sos_a33x_common.mk`, `lineage_sos_compat_a33x`, and
`lineage_sos_core_a33x` products. Compat carries the SOS Activity/overlay and
declares Android compatibility UI ownership. Core excludes the SOS Activity
and carries a 64-bit init-launched C++ SurfaceComposer/EGL probe with a dedicated
SELinux domain. Its init service is disabled and its ownership property is
`android-shadow`, so SystemUI and Launcher remain the recovery owner. Build and
inspection tooling now has `build-compat`, `build-core`, and `inspect-core`;
`build-sos` remains a Compat alias. The architecture, Core 0/Core 1 boundary,
per-surface cutover gate, and retained Android substrate are detailed in
[`android-product-split.md`](android-product-split.md).

**Evidence:** Product evaluation with `source build/envsetup.sh`, `breakfast
sos_compat_a33x` / `breakfast sos_core_a33x`, and `get_build_var` confirmed that
both products inherit the same a33x graph and SOS services. Compat selects
`SosShell` and its overlay; Core selects `sos-core-surface-probe` and excludes
both. SystemUI, Launcher3, Settings, and LatinIME remain selected in both at
this shadow stage. `m -j8 sos-core-surface-probe` completed, and `m -j8
selinux_policy` passed the merged policy, compatibility, and neverallow gates.

`./tools/a33xctl build-compat` completed in 6:09 and
`./tools/a33xctl inspect-sos` passed ZIP integrity, whole-package test-key
signature, VINTF, AVB, PIT partition ceilings, target-files contents,
properties, and SELinux checks. The artifact
`lineage-23.0-20260815-UNOFFICIAL-sos_compat_a33x.zip` uses build number
`sos.compat.301cefc50d20.0ee968c48b29`, is 1,276,655,898 bytes, and has SHA-256
`146b681f6472ac60006220faaf71fafffd6102af2d5ead6bacbca05475a77fa7`.

After formatting and signal-handling cleanup, `./tools/a33xctl build-core`
completed in 4:01 and `./tools/a33xctl inspect-core` passed the same image and
boot-chain gates. It additionally proved that the probe is AArch64, links
`libgui`, is correctly labeled, and remains disabled; that `SosShell.apk` is
absent; and that SystemUI plus Launcher are deliberately retained. The final
artifact `lineage-23.0-20260815-UNOFFICIAL-sos_core_a33x.zip` uses build number
`sos.core.301cefc50d20.c3937d2e6275`, is 1,256,417,960 bytes, and has SHA-256
`5f6bcabcb1160f48e75d41e5bbc0b4a7affdbb3498dff1d32723b1c0f205a60a`.
Its packaged `sos-core-surface-probe` is 51,464 bytes with SHA-256
`ba9799dfbcf61559e34550743066e10e0f4a7c5223d77993dc36216e909d5b73`.
`bash -n tools/a33xctl`, the checkout clang-format gate, and `git diff --check`
pass. `shellcheck` was unavailable on this host. No image was flashed or run on
the phone, so none of this is physical display, input, recovery, or latency
evidence.

**Failures / fixes / rejected approaches:** Android 16 rejected initial
two-field `COMMON_LUNCH_CHOICES`; the entries were removed and `breakfast`
allowed the release configuration to select `bp2a`. The first probe compile
used an absent `ISurfaceComposerClient` header, the wrong `DisplayMode`
namespace, and missing pthread/error declarations; using the available Android
16 interfaces fixed it. `Transaction::remove` is unavailable in this branch,
so shutdown hides the surface and releases its client. The first strict source
check found clang-format drift; formatting and blocking termination signals
before starting Binder threads were followed by a complete Core rebuild and
reinspection. A later cross-profile inspection found the Compat ZIP had been
removed by Android's expected install-clean while switching to Core; the docs
now require adjacent build/inspect pairs and the tool reports the correct
profile build command. Filtering inherited `PRODUCT_PACKAGES` at the leaf was
rejected because inherited package accumulation is not a safe ownership
boundary.
Immediately deleting SystemUI/Launcher or selecting no-Zygote was also rejected:
there is no native input, trusted lockscreen, recovery UI, or replacement for
the Java-managed phone/connectivity services yet.

**Decision / remaining risk / next gate:** The two product identities and
shared base are accepted. Compat is the continuation of the installable Android
product. Core is accepted only as a non-autostarting shadow bring-up target and
must not be called Core 0. The next gate is physical SM-A336B execution of the
manual native surface alongside Android: verify presentation through
SurfaceFlinger/HWC, start/stop and failure recovery, enforcing SELinux behavior,
and the absence of display/suspend regressions. Then port the GPUI host to that
`ANativeWindow`, add native input plus fixed trusted recovery/lock surfaces,
and only after those physical gates switch `ro.sos.ui_owner` to `native-sos`
and remove superseded Android UI packages from Core. No-Zygote remains a later
service-migration gate.

## 2026-08-15 — Physical Compat and Core-shadow device gates

**Goal / hypothesis:** Install both split products without wiping the physical
SM-A336B, prove Compat retains the working Android substrate while SOS owns
HOME, then run the disabled Core SurfaceComposer probe with Android still
available as the recovery UI.

**Changed / environment:** The connected `RFCT50EGFCN` handset identified as
`SM_A336B` / `a33x`, reported 99% battery, completed Android boot, orange
verified-boot state, and enforcing SELinux. The owner clarified that Launcher3
had only been selected while recovering from an earlier SOS crash. After the
Compat update, `cmd package set-home-activity
dev.sos.experience/.SosHomeActivity` restored the intended explicit HOME
choice without clearing application data. The Core probe remains disabled at
boot, but its init file now maps transient shell-writable property
`debug.sos.core.surface_probe=1` to start and `=0` to stop; the architecture
document records those manual development commands. A canary on the installed
pre-split image proved that shell can write and clear the `debug` namespace.

**Compat build, install, and device evidence:** A fresh
`./tools/a33xctl build-compat` completed in 4:09 with build number
`sos.compat.301cefc50d20.3d43de4361b8`. `./tools/a33xctl inspect-sos` passed
ZIP and whole-package signature integrity, live PIT ceilings, VINTF, recovery
init, every embedded AVB footer and vbmeta descriptor, APK/runtime identity,
product properties, and compiled SELinux assertions. The exact installed
archive is `lineage-23.0-20260815-UNOFFICIAL-sos_compat_a33x.zip`,
1,276,666,944 bytes, SHA-256
`e22f979c6f51a8f38463e7d2434b725f2714046ecb8e063de10c1a540354fc5c`.
From authorized Android, `adb reboot sideload-auto-reboot`, `adb
wait-for-sideload`, and `adb sideload` identified the expected a33x and
completed at `Total xfer: 1.00x`; no format-data action occurred.

Android returned boot-complete and enforcing with the exact increment,
`ro.sos.profile=compat`, `ro.sos.ui_owner=android-compat`, revision format 3,
Zygote, `system_server`, SurfaceFlinger, SystemUI, and the on-device authority.
Phone, connectivity, Bluetooth, NFC, and SurfaceFlinger services were found and
Wi-Fi remained connected. The application CE/DE data inodes remained exactly
3743/3592 across the OTA. A Home-key test put
`dev.sos.experience.SosHomeActivity` in focus; Settings then opened normally in
354 ms and another Home key returned to the same SOS PID. The active generated
Daily experience and configured Codex-provider status rendered after reboot.
The app ran in the MCS-labelled `priv_app` domain, the authority in
`sos_authority`, PackageManager exposed no `DEBUGGABLE` flag, the SOS PID was
absent from JDWP, no ADB reverse existed, and final scans counted zero
fatal/ANR/native-signal records and zero SOS-related AVC denials.

Two ignored evidence captures were inspected and kept out of Git:

| Capture | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/sos-compat-post-ota.png` | 498,404 | `14d77abcf0c25b257cae56247d92ec5806c7722734acd796e972f381f9b95edd` |
| `/tmp/sos-compat-home.png` | 177,585 | `d8ba757bc1cd729b6bec3907d11223a7ccbcec3800411720d4b88c0386106ba2` |

**Initial Core artifact and interrupted entry:** A fresh
`./tools/a33xctl build-core` completed in 3:50 with build number
`sos.core.301cefc50d20.1d93946d6967`. `./tools/a33xctl inspect-core` passed the
same complete artifact gate and additionally proved that target files contain
SystemUI, Launcher3, the disabled probe and its init/SELinux policy, but no
packaged `SosShell.apk`. The inspected archive
`lineage-23.0-20260815-UNOFFICIAL-sos_core_a33x.zip` is 1,256,407,136 bytes,
SHA-256
`9d72975319a83189e3246b133c4be740cf2fed02476218bb735f300a8448d2f4`.
The AArch64 probe remains 51,464 bytes, SHA-256
`ba9799dfbcf61559e34550743066e10e0f4a7c5223d77993dc36216e909d5b73`;
the revised 483-byte init file has SHA-256
`2e761434184b5e2eef18deb5d3244c39396ea88dbcd12df704af39ca088b5606`.

The subsequent `adb reboot sideload-auto-reboot` cleanly disconnected USB at
21:01:42 CEST, but the handset did not re-enumerate as Samsung, Android, or ADB
sideload during the next four minutes. The waiting host command was
interrupted. `adb sideload` was never entered, no Core bytes transferred, and
no wipe or completed device change occurred. This is a recovery-entry/power
failure, not evidence for or against the Core image or native display probe.

The handset later completed a normal Compat boot and re-enumerated without host
intervention. Repeating the same recovery command then entered sideload
normally, and the already inspected Core archive transferred at `Total xfer:
1.00x` without a wipe.

**Core physical evidence and termination fix:** The first Core installation
booted complete and enforcing with the exact
`sos.core.301cefc50d20.1d93946d6967` increment, `ro.sos.profile=core`,
`ro.sos.ui_owner=android-shadow`, and revision format 3. Zygote,
`system_server`, SurfaceFlinger, SystemUI, phone, connectivity, Bluetooth, NFC,
and Wi-Fi remained live. Target files contain no `SosShell.apk`; the SOS APK
reported by PackageManager is instead a pre-existing no-wipe update under
`/data/app`. Its `base.apk` is 37,813,431 bytes with SHA-256
`4c7b3904c50ccf4f43c60603a9e22dba42770bd215d026b744ba1551b0a0bb0a`.
This distinction matters: the Core product did not package SOS, but this shadow
test also deliberately did not destroy user data.

Setting `debug.sos.core.surface_probe=1` started PID 2763 in
`u:r:sos_core_surface_probe:s0`. It logged
`native_surface_ready width=1080 height=2400 ui_owner=android-shadow`.
SurfaceFlinger reported the full-screen probe as its sole visible layer and
assigned `DEVICE` composition through the Samsung Hardware Composer. Setting
the property to zero removed the layer and returned to Android's working
lockscreen, while SurfaceFlinger and SystemUI stayed alive. The initial init
definition used the default hard stop, however, so stop sent SIGKILL despite
the probe's signal handler. That was rejected as insufficient lifecycle
evidence.

The init service now has `gentle_kill`; `inspect-core` requires that flag as
well as both property triggers. After a complete rebuild,
`./tools/a33xctl inspect-core` again passed ZIP/signature, VINTF, PIT, AVB,
package/property, init, binary, and SELinux gates. The corrected installed
archive `lineage-23.0-20260815-UNOFFICIAL-sos_core_a33x.zip` uses build number
`sos.core.301cefc50d20.f600281a80ff`, is 1,256,433,258 bytes, and has SHA-256
`c815dabd75cd5bff777904f9f9538b4ad04dafdba484c607b9fbf5678ab0f045`.
Its probe is still 51,464 bytes with SHA-256
`ba9799dfbcf61559e34550743066e10e0f4a7c5223d77993dc36216e909d5b73`;
the 499-byte init file has SHA-256
`a77794926d5fbc4ccdbb8877b2d67821372be1a4805e7fe714e244b82bc657a5`.
The corrected archive transferred at `Total xfer: 1.00x` without a wipe and
booted with the exact expected Core identity.

On the corrected image, PID 2701 rendered the same 1080x2400 HWC `DEVICE`
layer. A power-key suspend moved the handset from awake to dozing without
stopping the process; resume returned it to awake with the same PID and layer.
The property stop sent SIGTERM, the probe logged `stopping after signal=15`,
exited with status 0, and removed its layer in 205 ms. Init's 200 ms process
group cleanup also logged a subsequent SIGKILL after the successful exit, so
the next lifecycle refinement is to finish comfortably inside that deadline.
Launching `com.android.launcher3/.uioverrides.QuickstepLauncher` then produced
a usable Android recovery surface in 262 ms with SystemUI and Launcher alive
and the probe stopped. Final log scans counted zero fatal exceptions, ANRs, or
native fatal signals.

EGL/Mali probing produced three unique enforcing AVC denials for directory
`search` on `system_data_file` named `data`; rendering still succeeded. Broad
`/data` search permission was deliberately not added merely to suppress these
failed driver probes. Determine the exact optional driver path before changing
policy.

Ignored physical evidence captures were inspected and kept out of Git:

| Capture | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/sos-core-probe-running.png` | 15,584 | `ba20c352f66694f517c51b748358882450f56a12a320fcad25530ac83167bd06` |
| `/tmp/sos-core-probe-stopped.png` | 491,036 | `4c774ea3c87e7c0356c0a86a2a3bb3e8b06de8d2a690a8416823f824817ba415` |
| `/tmp/sos-core-probe-after-resume.png` | 15,584 | `ba20c352f66694f517c51b748358882450f56a12a320fcad25530ac83167bd06` |
| `/tmp/sos-core-android-shadow.png` | 611,006 | `ae02a7f286c66b1e26be3d0e2a548314c87c820a13676c7e742725e3ed80dd93` |

**Final Compat restoration:** Switching products removes the other profile's
archive from `out`, so Compat was rebuilt adjacent to its inspection before
restoration. `./tools/a33xctl build-compat` completed in 3:47 and
`./tools/a33xctl inspect-sos` passed the complete artifact gate. The exact
restoration archive `lineage-23.0-20260815-UNOFFICIAL-sos_compat_a33x.zip`
uses build number `sos.compat.301cefc50d20.8412fd499a46`, is 1,276,709,090
bytes, and has SHA-256
`a16edd84a0bc8cdc5d6d11daa65d0c5befc804a0be3f24a6b382f0edb47008b8`.
Recovery accepted it at `Total xfer: 1.00x` without formatting data.

The phone finished booting the exact Compat identity with enforcing SELinux,
revision format 3, and no Core probe binary or init service. SOS PID 2074 was
explicitly selected as HOME, then the preserved Daily experience rendered.
Android Settings opened warm in 396 ms and HOME returned to the same PID and
`SosHomeActivity`; Launcher3 was not selected as HOME. Zygote,
`system_server`, SurfaceFlinger, SystemUI, SOS authority, phone, connectivity,
Bluetooth, and NFC remained live, and Wi-Fi was connected. An isolated final
scan counted zero fatal exceptions, ANRs, native fatal signals, and SOS-related
AVC denials; ADB reverse and the JDWP process list were empty. The final ignored
capture `/tmp/sos-compat-final.png` is 178,036 bytes with SHA-256
`c8fc42e12ba999975d9d09f4937010b3ed25ca5eaf632c6b3858c09868969763`.

**Decision / remaining risk / next gate:** Compat passes its physical product
gate. Core passes the intended *shadow* gate: a native init service can own a
physical SurfaceFlinger/HWC layer, survive one doze/resume cycle, stop cleanly,
and return to Android recovery UI under enforcing SELinux. It remains
`android-shadow`, not Core 0: Android UI is deliberately packaged and running,
native input and trusted lock/recovery surfaces do not exist, the residual EGL
path probes need attribution, and a no-wipe `/data` SOS update remains installed.
The handset is restored to Compat as the daily target with SOS explicitly
selected as HOME. The next Core gate is the GPUI host on this `ANativeWindow`,
raw/native input, fixed trusted lock and recovery surfaces, repeated
suspend/failure tests, and only then switching ownership to `native-sos` and
removing superseded Java UI.

## 2026-08-15–16 — Native GPUI Core host, fixed recovery, and Compat restoration

**Goal / hypothesis:** Replace the one-frame Core display probe with the real
permanent GPUI experience without an Activity, APK lifecycle, or Java UI host;
isolate it behind a fixed signed supervisor; prove native presentation,
suspend, failure, retry, and Android escape on the physical SM-A336B; and leave
the handset on Compat with SOS—not the accidentally selected Launcher3—as
HOME. Android UI removal was explicitly conditional on trusted input,
lockscreen, and recovery gates rather than an objective to force through this
experiment.

**Changed:** `gpui-mobile` now supports a standalone Android platform around a
caller-supplied `ANativeWindow`. It records the constructing thread as GPUI's
main thread, manually drains foreground work and requests frames at 16 ms, and
never calls looper teardown for a null non-Activity looper. The experience
library adds a `core-native` feature and exported `sos_core_main` boundary,
uses `/data/misc/sos/core`, installs signal handling, and shares the normal
experience/revision/runtime code without creating an Activity. Its native
input reader discovers `sec_touchscreen`, `gpio_keys`, and `sec-pmic-key`,
translates multitouch and power/volume events into the GPUI Android event path,
and requests fixed recovery after a two-second Volume Up+Down chord.

Core now packages `sos-core-host` and
`libsos_core_experience.so`. The host creates the full-screen SurfaceComposer
layer and supervises a separately exec'd GPUI child. A nonzero exit or signal
cannot unwind through Binder/SurfaceFlinger state: the parent draws a fixed
CPU-rendered `SOS RECOVERY` surface with Volume Up retry and Volume Down Android
escape. Retry forks and execs a clean child; it does not continue from a
post-Binder fork. Transient edge-triggered `debug.sos.core.fault` and
`debug.sos.core.recovery` properties support unattended fault/recovery tests.
The service remains `disabled`, `oneshot`, and `gentle_kill` in the dedicated
`sos_core_host` SELinux domain. Product inspection now verifies both native
binaries, the runtime exports, supervisor/recovery/fault strings, init
triggers, and labels. Core's content revision now includes product, Blueprint,
host, init, policy, runtime, authority, and default-experience inputs, fixing a
case where policy-only changes did not alter the build identity. The detailed
stage boundary and manual test commands are recorded in
[`android-product-split.md`](android-product-split.md).

The final reviewed source also fails closed before presentation when
`sys.user.0.ce_available` is false: the child exits with a distinct status and
the supervisor returns directly to Android keyguard without drawing either the
experience or fixed recovery over it. This is a temporary preservation of the
existing credential boundary, not a native lockscreen implementation.

**Build and static evidence:** The final physically tested Core archive was
`lineage-23.0-20260815-UNOFFICIAL-sos_core_a33x.zip`, build
`sos.core.0805cf6bd0b4.5bb309d830bd`, 1,262,746,472 bytes, SHA-256
`4b1dc874c7e49eb82206052e388b8a69b4b90f61d6bcd70d8bd74d756699a767`.
Its `sos-core-host` was 51,712 bytes, SHA-256
`912557a27c23d6660dbf74f263e1846d33f9ea841e4a29a1d2cb36fe17b3df62`;
the GPUI runtime was 14,582,728 bytes, SHA-256
`d6eb57348e9a1cb45c0ae9d74c3bb5466acb1123ace5103539324dbb24fc85e4`.
`./tools/a33xctl inspect-core` passed ZIP integrity, whole-package signature,
PIT ceilings, AVB graph, VINTF, recovery init, target-file contents, exported
symbols, init/service assertions, compiled SELinux contexts, neverallow tests,
and APEX policy tests. Recovery installed the archive without formatting data
and `adb sideload` completed at `Total xfer: 1.00x`.

After the final source cleanup, `cargo fmt --all -- --check`,
`./tools/sosctl m1-check --abi arm64-v8a`, and `cargo ndk -t arm64-v8a -P 31
check -p sos-experience --release --locked --no-default-features --features
core-native` passed. The Android checkout's clang-format dry-run, `bash -n
tools/a33xctl`, and `git diff --check` also passed. A targeted AOSP
`m -j8 sos-core-host` rebuilt the formatted C++ source and its pre-unlock guard.
That post-test guard was not reflashed, so its evidence is compile/inspection
only; physical evidence below remains tied to the exact `5bb309d830bd` archive,
whose user was already CE-available during the native test.

**Physical native presentation and lifecycle evidence:** On the installed Core
image, SELinux was enforcing and `ro.sos.ui_owner=android-shadow`. Setting
`debug.sos.core.host=1` left supervisor PID 2690 alive and exec'd GPUI child
2691, both in `u:r:sos_core_host:s0`. The child initialized Vulkan on the
Mali-G68, opened all three expected raw input devices, connected to the real
SOS authority/provider snapshot, started the Luau runtime worker, and rendered
the Daily experience at 1080x2400. `dumpsys SurfaceFlinger` showed the SOS
native buffer as the only display layer with Samsung HWC `DEVICE` composition.
A doze/resume cycle retained the same PID and layer. A property stop delivered
SIGTERM, logged reason 15/status 0, removed the layer in 100 ms, and exposed
Android's still-working lockscreen.

The ignored capture `/tmp/sos-core-native-88b6e0bd7c0f.png` is 171,492 bytes,
SHA-256
`d548132e4daa017f69b51ffe4a1924c4cc7aa1222cc19ff1ccc988c8611999a2`.
The matching Android-fallback capture
`/tmp/sos-core-android-fallback-88b6e0bd7c0f.png` is 487,680 bytes, SHA-256
`c62d1f71c32b6447d1af042ea0c2f742de8ae2e3e9775b40fb1c282f0c3cf98f`.

**Physical fault and recovery evidence:** Changing
`debug.sos.core.fault` from false to true sent SIGABRT to the GPUI child while
the supervisor remained alive. The fixed recovery became the only 1080x2400
HWC `DEVICE` layer; SurfaceFlinger, `system_server`, SystemUI, phone,
Bluetooth, and NFC processes stayed live. The ignored capture
`/tmp/sos-core-fixed-recovery-5bb309d830bd.png` is 17,286 bytes, SHA-256
`37f1c452a9898e0e307412e00331d19c715f8db9cc6325522360bb84eea2fd53`.
The instrumentation retry action exec'd a clean child PID 2799, which returned
to the same GPUI experience; `/tmp/sos-core-after-retry-5bb309d830bd.png` is
171,492 bytes with SHA-256
`d548132e4daa017f69b51ffe4a1924c4cc7aa1222cc19ff1ccc988c8611999a2`.
A second injected failure followed by the Android action removed the native
host/recovery in 204 ms while SurfaceFlinger PID 567, `system_server` PID 1020,
SystemUI PID 1297, Launcher3 PID 1604, phone PID 1473, Bluetooth PID 1425, and
NFC PID 1448 remained alive under enforcing SELinux.

Raw device discovery is physical evidence, but actual touchscreen dispatch and
the physical two-volume-button chord were not exercised by the owner during
this run. Those gates remain open; the property-driven paths do not substitute
for hardware interaction.

**Failures, fixes, and rejected approaches:** The first GPUI library could not
be loaded because `android-activity` retained an undefined `android_main`; an
inert Core-only symbol satisfies load-time glue without creating an Activity.
The next child reached Vulkan but panicked because GPUI did not recognize the
standalone thread as main, then touched a null looper during panic cleanup;
explicit thread identity and null-safe unregister fixed both. SELinux initially
blocked `/dev/input` directory search; the dedicated domain received only the
input directory/device access it needs. The native host initially missed
`libnativewindow`; declaring the shared library fixed the AOSP link.

The first recovery retry property was level-triggered and created a retry
storm. The first retry also continued in a child forked after Binder threads
had started and aborted. Edge-triggering the property and fork/execing the
signed host fixed both. SELinux then denied that self-exec as
`execute_no_trans`; one type-specific rule for `sos_core_host_exec` fixed the
clean re-exec without granting general execution. Four failed optional Mali
path probes still request broad `/data` directory search; rendering does not
need it, so the broad permission remains rejected pending exact attribution.

Android incremental packaging twice reported a missing-restat condition
because the generated kernel `Image` timestamp was older than a regenerated
partition file list. Refreshing that generated output allowed the already
successful kernel build to package; this is recorded as a build-environment
workaround, not a source or device fix. Standalone Core also proved that the
remote SOS authority transport works, while current network, agent-status, and
accessibility adapters report that no Java VM exists. Those JNI paths must be
replaced by a native framework/provider bridge rather than granting Core an
Activity merely to reuse them.

**Compat rebuild, restoration, and HOME ownership:** After the Core tests,
`./tools/a33xctl build-compat` completed successfully and
`./tools/a33xctl inspect-sos` passed ZIP, signature, PIT, AVB, VINTF,
partition, package, property, and SELinux gates. The no-wipe restoration
archive `lineage-23.0-20260815-UNOFFICIAL-sos_compat_a33x.zip` uses build
`sos.compat.0805cf6bd0b4.45459139a271`, is 1,276,678,027 bytes, and has
SHA-256
`ea2c540e4e01c060c8e1e9c3e6d071b08447a6faf9faa3466f1fa873c814ed79`.
Its packaged `SosShell.apk` is 40,708,721 bytes with SHA-256
`a2ea732ec533b32c85ff9579912ab8d8abcf81c1fd4b37ec1439f75b3f743e84`.
Recovery installed it at `Total xfer: 1.00x` without formatting data.

The phone booted the exact increment with `ro.sos.profile=compat`,
`ro.sos.ui_owner=android-compat`, revision format 3, enforcing SELinux, enabled
Wi-Fi, and live SurfaceFlinger, `system_server`, SystemUI, phone, Bluetooth,
NFC, and SOS authority processes. `/system_ext/bin/sos-core-host` and its init
service are absent. `cmd package set-home-activity
dev.sos.experience/.SosHomeActivity` succeeded, and PackageManager resolves
the default HOME uniquely to that component rather than Launcher3. Starting it
created SOS PID 3104 with live remote-provider and Luau-worker readiness logs.
The credential keyguard correctly remained visually authoritative after boot,
so the test did not bypass it merely to capture the HOME surface. The ignored
lockscreen capture `/tmp/sos-compat-restored-45459139a271.png` is 499,960
bytes, SHA-256
`807cc36153027055fa6d1b2ad58333515aa0bf2714222f6ae4e72a0c90a5137e`.
The isolated post-boot log contained zero fatal exceptions, ANRs, native fatal
signals, or SOS-related AVC denials; ADB reverse and the JDWP list were empty.

**Decision / remaining risk / next gate:** Keep both products and keep Core at
`android-shadow`. The native GPUI presentation and fixed failure-recovery
boundary pass their first physical gate, and Compat is restored with SOS as
the intentional HOME. Do not yet remove SystemUI, Launcher, Settings,
LatinIME, package installation, or keyguard: the remaining gates are physical
touch/chord evidence; a signed native FBE/Gatekeeper/Keystore unlock ceremony;
a native framework/provider bridge for network, agent status, accessibility,
phone, Bluetooth, and NFC state/actions; trusted attention for calls, alarms,
security, battery, and thermal events; and repeated boot/suspend/crash tests.
Only after those pass should Core switch to `native-sos` and remove superseded
Java UI. Core 1/no-Zygote remains a later service migration.

## 2026-08-16 — Exclusive Core input, JNI-free adapters, and final two-product device gate

**Goal / hypothesis:** Close the unsafe duplicate-input and Java-VM fallback
gaps found during the first native GPUI shadow run, prove the corrected Core
artifact on the physical SM-A336B, then rebuild and restore Compat with SOS as
the intentional HOME. This remains a Core shadow gate; it does not authorize
removing Android UI before the credential, urgent-attention, and physical
gesture gates pass.

**Changed:** Core now opens its three required input devices synchronously
before presenting. It uses `EVIOCGRAB` to acquire `sec_touchscreen` and
`gpio_keys` exclusively, while observing `sec-pmic-key` and leaving Android as
the display-power/suspend owner. Failure to open or grab the required touch or
volume device fails the GPUI child into fixed recovery. Fixed recovery also
grabs `gpio_keys`; if that device is missing or cannot be grabbed, the
supervisor fails safe to Android instead of displaying an unusable recovery
surface.

The `core-native` build no longer attempts the Activity/JNI adapters. Network
state reads only the presence and `operstate` of `wlan0`; it deliberately does
not infer SSID, scan results, Android validated-network state, or permit a
mutation. Agent status and candidate generation use the bounded deterministic
native provider, while live-provider configuration and credentials return an
explicit trusted-ceremony error. Accessibility still creates the bounded
semantic JSON document but does not publish it into a Java View hierarchy.
Core text focus reports that the native composition-aware IME is unavailable
instead of calling Java. These are honest intermediate contracts, not
substitutes for the remaining framework bridge, assistive service, live-agent
credential ceremony, or native keyboard.

The physical SCSC interface lives below
`/sys/devices/platform/11a70000.scsc_wifibt/net`, outside AOSP's virtual-net
label. A device-specific `genfscon` now labels only that subtree `sysfs_net`,
and `sos_core_host` receives read-only directory/file access to that type. It
receives no sysfs write permission. `inspect-core` verifies that compiled CIL
label as well as the exclusive-input and fail-safe recovery strings. The Core
content revision now includes `genfs_contexts` so a policy-only change cannot
reuse an older build identity. Overlay staging changed from timestamp-based
`rsync -a` to `rsync -a --checksum --no-times --delete` so Ninja cannot retain
a stale object when copied source content has an older or equal timestamp.

**Final Core build and static evidence:** `./tools/a33xctl build-core`
completed successfully for build
`sos.core.0805cf6bd0b4.17dd66593016`. The inspected archive was
`lineage-23.0-20260816-UNOFFICIAL-sos_core_a33x.zip`, 1,262,755,472 bytes,
SHA-256
`27da5b8a9cd9529439f99ff87092bee207f592111bea5aeaf7715921f1d85c7d`.
The packaged `sos-core-host` was 51,712 bytes, SHA-256
`034310121fa7e2467808938ae63ac543c34a97d8aa4e8ea447ff8f5bf876d5c7`;
`libsos_core_experience.so` was 14,534,696 bytes, SHA-256
`32861e76cb54c3a6b037b580a853c264ae727a547bb618cb3d9591165293d11b`;
and compiled `system_ext_sepolicy.cil` was 102,738 bytes, SHA-256
`9371e1fdf8ee5d18bd6a04c7134e254cc653dbc2b9b1d7a9cd33dda675a380a5`.
`./tools/a33xctl inspect-core` passed compressed ZIP integrity, the whole-file
OTA signature, PIT ceilings, the complete AVB graph, VINTF, packaged recovery
init, target contents, ELF exports/strings, init properties, compiled policy,
neverallow checks, and APEX policy tests. Recovery accepted this exact archive
at `Total xfer: 1.00x` without wiping data.

Final source verification passed `cargo fmt --all -- --check`,
`cargo test -p android-system-authority` (3/3), `./tools/sosctl m1-check
--abi arm64-v8a`, the release ARM64 `cargo ndk` check with only
`core-native`, `bash -n tools/a33xctl`, the pinned AOSP `clang-format
--dry-run --Werror` over `core/host.cpp`, and `git diff --check`.

**Final Core physical evidence:** The phone booted enforcing with
`ro.sos.profile=core`, `ro.sos.ui_owner=android-shadow`, revision format 3,
CE storage available, and the exact `17dd66593016` increment. Starting the
disabled service created supervisor PID 2743 and GPUI child PID 2744 in
`u:r:sos_core_host:s0`. The Mali-G68 Vulkan path rendered the Daily experience
at 1080x2400 through Samsung HWC. Logs reported touchscreen and `gpio_keys` as
`mode=exclusive`, the power key as `mode=observe owner=android-power`, and the
native snapshot as `enabled=true connected=true validated=false networks=0`.
The latter, together with the compiled/on-device `sysfs_net` directory label
and absence of another `operstate` denial, proves the confined Core process—not
the ADB shell—read the physical link state. The remote authority, Luau worker,
and native semantic-document generation also remained ready without a Java-VM
fallback.

An injected `SIGABRT` killed child 2744 while the supervisor remained alive
and presented `SOS Fixed Recovery` with exclusive recovery keys. Android
SurfaceFlinger, `system_server`, SystemUI, phone, Bluetooth, and NFC remained
alive. A separate edge-triggered campaign started supervisor 2975/child 2976,
injected the same failure, and selected Retry; the supervisor exec'd clean
child 3028, which reacquired both exclusive input devices, reread the native
Wi-Fi state, and rendered again. A subsequent Android action stopped the
native host and exposed the intact Android credential lockscreen. The two
fatal signals in that campaign were deliberate fault injections, not
unexplained crashes.

Ignored physical captures were visually inspected and remain outside Git:

| Capture | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/sos-core-native-17dd66593016.png` | 171,492 | `d548132e4daa017f69b51ffe4a1924c4cc7aa1222cc19ff1ccc988c8611999a2` |
| `/tmp/sos-core-recovery-17dd66593016.png` | 17,286 | `37f1c452a9898e0e307412e00331d19c715f8db9cc6325522360bb84eea2fd53` |
| `/tmp/sos-core-android-fallback-17dd66593016.png` | 491,321 | `802b837748180d1a060bae2a0540fc500623b8a5eb9bb16fd252ed35effca13f` |

**Final Compat build, restoration, and HOME evidence:** Switching products
performed an install-clean and explicitly removed `sos-core-host`, its probe,
both init files, and the standalone GPUI library. `./tools/a33xctl
build-compat` then completed successfully for
`sos.compat.0805cf6bd0b4.80ca8f2dfd8d`. `./tools/a33xctl inspect-sos` passed
the same ZIP/signature/PIT/AVB/VINTF/recovery and product gates. The exact
archive `lineage-23.0-20260816-UNOFFICIAL-sos_compat_a33x.zip` was
1,276,677,263 bytes, SHA-256
`3da14d2b1a81b98dc87dd73c9d9a351290f8a9918202b9b4336b1bad1073f51d`;
its packaged `SosShell.apk` was 40,708,721 bytes, SHA-256
`53593e99687563380cb940ee0eff48318b16e857cc1e011c19735d4c10429d90`.
Recovery accepted it at `Total xfer: 1.00x` without a wipe.

The phone booted the exact Compat increment with `ro.sos.profile=compat`,
`ro.sos.ui_owner=android-compat`, revision format 3, CE available, and
enforcing SELinux. `cmd package set-home-activity
dev.sos.experience/.SosHomeActivity` succeeded and HOME resolved uniquely to
SOS, not Launcher3. A final audit found that PackageManager was initially
running a preserved `/data/app` update over the flashed system package.
`cmd package uninstall-system-updates dev.sos.experience` reverted only that
update; the system app remained installed and the HOME preference remained
SOS. The repeated gate then showed both current focus/top-resumed activity as
`SosHomeActivity` and `pm path` as
`/system_ext/priv-app/SosShell/SosShell.apk`. SurfaceFlinger,
`system_server`, SystemUI, phone, Bluetooth, NFC, and `sos-authority` were live,
Wi-Fi was connected, the Core host binary/init service were absent, ADB reverse
was empty, and an isolated relaunch scan contained no fatal exception, ANR,
SOS-related AVC, provider failure, or accessibility publication failure. The
visually inspected ignored capture
`/tmp/sos-compat-system-final-80ca8f2dfd8d.png` is 177,472 bytes, SHA-256
`287c804a849cb9304527c99d8cc20717661d1528d1ca36730019289893c9a87e`.
The packaged SOS HOME filter also carries priority 1000 while Launcher3's HOME
filter has no priority. Android's `HomeRoleBehavior` chooses the unique
highest-priority candidate when no user role holder exists, so a fresh
role-less user falls back to SOS rather than Launcher3. `inspect-sos` now
requires that priority in addition to the HOME alias/category; rerunning it on
the exact installed archive passed.

**Failures, fixes, and rejected approaches:** A direct targeted AOSP compile
outside the normal release wrapper invalidated the broad product environment
and caused a 62,000-action rebuild; it did not expose a source failure. Editing
`tools/a33xctl` while an older Bash instance was still parsing it later ended
that already-successful package run with `line 704: t: command not found`; the
stable script passed `bash -n` and all subsequent builds exited normally.
Android's recurring generated-kernel `Missing restat` warning was handled by
refreshing the already-built generated `KERNEL_OBJ/.../Image` timestamp before
packaging; this remains a build-environment workaround.

The first final package inspection caught an older `sos-core-host` despite new
source. `rsync -a` had preserved a source timestamp older than Ninja's object;
content-based, no-timestamp staging fixed it and the inspector then found all
new host markers. The first native network run returned link-down while Android
Wi-Fi was connected: the read-only allow rule targeted `sysfs_net`, but this
physical device subtree was still generic `sysfs`. The narrow `genfscon` fixed
the label and the final confined run returned link-up. Adding generic `sysfs`
read access was rejected. The first unattended Retry attempt reused an
unchanged `retry` property and was correctly inert; resetting it to `idle` and
then making an edge change proved the intended behavior. Four optional Mali
probes still request broad `/data` search and are denied while rendering works;
that broad permission remains rejected pending path attribution.

**Decision / remaining risk / next gate:** Both products pass the implemented
physical gate, and the handset is left on Compat with the immutable system SOS
package explicitly selected as HOME. Launcher3 remains installed only as an
Android recovery fallback. Core remains `android-shadow`, not Core 0. Actual
finger-driven touch and the physical Volume Up+Down chord were not exercised;
the CE-unavailable guard was compiled and inspected but not physically tested
because this no-wipe user was already unlocked. Still missing are the signed
native FBE/Gatekeeper/fingerprint/Keystore ceremony; native IME; framework
state and mutations for network, phone, Bluetooth, and NFC; assistive-service
delivery/actions; trusted calls/alarms/security/battery/thermal attention;
Compat workspace chrome and installation policy; and repeated cold-boot,
suspend, crash, and rollback campaigns. Do not switch to `native-sos`, remove
SystemUI/Launcher/Settings/LatinIME, or begin Core 1/no-Zygote until those gates
pass.

## 2026-08-16 — Implement and physically gate all six Android-ownership stages

**Goal / hypothesis:** Replace the earlier two-product sketch with six explicit,
flashable ownership stages and exercise them in order on the connected
SM-A336B. The required sequence was Compat 0, Compat 1, Shadow, Core 0A, Core
0B, and Core 1, followed by an exact no-wipe restoration of the accepted Compat
1 artifact. The central hypothesis was that visible Android ownership can be
removed before removing useful native Android infrastructure, and that Core 0B
can keep Zygote/`system_server` strictly headless while Core 1 can prove the
no-Zygote/recovery boundary without pretending to unlock CE storage.

The concrete stage contract and product mapping are recorded in
[`android-ui-ownership-stages.md`](android-ui-ownership-stages.md) and
[`android-product-split.md`](android-product-split.md).

**Code and product changes:** Six product definitions now share the Samsung
a33x hardware/vendor graph, SOS authority, experience artifacts, SELinux
policy, and revision format 3:

- Compat 0 adds a platform-signed, persistent HOME policy that reasserts
  `dev.sos.experience/.SosHomeActivity` through the role API after boot,
  package replacement, or ownership drift. Launcher3 remains installed only
  in this measurement stage.
- Compat 1 removes Launcher3 and adds a trusted persistent chrome service,
  explicit launcher-activity workspace, typed notification adapter, bounded
  durable JSONL attention journal, and fixed attention renderer. The chrome
  exposes time/connectivity/battery plus Back, Apps, Attention, and Exit while
  suppressing stock status/expand/navigation content for owned tasks.
- Shadow packages disabled init services for the one-frame native
  SurfaceComposer probe and the supervised GPUI host. Android remains the
  display/recovery owner until an explicit debug property starts either
  service.
- Core 0A starts the native supervisor only after
  `sys.user.0.ce_available=true`, grabs touchscreen/volume input, observes the
  power key, rejects user PackageInstaller sessions, and retains Android as a
  fixed-recovery escape.
- Core 0B starts a CPU-rendered trusted lock surface as soon as SurfaceFlinger
  is available. Principal Android UI packages are removed, every Activity
  start is aborted by immutable product policy, user install sessions are
  rejected, and a no-Activity direct-boot bridge delegates bounded PIN status
  and verification to LockSettings. The framework has a product-gated direct
  boot-completion path because no HOME Activity exists to become idle.
  `PackageInstaller.apk` remains as a non-rendering bootstrap invariant because
  PackageManager requires exactly one installer.
- Core 1 selects AOSP `core_no_zygote.mk` at the shared Samsung product
  boundary. It starts no Zygote, `system_server`, APK process, or Java bridge;
  the native host shows an honest locked/recovery surface and never sets CE
  available. A tracked source patch makes the shared device choose
  `core_no_zygote.mk` only for `lineage_sos_core1_a33x`; all other a33x targets
  retain `core_64_bit_only.mk`.

The fixed native supervisor now owns a CPU-only recovery layer, fault
injection, Retry, stage-aware Android fallback, and a Volume Up+Down Recovery
reboot chord. The standalone GPUI/wgpu child obtains an `ANativeWindow` from
SurfaceComposer and reads `sec_touchscreen`, `gpio_keys`, and `sec-pmic-key`
without an Activity or JNI lifecycle. Core-native revision and provider calls
use SELinux-confined filesystem Unix sockets at
`/data/misc/sos/revision.sock` and `/data/misc/sos/provider.sock`; Compat keeps
its Android loopback adapters. Inspectors now scan every `classes*.dex`, verify
the framework policy/headless markers, require the native socket strings and
`connectto` rule, enforce stage-specific package presence/absence, verify the
no-Zygote init selection, and include all audited source patches in the
content-derived product identity.

**Accepted package artifacts:** All files below are ignored raw evidence in
`artifacts/device-stages/`; none is added to Git. Each OTA passed ZIP integrity,
whole-file signature, PIT ceilings, AVB verification, VINTF, recovery-init,
stage-property, package-content, binary-marker, and compiled-policy inspection
before sideload.

| Stage | Exact artifact | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| Compat 0 | `compat0-db36ed79bb16.zip` | 1,276,699,263 | `0abb652191b2ad61cf421f64afcd5255b2cab17c866572a9457d83c3207e44cc` |
| Compat 1 | `compat1-616ac2404a79.zip` | 1,264,854,020 | `d9b8f58ae09d8e405e7ba3754a21a9b12f52bfea955b5739b2fa444ea8eca3a5` |
| Shadow | `shadow-1aad692518b8.zip` | 1,262,728,002 | `4b111d0a83a21b203c735223b814dc3f8908e0bd197381317f64dfa765145f8b` |
| Core 0A | `core0a-1b0c9edec481.zip` | 1,262,798,713 | `eccd673087c315b327955c3ea279826da17313581548bcf803bbc3e22eff4246` |
| Core 0B | `core0b-4341fa73391c.zip` | 1,021,900,489 | `71a0933ed4a3fb35974a4da5a3b117d2d04c10547110d33168ee787433d7360f` |
| Core 1 | `core1-1f3cd4b232c2.zip` | 1,021,911,891 | `099839d1f29f5f6eea5121e9b8bcfab008e22225755fc7a821332fd8bd63344c` |

Accepted component evidence:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `compat0-SosShell-db36ed79bb16.apk` | 40,723,177 | `865028c917a3d85b35115366013a4b581eca1f1c48f3431ba0058274c04a17ce` |
| `compat1-SosShell-616ac2404a79.apk` | 40,722,849 | `52380aae15a5340b3efd3721d95b356d04b09d072f8dd21588baff913992f6e2` |
| `shadow-sos-core-host-1aad692518b8` | 68,328 | `0510f65ee969ff2e8107784121414fe1d0f74de2b6ca4d1e2590c0a3c6d51559` |
| `shadow-libsos-core-1aad692518b8.so` | 14,534,696 | `caaef737801d64c57dc66290f9d23bdeb617e3bfa29da27e84f140290332e8c4` |
| `shadow-surface-probe-1aad692518b8` | 51,464 | `ba9799dfbcf61559e34550743066e10e0f4a7c5223d77993dc36216e909d5b73` |
| `core0a-framework-bridge-1b0c9edec481.apk` | 16,798 | `3f9306ff6daf4a0b96154bc18d34c22868aec0d087463be085791b433e95b59a` |
| `core0b-sos-core-host-4341fa73391c` | 68,328 | `0510f65ee969ff2e8107784121414fe1d0f74de2b6ca4d1e2590c0a3c6d51559` |
| `core0b-libsos-core-4341fa73391c.so` | 14,526,424 | `6ae4b8217349af06d3b52e9832b2f345aa6f90b0c4ad6fbeb94d26a881189201` |
| `core0b-sos-authority-4341fa73391c` | 1,242,848 | `a33368140e004e027f94d7260058b2d2936c6f60ab44d19f6d11b84af3fba5b5` |
| `core0b-framework-bridge-4341fa73391c.apk` | 16,798 | `3f9306ff6daf4a0b96154bc18d34c22868aec0d087463be085791b433e95b59a` |
| `core0b-ui-removal-marker-4341fa73391c` | 51,216 | `6ca538a90323d4f1907be2dd5ab2381e7507b5fd62e6b295e6c0f464a16a4c14` |
| `core1-init-no-zygote-1f3cd4b232c2.rc` | 128 | `3b353112b688b2e1d5c5c19343bcc44b1f9581dd7ba19fb95f499115e12adcfc` |
| `core1-sos-core-host-1f3cd4b232c2` | 68,328 | `0510f65ee969ff2e8107784121414fe1d0f74de2b6ca4d1e2590c0a3c6d51559` |
| `core1-libsos-core-1f3cd4b232c2.so` | 14,526,424 | `6ae4b8217349af06d3b52e9832b2f345aa6f90b0c4ad6fbeb94d26a881189201` |
| `core1-sos-authority-1f3cd4b232c2` | 1,242,848 | `a33368140e004e027f94d7260058b2d2936c6f60ab44d19f6d11b84af3fba5b5` |
| `core1-ui-removal-marker-1f3cd4b232c2` | 51,216 | `6ca538a90323d4f1907be2dd5ab2381e7507b5fd62e6b295e6c0f464a16a4c14` |

**Compat 0 and Compat 1 physical evidence:** Compat 0 booted exact increment
`sos.compat0.0805cf6bd0b4.db36ed79bb16`, enforcing, with SOS as HOME even
though Launcher3 remained a candidate. Replacing or killing the SOS process did
not transfer HOME ownership; the policy relaunched/reasserted it. Android UI,
PackageManager, and user install remained usable. The final fix limited the
status-bar privilege to the signed Compat chrome and terminated an obsolete
Activity process after package replacement so stale code could not remain the
apparent result.

Compat 1 booted exact increment
`sos.compat1.0805cf6bd0b4.616ac2404a79`. `pm path` found no Launcher3, HOME
resolved to SOS, and user install remained allowed. Files and Settings opened
inside the explicit workspace with the SOS status/actions layer remaining on
top; Back and Exit returned control to SOS. A test notification became a typed
attention record, survived a process restart, and rendered through the fixed
Attention Activity. No unexplained SOS crash, ANR, or SOS-related AVC was
present in the accepted runs.

**Shadow and Core 0A physical evidence:** Shadow booted with Android as owner.
The manual probe created a top one-frame SurfaceComposer layer, after which the
supervised host rendered the GPUI Daily experience through Mali-G68 Vulkan at
1080x2400. Logs reported exclusive `sec_touchscreen` and `gpio_keys`, observed
`sec-pmic-key`, native provider/agent/semantics readiness, and the fixed
watchdog. Injected `SIGABRT` produced the CPU recovery surface; Retry launched a
clean GPUI child, and Android escape exposed the still-live framework UI.

Core 0A booted exact increment
`sos.core0a.0805cf6bd0b4.1b0c9edec481`, waited for Android's CE unlock, then
started the same native shell/input/watchdog. Zygote, `system_server`, SystemUI,
Launcher, phone, Bluetooth, and NFC remained present behind it. The bridge
reported `credential_type=-1 user_unlocked=true` without accepting secret
bytes, `pm install-create` was rejected, and injected failure proved both Retry
and explicit Android fallback.

**Core 0B rejected candidates and fixes:** Four exact candidates were preserved
because each exposed a different architectural dependency:

| Revision | Bytes | SHA-256 | Rejection and correction |
| --- | ---: | --- | --- |
| `3d66364bccad` | 1,020,563,015 | `6ebfb2ec3ec876ee1f831ff02f96dc90dc3bb03c4122a03326a2720f4e634fd2` | Removing PackageInstaller made `system_server` loop with `There must be exactly one installer; found []`. Retain the package as a bootstrap invariant; block sessions and all Activity rendering instead. |
| `42b794490bd0` | 1,021,916,363 | `323d17c7135710e59936cea0e654014969f2af82fbb6d57efb8f77062e360c2e` | `system_server` was stable, but user 0 remained BOOTING and CE locked because no HOME Activity could become idle. Add a product-gated `ensureBootCompleted()` path and skip HOME creation. |
| `7db350df7a62` | 1,021,904,148 | `4ca394ed24b7c967d87dc8c33f9cc94656764e2cd2ce546c8713ce91f56c39c8` | Headless boot/CE passed, but the Core host could not reach the revision authority over Android loopback TCP; strict revision failure correctly reached fixed recovery. Move revision IPC to a confined Unix socket. |
| `2c0e1554da62` | 1,021,923,522 | `0569a90cb9bdadc78c8f768d6458aa542771832dade0420c7bb5d1819db8d22c` | Revision IPC passed, but provider state still used loopback TCP and the strict provider gate reached fixed recovery. Move provider IPC to its own confined Unix socket as well. |

The accepted `4341fa73391c` cold boot first displayed the native PIN surface
while both `sys.boot_completed` and CE availability were unset. Without any ADB
input, the no-credential device then reached `sys.boot_completed=1`,
`sys.user.0.ce_available=true`, and user state `RUNNING_UNLOCKED`. The fixed
layer hid and GPUI rendered. Logs proved the direct headless boot path,
Mali-G68 Vulkan selection, all three raw-input modes, and
`provider_snapshot_remote transport=unix`. The bridge returned
`credential_type=-1 user_unlocked=true`.

`SystemUI`, Launcher, Settings, and the other inspector-listed Android UI APKs
were absent; PackageInstaller remained present but non-rendering. Zygote,
`system_server`, `com.android.phone`, `com.android.bluetooth`, and
`com.android.nfc` remained live. WindowManager reported null current/focused
Activity, and SurfaceFlinger listed only `SOS Core Experience` among SOS/UI
layers. `pm install-create -S 1` threw
`SecurityException: SOS Core product policy prevents user APK installation`.
Both Settings and INSTALL_PACKAGE Activity starts returned code 102 and logged
the product-policy block.

An injected SIGABRT killed only the GPUI child. The parent presented `SOS Fixed
Recovery`; an unattended `android` edge logged
`fixed_recovery_android_unavailable stage=headless` and remained on recovery.
`retry` launched a clean child, which reconnected over Unix IPC and rendered
again with no Android Activity focus. Four Mali/Vulkan loader probes attempted
to search `/data/data` and received enforcing AVC denials against
`system_data_file`. Rendering continued. Broad `/data/data` access was
deliberately rejected; the denial remains a known, narrowly observed probe, not
a granted dependency.

**Core 1 build and physical evidence:** The first Core 1 build failed before
packaging because the shared Samsung product contributed
`ro.zygote=zygote64` while the Core 1 file contributed
`ro.zygote=no_zygote`. Layering two immutable properties was rejected. The
tracked `0005-s5e8825-select-no-zygote-for-sos-core1.patch` now makes the
shared hardware definition choose AOSP `core_no_zygote.mk` for only the Core 1
target, leaving one authoritative property and preserving the other products.

The accepted `1f3cd4b232c2` boot reported `ro.zygote=no_zygote`, enforcing
SELinux, encrypted FBE, empty boot-complete/CE properties, and running
SurfaceFlinger, service managers, native SOS authority, native host, and ADB.
No Zygote, `system_server`, `com.android.*`, or framework-bridge process
existed. SurfaceFlinger listed only `SOS Core 1 Locked`; the screen visibly
reported `NO ZYGOTE` and `CE DATA LOCKED`. Logs recorded
`core1_locked_surface_ready native_synthetic_password=false`. The state
remained stable after ten seconds. An injected child fault produced fixed
recovery, and Retry returned to a fresh locked Core 1 child without starting a
Java process.

Core 1 deliberately proves absence of the APK runtime/process boundary, not
read-only payload minimization: inherited non-UI framework APK files that can
never execute without Zygote remain image-size debt for a later product-pruning
pass. It also deliberately provides no phone/network/Bluetooth/NFC framework
service replacement and no synthetic-password unwrap.

**Ignored visual evidence:** The following selected captures were inspected at
original 1080x2400 resolution:

| Capture | Bytes | SHA-256 |
| --- | ---: | --- |
| `sos-compat0-db36ed79bb16.png` | 177,551 | `f08e28febee54fb3a31668457e5dfa0889c69ccc76d1e6fcb9a6bd76b7f2ae3c` |
| `sos-compat1-home-unlocked-616ac2404a79.png` | 184,482 | `0b64ea783fc852298743f9c42de5116ce14015cd24a03e0a8523da8bb858acae` |
| `sos-compat1-workspace-616ac2404a79.png` | 123,323 | `8885d4dd9117cc91278427bcf1e1d340a32d78a3668316513ad30d894a6e4f5f` |
| `sos-compat1-settings-settled-616ac2404a79.png` | 195,437 | `2e4b7cc01231133d75e2e9ed518cf1696064c57747a6c82f14bb9684f2506741` |
| `sos-compat1-attention-616ac2404a79.png` | 154,602 | `6833692389e87ebb6431dab883fe9e5bbc2a0dcebed967b148226d0e730ecfba` |
| `sos-shadow-surface-probe-1aad692518b8.png` | 15,584 | `ba20c352f66694f517c51b748358882450f56a12a320fcad25530ac83167bd06` |
| `sos-shadow-native-gpui-1aad692518b8.png` | 171,492 | `d548132e4daa017f69b51ffe4a1924c4cc7aa1222cc19ff1ccc988c8611999a2` |
| `sos-shadow-fixed-recovery-1aad692518b8.png` | 17,286 | `37f1c452a9898e0e307412e00331d19c715f8db9cc6325522360bb84eea2fd53` |
| `sos-shadow-android-escape-1aad692518b8.png` | 495,956 | `9cbafe114ef767f05ccd7b5d287a99df8cb0914feb96ad2033954f321718b135` |
| `sos-core0a-native-1b0c9edec481.png` | 171,492 | `d548132e4daa017f69b51ffe4a1924c4cc7aa1222cc19ff1ccc988c8611999a2` |
| `sos-core0a-fixed-recovery-1b0c9edec481.png` | 17,286 | `37f1c452a9898e0e307412e00331d19c715f8db9cc6325522360bb84eea2fd53` |
| `sos-core0a-android-fallback-1b0c9edec481.png` | 496,468 | `a6225ba15ac52ee950d882d7303d08db96dfd15d03a6cfd9e1e8aff3ebb85f28` |
| `sos-core0b-boot-stuck-42b794490bd0.png` | 17,775 | `5a69e3a3f207e1eb96c92951936a0f7d1694efbed0ed885bc1d58d425683826d` |
| `sos-core0b-preunlock-4341fa73391c.png` | 17,451 | `ebcd8bf5257054ca3d3530acd29bf4c5b1cc4bf74e5e50361418b796c5b73c94` |
| `sos-core0b-native-4341fa73391c.png` | 171,492 | `d548132e4daa017f69b51ffe4a1924c4cc7aa1222cc19ff1ccc988c8611999a2` |
| `sos-core0b-fixed-recovery-4341fa73391c.png` | 17,304 | `45dd5b34b3dda575283cff865c38165454b87d6684522130a5f877286b6cbad8` |
| `sos-core0b-after-retry-4341fa73391c.png` | 171,492 | `d548132e4daa017f69b51ffe4a1924c4cc7aa1222cc19ff1ccc988c8611999a2` |
| `sos-core1-locked-1f3cd4b232c2.png` | 17,324 | `fd11466f534905543f75d6c896942cf633250f221ba324e7766890f751de7890` |
| `sos-core1-watchdog-recovery-1f3cd4b232c2.png` | 17,304 | `45dd5b34b3dda575283cff865c38165454b87d6684522130a5f877286b6cbad8` |
| `sos-compat1-restored-home-616ac2404a79.png` | 183,035 | `d0be3a6238351c1de18e77d4a154ac30742669065f1f77a990048952bb51e9ce` |

**Restoration:** From Core 1, `adb reboot sideload-auto-reboot` reached the
packaged Recovery, which accepted the preserved
`compat1-616ac2404a79.zip` at `Total xfer: 1.00x` without formatting data. The
phone then reported exact increment
`sos.compat1.0805cf6bd0b4.616ac2404a79`, profile `compat`, stage `1`,
`ro.zygote=zygote64`, boot complete, CE available, and enforcing SELinux. HOME
resolved to `dev.sos.experience/.SosHomeActivity`; `pm path` found no
Launcher3 and retained SystemUI for Android compatibility ceremonies. Because
the test user has no credential, `wm dismiss-keyguard` performed only the
ordinary no-secret dismissal. Window focus then named `SosHomeActivity`, and
the final capture shows the SOS home plus compatibility chrome. The handset is
left in this accepted Compat 1 state.

**Final source verification:** The final tree passed:

```text
cargo fmt --all -- --check
cargo test -p android-system-authority                 # 3 passed
./tools/sosctl m1-check --abi arm64-v8a
cargo ndk -t arm64-v8a -P 31 check -p sos-experience \
  --release --locked --no-default-features --features core-native
AOSP clang-format 20 --dry-run --Werror over all Core C++ sources
bash -n tools/a33xctl
bash -n tools/sosctl
git diff --check
git apply --reverse --check for framework patch 0004 and device patch 0005
```

The Rust checks emitted only the already-known future-incompatibility warning
from `proc-macro-error2`; no test or compilation failed. Every accepted OTA was
physically installed only after its matching inspector passed.

**Decision / remaining risks / next gates:** The six-stage product split is now
implemented and physically demonstrated. Core 0B is the first stage that
removes Android-rendered ownership while preserving useful Java framework
services; Core 1 is an intentionally locked architecture/recovery validation
target. Neither is yet a daily-use Core release.

The owner still needs to exercise real touch dispatch and the physical
Volume Up+Down Recovery chord. The handset has `CredentialType: NONE`; no
credential was added without authorization, so real PIN verification,
Gatekeeper throttling, fingerprint lockout/unlock, CE-key release, and
authentication-bound Keystore behavior remain open. Compat attention still
needs real call/alarm/security/battery/thermal events, and its workspace is task
containment rather than a separate-user or VM data sandbox. Core needs a native
IME, display-power/suspend repetition on every accepted stage, phone/network/
Bluetooth/NFC provider state and mutations, calls/emergency behavior, audio and
thermal warnings, accessibility delivery/actions, native synthetic-password
ownership, and removal of dead APK payloads from Core 1. Core 0B also preserves
existing `/data/app` data across no-wipe transitions; Activity rendering is
blocked, but a separate background-component/data migration policy remains to
be designed.

## 2026-08-16 — Compat 1 side-button lock gesture ownership repair

**Goal / hypothesis:** Reproduce the report that exact Compat 1 revision
`sos.compat1.0805cf6bd0b4.616ac2404a79` could enter Android keyguard with the
side button but could not return to SOS HOME by touch. Determine whether this
was a touchscreen failure, a credential problem, or an SOS/SystemUI ownership
conflict, recover without altering credentials or user data, and repair the
source boundary.

**Physical evidence:** The connected SM-A336B remained reachable over ADB.
`dumpsys input` reported `sec_touchscreen` enabled on `/dev/input/event5` with
current absolute coordinates, while `dumpsys window` named `NotificationShade`
as focus, `SosHomeActivity` as the focused app behind it, and
`mDreamingLockscreen=true`. Window policy reported keyguard showing; the trust
service reported `deviceLocked=0`; UI Automator described the entry icon as
`Unlocked`. Thus no PIN or Gatekeeper decision was pending. The SOS process
still held status-bar disable record `0x01F70000`. A synthetic ordinary upward
swipe (`adb shell input swipe 540 2050 540 450 500`) left every state unchanged.

Source inspection then found two conflicting Compat policies. The static
`SosCompat1SystemUiOverlay` set
`config_enableNotificationShadeDrag=false`; Android 16's
`NotificationPanelViewController` checks that value in the gesture handler
that also performs keyguard swipe-to-unlock. Independently, the persistent
`SosCompatChromeService` retained `DISABLE_EXPAND` and the other SOS navigation
flags while keyguard was showing. The top-down SystemUI/notification controls
could still respond through separate paths, which explained the apparently
partial touch behavior.

`am stopservice`, `pm disable-user`, and `am force-stop` were rejected or had
no effect because SOS is a non-exported persistent privileged package; these
approaches were discarded. `adb shell wm dismiss-keyguard` performed the
ordinary no-secret dismissal and immediately changed focus to
`SosHomeActivity`, `mDreamingLockscreen=false`, and keyguard not showing. It
modified no credential and recovered the phone to SOS HOME.

**Source repair:** Compat's SystemUI overlay now keeps
`config_enableNotificationShadeDrag=true`. The chrome service registers for
the protected screen-off, screen-on, and user-present broadcasts, releases its
status-bar disable token on screen-off or while `KeyguardManager` reports the
keyguard locked, and restores SOS navigation ownership only after the user is
present. This preserves Android ownership of credential/lock ceremonies while
retaining SOS-owned chrome and shade suppression in the unlocked compatibility
workspace.

The Java change compiled with:

```text
cd apps/experience/android/gradle
./gradlew :app:compileDebugJavaWithJavac \
  -PsosHomeEnabled=true -PsosCompatEnabled=true -PsosAndroidAbi=arm64-v8a
./gradlew :app:lintDebug \
  -PsosHomeEnabled=true -PsosCompatEnabled=true -PsosAndroidAbi=arm64-v8a
# javac BUILD SUCCESSFUL; only the existing Java 8 source/target warnings
# lint task completed under abortOnError=false; its report retained the existing
# unrelated permission/API-level findings and reported no new receiver issue
```

Raw captures remain outside Git in
`/home/carlid/sos-samsung-work/lineage-a33x/evidence-20260816/`:

| Capture | Bytes | SHA-256 |
| --- | ---: | --- |
| `sos-compat1-lockscreen-stuck-616ac2404a79.png` | 482,178 | `5b63b3683596afc02690bc0625d646cb29307e301d39b5b4fe511322b889a310` |
| `sos-compat1-lockscreen-wm-recovered-616ac2404a79.png` | 182,704 | `e9eedbd0ccf51159885298b1a7a481c0d4d624841b4498c1771e07416351e110` |

**Decision / risk / next gate:** Revision `616ac2404a79` remains useful
evidence for the earlier Compat workspace gates but is rejected for the
lock/resume gate. The connected phone is recovered and still runs that old
image, so the defect will recur after another physical lock; the bounded
development recovery is `adb shell wm dismiss-keyguard`. For the current
plugged-in handoff, `adb shell svc power stayon usb` set
`stay_on_while_plugged_in=2`; `dumpsys power` reported `mStayOn=true`, and
keyguard was dismissed with `SosHomeActivity` focused. Unplugging restores
normal timeout behavior, and `adb shell svc power stayon false` clears the
development setting. Do not mark the repair complete from the desktop
compilation. Build and inspect a new Compat 1 OTA,
sideload it without wiping data, then physically repeat side-button lock, wake,
finger swipe-to-dismiss, and return to SOS HOME. The same run must verify that
the Android shade remains suppressed while unlocked, that SOS chrome returns
after `USER_PRESENT`, and later that a real enrolled credential still reaches
the Android bouncer and preserves Gatekeeper/Keystore semantics.

## 2026-08-16 — Redefine Compat as native SOS with an Android app-runtime island

**Goal / corrected hypothesis:** The product owner clarified that Compat is
not SOS chrome around an Android system experience. It is the native SOS
system, visually and behaviorally equivalent to Core, with Android retained
only to execute explicitly selected compatible applications. The visible-frame
invariant is now: SOS, or SOS controls plus one selected non-system Android
application's content; never Android keyguard, SystemUI, navigation/status
bars, notification/quick-settings shade, Settings, permission/install UI,
chooser/file picker, IME, setup, dialer/emergency UI, crash/ANR UI, or Android
Recovery.

This clarification rejects the immediately preceding proposal to repair the
Android keyguard gesture. Re-enabling
`config_enableNotificationShadeDrag` and dynamically returning the status-bar
disable token would have made the old Android ceremony usable, but would have
preserved the wrong UI owner. That Java workaround was reverted and the
`SosCompat1SystemUiOverlay` source was deleted instead.

**Source and build changes:** `lineage_sos_compat_a33x` now packages the Core
fixed pre-unlock host, GPUI runtime, non-rendering LockSettings bridge, and the
generic `sos-ui-removal-marker` alongside the SOS HOME APK. The marker overrides
SystemUI, Launcher, Settings, DocumentsUI, IntentResolver, LatinIME, dialer,
setup/provisioning, and the other inherited Android UI packages. The product
sets `ro.sos.core.autostart=preunlock`, `ro.sos.core.stage=compat`, and
`ro.sos.block_android_system_activities=true`; it deliberately does not set
the full Core Activity block or install-session denial, because compatible
non-system applications must still install and render.

The native host now runs the fixed lock/PIN bridge before CE is available and,
for the Compat stage, hides that surface and hands display ownership to SOS
HOME after unlock. `ActivityStarter` now aborts any remaining system or
updated-system package Activity launch in this product, except the trusted
`dev.sos.experience` host, while retaining the existing all-Activity Core
policy. The Compat workspace
also excludes system and updated-system launcher candidates. The inspector now
requires the native host/runtime/bridge, UI-removal marker, selective framework
policy marker, and pre-unlock properties, rejects the old SystemUI overlay, and
rejects every known Android UI APK in target files.

The source revision was `19d8a653fbd7-dirty`. Evidence commands and results:

```text
cd apps/experience/android/gradle
./gradlew :app:compileDebugJavaWithJavac \
  -PsosHomeEnabled=true -PsosCompatEnabled=true -PsosAndroidAbi=arm64-v8a
# BUILD SUCCESSFUL; only existing Java 8 source/target warnings

cd /home/carlid/dev/sos
cargo ndk -t arm64-v8a -P 31 check -p sos-experience --release --locked \
  --no-default-features --features core-native
# passed; only the existing future-incompatibility warning
./tools/sosctl m1-build --abi arm64-v8a --home --compat
./tools/a33xctl stage-sos
# both passed

cd /home/carlid/dev/lineage-a33x
source build/envsetup.sh
breakfast sos_compat_a33x
m -j8 SosFrameworkBridge sos-core-host sos-ui-removal-marker SosShell \
  SosCompat1FrameworkOverlay
# completed successfully in 02:13
m -j8 services
# completed successfully in 01:00; the selective Activity policy compiled,
# was optimized, dexpreopted, and installed as system/framework/services.jar

# After the final init-comment/source synchronization:
m -j8 sos-core-host
# completed successfully in 04:50, including regenerated product/Soong state

# After extending the selective rule to FLAG_UPDATED_SYSTEM_APP as well:
m -j8 services
# completed successfully in 08:46 after a broader local framework dependency
# refresh; ActivityStarter javac/R8/dex packaging and API checks passed

git -C frameworks/base apply --reverse --check \
  /home/carlid/dev/sos/aosp/patches/a33x-lineage-23.0/\
0004-frameworks-base-enforce-sos-core-install-policy.patch
# passed: the tracked patch exactly reverses the staged framework change

cd /home/carlid/dev/sos
bash -n tools/a33xctl
git diff --check
# passed; shellcheck was unavailable in this environment
```

Generated build evidence remains outside Git:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `/home/carlid/dev/sos/artifacts/sos-experience.apk` | 37,786,852 | `9a14eb48eb1e004de85a78703c8b3c2cd44d6d1097e91f48807ec5c3b7a3e703` |
| `out/target/product/a33x/system_ext/bin/sos-core-host` | 68,328 | `3f915d9b4f171a9442c26c70f27969c61bb48e1ff23df74f117f6658db4a7c7b` |
| `out/target/product/a33x/system_ext/etc/init/sos-core-host.rc` | 1,385 | `85e75fa5f280f01e37ec7231179821d14d3cdde8aa8bd910a9e7ef2e40d2463d` |
| `out/target/product/a33x/system_ext/bin/sos-ui-removal-marker` | 51,216 | `6ca538a90323d4f1907be2dd5ab2381e7507b5fd62e6b295e6c0f464a16a4c14` |
| `out/target/product/a33x/system_ext/priv-app/SosShell/SosShell.apk` | 40,722,809 | `981e34f59b541509ab4c76b7dafd1a88989a0ece9bcafb82cabfcd361d18420c` |
| `out/target/product/a33x/system_ext/priv-app/SosFrameworkBridge/SosFrameworkBridge.apk` | 16,798 | `3f9306ff6daf4a0b96154bc18d34c22868aec0d087463be085791b433e95b59a` |
| `out/target/product/a33x/system/framework/services.jar` | 22,164,791 | `df44c18185e8d007b3c7b10cb703f9910d42305ca493c57f03188936b0f15b25` |

**Decision / risks / next gate:** This is a compiled architectural scaffold,
not an accepted Compat build. No complete OTA was produced, inspected, or
flashed. The connected phone remains on the rejected Android-visible revision
`616ac2404a79`, recovered to `SosHomeActivity` with `mStayOn=true` while USB is
connected. Its current side-button defect therefore still exists.

The highest-priority missing implementation is native runtime re-lock after the
post-unlock host hands off to HOME, including display power, side-button wake,
credential/Gatekeeper/Keystore state, fingerprint lockout, and fixed recovery
if HOME dies. SOS-native permission, install, chooser, keyboard, calls,
emergency, attention, alarm, crash, thermal, and recovery brokers are also
required; their Android implementations now fail closed rather than render.
The Java `View`/`Button` workspace, attention, and side-chrome prototypes also
remain a visual trace risk and cannot pass the “feels like Core” gate until
GPUI/fixed SOS rendering replaces them or physical inspection proves no
platform-default widget appearance.
App task containment, Back/Home/Exit controls, process restart, app
crash/ANR, suspend/resume, and a separate app-data isolation policy remain
physical/security gates. The next image may be flashed only after a complete
OTA passes `inspect-compat1`; physical acceptance then requires boot and
pre-unlock, side-button lock/wake/unlock, app launch/return, failure/recovery,
and SurfaceFlinger/window inspection proving that no Android system surface is
ever visible.

## 2026-08-16 — Share the native Core shell with Compat and close runtime Android UI escapes

**Goal / hypothesis:** Implement the corrected Compat model without creating a
second SOS experience that can drift from Core: one native SOS shell and fixed
renderer should own boot, lock, unlock, HOME recovery, and system attention in
both products. Compat may retain Android only behind headless adapters needed
to enumerate, launch, contain, and report compatible non-system applications.
Any remaining Android system Activity, framework dialog/window, crash/ANR
surface, keyguard, shade, launcher, Settings, picker, installer, keyboard, or
other system chrome must be absent or fail closed.

**Shared product and runtime composition:** Added
`sos_native_host_common.mk` as the single package/property fragment for
`sos-core-experience-runtime`, `sos-core-host`, the generic
`sos-ui-removal-marker`, and fixed pre-unlock startup. Added
`sos_headless_android_adapter_common.mk` on top of it for products that retain
the non-rendering `SosFrameworkBridge`. Compat and Core 0B now inherit the
headless adapter fragment; Core 1 inherits the native host fragment directly.
The variation is therefore an adapter boundary, not a fork of the shell:

```text
ExperienceHost + fixed SOS renderer
  |- Core 1: SurfaceComposer/raw-input adapter
  `- Compat/Core 0B: NativeActivity/task + headless framework adapter
```

The former Compat-only and Core-only UI removal markers were collapsed into
`sos-ui-removal-marker`, with the same override set for SystemUI, launcher,
Settings, DocumentsUI, IntentResolver, LatinIME, dialer/emergency UI, setup,
provisioning, and other inherited Android UI packages. The now-contradictory
Compat SystemUI overlay was removed. Compat identifies its owner as
`native-sos-android-runtime` and keeps only the selective system-Activity and
framework-window block; Core retains the full Activity and install-session
blocks.

**Native lock and recovery:** The shared C++ host now remains as a Compat
supervisor after fixed pre-unlock. It owns the fixed lock/PIN surface, uses the
existing headless LockSettings bridge for credential verification, and hands
off to SOS HOME only after unlock. An abstract Unix control socket
`sos_native_shell_control` accepts versioned commands only from Android's
system UID. The framework bridge sends the runtime-lock command during
`SCREEN_OFF` using `goAsync()` so the fixed native lock has acquired the
surface, touchscreen, and GPIO volume keys before a native readiness byte
releases the broadcast and suspend may proceed. A review found and rejected an
initial write-only version of this protocol: socket delivery did not prove the
native lock was ready. The Samsung side power key remains owned by its separate
`sec-pmic-key` input path for display power and wake. Credential type `NONE`
uses the fixed enter-to-unlock path; unsupported credential state fails closed.

Core 0B shares only the bridge's credential commands. The screen-off receiver,
HOME start command, and heartbeat monitor are gated on
`ro.sos.core.stage=compat`, preventing Compat lifecycle policy from leaking
back into the headless Core product.

The same supervisor starts SOS HOME through the bridge and presents a fixed
native recovery surface if HOME never becomes ready or later stops reporting.
Recovery Retry asks the bridge for another bounded HOME start rather than
falling through to Launcher/SystemUI. Native evidence markers cover control
readiness, runtime unlock, HOME failure, and the fixed recovery action.

HOME liveness is readiness-based rather than process-based. The bridge declares
the signature permission `dev.sos.permission.REPORT_HOME_HEARTBEAT`, protects
its exported dynamic heartbeat receiver with that permission, grants the same
permission to the platform-signed SOS package, waits 30 seconds at initial
boot, and then requires a heartbeat within 16 seconds. `SosApplication` sends
one every five seconds only while `GpuiActivity` has both been created and
completed native initialization and `SosCompatChromeService` has installed its
trusted overlay. Start grace is bounded, and failure state resets after a
restart/heartbeat so repeated failed retries return to fixed recovery.

**Android app adapter and fixed surfaces:** `SosAndroidAppAdapter` is now the
single headless boundary for listing exported launcher Activities from
non-system/non-updated-system packages, launching the explicitly selected app,
injecting Back, and returning to SOS HOME. Compat disables the Android picker,
permission, and biometric helper Activities at manifest generation time.
Workspace, side chrome, and attention were rewritten as fixed custom Canvas
surfaces using `SosFixedUi`; the platform `Button`, `TextView`, `LinearLayout`,
and `ScrollView` visual prototypes were removed. This closes the obvious
widget-theme drift but does not yet prove pixel parity, accessibility virtual
children, transitions, touch geometry, or truncation on physical hardware.

`SosSystemAttentionReceiver` records crash/ANR facts in the SOS attention
journal without showing Android dialogs. It is protected by
`android.permission.STATUS_BAR_SERVICE`, so ordinary applications cannot forge
system attention.

**Framework membrane:** The tracked `frameworks/base` patch now has three
Compat-specific enforcement layers in addition to the existing Core policies:

- `ActivityStarter` refuses Activities belonging to system or updated-system
  packages, except the trusted SOS experience host.
- `AppErrors` preserves PackageWatchdog notification but suppresses Android
  crash/ANR dialogs, posts a protected explicit fact broadcast to SOS outside
  the process lock, and kills an unresponsive app after reporting it.
- `WindowManagerService` keeps system-server callback windows logically alive
  but makes every system-UID system-window type fully transparent,
  non-focusable, non-touchable, and non-dimming. This contains global actions,
  boot/shutdown, debugger, strict-mode, IME, and system-dialog escape paths
  without throwing from framework call sites and destabilizing system_server.

The added SELinux rule permits the persistent `system_app` bridge to connect to
the native host socket; the socket still authenticates `SO_PEERCRED` as the
system UID.

**Build and static evidence:** A first product-matrix attempt used the old
`lunch lineage_sos_*_a33x-userdebug` form and failed because Android 16 requires
an explicit release field. This was a command-selection failure, not a source
failure. Repeating the matrix through each product's supported `breakfast`
target passed and resolved the intended composition:

```text
lineage_sos_compat_a33x  shared_native_host=PASS  bridge=present
lineage_sos_core0b_a33x  shared_native_host=PASS  bridge=present
lineage_sos_core1_a33x   shared_native_host=PASS  bridge=absent
```

The following source and targeted-build gates passed:

```text
cd apps/experience/android/gradle
./gradlew :app:compileDebugJavaWithJavac :app:assembleRelease \
  -PsosHomeEnabled=true -PsosCompatEnabled=true -PsosAndroidAbi=arm64-v8a
# BUILD SUCCESSFUL; only the existing Java 8 source/target warnings

cd /home/carlid/dev/lineage-a33x
source build/envsetup.sh
breakfast sos_compat_a33x
m -j8 SosFrameworkBridge sos-core-host sos-ui-removal-marker SosShell \
  SosCompat1FrameworkOverlay services
# completed successfully in 03:05
m -j8 services
# completed successfully in 00:56 after adding the system-window membrane
m selinux_policy
# completed successfully in 00:41, including neverallow and compatibility tests

cd /home/carlid/dev/sos
/home/carlid/dev/lineage-a33x/prebuilts/clang/host/linux-x86/\
clang-stable/bin/clang-format --dry-run --Werror \
  aosp/device/sos/a33x/core/host.cpp
bash -n tools/a33xctl
git diff --check
git -C /home/carlid/dev/lineage-a33x/frameworks/base apply --reverse --check \
  /home/carlid/dev/sos/aosp/patches/a33x-lineage-23.0/\
0004-frameworks-base-enforce-sos-core-install-policy.patch
# all passed; the tracked patch exactly reverses the staged framework source
```

The unqualified `clang-format` spelling was initially retried from the SOS
shell and failed because that binary is not on its PATH. The AOSP-pinned
`clang-stable` binary above was used instead and passed; no source change was
needed.

`a33xctl inspect-compat1` was extended to require the shared host, fixed lock
and recovery markers, bridge and signature heartbeat path, system Activity
policy, crash/ANR suppression, system-window membrane, disabled Android helper
Activities, and generic removal marker. It rejects the deleted SystemUI overlay
and every known visible Android UI APK.

The initial complete build expanded to 74,132 tasks after the earlier product
matrix changed Soong configuration from Core 1. At 20%, review found that the
screen-off command had no readiness acknowledgement, so that knowingly stale
build was stopped and the completed Ninja outputs were retained. Killing the
outer wrapper left its containerized Soong child holding `out/.lock`; the first
retry therefore failed after the documented 10-second lock timeout. Stopping
that exact ephemeral build container required Podman's SIGKILL fallback after
its 10-second SIGTERM deadline. A second early build was likewise stopped after
review found that Core 0B would inherit Compat HOME monitoring. Both findings
were fixed before the final stage. No generated output was treated as evidence
until a clean final `bacon` invocation completed.

Final full-image and inspection evidence:

```text
cd /home/carlid/dev/sos
./tools/a33xctl build-compat1
# build completed successfully in 10:00
# revision sos.compat1.19d8a653fbd7.398c68858f5e

./tools/a33xctl inspect-compat1
# passed OTA ZIP integrity and whole-package signature verification
# passed boot/dtbo/recovery/vbmeta PIT and AVB checks
# passed product properties, native/bridge/framework markers, manifest policy,
# source-to-package comparisons, and the Android UI APK absence scan
# ==> SOS compat1 ARM64 target-files gate passed

cd apps/experience/android/gradle
./gradlew :app:lintDebug \
  -PsosHomeEnabled=true -PsosCompatEnabled=true -PsosAndroidAbi=arm64-v8a
# BUILD SUCCESSFUL in 4s; abortOnError=false
```

The lint report is not a clean quality gate: it contains 32 errors and 54
warnings, dominated by the existing platform-privileged camera/location/Wi-Fi
permission calls and hidden `statusbar` service constant. The new fixed
surfaces add non-blocking default-locale/draw-allocation warnings, and the app
adapter adds a package-query warning. There was no new manifest receiver or
compilation error. These findings remain follow-up work; the successful AOSP
platform build and inspector, not Gradle's non-failing lint configuration, are
the static acceptance evidence above.

Generated evidence remains outside Git:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `out/target/product/a33x/lineage-23.0-20260816-UNOFFICIAL-sos_compat_a33x.zip` | 1,042,595,904 | `2a89be077727beb19f59c146afe9b5598760b2bd5af8d70b9d371b6de55d2962` |
| `/home/carlid/dev/sos/artifacts/sos-experience.apk` | 37,786,976 | `26739998ab095156f2006e579954884f379380d346318028b7aea5a6d2d04618` |
| target files `SYSTEM_EXT/bin/sos-core-host` | 84,776 | `4cd0d5fab4785ae247d9c288e4cf0a79f86e3ee5396196bdae802eae2861f6c8` |
| target files `SYSTEM_EXT/lib64/libsos_core_experience.so` | 14,526,424 | `6ae4b8217349af06d3b52e9832b2f345aa6f90b0c4ad6fbeb94d26a881189201` |
| target files `SYSTEM_EXT/priv-app/SosFrameworkBridge/SosFrameworkBridge.apk` | 20,894 | `4436eea159189ba33f5f679ae2a44ac926d000b4ba8414ac063fb84c7dff2c5f` |
| target files `SYSTEM_EXT/priv-app/SosShell/SosShell.apk` | 40,717,609 | `631d8fcc91ea542b984f0f111cac479bfa6bb70932703ab449d3c0c7f9ab1534` |
| target files `SYSTEM_EXT/bin/sos-ui-removal-marker` | 51,216 | `6ca538a90323d4f1907be2dd5ab2381e7507b5fd62e6b295e6c0f464a16a4c14` |
| target files `SYSTEM/framework/services.jar` | 22,166,123 | `2b3abc71a4a8e4230e784c912083140af1e0c2255a464bfcb943e2868eb424be` |

The inspector extracted revision `398c68858f5e` bootstrap images under
`/home/carlid/sos-samsung-work/lineage-a33x/lineage-23.0-20260816-UNOFFICIAL-sos_compat_a33x-bootstrap/`:

| Bootstrap artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `boot.img` | 67,108,864 | `1a8305a6fb8328a398caeb2301a1d5098bc1b20410f47e41ac1b562437733f9c` |
| `dtbo.img` | 8,388,608 | `9b4bdf238d8f30c9de94f9ebf6503a203e0de3e952ba73eb38a7d2130b7c161c` |
| `recovery.img` | 100,663,296 | `b603066aee10469551901f22ad9639a75d482ee96b0a6396842fbbd3591e79e4` |
| `vbmeta.img` | 8,192 | `8bd23c4eb6d775be0828c3fc7c7d993810a23cbfdf3c1a5786d8bb2831d2dab3` |
| `vendor_boot.img` | 33,554,432 | `1fdad7b9c3ca06d2021063bff762290d3219df788056cc7877c0e405b6cc9106` |

**Decision / remaining risks / next gate:** Revision
`sos.compat1.19d8a653fbd7.398c68858f5e` passes the complete build and static
target-files gate for the one-shell architecture and Android UI membrane. This
is not a hardware acceptance result. The connected A33x was deliberately left
on the rejected revision `616ac2404a79`, recovered to SOS HOME with USB
stay-awake enabled; no image from this change was flashed. The new archive is
eligible for a later controlled sideload, but is not accepted as Compat until
the physical gates below pass.

Physical acceptance must then cover boot/pre-unlock, credential and
credential-none paths, side-button lock/suspend/wake/relock, exclusive touch,
volume/power routing, HOME-ready heartbeat and repeated recovery failure,
selected app launch/Back/Home/Exit, app crash and ANR, and SurfaceFlinger/window
captures proving no Android system surface is visible. SOS-native permission,
install, chooser, keyboard, call/emergency, alarm, thermal, and Recovery
brokers remain absent and fail closed; app-data isolation, accessibility
virtual nodes, fixed-layout clipping, and fingerprint/Gatekeeper/Keystore
behavior remain explicit security and physical gates.

## 2026-08-16 — Native Compat A33x hardware pass with release-blocking Android UI escapes

**Goal / hypothesis:** Sideload revision
`sos.compat1.19d8a653fbd7.398c68858f5e` onto the connected Samsung A33x and
test the claim that Compat now feels like Core while retaining only contained
Android application execution. The required evidence was boot ownership,
side-button-equivalent screen-off locking, raw-input ownership, application
launch/return, system-Activity and crash-dialog suppression, HOME liveness,
fixed recovery, and an Android-surface audit. No data wipe was authorized or
performed.

**Device and image:** Device `RFCT50EGFCN` (`SM_A336B`) was authorized over
USB, at 99% battery with external power and USB stay-awake enabled. Preflight
confirmed the rejected old Compat revision
`sos.compat1.0805cf6bd0b4.616ac2404a79`, profile `compat`, completed boot, and
SELinux enforcing. The following OTA was hash-checked before use and installed
with `adb reboot sideload-auto-reboot` followed by `adb sideload`; recovery
reported `Total xfer: 1.00x` and the device was not wiped:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `/home/carlid/dev/lineage-a33x/out/target/product/a33x/lineage-23.0-20260816-UNOFFICIAL-sos_compat_a33x.zip` | 1,042,595,904 | `2a89be077727beb19f59c146afe9b5598760b2bd5af8d70b9d371b6de55d2962` |

**Boot and ownership evidence:** The first boot and a final unperturbed cold
boot both reached the exact intended revision with
`ro.sos.ui_owner=native-sos-android-runtime`, profile/stage `compat`,
`ro.sos.compat.block_system_activities=true`, `sys.boot_completed=1`,
`sys.user.0.ce_available=true`, and SELinux enforcing. HOME resolved uniquely
to `dev.sos.experience/.SosHomeActivity` from the system-ext `SosShell.apk`;
there was no `/data/app` override. `pm path` returned no SystemUI or launcher,
and SurfaceFlinger showed only SOS HOME plus `SOS Trusted App Controls` after
handoff. The final device state had a single HOME task (`t2`) focused, the SOS
package enabled with `stopped=false`, no trusted lock/recovery layer, and an
empty `pm list packages -3` result.

The visual boot gate nevertheless failed. Both HOME captures contain blank
white bands across the physical top and bottom edges. WindowManager reports a
1080x2400 HOME frame but `mAppBounds=Rect(0, 88 - 1080, 2400)`, and the current
`GpuiActivity` does not explicitly opt out of decor fitting or configure the
bar surfaces. There are no Android status/navigation icons, but the visible
frame is not the full SOS-native frame and therefore does not meet Core parity.

**Runtime lock:** With logs cleared, `adb shell input keyevent 26` took the
device to `Wakefulness=Asleep`. The bridge logged
`native_compat_control command=lock`, the native host exclusively acquired
`sec_touchscreen` and `gpio_keys`, rendered the trusted lock, and acknowledged
readiness before the bridge completed its `reason=screen-off` path. A second
power keyevent woke to a full-frame native `SOS LOCK` keypad with `PRESS ENTER`;
SurfaceFlinger contained `SOS Trusted Lock` and no Android system surface. This
passes the suspend readiness/race and native ownership gates. It does not pass
the human input gate: injected framework taps cannot cross the intentional raw
grab, SELinux correctly denied shell `sendevent` writes, `adb root` was
correctly unavailable, and no physical ENTER tap was completed during the
observation window. Physical side-key routing and physical touch unlock remain
pending.

The lock logs exposed a separate implementation defect. Android encodes
credential type `NONE` as `-1`, while the host also uses `-1` as its
"status not initialized" sentinel. It consequently re-queries and logs
`framework_bridge_status credential_type=-1` approximately every 100 ms until
CE becomes available, and does the same indefinitely on a runtime relock. The
host needs a separate status-ready bit or a non-overlapping sentinel.

**Android application and membrane evidence:** Two local AOSP test APKs were
used and removed afterward:

| Test APK | Bytes | SHA-256 |
| --- | ---: | --- |
| `MultiDexLegacyTestApp_without_corrupted.apk` | 25,057 | `ee924db9163b2e243374fcaee2f51c90b2e9bf848ac549c204fa1b80c294997a` |
| `ExactCalculator.apk` | 4,740,863 | `25226826e80c4df6dc73e34016ac90cf084701e230d24c1705924b576104a230` |

The target-SDK-19 MultiDex install was initially rejected with
`INSTALL_FAILED_DEPRECATED_SDK_VERSION`; retrying the intentional compatibility
fixture with `adb install --bypass-low-target-sdk-block` succeeded. The SOS
workspace still rendered `NO COMPATIBLE APPLICATIONS` even though shell package
queries found the exported non-system launcher Activity. This disproves the
enumeration path: `SosAndroidAppAdapter` lacks the launcher-intent `<queries>`
visibility declaration (or an equivalent narrowly scoped privileged query).

Explicitly launching that legacy app produced a visible Android
PermissionController `ReviewPermissionsActivity` beneath the persistent SOS
chrome. A direct shell launch of the same PermissionController component is
blocked with result code 102, so the existing `ActivityStarter` check does not
cover the framework-initiated permission-review route used during application
start. This is a direct Android-experience escape and a release blocker.

The modern target-SDK-35 ExactCalculator launch passed: calculator content was
visible with only the fixed SOS side chrome, SurfaceFlinger showed no Android
system chrome, and tapping SOS BACK logged
`compat_app_action action=back` and returned to HOME. Forcing Calculator to
crash with `adb shell am crash com.android.calculator2` also passed:
`AppErrors` logged `SOS Compat suppressed Android crash dialog`, the protected
SOS attention receiver persisted the crash fact, and focus returned to HOME
without an Android dialog. A real `INSTALL_PACKAGE` intent resolved to
`com.android.packageinstaller/.InstallStart` but was blocked with result code
102 while HOME retained focus. ANR suppression was not dynamically exercised.

**Native recovery:** Force-stop alone did not terminate the platform-signed
SOS process because the package is persistent. Disabling it for user 0 and
then issuing a controlled `am crash dev.sos.experience` removed the healthy
HOME heartbeat; after approximately 20 seconds the native host placed a
full-frame `SOS Fixed Recovery` surface above Android. This passed the liveness
timeout and fail-closed rendering gate. The package was re-enabled and restored
to `stopped=false`. Recovery is intentionally sticky, and its retry action uses
raw volume input, so the physical Retry gate was not exercised.

The abnormal disable/re-enable sequence also exposed lifecycle fragility. The
disabled-user state persisted across the first immediate reboot. Re-enabling
and explicitly starting HOME before CE availability caused repeated
`must construct App on main thread` GPUI panics and transient HOME-task churn;
fixed recovery remained in control. After restoring and flushing package state,
a cold boot without any pre-unlock Activity injection reached a single healthy
HOME task normally. This was a test-induced sequence rather than a normal-boot
failure, but recovery retry and duplicate NativeActivity initialization need a
dedicated hardware regression before acceptance.

Representative commands were:

```text
adb shell getprop ro.system_ext.build.version.incremental
adb shell dumpsys window
adb shell dumpsys SurfaceFlinger --list
adb shell input keyevent 26
adb install --bypass-low-target-sdk-block MultiDexLegacyTestApp_without_corrupted.apk
adb install ExactCalculator.apk
adb shell am crash com.android.calculator2
adb shell am start -W -a android.intent.action.INSTALL_PACKAGE \
  -d content://dev.sos.test/app.apk \
  -t application/vnd.android.package-archive
adb shell pm disable-user --user 0 dev.sos.experience
adb shell am crash dev.sos.experience
adb shell pm enable --user 0 dev.sos.experience
adb shell sync
```

Raw generated evidence is outside Git under
`/home/carlid/sos-samsung-work/lineage-a33x/evidence-20260816-native-compat/`:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `sos-native-compat-boot-398c68858f5e.log` | 4,079,095 | `2bc88bbdee2f04fb78da4bc135d6d7768bec249e758e26c194c9c14811fd75cd` |
| `sos-native-compat-home-398c68858f5e.png` | 180,662 | `f8e2db40faae41784a3ffe7f0c69a58a6c02aa93d26f35dfdbbbede22c96c4f4` |
| `sos-native-compat-runtime-lock-398c68858f5e.log` | 1,090,104 | `965011a27fbec5801bcd5830b21f5c2196fa4095b8cb256e0a38eead6ecc1a48` |
| `sos-native-compat-runtime-lock-398c68858f5e.png` | 17,427 | `6d95ba3998bec036caeac94922e8897a884cf9927b26a4b8b01fdf75ed333d9e` |
| `sos-native-compat-workspace-398c68858f5e.png` | 59,098 | `bf9ba72b33b4f4eec36cc3a6c6f7c07e4ff5f830fa1f23a3f6acb61b37dd5b0b` |
| `sos-native-compat-test-app-398c68858f5e.png` | 86,034 | `7ec3be7b1922e78d9e95d0264a87820c9ff47c012a5f3e17629f41cfb3fba630` |
| `sos-native-compat-calculator-398c68858f5e.png` | 111,445 | `29866fec853087034ee0d63d2316edcde1cc61694af91fc972e9eaddca0cf0f9` |
| `sos-native-compat-crash-398c68858f5e.log` | 111,266 | `7619cef2f25da7908be29a5f50ca3e632a3853f1c546f00d9833af8c96ca4088` |
| `sos-native-compat-crash-398c68858f5e.png` | 181,173 | `42f4f576345e2d83d91fd74d9179aa576e49ed2aca4e127132a3c033062ccead` |
| `sos-native-compat-installer-block-398c68858f5e.log` | 67,713 | `bac234b3535630684ba88d5982fc9abd8d748b5bba14d9f3cde7257235b80101` |
| `sos-native-compat-fixed-recovery-398c68858f5e.log` | 205,952 | `34a44d30a17b3942791021a52d2f963ed099e6bafa6de4f6b38bb034befdc06c` |
| `sos-native-compat-fixed-recovery-398c68858f5e.png` | 17,304 | `45dd5b34b3dda575283cff865c38165454b87d6684522130a5f877286b6cbad8` |
| `sos-native-compat-fixed-recovery-surfaces-398c68858f5e.txt` | 3,240 | `bf3904702e826fd351b19e143bbf90ec96e734956ce8ba23859f0897cf9e0e6a` |
| `sos-native-compat-final-boot-398c68858f5e.log` | 3,649,365 | `f3bda2fbe4a4774cf9fd1e6357c76f6dc2879abc2aae12b616318596bb12c021` |
| `sos-native-compat-final-home-398c68858f5e.png` | 181,849 | `d51a1f5ab8f99949bdcca213d1b0b9b20b97a511357f249bf18a05e85cec5367` |
| `sos-native-compat-final-surfaces-398c68858f5e.txt` | 3,308 | `ca0ee2d55645a7ba1bda872a86054ed636312908bc46ecab9f610fab6f8b3d21` |

**Decision / remaining risks / next gate:** Reject revision
`sos.compat1.19d8a653fbd7.398c68858f5e` for Compat acceptance. The shared native
host, readiness-acknowledged lock, fixed recovery, selected modern-app chrome,
crash suppression, installer block, and removal of packaged SystemUI/Launcher
all work on hardware. Acceptance remains blocked by the HOME inset bands, app
enumeration visibility failure, permission-review Android UI escape, and the
credential-`NONE` sentinel bug. The next image must fix those four defects,
then repeat the physical side-button/touch ENTER and recovery Retry gates plus
permission, ANR, chooser, keyboard, emergency/call, alarm, thermal, and
accessibility brokers. The phone was left booted on the exact tested revision,
awake at SOS HOME, with CE unlocked, the SOS package enabled, and all temporary
test applications removed.

## 2026-08-16 — Compat 1 Core-parity integration and exact-image hardware rerun

**Goal / hypothesis:** Resolve every blocker found by the rejected native-Compat
hardware pass and enforce the clarified product invariant: Compat is Core's SOS
presentation and supervision stack plus a narrowly bounded Android application
runtime, never an Android experience with an SOS launcher. The rebuilt image
had to remove the Android lockscreen/shade path, reuse Core implementation
instead of forking it, fill the physical frame, enumerate only eligible apps,
block framework-redirection ceremonies, survive HOME process death, and show
complete SOS chrome across task handoffs. A desktop build could not close the
gate; the exact OTA had to be flashed and measured on `RFCT50EGFCN`.

**Architecture and code changes:** Compat 1, Core 0B, and Core 1 now include the
same `sos_native_host_common.mk` and `sos_headless_android_adapter_common.mk`
fragments. They share the fixed native host/runtime, pre-unlock autostart,
headless LockSettings bridge, UI-removal marker, and inherited-package removal
set. The obsolete Compat SystemUI overlay was deleted because there is no
SystemUI process to configure. Compat still builds the same Rust
`ExperienceHost` as Core; only its NativeActivity/task adapter is product-
specific.

The native host remains alive after unlocked handoff and re-enters its direct
SurfaceComposer lock on protected screen-off. Bridge status readiness is now a
separate state from credential type, so Android's valid `NONE == -1` result no
longer collides with an uninitialized sentinel. The HOME watchdog is
restart-first: heartbeat loss asks the persistent bridge to start a clean HOME,
and enters fixed native Recovery only if that bounded request cannot restore
readiness. The framework bridge accepts that request only after CE is available
and records the result without exposing an Activity of its own.

The framework policy patch now checks the final Activity target after legacy
permission-review and other framework interception, not only the caller's
initial resolution. It also makes remaining system-UID system windows
transparent, non-focusable, and non-touchable while preserving framework
progress callbacks. `SosShell` declares narrowly scoped launcher queries;
`SosAndroidAppAdapter` exposes only exported launcher Activities from eligible
non-system, non-updated-system, non-legacy packages.

The Android-hosted fixed surfaces were consolidated instead of copied:
`SosWindowPolicy` owns full-frame/cutout/system-bar behavior,
`SosFixedActivity` owns the focus lifecycle, and `SosFixedUi` owns Canvas
rendering for workspace, attention, and controls. `SosVisibleIdentity` maps
installed packages to their application labels, platform package `android` to
`SOS RUNTIME`, and unknown package-shaped strings to `COMPATIBILITY APP`; raw
package rows and the old substrate copy were removed. Workspace copy now reads
`Open a compatible application. SOS remains in control.`

The permanent control service uses a software text layer and an atomic
transition protocol. `beginTransition()` hides the whole overlay synchronously
inside the control-up or app-launch event, before Activity transition work can
reuse the old surface. Destination focus then reveals one complete frame after
250 ms for SOS-owned Activities or 750 ms for a foreign application. Workspace
and attention no longer issue racing service starts. The inspector requires the
compiled marker
`transition_reveal=atomic controls=back,apps,attention,exit`, the shared
full-frame classes, visible-identity helper, restart-first watchdog markers,
and the absence of the old SystemUI overlay and Android-visible copy.

The detailed product boundary and ownership record were updated in
[`android-product-split.md`](android-product-split.md) and
[`android-ui-ownership-stages.md`](android-ui-ownership-stages.md).

**Rejected approaches and fixes:** The following failures were material to the
result and were not treated as passes:

- Full-frame HOME/window flags alone fixed the 88-pixel Android inset but did
  not prevent the SOS control rectangles from surviving a task switch while
  their text layer was clipped.
- Extra invalidation, a software layer by itself, and a 250 ms focus redraw
  each still admitted a partial control frame on hardware. Splitting the delay
  to 250/750 ms fixed foreign-app settling but duplicate Activity `onCreate`
  service starts still raced the focus owner. Removing those starts and hiding
  synchronously at the input boundary produced the first invariant handoff:
  destination-only at 100 ms, complete chrome at 350 ms for SOS, and complete
  chrome at 850 ms for Calculator.
- A generic Gradle release APK used for rapid proof had
  `sosHomeEnabled=false`. Installing it as a system-app update correctly left no
  enabled HOME Activity and drove fixed Recovery; `aapt` exposed the disabled
  alias. This was a proof-artifact configuration error, not an image defect.
  `pm uninstall-system-updates dev.sos.experience` restored the `/system_ext`
  APK, and every later proof build used
  `-PsosHomeEnabled=true -PsosCompatEnabled=true` plus the product platform
  certificate.
- Revision `sos.compat1.19d8a653fbd7.f21bca865cea` was a flashed exact-image
  baseline while the final handoff was isolated with platform-signed app
  updates. Its overwritten OTA was 1,042,585,450 bytes, SHA-256
  `289209155ecb56b9f9f569b5d7a8c69c6e51a78f75f88d4cf905f5653e063a32`;
  it is not the accepted rollback artifact.
- The last rapid hardware proof APK was kept outside Git at
  `/tmp/sos-chrome-handoff-proof.6c7geK/SosShell-platform.apk`, 37,829,815
  bytes, SHA-256
  `2a086f38f12d9cf413b139bba99a2560cb4ea9840da51cc10686d2b5732e3715`.
  Its source-matched 100/350/1000 ms captures validated the event boundary
  before committing to another full OTA build. The update was removed before
  final flashing, and `pm path` returned only the system-ext APK.

**Build and package evidence:** `./tools/a33xctl build-compat1` completed in
3:09 and produced revision
`sos.compat1.19d8a653fbd7.220e268c228f`. The build reported 112/112 targets and
`Package Complete`. `./tools/a33xctl inspect-compat1` then passed archive CRC,
whole-package signature, PIT ceilings, boot/recovery/dtbo/vbmeta AVB,
target-files hashtrees, package/removal policy, manifest, compiled marker,
SELinux labeling, and stage-property checks:

| Final artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `/home/carlid/dev/lineage-a33x/out/target/product/a33x/lineage-23.0-20260816-UNOFFICIAL-sos_compat_a33x.zip` | 1,042,568,904 | `bbba76ca12ed831bbd3feb053f1c21196749f888051b8447a809f7f46e27cf52` |
| `SYSTEM_EXT/priv-app/SosShell/SosShell.apk` | 40,747,113 | `55ed4f3694f01869e20ed19724ae229d9a93e2a2d11cccbe75e43ed87fb1da01` |
| `SYSTEM_EXT/bin/sos-core-host` | 84,808 | `826200b26b2a97927ea7f22cd7ede225183347928d81635b6dd0b06443109995` |
| `SYSTEM_EXT/lib64/libsos_core_experience.so` | 14,526,296 | `0a1acf64edf7a9b1ff7101699249bef19dea9bf28d3837245f45a49585af328c` |
| `SYSTEM_EXT/priv-app/SosFrameworkBridge/SosFrameworkBridge.apk` | 20,894 | `4ebe01c795fedfe0c1513d8d1ae3a42887a99d0c06f03f72a3f6d7eff81ccd7b` |
| `SYSTEM/framework/services.jar` | 22,166,311 | `be860e74b0db32e0cc91689713c124fcfd9c1109b361959503caf331eebcfac5` |

The exact OTA was installed with `adb reboot sideload-auto-reboot`,
`adb wait-for-sideload`, and `adb sideload`; Recovery reported
`Total xfer: 1.00x`. The no-wipe boot reached the exact revision with profile
`compat`, core stage `compat`, Compat stage `1`, CE available, and SELinux
enforcing. `dev.sos.experience` resolved only to
`/system_ext/priv-app/SosShell/SosShell.apk`; there was no `/data/app` update and
`pm path com.android.systemui` returned no package.

**Exact-image device results:** The following checks were performed on the
flashed revision, not on the rapid proof APK:

- HOME was the focused 1080x2400 Activity with `isKeyguardShowing=false` and a
  complete SOS control frame. Window and SurfaceFlinger audits contained no
  SystemUI, status/navigation bar, notification shade, PermissionController,
  keyguard, or fixed-recovery surface.
- APPS produced a full-frame fixed SOS workspace containing only `Calculator`,
  not the legacy fixture or a raw package identifier. At 100 ms the destination
  was complete and chrome intentionally absent; at 350 ms the entire chrome
  appeared in one frame.
- ATTN produced a full-frame fixed SOS attention surface. Platform facts were
  labeled `SOS RUNTIME` and application crashes `Calculator`; neither
  `android` nor `com.android.*` appeared as visible identity. Its 100/350 ms
  handoff repeated the atomic result.
- Calculator launched as the selected non-system app. At 350 ms the app filled
  the frame with chrome intentionally absent; at 850 ms all four SOS controls
  were complete. An in-bounds SOS BACK press returned to the workspace.
- Explicit launch of the target-SDK-19 fixture returned Activity error 102.
  The framework logged `SOS Compat blocked redirected Android system Activity`
  for `ReviewPermissionsActivity`; workspace retained focus and no
  PermissionController surface existed.
- With Calculator foreground, `adb shell am crash dev.sos.experience` removed
  HOME. The native host logged `native_compat_home_failed action=restart-home`,
  the bridge and host both logged an accepted HOME request, and HOME was focused
  again after 14.216 seconds. There was no
  `native_compat_home_restart_failed` and no fixed-recovery surface.
- After uninstalling `com.android.calculator2` and
  `com.android.multidexlegacytestapp` and deleting the exact staged
  `/data/local/tmp/sos-test.apk`, screen-off/wake rendered the direct native
  `SOS LOCK / PRESS ENTER` frame. Logs show exclusive `sec_touchscreen` and
  `gpio_keys`, `trusted_lock_ready`, and a one-shot bridge status of
  `credential_type=-1 unlocked=true`. Window policy says
  `deviceHasKeyguard=false`; Android keyguard remains false and SurfaceFlinger
  contains `SOS Trusted Lock`, not SystemUI or a shade.
- The owner then performed repeated physical side-button lock/wake and native
  ENTER cycles and explicitly confirmed that unlock works. The host recorded
  eight `native_runtime_unlock_complete credential=none` events. The final
  activity/focus was `SosCompatWorkspaceActivity`, `SOS Trusted Lock` was absent
  from SurfaceFlinger, and the empty `pm list packages -3` result explains the
  correct `NO COMPATIBLE APPLICATIONS` workspace state after fixture cleanup.

Representative commands were:

```text
./tools/a33xctl build-compat1
./tools/a33xctl inspect-compat1
adb reboot sideload-auto-reboot
adb wait-for-sideload
adb sideload lineage-23.0-20260816-UNOFFICIAL-sos_compat_a33x.zip
adb shell dumpsys activity activities
adb shell dumpsys window windows
adb shell dumpsys window policy
adb shell dumpsys SurfaceFlinger --list
adb shell am start -W -n com.android.multidexlegacytestapp/.MainActivity
adb shell am crash dev.sos.experience
adb shell input keyevent KEYCODE_SLEEP
adb shell input keyevent KEYCODE_WAKEUP
```

Raw generated evidence remains outside Git in
`/home/carlid/sos-samsung-work/lineage-a33x/evidence-20260816-native-compat-fixes/`:

| Evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| `55-final-220e-home.png` | 193,106 | `761f762d605d397037f335d9ee9a6fdea83d16862f17df436050b32a7334aba3` |
| `55-final-220e-home-activities.txt` | 14,643 | `86e638b9beb69c30f42f29a9467d2bf93ac148399629d7278ac2228ed3a2e6de` |
| `55-final-220e-home-windows.txt` | 6,235 | `3b2cac587ef0a2d77af77b36ee4f78c331fc772e3044f35c720317c359b0a48f` |
| `55-final-220e-home-surfaces.txt` | 3,309 | `f79bf4c74f8bee4a675605f6ace5106c96a63b2363f11b784e43b5bc45deca90` |
| `56-final-220e-workspace-100ms.png` | 39,472 | `84de919efd19dbec6699e8cc5562ba21ae24d45cbf28d2113aa65baabd7f21da` |
| `56-final-220e-workspace-350ms.png` | 55,832 | `25ee9324dd35d0f670bed281145eb76a14f0716652d7354291e6fc01eed87565` |
| `57-final-220e-attention-100ms.png` | 128,539 | `30b6abf0693fbd81abbc9203df75bbcb23eab1496f3798a47b8131c2470b6925` |
| `57-final-220e-attention-350ms.png` | 144,612 | `ce96c0b2186df002343fcb1484fbd04045ed207f70b1b1edcfa60d480b56d5b9` |
| `58-final-220e-calculator-350ms.png` | 104,349 | `89440da4fd366a98840072c71b5f76eb2366eed1ef8a2da8e8239dce5ab228aa` |
| `58-final-220e-calculator-850ms.png` | 111,965 | `281bbff2b1bb4aaa52967b29751d45c9b4ffc6a459dda4cf0267b53948877e8a` |
| `59-final-220e-legacy-start.txt` | 125 | `574408882be0ec65cddc37af3658a21df0d6cd7011e67f573fa1ca77eea6ab93` |
| `59-final-220e-legacy-logcat.txt` | 8,779 | `11f78cdde412e9b08de27a90cc5d48c09ae17d46ca1186b23600c4896558aba4` |
| `60-final-220e-crash-recovery.png` | 193,437 | `a0ae1d6eff0c325e3a1a20077cee568dcd2729e610b7f083f4d719df71e6f412` |
| `60-final-220e-crash-recovery-logcat.txt` | 138,140 | `439494662b3aac842a47e9e3932a7fbdaf395c38b810090354b8e93b27b8ba90` |
| `61-final-220e-native-lock.png` | 16,499 | `da5ff01426dc98a02ce6d15e0b3cfadcbff1b159ff62f724257b5c60e740df0c` |
| `61-final-220e-native-lock-windows.txt` | 6,262 | `1188e1ed994469bc88e46ef4fb27983955f5d9b4102747a7e7527e8ef5793285` |
| `61-final-220e-native-lock-surfaces.txt` | 3,505 | `759b07ef4835ed6af7a30f299f2f57290a323b6ee5be3f8085416c50b3180b8c` |
| `61-final-220e-native-lock-logcat.txt` | 75,822 | `6429ad0b3bb9818a26a352d86fd9dfaff09f15afce7fae8460ad3eb225c9d84b` |
| `62-final-220e-physical-unlock-workspace.png` | 55,975 | `b4029b8588077737b527762c90f49d5e971dab50355dbc773802d9fbe63b5ccd` |
| `62-final-220e-physical-unlock-logcat.txt` | 2,740,973 | `70ac795ae3a40afe404acb2132b72c41215d89b9c1b543ea1c975a5d4f2687ed` |
| `62-final-220e-physical-unlock-activities.txt` | 19,753 | `9a9f53d2e6a61202eb034d73fd1c1830f34eb8ef6aadc13f7b5ba025f353ca1d` |
| `62-final-220e-physical-unlock-surfaces.txt` | 3,863 | `53bf9e329952bb307ddc5a45bfccff23c3141aa4ad9063649c379ba006fcf98c` |
| `62-final-220e-third-party-packages.txt` | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

Final source verification passed `cargo fmt --all -- --check`, product-flagged
`:app:compileReleaseJavaWithJavac`, the pinned AOSP `clang-format --dry-run
--Werror` over `core/host.cpp`, `bash -n tools/a33xctl`, and
`git diff --check`. Applying the tracked framework patch in reverse with
`--check` also passed against the staged Lineage `frameworks/base` checkout,
proving that the patch and built framework source match.

**Decision / remaining risks / next gate:** Accept revision
`sos.compat1.19d8a653fbd7.220e268c228f` as the native-Compat exact-image
rollback artifact and as passing the automated hardware presentation,
application membrane, transition, restart, side-button lock/wake, and physical
touchscreen ENTER gates. The handset has credential type `NONE`;
PIN/Gatekeeper throttling, fingerprint, authentication-bound Keystore, the
Volume Up+Down Recovery chord, emergency calling, ANR, chooser/IME, call/alarm,
thermal, accessibility, and data-containment gates remain separate work. The
phone was left on the exact revision in the SOS application workspace, CE
available and SELinux enforcing, with the temporary app update, both test apps,
and staged test APK removed.

## 2026-08-16 — User-facing documentation audit after the native-ownership campaign

**Goal / hypothesis:** Reconcile the public project entry points with the
implemented and measured system after the AOSP, Linux, resident-agent, Samsung
a33x, and six-profile native-ownership work. The root README had last changed at
`b6da036` and still described Milestone 0 as the current physical status and
Milestone 1 as the current architecture. The vision document likewise called
the Android APK laboratory current and described Core 0 as a future claim.

**Documentation changes:** `README.md` now leads with the current research
status, separates the APK regression harness, Linux, Cuttlefish, and physical
a33x evidence, explains the Compat/Core profile matrix, provides verified
developer entry points for each track, links the current evidence reports, and
states the principal security and hardware limitations. It also adds a
prominent physical-flash warning and avoids presenting any build command as
authorization to unlock or flash a device.

`docs/vision.md` now marks the Android laboratory as passed historical work,
identifies privileged shell/service migration as the current phase, and records
the actual Shadow, Core 0A, Core 0B, and intentionally locked Core 1 boundaries.
The original exit criteria remain intact as historical regression criteria.
The decision summary in `docs/samsung-sm-a336b.md` now identifies accepted
Compat revision `sos.compat1.19d8a653fbd7.220e268c228f`, the complete staged
physical campaign, the native Pi/live-revision result, and the gates that remain
open.

**Evidence and checks:** `./tools/sosctl` and `./tools/a33xctl` help output were
compared directly with every README command and profile. A repository-local
shell check extracted every Markdown target from `README.md` and `docs/*.md`,
resolved each non-HTTP path relative to its source file, and found no missing
target. A second extraction verified that every `./tools/...` path named in the
README exists. `git diff --check` passed. No build, VM, device, flash, runtime,
latency, or hardware test was run because this change makes no implementation
or evidence claim beyond the already recorded accepted results.

**Failures / fixes / rejected approaches:** The old README was not extended as
another chronological status list: that structure made the earliest milestones
look current and buried the runnable paths. It was reorganized around current
status, architecture, selectable development tracks, repository orientation,
and limitations. Detailed artifact hashes and campaign transcripts remain in
their focused reports rather than being duplicated into the entry point. The
first combined patch attempt did not apply because a wrapped `docs/vision.md`
context line differed from the patch; it made no partial change. The update was
then applied as exact per-file patches and the resulting diff was rechecked.

**Decision / remaining risks / next gate:** Treat the README as the public
status and navigation surface, `docs/vision.md` as the architectural north
star, and the focused gate reports plus this ledger as evidence. This audit
does not close the open PIN/Gatekeeper, fingerprint, authentication-bound
Keystore, Recovery chord, urgent-attention, IME, accessibility, data
containment, audio/call, thermal, soak, native CE-unlock, or displaced-service
gates. Future material architecture or hardware work must update both its
focused report and the README when it changes the public status matrix.

## 2026-08-16 — System Providers v1 and Stock Base v0 implementation slice

**Goal / hypothesis:** Replace the Android system authority's
`providers_fake::snapshot()` HOME payload before extending the product UI. The
first vertical slice should prove one canonical, capability-controlled ABI for
live clock, power/thermal, connectivity/Wi-Fi, audio/media, compatible
applications, and attention facts; the product stock Luau source and generated
Luau revisions must receive the same value. Android package components,
notification keys, Binder objects, Intents, credentials, and trusted ceremony
implementation must remain outside Luau.

**Code and architecture changes:** `experience-ir` now defines System Providers
ABI 1 at `model.providers`, including closed fact, attention-kind, thermal, and
capability types. The init-owned Android authority now owns a
`SystemProviderRegistry`: native adapters read wall clock and available bounded
sysfs link/power/thermal facts, while a peer-credential-checked extension of the
existing direct-boot framework bridge supplies locale/time-zone labels,
framework battery/thermal state, validated connectivity and saved Wi-Fi,
audio/media, compatible applications, and active attention. The authority
crate no longer depends on `providers-fake`; its state transaction helper is
now authority-local. Legacy fixture and APK-laboratory providers remain
available outside the Android system-product path.

The framework bridge exposes only visible labels, scalar facts, and SHA-256
derived opaque IDs. Package/Activity components, Wi-Fi configuration IDs, and
notification keys are resolved again inside the bridge immediately before an
action. Rust converts every Luau effect to a closed `SystemAction`, enforces
payload and opaque-ID bounds, requires a matching capability from a fresh
bridge snapshot, and intersects bridge grants with a fixed authority allowlist.
Volume/mute, media, saved-Wi-Fi, application-launch, and attention-acknowledge
actions are implemented. Typed lock/restart/shutdown requests exist at the
boundary but cannot be granted until a fixed native confirmation ceremony is
implemented. SELinux adds only read access for `sysfs_net`/`sysfs_thermal` and
an authority-to-`system_app` local-socket connection; battery health stays
behind the framework/health boundary.

Android HOME now polls the system authority for the full canonical model and
preserves only its separate resident-agent conversation across refreshes. The
non-system APK laboratory retains its prior local Wi-Fi adapter. Stock Base v0
in `experiences/default.luau` renders a live status plane, capability-aware
quick controls, compatible-application workspace, attention center, and
Luau-authored resident-agent composer. It contains no Rome weather, seeded
calendar/notes, Tycho media, or fixed prototype date. The tracked stock source
is 15,164 bytes with SHA-256
`ba77495fec9b6bcefa69243922002e6525c34dc02ca83f28a160edfe69227aca`.
The product already stages that source as AVB/OTA-protected
`/system_ext/etc/sos/default.luau`.

The authority now installs and pins that immutable product source independently
of its mutable current pointer. Revision responses identify the trusted stock
revision. If a generated revision fails runtime validation during startup, the
host submits the exact revision it booted; the authority rejects stale or stock
self-fallback requests, journals a coordinated state/pointer transition,
restores empty stock state, and relies on the fixed supervisor for the clean
restart. Failure of stock escalates to fixed Recovery instead of looping. The
detailed ABI, ownership, and gate are recorded in
[`system-providers-v1.md`](system-providers-v1.md), with the agent-facing shape
in [`experience-api.md`](experience-api.md).

**Commands and evidence:** The focused Rust suite

```text
cargo test --locked -p android-system-authority -p android-authority-protocol \
  -p experience-ir -p providers-fake -p runtime-luau
```

passed 8 authority tests, 4 experience-IR tests, 5 fixture-provider tests, and
22 Luau runtime tests. These cover native fixture sysfs collection, framework
merge/version rejection behavior, absence of package/Activity fields, typed
payload bounds, missing-capability rejection, non-seeded authority snapshots,
and coordinated generated-to-stock fallback. `cargo check --locked -p
sos-experience --tests` passed the stock host test code. The standalone Luau
validator reported `source_bytes=15164`, `nodes=50`, `inputs=1`, and
`semantics=2` for the stock source.

Both Android/Bionic checks passed:

```text
cargo ndk -t arm64-v8a -P 31 check -p android-system-authority --locked
cargo ndk --link-libcxx-shared -t arm64-v8a -P 31 check \
  -p sos-experience --locked --no-default-features --features core-native
```

The complete framework-bridge Java source compiled with `javac -source 8
-target 8` against the Android 34 SDK and the built Lineage
`framework-minus-apex-headers` jar. `xmllint --noout` passed the bridge manifest
and privileged-permission allowlist. `cargo fmt --all`, `git diff --check`, and
the tracked source size/SHA-256 checks passed.

**Failures, fixes, and rejected evidence:** The first Android authority check
found that Bionic's `c_char` is unsigned, while the local clock/property buffers
had been declared as `i8`; changing both buffers to `libc::c_char` made desktop
and Android builds agree. A full desktop `cargo test -p sos-experience --lib`
compiled the test target but could not link because this workstation lacks
`libxkbcommon` and `libxkbcommon-x11`; it was not counted as a pass. The
link-free test-target check and standalone Luau runtime validator passed
instead. Framework compilation reports deprecation notes for the Android saved
network APIs. Those APIs were deliberately bounded to already configured
networks and a privileged system bridge, but their actual Android 16 behavior
remains a hardware/integration risk rather than a closed gate. No generated
APK, OTA, revision directory, screenshot, or raw hardware artifact was used as
evidence in this desktop implementation slice.

**Decision / remaining risks / next gate:** Accept the provider ABI, authority
registry, opaque framework membrane, typed action boundary, stock source, and
transactional fallback as the implementation baseline. Do not mark the
milestone or any hardware/latency/security gate complete from these desktop and
cross-compilation checks. The next gate is an exact-image a33x build and device
campaign covering real values, SELinux denials, authority/bridge restart,
generated-revision failure and stock recovery, volume/mute/media, saved-Wi-Fi,
application handoff/return, attention acknowledgement, screen-off/wake,
thermal behavior, and refresh soak. Display/rotation, session/power facts,
trusted power confirmation, calls, alarms, urgent call UI, notification actions,
personal data, credential variants, and Recovery input remain later work.

## 2026-08-16 — System Providers v1 + Stock Base v0 exact-image A33x gate

**Goal / hypothesis:** Prove that the first system-provider vertical slice
replaces the seeded Android HOME model on physical hardware without widening
the Android/Luau trust boundary. The exact no-wipe Compat 1 image had to boot a
signed stock revision, report live clock, power, network, audio, application,
and attention state, enforce typed action capabilities, recover a failed
generated revision to stock, retain fixed native lock ownership, and remain
stable under repeated refresh.

**Code, device, and image changes:** Testing used the USB-connected Samsung
SM-A336B `RFCT50EGFCN`, battery 99% and USB powered, with the accepted FYH2
vendor base. The first complete provider image was revision
`sos.compat1.a3f3bae010bf.cfb7f6732eb5`. It exposed two hardware-only defects:
the framework bridge returned the same saved Wi-Fi network/opaque ID twice,
and `sos_authority` generated an enforcing SELinux denial approximately every
two seconds while probing Samsung's vendor-private `sysfs_battery` type. That
image was not accepted.

`SosSystemProviders.wifiNetworks()` now de-duplicates stable opaque IDs. The
Android native snapshot no longer reads battery sysfs; battery, charging,
temperature, and thermal state come through the typed framework/health bridge,
while desktop/native fixture tests retain their bounded sysfs adapter. A first
attempt to add `allow sos_authority sysfs_battery` was rejected by the build
because the vendor-private type is not visible to system-ext policy. No image
was packaged or installed from that attempt. The final policy retains only
public `sysfs_net`/`sysfs_thermal` reads and documents the vendor health
boundary. `a33xctl` inspection now also requires the notification-listener
manifest permission/service, typed snapshot/action markers, and absence of a
packaged `sysfs_battery` allow.

The corrected exact revision is
`sos.compat1.a3f3bae010bf.b093c3a0b50a`. `./tools/a33xctl build-compat1`
completed in 3:25 and `./tools/a33xctl inspect-compat1` passed whole-package
signature, ZIP integrity, PIT/AVB/VINTF, target-files/source/package equality,
provider bridge, manifest, and SELinux policy gates. Final artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `lineage-23.0-20260816-UNOFFICIAL-sos_compat_a33x.zip` | 1,042,724,759 | `13cc2afb209dc8bdfe36c7b14138f81acb0ded4687cfffbae7c666d22e69097d` |
| `SYSTEM_EXT/bin/sos-authority` | 1,328,760 | `39f6f4cb581d2e3c50bfa3f213a2d055ebc3c09892be8994ffb205af8a5863d0` |
| `SYSTEM_EXT/priv-app/SosFrameworkBridge/SosFrameworkBridge.apk` | 37,278 | `5ffa80944cb08bcb481592f0a073f0b4fd322ae1ade2aaaa95914264f3604e7f` |
| `SYSTEM_EXT/etc/sos/default.luau` | 15,164 | `ba77495fec9b6bcefa69243922002e6525c34dc02ca83f28a160edfe69227aca` |

For rejected-artifact traceability, the first installed OTA was 1,042,732,400
bytes with SHA-256
`be57e207fa502927681e63499279e067d3f99dceaa751c1282a860d1ef8da111`
and revision `sos.compat1.a3f3bae010bf.cfb7f6732eb5`. Both completed sideload
operations used `adb reboot sideload-auto-reboot`, `adb wait-for-sideload`, and
`adb sideload`; Recovery reported `Total xfer: 1.00x`. Neither operation wiped
`/data`. The corrected image reached `sys.boot_completed=1` at the exact
incremental revision with profile `compat`, stage `1`, and SELinux enforcing.

**Physical commands and measurements:** Direct provider requests used Android
loopback ports 47777/47778 through USB ADB. The corrected snapshot reported ABI
1, locale `en-US`, timezone `Europe/Zurich`, `8:34 PM` / `August 16, 2026`, 99%
USB charging, 33.3--33.6 C battery temperature, thermal status `none`, the
validated saved Wi-Fi connection at signal level 4 on `wlan0`, and 32% unmuted
media volume. The live SSID was compared exactly but is redacted from tracked
documentation. `dumpsys battery`, `dumpsys wifi`, and `dumpsys audio` matched
those values; Android stream volume 8/25 matched 32%. The Wi-Fi list contained
one item and one unique opaque ID. Legacy fields derived from the same live
model reported `August 16, 2026`, `Weather unavailable`, and an empty track;
`Saturday, 8 August`, `Clear over Rome`, `Tycho`, and `synthetic0` were absent.
A recursive ABI audit found no Binder, Intent, credential, BSSID/MAC,
package-name, or component-name key.

The authority current response named identical current and pinned stock IDs,
`31f8e1d31b6e2c91a8a0b0829e5f29934440c64ed8f535bb86d81a5a836c49e5`,
with `stock_trusted=true` and the packaged stock source hash. A 1080x2400
capture showed Stock Base v0's live status plane, quick controls, empty
compatible-application workspace, and attention center. The raw capture
`/tmp/sos-stock-v1.png` is outside Git, 203,148 bytes, SHA-256
`b0ae1d8414b7907c74d259b64246998cf74184abbd5b868f50174cbc4d39db5e`.

Typed action evidence was reversible and compared against fresh snapshots:

- `audio.set_volume(52)` was accepted and produced Android index 13/25;
  mute/unmute was accepted; volume and mute were restored to 32%/false.
  Percent 101 was rejected before the bridge.
- `network.disconnect` changed the snapshot to disconnected/unvalidated while
  leaving Wi-Fi enabled and the saved opaque selection present. Reconnect by
  that opaque ID restored the same validated SSID and `wlan0`.
- Acknowledging the non-urgent `Discover Trust` attention ID was accepted and
  removed it from the next snapshot. Android re-posted that system notification
  after the later reboot, which is correct source ownership rather than stored
  Luau state.
- With no active media session or compatible third-party app, the bridge
  withheld `media_*` and `app_launch`; attempted play/pause and launch were
  rejected by the authority. `power.request_restart` was likewise rejected
  because trusted power confirmation is intentionally not grantable.

HOME process replacement changed PID 1461 to 2586 in 1,191 ms, refocused
`SosHomeActivity`, and loaded pinned stock plus live providers. For the stronger
fallback gate, the authority installed and activated deliberately invalid
generated revision
`29108c4bed03cc408b1cffbd121e41f8aa2f1cdbdcb485fe2bab167756c36c53`.
On HOME restart the Luau parser rejected it; the runtime logged
`stock_fallback=true`, the authority advanced state revision 586 to 587 with
empty stock state/source hash, the bad process aborted, and replacement PID
2803 rendered stock. Current and pinned stock IDs matched again 1,248 ms after
the test began.

Screen-off reached `mWakefulness=Asleep`. The bridge requested native lock, the
host logged `trusted_lock_ready`, and the bridge acknowledged
`reason=screen-off` before transition completion. Wake exposed only
`SOS Trusted Lock`, with no SystemUI/status/navigation/notification layer. The
raw-input screen capture `/tmp/sos-provider-lock.png` is outside Git, 16,499
bytes, SHA-256
`da5ff01426dc98a02ce6d15e0b3cfadcbff1b159ff62f724257b5c60e740df0c`.
Because the fixed layer intentionally rejects Android-injected input, this run
did not repeat the already accepted physical ENTER gate. A subsequent `adb
reboot` returned to the same exact revision and focused stock HOME; stock state
revision 587, Wi-Fi, audio, attention, and unique-ID invariants persisted.

The final smoke soak issued 60 snapshots at two-second intervals:

```text
duration_ms=124729 samples=60 failures=0 max_request_ms=107
pid_stability authority=941/941 host=924/924 bridge=1372/1372 home=1391/1391
soak_error_audit matches=0
```

Every sample required ABI 1, unique Wi-Fi IDs, and absence of all demo values.
The post-soak all-buffer log audit found no `sos_authority` AVC, framework
provider failure, HOME provider-poll failure, SOS crash/ANR, or failed HOME
restart. One `Try again (os error 11)` HOME poll warning appeared once during
the initial post-OTA startup before this clean reboot/soak; it did not recur.

The final repository recheck repeated the focused Rust matrix (8 authority, 4
experience-IR, 5 fixture-provider, and 22 Luau tests), `cargo check --locked -p
sos-experience --tests`, and the standalone stock validator
(`source_bytes=15164`, `nodes=50`, `inputs=1`, `semantics=2`). `cargo fmt --all
-- --check`, `git diff --check`, `bash -n tools/a33xctl`, both XML parses, and
the complete framework-bridge `javac -source 8 -target 8` compile passed. The
first standalone Java invocation put the public Android SDK jar before the
Lineage framework header jar and therefore hid five internal `UserHandle`
symbols; reversing that test-only classpath order compiled all sources with
only the already documented deprecation/Java-8 warnings. Temporary javac output
directories were created under `/tmp`, outside Git.
Both cached ARM64/Bionic checks also passed. The authority check reports three
expected dead-code warnings because its desktop sysfs power/thermal fixture
helpers are compiled but intentionally unused on Android after moving battery
facts behind the health bridge; the experience check reports only the existing
future-incompatibility notice for `proc-macro-error2`.

**Failures, fixes, decision, and next gate:** Reject the duplicated-network and
battery-sysfs image and reject a cross-partition SELinux allow as the wrong
architectural fix. Accept corrected revision
`sos.compat1.a3f3bae010bf.b093c3a0b50a` as the first physical-hardware pass for
clock, normal power/thermal observation, Wi-Fi, audio volume/mute, empty app
inventory, attention acknowledgement, signed stock rendering/fallback, native
lock handoff, reboot persistence, and short refresh stability. This does not
close successful media control or application launch/return because the device
had no eligible resources, a long-duration soak, thermal-load response,
bridge/authority forced-restart recovery, physical lock input, or any latency
acceptance gate. Display/rotation, session/power facts, trusted restart/shutdown
confirmation, calls, alarms, urgent attention, personal data, credential
variants, and Recovery remain later milestones.
The README status now names this focused pass without replacing the earlier
Compat revision that still owns the broader application and physical-input
acceptance evidence.

## 2026-08-16 — Retire Core 0A and freeze Core 0B behind Core 1

**Goal / hypothesis:** Reduce the supported Core matrix now that Core 1 exists.
Core 0A no longer provides unique implementation or migration evidence, while
Core 0B still provides a useful pre-unlock Android-hosted oracle until the
native Core 1 path owns equivalent unlock, service, and Recovery behavior. The
intended product state is one active Core target, one explicitly frozen legacy
target, and no buildable Core 0A target.

**Code, product, and documentation changes:** Removed
`lineage_sos_core0a_a33x.mk` from the repository and from
`AndroidProducts.mk`, removed its post-credential-encryption init trigger, and
deleted the `build-core0a` / `inspect-core0a` command paths. Core 1 remains the
only active Core product and now declares `ro.sos.lifecycle=active`. Core 0B is
retained as a frozen migration oracle with `ro.sos.lifecycle=legacy`; its build
command fails closed unless the operator explicitly sets
`SOS_ENABLE_LEGACY_CORE0B_BUILD=1`. Inspection remains available for existing
legacy artifacts. The product split, UI-ownership campaign, device notes,
vision, README, init comments, and command help now distinguish historical
stage evidence from supported targets. Historical Core 0A measurements and
artifact identities remain in documentation rather than an indefinitely
buildable product definition.

Staging the revised device overlay into the external Lineage checkout used the
existing `rsync --delete` path and removed the retired Core 0A makefile there as
well. Switching the existing output tree from Compat 1 to Core 1 caused the
expected automatic install-clean, removing APK/bridge outputs that Core 1 must
not contain.

**Commands, build evidence, and artifact identity:** `bash -n tools/a33xctl`
passed. `./tools/a33xctl build-core0a` now terminates as an unknown command, and
an ordinary `./tools/a33xctl build-core0b` terminates with the legacy opt-in
instruction. A source audit confirmed that `AndroidProducts.mk` exposes Core
0B and Core 1 but no Core 0A product, and the Core 0A product file is absent
from both the repository and staged Lineage device tree.

The exact active target build

```text
./tools/a33xctl build-core1
```

completed successfully in 5:27 at revision
`sos.core1.c0d13c5a8169.68abc2e6bd71`. Soong/Make accepted the reduced product
graph and packaged
`lineage-23.0-20260816-UNOFFICIAL-sos_core1_a33x.zip`, 1,022,034,187 bytes,
SHA-256
`351f996142c3e88a2fa529f4a7819f57316df7dabc60b30d7679eea13b7412f2`.
This generated OTA remains outside Git.

`./tools/a33xctl inspect-core1` passed whole-package signature and compressed
data checks, PIT ceilings, AVB verification for every packaged partition,
VINTF compatibility, Recovery device-init presence, Core 1 target-files
composition, ARM64 host/runtime/authority identities, no Android UI or zygote
path, pre-unlock init ownership, provider authority/stock source and SELinux
contracts, and these exact product properties:

```text
ro.sos.core.stage=1
ro.sos.ui_owner=native-sos-no-zygote
ro.sos.lifecycle=active
ro.sos.profile=core
ro.sos.core.autostart=preunlock
ro.sos.block_android_activities=true
```

The inspected target-files contained an 84,808-byte `sos-core-host` with
SHA-256
`826200b26b2a97927ea7f22cd7ede225183347928d81635b6dd0b06443109995`,
the 14,597,576-byte Core experience runtime with SHA-256
`50d8baf7f5a09cbca4662ab424a6db901cc77963e9c2876d6a38816716ec38b1`,
and the 1,328,760-byte system authority with SHA-256
`39f6f4cb581d2e3c50bfa3f213a2d055ebc3c09892be8994ffb205af8a5863d0`.

The final repository regression repeated `cargo fmt --all -- --check`,
`git diff --check`, and `bash -n tools/a33xctl`. The focused provider/runtime
suite passed 8 system-authority tests, 4 experience-IR tests, 5 fixture-provider
tests, and 22 Luau runtime tests:

```text
cargo test --locked -p android-system-authority \
  -p android-authority-protocol -p experience-ir \
  -p providers-fake -p runtime-luau
```

The zero-result active-reference audit covered `build-core0a`,
`inspect-core0a`, and `lineage_sos_core0a_a33x`; lifecycle-marker inspection
found only the intentional Core 0B legacy opt-in and Core 0B/Core 1
`legacy`/`active` product properties.

**Failures, decision, and next gate:** There was no build or inspection
failure. The OTA tooling emitted its existing non-fatal property-read,
unprivileged device-node extraction, and deprecated OpenSSL warnings; the
build still completed, VINTF reported `COMPATIBLE`, and the inspector verified
the final package. No OTA was flashed and no physical-device test was run for
this retirement change. The connected SM-A336B remains on the previously
accepted Compat 1 revision
`sos.compat1.a3f3bae010bf.b093c3a0b50a`, so this entry does not close a Core 1
hardware, unlock, service, latency, or Recovery gate.

Accept Core 0A retirement, Core 0B's frozen opt-in status, and Core 1 as the
sole active Core target. Delete Core 0B only after Core 1 demonstrates on
hardware: native synthetic/FBE unlock and credential handoff; equivalent
clock/power/network/audio/application/attention behavior without the Android
host; calls, alarms, session/update, and Recovery transitions needed for
migration debugging; and a sustained boot/wake/lock/restart soak with no need
to compare against the Android-hosted pre-unlock oracle. Until then, Core 0B
receives no features or release-support promise—only fixes necessary to keep
that bounded migration oracle usable.

## 2026-08-16 — Core 1 native System Providers v1 parity

**Goal / hypothesis:** Give the active no-Zygote Core 1 product the same typed
System Providers ABI v1 and stock-experience contract as Compat 1 without
recreating `system_server` or exposing platform handles to Luau. The first
slice is clock, power/thermal, network/Wi-Fi, audio/media, signed application
inventory, attention, and their resource-scoped actions. "Parity" means ABI,
product, and build parity in this entry; physical provider behavior requires a
separate A33 acceptance run.

**Code, product, and environment changes:** Generalized the authority's
framework-only bridge into a `ProviderAdapter` selected by the immutable
`ro.sos.providers` product property. Compat 1 and frozen Core 0B retain the
headless Java adapter; Core 1 selects a new init-owned `sos-core-platform`
daemon over the same peer-credential-checked abstract Unix-socket ABI 1 JSON
boundary. The authority now applies one fixed platform-capability allowlist to
both published capabilities and action authorization, rejects adapter ABI
mismatches, and falls back to native clock, link, and public thermal facts with
no adapter actions when the daemon is absent.

The new ARM64 C++ daemon reads battery, charging, and temperature through the
stable Health AIDL v4 service; link facts through public network sysfs; saved
network inventory, RSSI, connect, and disconnect through stable Supplicant
AIDL v4; and music volume/mute through `AudioSystem` and audioserver. Media and
application actions use bounded fixed native datagram targets, while attention
uses a bounded durable journal and opaque normalized IDs. Capability emission
is resource-derived. The signed `/system_ext/etc/sos/core-apps.json` inventory
is initially empty because Core 1 has no APK runtime or registered native
applications; media and attention are likewise truthfully inactive/empty
until native owners and producers register. Calls, alarms, personal data,
display/session, and trusted power confirmation remain outside this slice.

Core 1 conditionally starts the existing vendor `wpa_supplicant` for the
`core-native` profile. Its dedicated `sos_core_platform` SELinux domain has
stable Health/Supplicant HAL client access, bounded audio service access,
read-only public network sysfs, and private platform state. Only the
system-UID authority may connect to its stream socket. Binder objects, Android
Intents, credentials, BSSID/MAC values, package internals, and raw platform
keys do not cross into the authority or Luau. Lock, credential, permission,
emergency, restart, shutdown, and Recovery remain fixed native surfaces and
cannot be granted by this adapter. Detailed ownership and acceptance limits
are in [`core1-provider-parity.md`](core1-provider-parity.md).

**Commands, tests, and measurements:** Formatting passed, and the focused
workspace command

```text
cargo test --locked -p android-system-authority \
  -p android-authority-protocol -p experience-ir \
  -p providers-fake -p runtime-luau
```

passed 10 system-authority, 4 experience-IR, 5 fixture-provider, and 22 Luau
tests (41 total), plus all selected doctests. The authority tests cover live
native fallback facts, complete adapter merge, typed payload bounds,
capability absence, ABI mismatch, and malicious privileged-capability
injection. `cargo ndk -t arm64-v8a -P 31 check -p
android-system-authority --locked` passed. `cargo fmt --all -- --check`,
`bash -n tools/a33xctl`, and `git diff --check` also passed before the exact
product gate.

The final clean command

```text
./tools/a33xctl build-core1
./tools/a33xctl inspect-core1
```

completed the exact product build in 3:04 and passed C++ compilation with
warnings as errors, complete SELinux neverallow policy, VINTF compatibility,
ZIP integrity and whole-package signature, PIT ceilings, the AVB chain for
every packaged partition, ARM64/ELF identity, Health v4 and Supplicant v4
needed libraries, audio/action markers, init/property selection, signed app
manifest parsing, authority connection policy, and absence of direct vendor
supplicant-socket or battery-sysfs access. The raw generated OTA remains
outside Git:

| Artifact | Revision | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| `lineage-23.0-20260816-UNOFFICIAL-sos_core1_a33x.zip` | `sos.core1.f4d780007972.812bca990cc5` | 1,022,102,986 | `3216038f337c44bb39f114485765db665908a3700325bac755021f3f251b2d25` |
| `SYSTEM_EXT/bin/sos-core-platform` | same | 118,816 | `481c43e971b518fc8a93cc1272b188916427d2e3078ec1c24cd72fc86ae067e1` |
| `SYSTEM_EXT/bin/sos-authority` | same | 1,332,000 | `7da9c058d7e7fbee12cd7f7144042ebd2c3713af8ea865d4accff328cfe9e3d1` |
| `SYSTEM_EXT/etc/init/sos-core-platform.rc` | same | 353 | `5b3563f02b16bdc8d0457dd4021a15c9248df9a4b17de4b719f0fd2234fe9790` |
| `SYSTEM_EXT/etc/sos/core-apps.json` | same | 33 | `90a2246ac8d3369de1cd93eb04ad4733799a689e31fc8dada25785998cbef731` |

**Failures and fixes:** A new security regression test initially exposed that
an adapter could inject `power.request_restart` into its advertised
capabilities and pass action authorization. Filtering snapshot publication was
insufficient; applying the fixed allowlist again at authorization closed the
confused-deputy path, and the full suite then passed. The first exact product
attempt, revision `sos.core1.f4d780007972.fce3ffd8989f`, tried to use the
vendor-private supplicant control socket. System-ext policy could not see the
vendor implementation domain, and a platform neverallow correctly prohibited
the direct datagram. That design was rejected rather than weakened; the daemon
now uses the stable Supplicant AIDL service.

Two early inspector checks also failed despite correct packaging: stripped ELF
binaries retain needed-library names rather than full AIDL descriptor strings,
and a broad battery-policy search matched the unrelated public
`sysfs_batteryinfo` attribute. The checks now inspect the v4 NDK needed
libraries and an anchored actual-allow rule. A later build and inspection at
revision `sos.core1.f4d780007972.c911d5a1874f` produced a 1,022,117,011-byte
OTA with SHA-256
`5c823db1a627757729b1bee4f3e9adc41199d37ac2e96c28754f201460f2e986`,
but its wrapper exited after the successful AOSP build because the shell
inspector was edited concurrently while Bash was still lazily reading it. A
fresh syntax check and inspection passed; concurrent source edits were then
stopped and the clean final build above was repeated to obtain an unambiguous
zero exit.

**Decision, remaining risks, and next gate:** Accept Core 1 as having System
Providers v1 ABI and exact-product build parity with Compat 1 for this first
slice. Do not claim physical parity: no OTA was flashed, and the connected
SM-A336B remains on accepted Compat 1 revision
`sos.compat1.a3f3bae010bf.b093c3a0b50a`. A clean Core 1 boot may correctly
withhold saved-Wi-Fi actions because no native credential/provisioning owner
yet reconstructs framework Wi-Fi state; validated Internet reachability,
media/app owners, and attention producers are also still absent.

The next hardware gate must capture live daemon and authority snapshots;
compare Health, thermal, audio, link, and Supplicant facts with device truth;
provision/select/disconnect/reconnect a saved network without leaking its
credentials; exercise reversible volume/mute and all unavailable-action
failures; force daemon and authority restarts; force generated-revision failure
and stock fallback; verify lock/Recovery coexistence; and run a sustained
wake/restart/power/thermal soak. Core 0B remains the frozen opt-in migration
oracle until Core 1 also owns the outstanding unlock, displaced services,
calls/alarms, and Recovery transition evidence on hardware.

## 2026-08-17 — Core 1 on-device acceptance preflight

**Goal / hypothesis:** Run the physical-provider acceptance gate for the
supplied Core 1 artifact revision `sos.core1.f4d780007972.812bca990cc5`,
expected to be 1,022,102,986 bytes with SHA-256
`3216038f337c44bb39f114485765db665908a3700325bac755021f3f251b2d25`,
without substituting a different build.

**Environment and evidence:** Searches under `/home/carlid/dev` and
`/home/carlid/sos-samsung-work` did not find the supplied artifact. The only
nearby local OTA was
`/home/carlid/dev/lineage-a33x/out/target/product/a33x/lineage-23.0-20260816-UNOFFICIAL-sos_core1_a33x.zip`;
it is a different 1,021,744,265-byte package with SHA-256
`4ff51b0c6d963d165ca707e4c1dc9d341aa9efad4f2a792dc56e7c1d3e422ae5`.

The connected SM-A336B (`RFCT50EGFCN`) reported fingerprint
`samsung/a33xnsxx/essi:15/AP3A.240905.015.A2/A336BXXSEFYH2:user/release-keys`,
revision `sos.core1.40c433d4fb63.081717db2c0b`, Core stage 1,
`core-native` providers, no Zygote, encrypted storage, verified boot orange,
and running native Core processes. `sys.boot_completed` was empty.
`./tools/a33xctl inspect-core1` passed its static signature, AVB, boot-chain,
ELF, property, and no-Zygote checks for the different local OTA. The device
state above was collected separately; `inspect-core1` did not validate the
current device's live provider values or provider actions. No reboot,
sideload, flash, or other device mutation was performed.

**Decision, remaining risk, and next gate:** Physical-provider acceptance for
the supplied artifact remains open; the static inspection of a different OTA
cannot close it. Make the exact artifact available and explicitly authorize
its installation, then run the live provider value/action, daemon and
authority restart, generated-revision failure/recovery coexistence, and
sustained wake/restart/power/thermal soak matrix documented in
[`core1-provider-parity.md`](core1-provider-parity.md).

## 2026-08-17 — Core 1 fresh build and interrupted install

**Goal / hypothesis:** Rebuild the provider-parity source into an exact,
locally available Core 1 OTA, install that signed artifact, and run the live
acceptance matrix only after confirming that the device booted its revision.

**Build and package evidence:** The source provider-parity markers were
present. `./tools/a33xctl build-core1` completed successfully in 05:08 and
produced the raw, untracked OTA below. `./tools/a33xctl inspect-core1` passed
signature and ZIP integrity, AVB, PIT ceilings, ELF identity, product
properties, SELinux policy, package contents, and provider-hash checks.

| Artifact | Revision | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| `/home/carlid/dev/lineage-a33x/out/target/product/a33x/lineage-23.0-20260817-UNOFFICIAL-sos_core1_a33x.zip` | `sos.core1.40c433d4fb63.1a338e2f0fb5` | 1,022,100,245 | `6262aa874877aae00b46f60882d90cd286a9893010c138ff0bac6669a2942f52` |

**Install attempt and failure:** The authorized sequence was:

```text
adb reboot sideload-auto-reboot
adb wait-for-sideload
adb sideload <exact OTA>
adb wait-for-device
```

The first sideload transport reached approximately 30% before its host wrapper
detached; a second attempt returned `Total xfer: 0.00x`. The device did not
re-enumerate as Android and instead enumerated over USB as Samsung `04e8:685d`
in Download mode, with adb unavailable. The booted revision therefore remains
unconfirmed and no live provider-value, action, restart, recovery-coexistence,
or soak test ran.

No wipe, factory reset, Odin, Heimdall, individual-partition flash, or
bootloader action was performed. `a33xctl` has no Download-mode recovery
recipe, so the user performed the repository-precedent physical Side + Volume
Down restart. The runner verified that no stale adb sideload wrapper remained.
After the user reported that the phone was still rebooting, the runner
completed two project-sanctioned 300-second `adb wait-for-device` windows (10
minutes total), with USB and adb checks between and after them. No Android,
Recovery/sideload, or Download transport appeared: the `lsusb`
Samsung/Android/`04e8` check and `adb devices -l` remained empty. The
controlled OTA retry was therefore not started, and no new device mutation
occurred.

For the next recovery attempt, the A33x-specific physical sequence was
confirmed and documented to the user: keep USB connected; hold Side + Volume
Down until the screen turns black; immediately release Volume Down while
keeping Side held and press Volume Up; enter Lineage Recovery; then select
Apply update -> Apply from ADB, without wiping. During two subsequent bounded
300-second monitoring windows, the device remained in Samsung Download mode
as USB `04e8:685d`; the expected Recovery transport `18d1:d001` and
ADB/sideload never appeared. The exact OTA retry was not started, and the host
issued no reboot, flash, wipe, or other device mutation.

The user subsequently reached Lineage Recovery's Apply from ADB state. The
runner positively identified the device on USB `18d1:d001` as:

```text
RFCT50EGFCN sideload product:a33xnsxx model:SM_A336B device:a33x
```

The runner reverified the exact OTA's SHA-256 as
`6262aa874877aae00b46f60882d90cd286a9893010c138ff0bac6669a2942f52`.
Exactly one `adb sideload` was issued; it exited 0 with `Total xfer: 1.00x`.
At the time, the user was instructed to return to the Recovery main menu and
choose Reboot system now, without wiping. Later user clarification supersedes
that runbook assumption: this Lineage Recovery flow automatically reboots
after a successful sideload, so manual **Reboot system now** is not required.

Across two bounded 300-second waits after the transfer, the phone remained on
Recovery USB `18d1:d001` with adb unauthorized (`transport_id 114`) and never
reached Android. Boot completion, active slot, revision, product properties,
provider values, and provider actions therefore remain unconfirmed. The host
issued no second transfer, reboot, wipe, or other device mutation.

Android subsequently booted the exact revision
`sos.core1.40c433d4fb63.1a338e2f0fb5`. The product identity checks passed:
`ro.sos.profile=core`, `ro.sos.providers=core-native`, and
`ro.zygote=no_zygote`; the authority and host processes, provider and revision
Unix sockets, and native Recovery UI were present. This boot observation does
not imply that a manual Recovery reboot selection was required. An empty
`sys.boot_completed` is expected in no-Zygote mode and is not a failure.

**Physical acceptance failure:** `init.svc.sos_core_platform` remained
`restarting` with repeated exit status 1. The earliest and repeated AVC denied
the `u:r:sos_core_platform:s0` source directory `{ search }` on the `sos` path
component labeled `u:object_r:sos_authority_data_file:s0`; no secondary AVC
class appeared. Source policy labels `/data/misc/sos(/.*)` as authority data
and `/data/misc/sos/platform(/.*)` as platform data but grants the daemon only
its own type, identifying missing parent traversal as the smallest justified
fix. Live `ls -lZ` was permission-denied, so runtime label contents were not
claimed. The provider snapshot failed, and no provider action mutated the
device. The remaining live provider/action, restoration, restart/fallback,
Recovery-coexistence, and soak matrix was not run. Detailed evidence and raw
artifact metadata are in
[`core1-provider-parity.md`](core1-provider-parity.md#2026-08-17-physical-device-acceptance-result-failed).

**Decision, remaining risk, and next gate:** The exact OTA installed and
booted, but do not claim that physical-provider acceptance passed: this
attempt failed at the earliest provider-daemon blocker. Make the smallest
SELinux parent-directory traversal fix without broadening the platform
daemon's authority, rebuild, pass static policy and neverallow checks, reflash,
and rerun the complete live matrix.

## 2026-08-17 — Core 1 provider parent-traversal fix

**Goal / hypothesis:** Fix the failed physical-provider gate at its earliest
blocker. The hypothesis is that `sos_core_platform` exits because it can access
its separately labeled `/data/misc/sos/platform` subtree but cannot search the
authority-labeled `/data/misc/sos` parent path component.

**Policy and regression change:** Added only
`allow sos_core_platform sos_authority_data_file:dir search;` to the A33x Core
platform domain policy. This grants parent traversal without `open`, `read`,
`write`, `getattr`, create, `add_name`, `remove_name`, or any file permission.
It does not relabel authority data, broaden authority/capability controls, or
change the Core 0B product. The Core 1 inspector now gathers every direct
compiled allow from `sos_core_platform` to `sos_authority_data_file` and
requires the complete result to equal exactly `(allow sos_core_platform
sos_authority_data_file (dir (search)))`, so either omission or permission/
class broadening fails inspection.

**Local evidence:** These fast source checks passed:

```text
bash -n tools/a33xctl
policy_rule="$(rg '^allow sos_core_platform sos_authority_data_file:' \
  aosp/device/sos/a33x/sepolicy/system_ext/private/sos_core_platform.te)"
test "$policy_rule" = \
  'allow sos_core_platform sos_authority_data_file:dir search;'
git diff --check -- \
  aosp/device/sos/a33x/sepolicy/system_ext/private/sos_core_platform.te \
  tools/a33xctl docs/core1-provider-parity.md docs/progress.md
```

Broader directory macros, file access, parent relabeling, and changes to the
authority's controls were rejected because the observed AVC requested only
directory `search` and no secondary class appeared.

**Decision, remaining risk, and next gate:** Accept the source patch as the
smallest justified candidate, not as a passed device fix. No full AOSP build,
compiled policy/neverallow gate, new signed OTA, or physical re-acceptance run
has occurred. Run the full Core 1 build and inspection, record the new OTA's
revision/size/hash, reflash it, verify `sos_core_platform` stays active without
the parent-search AVC, and then rerun every provider/action, restoration,
restart/fallback, Recovery-coexistence, and soak test skipped by the failed
gate. Detailed status remains in
[`core1-provider-parity.md`](core1-provider-parity.md#minimal-fix-prepared-device-re-acceptance-pending).

## 2026-08-17 — Non-shipping Core 1 provider acceptance probe

**Goal / testability gap:** Make the previously blocked live provider matrix
independently runnable against the real Android authority and Core platform
adapter. The existing `provider-state-probe` speaks to the Linux provider
service and is not evidence for `/data/misc/sos/provider.sock`. A transient
shell binary is also invalid: the shell domain lacks the trusted Core host's
DAC and SELinux access to that socket.

**Chosen design and security invariant:** Added a small test-only Rust crate
that consumes the existing Android provider request/response, effect, state,
provider, and capability types. A feature-gated C export in the Core runtime
runs it through the existing init-owned `sos_core_bridge_probe` service and
`sos_core_host` domain. The named `build-core1-provider-probe` recipe enables
that export only for a non-shipping test OTA; `inspect-core1-provider-probe`
requires the export, trusted invocation markers, supported modes, and absence
of a separate packaged probe executable. Normal `build-core1` excludes the
feature and normal `inspect-core1` rejects any image containing the export.

No product SELinux permission, authority/provider semantics, platform
implementation, socket, wire format, credential path, capability allowlist,
or Core 0B behavior changed. The existing client was refactored only to expose
its already-decoded raw response internally; normal callers retain identical
error behavior.

**Probe coverage:** `snapshot` emits redacted presence/count/status records
for Health, thermal, audio/media, link, Supplicant, apps, attention, and fixed
capability names. `security` injects a staged privileged restart effect and
requires rejection before state staging, aborting if a regression accepts the
stage. `unavailable` uses a reserved bogus opaque ID and requires explicit
capability rejection. Separately authorized `audio-restore` and `wifi-restore`
modes capture the initial state, apply one bounded reversible change, observe
it, restore the exact prior state, and observe restoration. Output omits
labels, SSIDs, opaque IDs, interface names, titles, provider error payloads,
and credentials. Exit 0/1/2 means `PASS`/`FAIL`/`SKIP`.

Shell-domain socket grants, transient relabeling, a second authority endpoint,
duplicated ad-hoc JSON, embedded Wi-Fi secrets, and a probe enabled in normal
Core 1 builds were rejected as broader or less auditable.

**Local evidence:** Formatting passed. `cargo test --locked -p
android-provider-acceptance -p android-system-authority` passed six probe and
10 authority tests, covering snapshot redaction, privileged-effect rejection,
cleanup of an accidentally accepted stage, explicit unavailable semantics,
exact audio/Wi-Fi restoration action sequences, and the existing authority
boundaries. `cargo clippy --locked -p android-provider-acceptance --all-targets
-- -D warnings`, `cargo check --locked -p sos-experience
--no-default-features --features core-provider-acceptance`, `bash -n
tools/a33xctl`, and `git diff --check` passed. No ARM64/API 31 build, AOSP
build, OTA inspection, adb command, or device action ran in this change.

**Decision, remaining risk, and next gate:** Accept the harness design as a
non-shipping test facility, not as provider evidence. Run the named ARM64 Core
1 probe build and inspection, record the signed test OTA metadata, install it
only with explicit authorization, and verify the fixed platform daemon first.
Then run read-only/security/unavailable modes, explicitly authorize and run
the restoring audio/Wi-Fi modes, clear both debug properties, and complete the
restart/fallback, Recovery-coexistence, and soak matrix. Rebuild normal Core 1
afterward and require normal inspection to prove the probe export is absent.
Detailed mode and cleanup rules are in
[`core1-provider-parity.md`](core1-provider-parity.md#non-shipping-live-acceptance-probe-prepared).

## 2026-08-17 — Core 1 provider probe contract repair

**Goal / hypothesis:** Diagnose the first non-shipping probe OTA failure
without masking platform crashes or changing product security semantics. The
hypothesis was that the test client did not honor the real Android authority
response contract.

**Failed device evidence:** Probe revision
`sos.core1.40c433d4fb63.940ce909570c` returned snapshot `FAIL
request_or_decode`, security `FAIL wrong_rejection`, and unavailable `FAIL
snapshot`, each with exit 1. Audio, Wi-Fi, and later gates were not run.
`sos_core_platform` received signal 13 and restarted at PIDs 944, 1453, and
1484, later running as PID 1517. The aggregate raw log stays outside Git as
`/tmp/core1-probe-matrix.log`, 1,791 bytes, SHA-256
`271c6ca130736149dbd018db342cf3380730e0b84dedbd3b63a7e34e56f4d859`;
separate per-mode raw logs were not retained. Cleanup passed on normal revision
`sos.core1.40c433d4fb63.36d94625c31f`: the probe was absent, services were
stable, and no AVC appeared.

**Proven contract defects and fix:** The probe reused the normal client helper,
which collapsed an authority `ok=false` response into a generic error. It
therefore could not identify the expected capability denial. It also
inherited the UI's 500 ms socket deadline although the authority's nested
platform request is allowed two seconds, permitting the probe to close before
the authority can return a valid response. The feature-gated probe now uses a
five-second deadline and consumes the raw response; shipping callers retain
the 500 ms timeout and previous error behavior. Client and authority share one
newline-JSON framing implementation, and mode reports now distinguish
transport/framing, load-state, wrong-rejection, and expected-denial outcomes.
The wire schema, platform adapter, capability/signature checks, SELinux policy,
product packaging boundary, and Core 0B remain unchanged.

Signal 13 is evidence of a server writing after a client peer closed, but the
aggregate log does not prove that the corrected outer probe deadline also
eliminates every platform restart. Ignoring SIGPIPE, weakening fail-closed
checks, widening the shipping UI timeout, changing the authority's two-second
platform deadline without a measured product bug, or granting new policy were
rejected.

**Local evidence:** `cargo test --locked -p android-authority-protocol -p
android-provider-acceptance -p android-system-authority` passed three framing,
six probe, ten authority-library, and four real-handler probe tests. These
cover snapshot success, exact injected-capability denial, unavailable-action
semantics, EOF, truncated response, and server write after an early client
close. Focused Clippy passed with warnings denied. The release ARM64/API 31
`core-provider-acceptance` feature check and host feature check passed.
The broader Android Clippy command reached the target but stopped on the
unchanged vendored `gpui-mobile`
`clone_on_copy` warning at `android/platform.rs:543`. Formatting passed; Bash
syntax and final diff checks remain part of handoff. No AOSP build, OTA, adb
call, or device mutation ran here.

**Decision, remaining risk, and next gate:** The harness contract repair is a
local candidate, not hardware acceptance. Build and inspect a fresh
non-shipping probe OTA, record its revision/size/hash, install it under the
existing authorization, and require stable platform/authority services with
no signal-13 restart while `snapshot`, `security`, and `unavailable` pass.
Only then run explicitly authorized restoring audio/Wi-Fi modes and the
remaining restart/fallback, Recovery, and soak matrix. Restore a normal Core 1
OTA afterward and prove the probe export is absent. Detailed status is in
[`core1-provider-parity.md`](core1-provider-parity.md#first-probe-ota-attempt-failed-contract-fix-pending-reflash).

## 2026-08-17 — Core 1 platform reply SIGPIPE hardening

**Goal / hypothesis:** Preserve the corrected provider replies while stopping
a closed authority peer from terminating the long-lived Core platform daemon.
The narrow hypothesis is that `sos-core-platform` finishes a slow provider
snapshot after the authority's nested deadline and its plain Unix-stream
`write()` receives `SIGPIPE`.

**Corrected device evidence:** Snapshot, security, and unavailable on the
corrected probe image each received a complete `PASS` response and exited 0.
Snapshot completed at 11:05:54.833 and platform PID 938 received signal 13 at
11:05:58.262. Security completed at 11:05:57.999, at the same recorded time as
that signal. Unavailable completed at 11:06:03.278 and platform PID 1550
received signal 13 at 11:06:04.266. The aggregate raw diagnostic remains
outside Git as `/tmp/core1-probe-final-diagnostic.log`, 15,211 bytes, SHA-256
`fd019793daa2261b4b3b0e9eebc1b8844c16976dbc3abd9726a2c7f806449e2d`.
Clean revision `sos.core1.40c433d4fb63.57ac4b474afb` was stable afterward,
with the probe absent and no current AVC.

**Proven failure class and code delta:** The Core platform response helper used
plain `write()` and returned an ignored Boolean. A forked host control with
default signal disposition reproduces termination by `SIGPIPE` on a closed
Unix peer. The production helper now uses `send(..., MSG_NOSIGNAL)` per send,
retries `EINTR`, completes partial sends, returns the concrete socket error,
and logs one `core_platform_response_send_failed` warning without killing the
service. Successful frame order and bytes are unchanged. The product inspector
requires that error-handling marker. No process-wide signal disposition,
authorization, SELinux, package policy, provider semantics, or Core 0B changed.

The failure class and unsafe primitive are proven; the exact physical
instruction remains an inference because no on-device stack trace was
captured. The signal timing and receiving process make the platform response
helper the smallest supported hypothesis. The ignored Unix reply errors in
the separate Linux accessibility service are not part of the platform binary
and were not broadened into this fix. Process-wide `SIGPIPE` masking and
silently ignored send errors were rejected because they hide unrelated faults
or discard the peer-close result.

**Local evidence:** The focused C++ test proves that the old primitive dies by
signal 13, the new helper survives and reports `EPIPE`/`ECONNRESET`, a full
one-MiB response arrives unchanged, partial sends and `EINTR` are retried, a
mid-response close is handled, and the next peer still receives a response.
The test compiles with C++17, warnings as errors, and pthread support. Focused
Rust/framing tests, ARM64 feature checks, formatting, Bash syntax, and diff
checks remain in the handoff; no AOSP build or device command ran here.

**Watch item at this stage:** One security-probe AVC denied
`u:r:hal_wifi_supplicant_default:s0` `efs_file:dir { search }` for name `/`,
device `sda2`, inode 2, permissive 0. It was absent on clean boot; later probe
windows reproduced it. Do not add an allow or relabel without source
attribution.

**Decision, remaining risk, and next gate:** This is a source-level resilience
candidate, not a passed hardware gate. Rebuild and inspect a signed probe OTA,
reflash it, rerun the three passing modes while requiring stable platform PIDs
and no signal-13 exit, then run the explicitly authorized audio/Wi-Fi restore
and remaining restart/fallback, Recovery, and soak matrix. Restore and inspect
a normal Core 1 image afterward. Detailed status is in
[`core1-provider-parity.md`](core1-provider-parity.md#corrected-probe-replies-pass-platform-peer-close-hardening-pending).

## 2026-08-17 — Core 1 provider implementation and final partial retest

**Goal / chronology:** Close the proven platform blockers, exercise every
currently available provider gate on the SM-A336B, restore a clean shipping
image, and distinguish passed fixes from provider capabilities that still
lack runnable hardware state.

The first physical build failed before a snapshot because
`sos_core_platform` could not search the authority-labeled
`/data/misc/sos` parent. The only policy change was
`allow sos_core_platform sos_authority_data_file:dir search;`; the compiled
inspector requires that exact complete relationship and rejects any other
class/permission. No file access, relabel, authority broadening, or Core 0B
change was made. The final image did not reproduce the parent-search AVC, so
this minimal fix physically passed.

A non-shipping probe was then added through the existing init-owned trusted
host domain. Normal builds exclude it and normal inspection rejects its
export. Its first device run exposed two harness defects: a 500 ms client
deadline was shorter than the authority's permitted two-second nested request,
and the normal helper collapsed `ok=false` into a generic error. The probe-only
path now uses five seconds, consumes raw negative responses, and shares the
authority's newline-JSON framing; the shipping client remains at 500 ms with
unchanged error behavior. Three framing, six probe, ten authority, and four
real-handler tests passed.

Complete corrected replies exposed the remaining product signal-13 failure.
A forked peer-close regression proved that the platform C++ response path's
plain `write()` can terminate on `SIGPIPE`. The production helper now uses
per-send `MSG_NOSIGNAL`, retries `EINTR` and partial sends, surfaces/logs
`BrokenPipe`, and continues with the next peer; no process-wide signal ignore
was added. `core_platform_socket_io_test status=PASS`, focused Rust/Clippy and
ARM64 checks passed, and the full AOSP build plus static product inspection
passed.

**Final probe matrix:** Non-shipping revision
`sos.core1.40c433d4fb63.f0ecbf1885d5` passed Core/core-native/no-Zygote
identity, authority/host/platform services, and provider/revision sockets.
Snapshot passed with exit 0. The prior corrected security and unavailable
runs passed with exit 0 and a stable platform PID. On the final image,
`audio-restore` was SKIP/exit 2 because capability/state was unavailable and
`wifi-restore` was SKIP/exit 2 because there was no saved network; neither
mutated device state. Platform PID 945 remained stable through the action
observations. A five-minute soak passed across five 60-second intervals with
the same PID and sockets and no AVC, crash, or restart.

Named daemon restart/fallback and Native Recovery/lock coexistence were not
run because there is no supported non-mutating named recipe. Active
applications, media, attention, calls/alarms, and a longer hardware soak also
remain open.

Raw final evidence stays outside Git:

| Artifact | Result | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| `/tmp/core1-final-snapshot-f0e.log` | snapshot PASS, exit 0 | 1,367 | `90d9bbc234b245a9b20bc729277c31722a7c222f4ae664374ca54ed5f5f497f8` |
| `/tmp/core1-final-audio-restore-f0e.log` | audio SKIP, exit 2; no mutation | 862 | `aef5bad22d0061d91317e7e0e2754c5bead6fc161f9166665f7b164955e856f4` |
| `/tmp/core1-final-wifi-restore-f0e.log` | Wi-Fi SKIP, exit 2; no mutation | 1,161 | `7b92a93435ef1eb001d4825bc8ad666965dfdcaf6f61885b2e6f3384524ffdfd` |
| `/tmp/core1-final-soak-f0e.log` | five-minute stability PASS | 1,005 | `a516b218803afafcc540fa95ee7fa41be25d4cd1b51e0f1b9285f78729564f07` |
| `/tmp/core1-final-probe-f0e.png` | final screenshot | 17,324 | `fd11466f534905543f75d6c896942cf633250f221ba324e7766890f751de7890` |

The `hal_wifi_supplicant_default` to `efs_file:dir { search }` denial recurred
during probe windows and was absent on the clean boot. It targets the
`sec_efs` `/dev/block/sda2` filesystem, but causation and functional impact are
unproven. No allow or relabel was added; this is a vendor-owner watch item.

**Clean restore:** `build-core1` and `inspect-core1` passed and proved the
probe absent. The raw cleanup OTA remains outside Git:

| Artifact | Revision | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| `/home/carlid/dev/lineage-a33x/out/target/product/a33x/lineage-23.0-20260817-UNOFFICIAL-sos_core1_a33x.zip` | `sos.core1.40c433d4fb63.9fcf8d492e9b` | 1,022,100,714 | `91bb35f1d258b10166076af1dbd4a165beabfb3f4ce502bb2ac2a6b244fafbda` |

The controlled sideload exited 0 with `Total xfer: 1.00x` and automatically
rebooted. The final boot reached the exact revision with services/sockets
present, no current AVC or crash, and no probe. Earlier instructions assuming
a manual **Reboot system now** step are superseded; this Recovery flow
auto-reboots after sideload.

**Decision / next gate:** The minimal SELinux and SIGPIPE fixes physically
pass, as do snapshot, security, unavailable semantics, and five-minute
stability. Overall physical-provider acceptance remains **partial/open**.
Create capability-bearing audio and saved-Wi-Fi state to exercise restoration,
add supported named restart/fallback and Recovery/lock recipes, attach active
app/media/attention and calls/alarms owners, attribute any recurring vendor
EFS denial, and run a longer soak before calling parity acceptance complete.
The detailed record is
[`core1-provider-parity.md`](core1-provider-parity.md#final-2026-08-17-hardware-result-partialopen).

## 2026-08-17 — Reintegrate Core 1 main-experience bring-up

**Goal / stash judgment:** Selectively recover the useful work from the WIP
stash based on `40c433d` onto current `main`, including the later non-shipping
provider acceptance probe. Core 1 should boot the same Stock Base experience
as Compat 1 from device-encrypted storage and use its native provider adapter;
Compat remains on the unchanged Android/JNI and framework-provider paths.

The stash's DE-hosted launch, Samsung `BTN_TOUCH` fallback and TSP enable,
Core-native GPUI keyboard, read-only Supplicant snapshot, bounded audio probe,
and snapshot timing are coherent and retained. Its older copies of provider
framing, SIGPIPE handling, parent-directory SELinux, probe plumbing, tooling,
and provider documentation were rejected because `8786ec3` already replaces
them. The stash-wide documentation history and broad design-doc rewrites were
also not replayed. The authority timeout stays at two seconds rather than
replaying the stale five-second relaxation, and peer-close safety continues to
use the tested per-send `MSG_NOSIGNAL` helper rather than a process-wide signal
handler. The stash itself remains intact for recovery.

**Changed code / environment:** `sos-core-host` now bypasses only Core 1's
pre-unlock diagnostic surface by default and starts the existing shared GPUI
entry point from `/data/misc/sos/core`; it neither sets
`sys.user.0.ce_available` nor unwraps CE. `debug.sos.core.lock=1` retains the
old locked diagnostic and the supervisor still owns fixed Recovery on child
failure or the Volume Up+Down chord. Core-native input now recognizes both
type-B tracking IDs and Samsung `BTN_TOUCH`/legacy axes, cycles and enables the
sec_ts controller, and exposes a GPUI keyboard through the existing shared
text-session implementation. All additions are `core-native`-gated.

The Core platform adapter now starts its Binder pool, avoids creating a
Supplicant interface during snapshots, logs per-provider latency, and bounds
the first `AudioSystem` volume read to 250 ms before caching unavailable.
Missing audio services remain retryable. Product policy exports only the host
domain needed by the vendor TSP rule; pinned source patches add that vendor
policy directory and grant the system UID DAC access to the live
`/sys/class/sec/tsp/enabled` node. `a33xctl` stages those patches and inspects
the launch, touch, keyboard, TSP, audio, and policy markers. The Android cross
check used NDK `29.0.14206865`, API 31, and the installed AArch64 target.

**Host evidence:** `cargo fmt --all -- --check`, `bash -n tools/a33xctl`, and
`git diff --check` passed. With the pinned NDK compiler/linker variables,
`ANDROID_NDK_ROOT=/home/carlid/Android/Sdk/ndk/29.0.14206865
ANDROID_NDK_HOME=/home/carlid/Android/Sdk/ndk/29.0.14206865
CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=/home/carlid/Android/Sdk/ndk/29.0.14206865/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android31-clang++
CC_aarch64_linux_android=/home/carlid/Android/Sdk/ndk/29.0.14206865/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android31-clang
CXX_aarch64_linux_android=/home/carlid/Android/Sdk/ndk/29.0.14206865/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android31-clang++
AR_aarch64_linux_android=/home/carlid/Android/Sdk/ndk/29.0.14206865/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar
cargo check -p sos-experience --lib --no-default-features --features
core-native --target aarch64-linux-android` passed in 11.53 s. `cargo test -p
android-system-authority` passed 14 tests, `cargo test -p
android-provider-acceptance` passed 6, and `cargo test -p
android-authority-protocol` passed 3. The matching Android `cargo clippy`
completed with only the existing `gpui-mobile` `clone_on_copy` and current
provider-client `needless_return` warnings. Read-only `git apply --reverse --check`
against `/home/carlid/dev/lineage-a33x/device/samsung/s5e8825-common` confirmed
both new pinned source patches are already represented in that checkout.

Three rejected checks are environmental/pre-existing rather than passes:
desktop `cargo test -p sos-experience --lib` compiled the crate but could not
link because this host lacks `xkbcommon`/`xkbcommon-x11`; Android `cargo check
... --tests` reached existing target-incompatible tests (`providers_fake`,
desktop-only embedded validation, and stress parser cfgs); and strict
`cargo clippy ... -- -D warnings` stopped first in the existing `gpui-mobile`
warning.

**AOSP build and static gate:** The first host-only runner transaction ran
`./tools/a33xctl build-core1` and `./tools/a33xctl inspect-core1`; both exited
0 in 371.527 s. It produced revision
`sos.core1.9a68f1083157.706c4fd35e3f` (1,022,123,478 bytes, SHA-256
`366db0294da9c02f2409be160c894acd7827c6e3d49dc8e8a8811f12edbcfc32`),
but exposed the unconditional `rgb` import warning. Its transcript remains
historical evidence at
`/tmp/core1-main-experience-host-8786ec3/build-inspect.log` (1,602,733 bytes,
SHA-256
`8c19ab6426797f1fadb909943889078065ad72806a7499b515d2afd91cfbf2cc`).
That OTA was superseded and is not the device-gate candidate.

The import is now `core-native`-gated. After that cleanup, `cargo fmt --all --
--check`, `bash -n tools/a33xctl`, and `git diff --check` passed. Using the same
pinned NDK variables above, both `cargo check -p sos-experience --lib
--no-default-features --features aosp-system --target aarch64-linux-android`
and the matching `--features core-native` command passed, confirming both
sides of the cfg boundary.

A second host-only transaction on HEAD
`9a68f1083157503097ba95533288837127c136e6` reran
`./tools/a33xctl build-core1` and `./tools/a33xctl inspect-core1`; both exited
0 in 228.695 s, and the introduced unused-`rgb` warning was absent. Initial
and final repository status were identical. This exact post-cleanup build and
target-files inspection close the current host packaging/policy gate:

| Final device-gate candidate | Revision | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| `/home/carlid/dev/lineage-a33x/out/target/product/a33x/lineage_sos_core1_a33x-ota.zip` | `sos.core1.9a68f1083157.a537acc9b4d1` | 1,022,110,439 | `d111c199c56ef7d1e5964b23aa4d9a0743d2243e238d28fd937b4afda8994985` |

Final target-files are at
`/home/carlid/dev/lineage-a33x/out/target/product/a33x/obj/PACKAGING/target_files_intermediates/lineage_sos_core1_a33x-target_files`.
The final transcript is
`/tmp/core1-main-experience-host-postcleanup/build-inspect.log` (1,563,827
bytes, SHA-256
`207f65d3f67c2dfa13e95f3d32fb9f8c9556e15695b3f21bf3db116506dc6d8e`).
Neither transaction performed a device operation; no boot observation or
hardware claim is made.

**Decision / risks:** Accept the selective integration as the smallest shared
experience plus bounded Core-native platform delta. The AOSP build/static gate
passed on the post-cleanup tree. Remaining gates are physical default
boot into Stock Base with CE still locked, Samsung touch and text entry,
Recovery chord/retry, provider latency and lifecycle, and a longer soak. Native
synthetic-password unlock,
saved-network provisioning, reversible audio when available, native
media/app/attention owners, calls/alarms, and generated-revision fallback are
still open; the simple native keyboard is not an Android IME replacement.

**Runner recipe / next gate:** A Terra gate may bind serial `RFCT50EGFCN` to
the final device-gate candidate's exact OTA path, revision, byte size, and
SHA-256 above before authorizing one Luna transaction. The transaction may
sideload that exact artifact once with its inherent auto-reboot, write evidence
under `/tmp/core1-main-experience-<revision>/`, and terminate on wrong artifact
identity, unresolved prior recovery/sideload state, sideload failure, wrong
revision, boot timeout, crash/restart, or completion. Acceptance requires
Core 1/core-native/no-Zygote/enforcing identity; CE still unavailable; `SOS
Core Experience` instead of the locked surface; a visible touch hit; keyboard
focus, committed glyphs, backspace, and submit; a warm provider snapshot below
500 ms with live Health facts and no read-path Wi-Fi mutation; stable
host/authority/platform PIDs and no relevant enforcing AVC during a five-minute
soak. Owner-operated Volume Up+Down must enter fixed Recovery and Volume Up
must retry Stock Base before the gate can close. Do not infer any of these from
the host checks above.

## 2026-08-17 — Core 1 main-experience gate: sideload transition rejected

**Goal / authorized artifact:** Install the inspected Core 1 candidate on
SM-A336B `RFCT50EGFCN` and evaluate default Stock Base boot, physical input,
native providers, Recovery, and soak. The exact OTA was
`/home/carlid/dev/lineage-a33x/out/target/product/a33x/lineage_sos_core1_a33x-ota.zip`,
revision `sos.core1.9a68f1083157.a537acc9b4d1`, 1,022,110,439 bytes, SHA-256
`d111c199c56ef7d1e5964b23aa4d9a0743d2243e238d28fd937b4afda8994985`.

**Preflight:** Artifact and serial matched. The device was in ordinary `adb
device` mode on prior revision `sos.core1.40c433d4fb63.9fcf8d492e9b`, with
stage `1`, profile `core`, providers `core-native`, UI owner
`native-sos-no-zygote`, SELinux enforcing, and `ro.zygote=no_zygote`. PIDs were
host supervisor/child 927/931, authority 949, and platform 950. The captured
surface was the old `SOS CORE 1 / NATIVE RECOVERY / NO ZYGOTE / CE DATA LOCKED
/ USE RECOVERY` screen.

**Failed transaction:** The runner issued exactly one direct `adb sideload`
without first transitioning the ordinary adb device into Lineage Recovery
sideload. It failed closed with `adb: sideload connection failed: closed`; the
legacy retry also closed. Exit status was 1 after a measured 0.006376 s. No OTA
bytes were accepted and no reboot occurred. The device remained on the old
revision with the same screen. The authorized sideload attempt is consumed;
all intended post-boot, UI, provider, soak, and Recovery criteria remain
unproven.

Evidence is outside Git under
`/tmp/core1-main-experience-sos.core1.9a68f1083157.a537acc9b4d1/`:

| Evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| `artifact-metadata.txt` | 351 | `7187bdd6c3055a0e19494ada215bdbce014ff1474e814f08b617536661ed9136` |
| `transaction.log` | 17,186 | `276c81aca6534cf796c2db858b588935bfcca1a659dfeaa4101e4b43665ebba0` |
| `pre-mutation.png` | 17,324 | `fd11466f534905543f75d6c896942cf633250f221ba324e7766890f751de7890` |
| `post-failure.png` | 17,324 | `fd11466f534905543f75d6c896942cf633250f221ba324e7766890f751de7890` |

The identical screenshots corroborate that the failed attempt did not change
the displayed state.

**Decision / next gate:** Reject direct `adb sideload` from ordinary device
mode as an invalid transaction recipe. No further action is authorized. A new
explicit authorization must preserve the same serial/artifact binding and
name the established transition sequence: `adb reboot sideload-auto-reboot`,
`adb wait-for-sideload`, then one `adb sideload` of the exact OTA, followed by
the inherent automatic reboot. An unresolved recovery/sideload process remains
a stop-and-escalate condition. Only a fresh bounded transaction may resume the
still-open boot/UI/provider/soak/Recovery acceptance gate.

## 2026-08-17 — Core 1 main experience installed; physical gate partial

**Corrected transaction:** A fresh authorization bound serial `RFCT50EGFCN`
to the same exact OTA path, revision `sos.core1.9a68f1083157.a537acc9b4d1`,
1,022,110,439-byte size, and SHA-256
`d111c199c56ef7d1e5964b23aa4d9a0743d2243e238d28fd937b4afda8994985`.
Preflight found no unresolved prior recovery or sideload process. The runner
issued `adb -s RFCT50EGFCN reboot sideload-auto-reboot`, reached sideload in a
measured 31.728574 s, then issued exactly one sideload. It exited 0 with
`Total xfer: 1.00x` in 77.493834 s. Lineage Recovery performed its inherent
automatic reboot and adb returned in 72.942698 s. There was no data wipe,
second sideload, or manual reboot.

**Installed identity and UI:** The device reported the exact expected
revision, stage `1`, profile `core`, providers `core-native`, UI owner
`native-sos-no-zygote`, `ro.zygote=no_zygote`, and SELinux Enforcing.
SurfaceFlinger focus identified `SOS Core Experience`. The screenshot visibly
showed the shared main experience with live `99% Charging` power, Offline
network, audio unavailable, applications and attention sections, and the
customization text field. Host supervisor/child PIDs 924/931, authority PID
945, and platform PID 946 were stable across the short observation. The final
AVC query was empty.

CE-related properties returned empty, but the explicit
`core1_experience_start ce_available=false ...` lifecycle marker was not
captured; CE/lifecycle evidence is therefore partial, not a completed gate.
The screenshot's existing `dfddds` text is also rejected as current physical
touch or keyboard proof because this transaction witnessed no corresponding
action sequence.

**Verdict:** Installed identity and main-experience rendering pass. Provider
acceptance, live Health correlation, proof that the snapshot read did not
mutate Wi-Fi, a warm snapshot below 500 ms, witnessed physical touch and Core
keyboard typing/backspace/submit, a measured five-minute soak, and the
owner-operated fixed-Recovery/retry sequence remain unproven. This physical
gate is **partial/open**.

Final evidence is outside Git under
`/tmp/core1-main-experience-sos.core1.9a68f1083157.a537acc9b4d1/retry-correct-recovery/`:

| Evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| `transaction-transcript.log` | 16,374 | `a66bbb51a03ab62c7792e768f47a0d74f572d226eef7d9dbdca127393e9f1d1a` |
| `ui-post-boot.png` | 184,186 | `245714adda55abb8ed7c775e3bf5651311ee8315d01830b9b2c9804116c729dd` |
| `logcat-tail.txt` | 48,280 | `c4d6d7afe2b86d3ebfa3689aa7b65f0cf9a5785e79b7ecdb7babba09c2c32a9c` |

**Physical-action boundary / next gate:** No new destructive authorization is
needed. The owner must first tap the customization text field, use the Core
keyboard to type a new recognizable glyph or string, backspace at least one
glyph, and submit. The owner must then hold Volume Up+Down until fixed Recovery
appears, capture or observe it, press Volume Up exactly once, and verify return
to Stock Base. A subsequent read-only runner observation and measured soak may
collect objective input, lifecycle, provider, PID, AVC, Recovery, and stability
evidence after those owner actions.

## 2026-08-17 — Keep generated-revision submit transactional and recoverable

**Goal / physical finding:** Diagnose and fix the Core keyboard Enter crash
without weakening the shared Compat/Core generated-revision transaction. The
owner's Enter touch ended at 13:38:08.212. The experience then logged
`agent_submit`, committed state revision 602, validated candidate request 299,
rendered the candidate, and emitted its focus-loss action. That action committed
revision 603 at 13:38:09.315 while candidate activation stage 5 was still
pending. Authority PID 945/TID 961 (`sos-core-revisi`) took SIGABRT at .321;
the client received EOF, logged `system_revision_activation_failed` for
revision `0ceaad33...`/stage 5 at .376, and intentionally aborted child PID 931.
The supervisor correctly caught status 6 and displayed fixed Recovery, but this
was not an intentional Recovery chord.

The candidate lifecycle already tried to queue input while `pending_frame` was
set, but `Render::render` moved that frame into a local before building the new
scene. Focus loss during that build therefore saw no activation marker and
advanced the shared provider state. `StateService::promote` then found stage 5
stale only after `activate_response` had written its durable journal; the
authority treated every post-journal promotion error as fatal. This is a shared
AOSP-system activation race exposed by Core's new Enter key, not a Core keyboard
or native-provider failure. A later retry after authority recovery activated a
revision successfully at 13:38:41.815, corroborating a bounded transaction race
rather than persistent input or provider breakage.

**Stack/source proof:** The supplied files under the AOSP `symbols/` path are
actually stripped (`file` and `nm` report no symbols), so `llvm-symbolizer` on
authority frames `0x124e94 0x107550 0x858fc ...` returned `??`; claiming source
line symbolization was rejected. The exact shipped binary still proves the path:
`llvm-objdump` at tombstone frame 3, PC `0x858fc`, shows the immediately preceding
`adrp/add` loading `.rodata` address `0x16105`, whose string is
`android_authority_fatal_activation error=`, followed by the call into tombstone
frame 2 (`0x107548`/reported return PC `0x107550`). The crashing thread name and
source then bind that fatal helper to the Core revision listener's
`activate_response` path. The experience tombstone independently matches its
logged explicit activation-error abort.

Raw evidence remains outside Git:

| Evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/core1-main-experience-sos.core1.9a68f1083157.a537acc9b4d1/submit-crash-diagnosis/crash-logcat.raw.txt` | 12,512 | `00a47b166bec4888a3530d14f4ef00370428c884218ac62e59c3fb97abbe33b2` |
| `/tmp/core1-main-experience-sos.core1.9a68f1083157.a537acc9b4d1/submit-crash-diagnosis/main-system-logcat.raw.txt` | 1,311,662 | `0bf05bafaab2a8f86a72c034670a8c5bb6019f6e7553016d711ab9dded8750bd` |
| `/tmp/core1-main-experience-sos.core1.9a68f1083157.a537acc9b4d1/submit-crash-diagnosis/kernel-logcat.raw.txt` | 1,625,710 | `2cb38cd76d09ad256e7e6f85b6368920e2526236f44dfb80dfee0bb1e8b300c4` |
| `/tmp/core1-main-experience-sos.core1.9a68f1083157.a537acc9b4d1/submit-crash-diagnosis/events-logcat.raw.txt` | 27,998 | `53533f7fa1948e3d0216a99e7525f9197d6ce87767a60936c6a86aec0042ef89` |
| `/tmp/core1-main-experience-sos.core1.9a68f1083157.a537acc9b4d1/submit-crash-diagnosis/derivative-filtered.txt` | 392,205 | `62258d4df1cdf07ee69be2fbb5ea250ef6f9cb9c640dd20bf5d5a1ed6d7743c8` |

**Changed:** A dedicated activation-pending marker now spans candidate scene
construction through the presentation callback, so candidate-generated focus
and input events cannot mutate provider state between staging and activation.
They are dispatched only after success. The authority validates stage freshness
before writing durable intent: unknown, stale, or source/schema-mismatched stages
return ordinary bounded request failures, while promotion/current-pointer/journal
failures after durable intent still abort for restart recovery. Revision request
and response framing is now shared in `android-authority-protocol`; empty,
unterminated, malformed, oversized, and wrong-ID messages remain bounded to one
connection.

On any activation request failure, the experience queries authority before
deciding. If the candidate is current, a lost response is reconciled as success.
If the previous revision is still current and its source/state hash agrees, the
experience discards candidate events, aborts the stale stage, starts a worker for
that last known-good revision, and surfaces a failed-submit status instead of
SIGABRT. An unavailable authority, a third revision, inconsistent source/state,
missing activation metadata, or an incomplete successful response remains a
strict integrity failure and still enters supervisor Recovery.

**Host evidence:** `cargo test -p android-authority-protocol` passed 5 tests,
including empty/truncated revision requests and responses. `cargo test -p
android-system-authority` passed 11 library plus 5 daemon tests; the regression
advances state after staging, verifies stale activation returns before creating
the journal, then proves the same authority accepts and activates the next valid
revision. `cargo clippy -p android-authority-protocol -p
android-system-authority --all-targets -- -D warnings` passed. With NDK
`29.0.14206865`, API 31, and the pinned AArch64 compiler/linker variables from
the reintegration entry, both Android checks passed: `cargo check -p
sos-experience --lib --no-default-features --features core-native --target
aarch64-linux-android` and the matching `--features aosp-system` Compat-side
check. `cargo fmt --all -- --check` and `git diff --check` passed. No device,
adb, or AOSP build action was performed for this fix. `cargo test -p
sos-experience --lib` reached the host linker but could not run because this
workstation lacks `libxkbcommon` and `libxkbcommon-x11`; the two Android target
checks above are the applicable compile evidence for the changed experience
path, and the missing desktop libraries remain an environment limitation.

**Rejected approaches / decision / remaining risk:** Do not remove
`fatal_activation`, ignore durability errors, retry Activate blindly, change the
Core Enter key, add SELinux access, or classify every EOF as success. Those
choices either conceal inconsistent durable state, risk double commitment, or
target the trigger rather than the shared race. Accept the bounded lifecycle,
pre-journal validation, framing, and reconciliation fix. Host tests do not close
the physical gate; the old OTA predates this source.

**Next gate:** Build and inspect a new Core 1 OTA, record its exact revision,
size, and SHA-256, then authorize the established one-sideload automatic-reboot
transaction on `RFCT50EGFCN`. After boot, type a new recognizable string,
backspace, and press Enter. Pass requires one successful generated-revision
activation, no authority or experience PID restart/SIGABRT/fixed Recovery,
focus/input state commit only after `android_authority_revision_activated`, and
matching current revision/provider-state source hashes. Repeat generated-revision
activation through Compat's TCP transport, then complete the still-open provider,
five-minute soak, and owner Recovery chord/retry gates. A deliberately truncated
shipping-device response is not required; its deterministic socket regressions
are host-covered.

**Post-fix AOSP packaging evidence / device-gate candidate:** On HEAD
`9a68f1083157503097ba95533288837127c136e6`, the host-only transaction ran
`./tools/a33xctl build-core1` and `./tools/a33xctl inspect-core1`; both exited 0
in a measured 238.210 s. Initial and final repository status were identical,
there were no integration-owned compiler warnings, and no adb, device, reboot,
or sideload action occurred. The inspected output is:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `/home/carlid/dev/lineage-a33x/out/target/product/a33x/lineage_sos_core1_a33x-ota.zip` (revision `sos.core1.9a68f1083157.ffe6d402644a`) | 1,022,150,544 | `7c25fb3bb09fde68036c3be93d1f314391eb25e6c2995d38016157be65a99b95` |
| Packaged `sos-authority` | 1,315,104 | `afd28c1ba479f664c0ba574b146c8ab463dbb9009418216bd0d67230087b5c96` |
| Packaged `sos-core-host` | 84,808 | `727fee4e27f984f58937bd8778b8b807010bc76d354e0e5431be24b5c201d03f` |
| Packaged `libsos_core_experience.so` | 14,615,192 | `086891fdbe82b111d796673f6ca82638b65de2e746d5edad99797a9ff4a3b058` |

Target files are at
`/home/carlid/dev/lineage-a33x/out/target/product/a33x/obj/PACKAGING/target_files_intermediates/lineage_sos_core1_a33x-target_files`.
The transcript is
`/tmp/core1-main-experience-submit-fix-host/build-inspect.log` (1,564,629
bytes, SHA-256
`c787bff28173f775abd7e2eb036753322a69867d6eded892c9da56bc12975219`).

This `ffe6d402644a` OTA supersedes the currently installed `a537acc9b4d1` OTA
for submit-crash regression; evidence from the old installed build remains
historical only. The exact next device-gate candidate is the `ffe6d402644a`
path, revision, size, and SHA-256 above. A bounded transaction must use the
established Recovery sideload/automatic-reboot path exactly once, then verify
physical typing, backspace, and Enter without authority/experience restart or
fixed Recovery; collect provider snapshots over the existing TCP path; complete
a measured five-minute soak; and finish the owner Recovery chord/retry check.
Host packaging does not close any of those physical gates.

## 2026-08-17 — Core 1 main experience and submit regression physically pass

**Goal / installed artifact:** Close the physical main-experience gate for the
selective Core 1 integration and verify the generated-revision submit-race fix
on SM-A336B `RFCT50EGFCN`. The installed OTA was exactly
`/home/carlid/dev/lineage-a33x/out/target/product/a33x/lineage_sos_core1_a33x-ota.zip`,
revision `sos.core1.9a68f1083157.ffe6d402644a`, 1,022,150,544 bytes, SHA-256
`7c25fb3bb09fde68036c3be93d1f314391eb25e6c2995d38016157be65a99b95`.
The first authorized transition returned from Recovery to ordinary device mode
without entering sideload, so no OTA bytes were sent. A separately authorized
exact retry reached sideload, transferred the OTA once successfully, and used
Recovery's automatic reboot. There was no wipe, manual reboot, or second
sideload.

**Readiness and submit regression:** Core-native readiness passed with exact
stage `1`, profile `core`, providers `core-native`, UI owner
`native-sos-no-zygote`, `ro.zygote=no_zygote`, SELinux Enforcing, and the native
main layer visible. Android `sys.boot_completed`/`dev.bootcomplete` are
intentionally absent on this no-Zygote product. BootAnimation and keystore wait
state remain known residuals, not readiness signals or failures. Before submit,
PIDs were supervisor 921, experience child 932, authority 941, platform 942,
and wpa_supplicant 832.

The owner used physical touch and the Core keyboard to append `fixz`, backspace
the `z`, and press Enter once. The resulting ADB screenshot showed the generated
light experience. Logs establish the required transaction order:
`agent_submit` at 14:45:55.353, `candidate_validated` at .440,
`android_authority_revision_activated` at .540, then post-activation state
commit at .579. There was no activation failure, EOF, authority or child
SIGABRT, fixed Recovery, or PID restart during submit. This closes the
reproduced Enter/submit crash regression.

**Shipping provider and soak evidence:** A temporary read-only host forward
from TCP 14777 to device TCP 47777 was removed after collection. One warmup and
six ABI 1 snapshot responses were all `ok`; measured RTTs were 46.378496,
35.814879, 24.508872, 24.087448, 33.169951, 22.966833, and 23.980907 ms, all
below 500 ms. Live Health reported 99% battery, USB charging, temperature
329→332 deci-C, and no thermal condition. Connectivity remained
`wifi_enabled=true`, disconnected and unvalidated, with no online interface or
network. Throughout the measured 300.000521554 s soak, the SOS surface and
supervisor/child/authority/platform/wpa_supplicant PIDs 921/932/941/942/832
remained stable. This proves the snapshot read did not mutate Wi-Fi. Audio was
unavailable and applications/attention were empty; these remain native-owner
limitations, not fabricated parity.

**Fixed Recovery / retry:** The final owner chord produced
`core_recovery_chord`, intentional child 932 exit code 100/wait status 25600,
and `fixed_recovery_ready child_status=25600`. One
`fixed_recovery_action action=retry` kept supervisor 921 stable and created
exactly one new experience child, PID 5781; authority/platform/wpa_supplicant
PIDs 941/942/832 did not change. Fresh lifecycle evidence reported
`core1_experience_start ce_available=false native_synthetic_password=false`
and runtime ready. The light main experience returned and PID 5781 remained
stable for a measured 30.151199286 s, with no SIGABRT or repeated Recovery.
Four new-child `sos_core_host` `/data` search AVCs are the already documented
Vulkan loader probes; they do not justify a broad allow.

**Evidence integrity:** The initial soak manifest was rejected because it
contained stale/self-referential hashes. Raw files were independently rehashed,
and the final runner wrote a clean audit only after closing its evidence files.
The final audit files are under
`/tmp/core1-main-experience-sos.core1.9a68f1083157.ffe6d402644a/final-recovery-observation/`;
the two prior-soak screenshots are under the sibling
`post-submit-provider-soak/` directory and are indexed by the audited manifest:

| Evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| `final-manifest.tsv` | 1,323 | `a0e0ac9feb7a299a95eb1376d65927f0a55bdb6b70d464843caabc9c746caec4` |
| `prior-soak-audited-manifest.tsv` | 4,524 | `d623882c430ef18ac9e6ea5586295e3343ce70bad1b0c2d93e9492224b2f704e` |
| `markers-filtered.txt` | 300,086 | `6d4d0eea1e9be521f4a8f9f2ed09f7c620e4b97cbff57a458bd758d5fd2ba66b` |
| `screenshot-repeat30.png` | 177,684 | `6b0ff1fa0cc008eeac022045ba967076bb910e3feb5e3a7c453d0ebd52576025` |
| `../post-submit-provider-soak/screenshot-pre.png` | 174,468 | `e74b19d5a8c1d3d988d62bab5f66ee7b8adfc12d237f6904237954d2de33b625` |
| `../post-submit-provider-soak/screenshot-final.png` | 174,440 | `c5d95d525511ccdc679175df2b4cbc77a5c08eac02c6588100c8d9a4a8819100` |

**Decision / remaining gates:** Accept the selective stash integration and
submit-race fix. Terra's Core 1 physical main-experience gate passes for
identity, shared main UI, CE lifecycle, corroborated physical
touch/type/backspace/Enter, correct activation ordering, live warm providers,
no Wi-Fi read mutation, five-minute stability, and fixed-Recovery retry. This
does not establish full Compat feature parity. Boot-complete/BootAnimation/
keystore residuals, audio, compatible application and attention owners, native
unlock/synthetic password, saved-network provisioning and provider actions,
and longer-term soak remain open as documented. The working tree remains
uncommitted, and the original stash remains available for recovery.

## 2026-08-17 — Deterministic Core 1 readiness and evidence tools

**Goal:** Make Core 1 readiness checks product-specific and evidence manifests
deterministic without touching a device.

**Changed:** `tools/a33xctl` adds a read-only `inspect-core1-readiness` snapshot requiring an
explicit serial and expected revision. It checks the Core 1 product identity,
SELinux mode, `SOS Core Experience` surface, init/process state for supervisor,
experience child, authority, and native platform adapter, and current native
lifecycle/runtime markers plus relevant post-lifecycle crashes and enforcing
AVCs. It intentionally never queries either Android boot-complete property.
The new host-only `evidence-manifest-generate` and
`evidence-manifest-verify` commands sort finalized relative paths, record byte
size and SHA-256, exclude the manifest and temporary outputs, detect files that
change while hashing, publish with an atomic rename, and independently verify
membership, order, size, and hash. `tests/a33xctl-host-test.sh` and its mock ADB
fixture cover the happy and wrong-revision readiness paths, deterministic
self-excluding manifests, temporary-output exclusion, and tamper rejection
without hardware.

**Evidence / failures / rejected approaches:** `bash -n` passed for the tool,
host test, and mock fixture; `git diff --check` passed. The targeted host test
was not executed and remains the immediate host check. No adb, device, build,
reboot, sideload, soak, or hardware action occurred. Reusing
`sys.boot_completed` for no-Zygote, self-referential manifests, and a fragile
polling readiness command were rejected.

**Decision / open risks / next gate:** Adopt the small read-only tooling
surface, but do not claim hardware readiness or evidence PASS from host checks.
First run `bash tests/a33xctl-host-test.sh`; then run the next explicitly
authorized exact Core 1 artifact/serial gate through `inspect-core1-readiness`,
soak, evidence finalization, manifest generation, and independent verification.
Android/Compat gates must use their distinct boot-complete/HOME predicate.

## 2026-08-17 — Shared Linux, Compat, and Core Pi runtime

**Goal:** Remove the platform runtime split without weakening AOSP's trusted
revision boundary: Linux, Compat, and Core must use one prompt policy, bounded
Pi runtime/tool contract, and packaged runner, while Core live credentials
remain explicitly out of scope.

**Changed / environment:** `services/sos-agent` now builds the generic
`dist/agent-runner.cjs` entrypoint. Linux uses its existing resident
socket/lifecycle commands; Compat and Core use its bounded `stdio` command.
The shared TypeScript prompt policy and tools enforce context first, validation
before submission, and byte-for-byte equality between validated and submitted
source. The one-shot adapter additionally rejects any tool sequence other than
`get_experience_context`, `validate_experience`, `submit_experience` and returns
only the exact staged source. Compat retains Android Keystore credential
storage and foreground lifecycle, but no longer constructs a second prompt.
Core's deterministic path launches the same immutable Node/Pi/faux bundle
instead of synthesizing a local Rust tool sequence; the Rust host still
independently compiles/renders/validates and owns activation. Core SELinux
policy grants only the fixed host permission to execute the immutable Node
binary, and product/build inspections require the generic bundle and exact
prompt documents. Linux packaging and developer tools now execute that same
bundle. Core's pipe adapter uses nonblocking reads/writes and one explicit
30-second monotonic deadline; a managed-child guard kills and reaps the process
on timeout and every post-spawn I/O/error return. The package build safely
removes only its resolved package-local `dist` before compilation, preventing
obsolete Android-runner outputs from entering Linux installation copies. The
prompt loader checks actual combined bytes after reading as well as metadata
before reading. Generated output remains ignored; the locally built evidence
artifact was `services/sos-agent/dist/agent-runner.cjs`, 1,874,682 bytes,
SHA-256
`cd101d1834e86001b8bace5f8d5c808b60f672ee71171e42d44cc9244897a3e4`,
from repository base `9f898686ef8c` plus this uncommitted change.

**Evidence / failures / rejected approaches:** `npm run check` passed;
`npm test` cleaned and rebuilt the single-file bundle and passed all 10 tests,
including absence of every obsolete `android-runner` output and rejection of
actual post-read prompt documents above the byte limit, in addition to the
packaged faux request, exact three-tool order, oversized-prompt rejection, and
mismatched staged-source rejection. Both
`cargo ndk -t arm64-v8a -P 31 check -p sos-experience --locked
--no-default-features --features core-native` and the corresponding Compat
check without `core-native` passed. `cargo test -p runtime-luau --locked`
passed 22 tests. `bash -n tools/a33xctl packaging/libexec/sos-agent-login
packaging/libexec/sos-login-session tools/linux-agent-e2e tools/sosctl` and
`git diff --check` passed. `cargo test -p sos-experience --lib --locked` was
attempted and rejected by the host linker because this workstation lacks
`libxkbcommon` and `libxkbcommon-x11`; compilation reached the final link, and
the two Android-target checks above cover the changed Rust module.
`./tools/linux-agent-e2e` then passed the complete existing Linux resident
path with the generic bundle: its Pi faux prompt emitted context, validation,
submission, reported the candidate active, and transactionally changed the
revision from `31f8e1d31b6e...` to `2303ba94d140...`. Retaining an Android-named
bundle, duplicated Java prompt rules, the Core-local faux tool sequence, or
accepting live Core credentials without a native ceremony were rejected. Final
review also rejected the initial blocking Core `read_to_end`/`wait`, the build
that left stale generated files behind, and the metadata-only prompt size
check; the bounded child helper, exact clean target, and post-read byte check
replace those approaches without hardware claims.

**Decision / open risks / next gate:** Adopt this as the shared-runtime source
milestone only. No AOSP product, OTA, device, SELinux, boot, credential, or
latency gate ran, so there is no hardware pass. The next runner first repeats
the exact host recipe `cd services/sos-agent && npm ci --ignore-scripts && npm
run check && npm test`, then from the repository root runs both `cargo ndk`
checks above, `cargo test -p runtime-luau --locked`, the listed `bash -n`, and
`./tools/linux-agent-e2e` and `git diff --check`. After a new Core 1 product
build produces an exact revision, path, byte size, and SHA-256, Terra must issue
a separate serial-bound hardware envelope. That runner must require the
packaged `agent-runner.cjs` identity,
enforcing no-Zygote Core readiness, one Luau deterministic prompt showing only
context/validate/submit, successful exact-source activation by the trusted
host, stable supervisor/experience/authority/platform/Node lifecycle, and no
relevant crash or enforcing AVC. Live Core provider credentials remain blocked
on a trusted native credential ceremony and require a later independent gate.

## 2026-08-17 — Trusted Core OpenRouter ceremony and one-model Android campaign

**Goal:** Close the no-Zygote Core credential gap without exposing a secret to
generated Luau or adding an Android/JNI dependency, and constrain both Core and
Compat live OpenRouter execution to the exact canonical model
`deepseek/deepseek-v4-flash`.

**Changed / environment:** Core now owns a fixed Rust/GPUI OpenRouter dialog
above the generated surface. It uses a credential-only branch of the existing
Core-native keyboard, including a fixed letters/numbers/symbols layout and
fixed Save/Cancel controls. Input is masked and bounded to 20–512 visible ASCII
bytes. The complete value exists only in a Rust state object and zeroized
temporary pipe buffers: it is never sent through `ExperienceModel`, provider
state, revision/state files, Luau, accessibility semantics, logging, argv, or
environment variables. Drafts and active values are zeroized on cancel,
replacement, explicit removal, and normal Core process exit. Core storage is
deliberately memory-only, so a host restart loses the credential. Compat keeps
its existing unlock-bound Android Keystore behavior.

The Core adapter now selects `openrouter` plus
`deepseek/deepseek-v4-flash`, places the credential only in the generic
`agent-runner.cjs` stdin document, and zeroizes both request and response
buffers. A successful response must identify OpenRouter and carry a valid
API-key refresh before Core replaces its in-memory credential. The existing
managed-child kill/reap guard remains active on every post-spawn failure, with
a 30-second monotonic deadline for faux and an explicit 240-second deadline for
live OpenRouter; timeout errors name the selected bound. The successful stdio
response now repeats the exact selected model (`faux` or the pinned live slug).
Core verifies provider and model before accepting source or refreshed
credentials. Compat's Java bridge verifies the model before source/credential
handling, propagates it to Rust, and Rust verifies it again. The verified
nonsecret provider/model pair is logged for device evidence. Faux remains the
deterministic diagnostic. The shared stdio decoder rejects every other
OpenRouter model, Compat's Java constant uses the same exact slug, and product
inspection requires the slug in the shared bundle and both Android runtimes,
the fixed Core ceremony marker, and the response-model evidence marker. The
runner still enforces context first, validation before submission,
byte-identical staged submission, and the Rust host's independent
compile/render/scene validation before activation.

**Evidence / failures / rejected approaches:** `npm run check` passed and
`npm test` rebuilt the generic bundle and passed all 11 tests, including exact
OpenRouter request/response model identity and rejection of the former slug.
The ignored
`services/sos-agent/dist/agent-runner.cjs` from base `0212677d7038` plus this
change is 1,874,972 bytes with SHA-256
`8e900a90e6d59d33b614f4c842a4b6f8ca1f676cdf5b324656f40b0deee02b60`.
The four credential/Android-agent-contract tests passed with temporary empty
host linker stubs for this workstation's absent `libxkbcommon` and
`libxkbcommon-x11`; an initial ordinary link failed only on those two missing
host libraries after successful
compilation. An exploratory unfiltered library run passed seven of eight tests;
the existing embedded-experience fixture rejected `audio.set_volume`, which is
outside this agent/credential change, while both focused filters passed all
four changed tests. Both ARM64 Android checks passed:
`cargo ndk -t arm64-v8a -P 31 check -p sos-experience --locked
--no-default-features --features core-native` and the corresponding Compat
command without `core-native`. Gradle
`:app:compileDebugJavaWithJavac` passed for HOME/Compat with only the existing
Java 8 deprecation warnings. `bash -n tools/a33xctl` passed. Persisting a Core
key, reusing the generated text-session/model path, adding JNI, accepting an
arbitrary OpenRouter model, or replacing Pi's provider implementation were
rejected. No secret or secret-shaped fixture was added or used.

**Host artifact transaction:** A non-device runner then executed the product
recipe sequentially from this dirty source. The first transaction measured
Compat build 353.267 s (exit 0), Compat inspection 17.067 s (exit 0), Core
build 354.801 s (exit 0), and Core inspection 17.990 s (exit 0). Both
inspectors passed the exact shared-runner identity and pinned-model checks;
Core additionally passed the trusted ceremony/response-evidence markers and
the compiled SELinux `sos_core_host` execute permission for the immutable Node
runner. Transaction PASS was nevertheless rejected: Core's product
`installclean` removed the just-built Compat OTA, so retaining both only in the
mutable product output was not a valid preservation approach.

The corrective transaction first preserved and reverified the exact Core OTA,
then rebuilt Compat in 227.474 s (exit 0), inspected it in 17.176 s (exit 0),
copied it beside Core, and reverified both stable copies. These are the only
host-approved candidates for later authorization:

| Profile | Stable path | Revision | Bytes | SHA-256 |
| --- | --- | --- | ---: | --- |
| Compat 1 | `/home/carlid/dev/lineage-a33x/sos-agent-openrouter-artifacts-20260817/lineage_sos_compat_a33x-sos.compat1.0212677d7038.64adb75c2630-ota.zip` | `sos.compat1.0212677d7038.64adb75c2630` | 1,042,153,072 | `7fc61b7a124dcdbdb1abf1bccec1c9f1a25d2c0cc69279cc9312fca34eebe88e` |
| Core 1 | `/home/carlid/dev/lineage-a33x/sos-agent-openrouter-artifacts-20260817/lineage_sos_core1_a33x-sos.core1.0212677d7038.8f0ffcb7cc3d-ota.zip` | `sos.core1.0212677d7038.8f0ffcb7cc3d` | 1,022,140,237 | `05d7849256eabd5296210301fe7e7b9c0df9ff1402fd0cf0e233b919d227d308` |

Both packages contain the identical 1,874,972-byte runner above and the pinned
91,280,288-byte Android Node, SHA-256
`e1e6cf7de807baea6fa1d2a81bd6da29d777ab08149645431ebbe283bda33607`.
The inspected common native runtime is 14,682,200 bytes, SHA-256
`eeab3ab7e98c8ef85ced9f3b343f29d744f5680bf864affb82c2bf8565031607`;
the authority is 1,315,104 bytes, SHA-256
`afd28c1ba479f664c0ba574b146c8ab463dbb9009418216bd0d67230087b5c96`;
and the final Compat platform-signed APK is 40,821,561 bytes, SHA-256
`5148c8085b5135d2089e56b5e73a5d21f3aaec73767190c2856f7440b3318a8b`.

The first closed transcript set is indexed by
`/tmp/sos-agent-openrouter-artifacts-20260817/MANIFEST.txt`, 818 bytes,
SHA-256 `5a91a7f3f79311afa85345c9e78e843997c189a2bef50981683dea9970108125`.
The corrective set is indexed by
`/tmp/sos-agent-openrouter-artifacts-20260817-preserved/MANIFEST.txt`, 553
bytes, SHA-256
`d5bf766893732c0d7430892ef4ab389c38c6f16ad3b806de76ebb9ac7f23b536`.
Each TSV manifest's listed path, byte size, and SHA-256 was independently
recomputed successfully. Initial and final Git status were identical, and
`git diff --check` passed. Non-fatal output was limited to the known generated
Node target-recipe overrides, Java 8/Rust future-compatibility warnings, the
releasetools hermetic-invocation warning, and OpenSSL's deprecated `rsautl`
notice. No device, credential, live-provider network request, reboot, sideload,
or hardware action occurred.

**Decision / remaining risk / next gate:** Adopt the source milestone, with
Core restart loss as an explicit product constraint. Host product builds and
offline inspection passed, but no device, credential, live network, SELinux
runtime, latency, or hardware gate ran, so this is not a live-provider or
hardware pass. Serial `RFCT50EGFCN` and each stable artifact above require a
separate explicit one-sideload authorization; Compat then Core is preferred so
the final installed state is Core. Each campaign must enter the credential
only through its trusted UI, never record it in evidence, invoke no model except
`deepseek/deepseek-v4-flash`, prove exact-source activation, clear the
credential afterward, and audit lifecycle markers, process cleanup, crashes,
and enforcing AVCs. The Core campaign must use the no-Zygote readiness
predicate and must not query Android boot-complete properties.

## 2026-08-17 — Compat composer explicitly presents the Android IME

**Goal / hypothesis:** Repair the live Compat agent gate in which tapping the
GPUI composer activates its Android editor but does not present the soft
keyboard. The first source change addressed a possible served-editor timing
race. The later exact-composer retest proved that focus and connection creation
already complete: Android serves `GpuiImeBridge$BridgeConnection`, but the
bridge is laid out wholly outside the window at `[-2,-2][-1,-1]`. The remaining
root cause is therefore the invalid off-window presentation anchor, compounded
by issuing a weak implicit request before requiring a completed visible layout.

**Changed / root cause:** The earlier generation-checked deferred request is
retained, and `GpuiImeBridge` now supplies Android a `VISIBLE`, fully
transparent, non-clickable one-pixel editor at the in-window top/start origin.
The empty View keeps nonzero framework alpha while its transparent background
draws no pixel, satisfying OEM visibility predicates without adding UI.
It explicitly returns `false` from touch handling, so it neither obstructs nor
consumes native-surface pointer input. IME presentation waits for attachment,
a non-null window token, completed nonzero layout, a nonempty globally visible
rectangle, View focus, and window focus. Activation restarts the input
connection once; attachment, in-window layout, window-focus restoration, and
`onCreateInputConnection` converge on one replaceable generation-checked show
runnable. The user-initiated show uses Android's normal explicit flags (`0`),
not weak `SHOW_IMPLICIT` or sticky `SHOW_FORCED`, and API 30+ presentation
targets the owning Activity window's `WindowInsetsController`. Activity
destruction cancels pending work and releases the static bridge state; blur
still increments the generation and clears the active node before hiding the
IME. Existing complete composing/selection/commit dispatch remains the only
text path into the keyed GPUI editor. Core's fixed native keyboard and shared
agent/runtime/provider contracts are unchanged. Nonsecret diagnostics now
report request reason/acceptance, window flags and soft-input mode, attachment,
token, shown/alpha/layout/focus state, visible bounds, size, and IMM active state,
without logging editor text.

**Prior hardware evidence:** Exact artifact
`sos.compat1.0212677d7038.64adb75c2630` booted to SOS Home. The protected
provider credential was configured, but one controlled nonsecret input attempt
left only `Change this experience…` and made no request. The finalized gate
files were:

| Path | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.64adb75c2630/resume-attempt-4/composer-focus.raw.txt` | 1,178 | `f2da7cdc9d7dac2fd43886455e75bda4fcf3991f4f1845226afd411d3904aad5` |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.64adb75c2630/resume-attempt-4/composer-injection.raw.txt` | 136 | `c1f7cc7b30099bee428af8c6c74f7c538beffe67f0a89daa94498a3ea4993baa` |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.64adb75c2630/resume-attempt-4/composer-token-candidate.png` | 199,754 | `f3504bca847e9899abffc00babbb8b4f92aa4e569f1a66481cd3d3c665804ec9` |

The focus capture reported `GpuiImeBridge$BridgeConnection` as the served
connection and the bridge view as focused, while `mInputShown=false`. This
rules out missing GPUI focus, missing editor classification, and missing input
connection creation; it localizes the defect to IME presentation timing. The
three supplied SHA-256 values were rechecked on the host. No device command,
product/OTA build, sideload, live request, or hardware retest was performed for
this source change. Compat-configured
`:app:compileDebugJavaWithJavac` completed successfully in 0.768 seconds; its
only diagnostics were the existing Java 8 source/target and deprecated-API
warnings. `git diff --check` also passed.

**Host artifact experiment:** A later non-device runner built the Compat 1
product with `./tools/a33xctl build-compat1`, which exited 0 in a measured
224.398372 seconds, then ran `./tools/a33xctl inspect-compat1`, which exited 0
in 17.150075 seconds. Total measured wall time was 241.548447 seconds. The
inspected OTA was copied out of mutable product output to the stable path
`/home/carlid/dev/lineage-a33x/sos-compat-ime-fix-artifacts-20260817/lineage_sos_compat_a33x-ota.zip`.
Its revision is `sos.compat1.0212677d7038.c387bc0e3d25`; it is 1,042,211,896
bytes with SHA-256
`f9fc74b2fc28aba27f1de0975eb368dc004257e02b2cdebdd11954ab6ea9b9af`.

The finalized host evidence is:

| Path | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/sos-compat-ime-fix-artifact-20260817/build-compat1.raw.txt` | 1,541,722 | `ca585c4d1066303394c7170813b9eded4645ed0d900259caa53dab0122435c97` |
| `/tmp/sos-compat-ime-fix-artifact-20260817/inspect-compat1.raw.txt` | 27,796 | `1b203dc9ce32f367919344bb490cb4d1abc633e1382e2a51c75997a73fae4baa` |
| `/tmp/sos-compat-ime-fix-artifact-20260817/manifest.tsv` | 746 | `955e71e3f83f8bff8a46974447735b6870d85445f399ef84b57bf9e7c4cdc9a9` |
| `/tmp/sos-compat-ime-fix-artifact-20260817/manifest-verify.raw.txt` | 160 | `53113ff0ada6fe5b68edbd82de9e44eb9e15f3e0b09a0b9a245a8bbcfb389434` |

Independent deterministic verification of the self-excluding manifest passed.
Initial and final Git status were identical, and `git diff --check` passed. The
transaction performed no device or live-provider operation. These host results
approve the exact artifact only for a later authorization decision; they do
not establish keyboard presentation, committed-text delivery, live request,
activation, stability, or soak on hardware.

**First hardware retest — invalid focus target / inconclusive:** One authorized
sideload of that exact 1,042,211,896-byte artifact completed successfully on
serial `RFCT50EGFCN`, and the base Android/Compat boot identity passed at
revision `sos.compat1.0212677d7038.c387bc0e3d25`: `sys.boot_completed=1`,
`SosHomeActivity` was top-resumed, SELinux was enforcing, and the expected SOS
host, authority, framework bridge, and experience processes were present. The
subsequent IME attempt did not establish focus on the agent composer. Its
screenshot visibly places the blue caret in the unrelated, upper
`Caffè ☕️ – 明日のデザイン` `note-draft` field; the lower
`Change this experience…` `agent-prompt` remains an empty placeholder. The
captured IME state reports `mInputShown=false`, `mServedView=SosHomeActivity`
`DecorView`, and `mServedInputConnection=null`:

| Path | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.c387bc0e3d25/ime.raw.txt` | 422 | `cebdafc3615421400c92cbe79b70e98f808f8655d094db032e6e202af8ae764f` |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.c387bc0e3d25/ime-keyboard-screenshot.png` | 201,166 | `e731b550690890b50fe872c36605bd58f312248271786d62c07a08ee01fe6e0c` |

Source review explains why these observations cannot reject the repair.
`note-draft` and `agent-prompt` are distinct keyed `text_session` nodes;
`note-draft` is explicitly autofocus while the composer is not. Android turns
each node into its own `NativeTextInput`. Mouse-down, GPUI focus, and Android
virtual-accessibility focus all route the selected node ID through the same
singleton `GpuiImeBridge`, whose generation selects the active keyed editor.
The visible caret reflects GPUI's internal focus handle, whereas `mServedView`
reflects Android's independent View focus/input-method state, so a caret in the
autofocused note can coexist with a decor-only served view. No captured bridge
log marker identifies `agent-prompt`, and no served `BridgeConnection` exists
in this attempt. Treating this as proof that the deferred/lifecycle-aware show
request failed was therefore rejected. No speculative source change, second
sideload, credential access, live-provider request, or agent submission was
made; the configured credential was untouched.

**Authoritative exact-composer retest:** The corrected focus-only retest on the
same installed revision selected the one visible editable node labeled
`Ask SOS to change this experience` and tapped its live bounds
`[132,2274][998,2398]`. The screenshot clearly places the blue caret in that
lower agent composer. Android accessibility still reports `focused=false`, but
that is a separate virtual-accessibility focus domain and does not contradict
GPUI editor focus. More importantly, `dumpsys input_method` reports both
`mServedView` and `mNextServedView` as `GpuiImeBridge`, with
`mServedInputConnection` backed by
`GpuiImeBridge$BridgeConnection`. This proves composer activation and rejects
the earlier invalid-target interpretation for the corrected attempt. The same
dump records the bridge at off-window bounds `-2,-2--1,-1`,
`mInputShown=false`, and no keyboard; limited logging contained none of the
expected informational markers. No text was entered, no prompt/model request
began, and the configured credential was untouched.

| Path | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.c387bc0e3d25/ime-retest.raw.txt` | 1,862 | `31a35d47b6d418804e95e29cd2a94d0839ef0aff9c455a8e857c51b87878e085` |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.c387bc0e3d25/ime-retest-screenshot.png` | 199,817 | `8d0006c002452fad15e9fa2e128a9aaf817c28c1d098c6ad2584b78fb111092e` |

Both file sizes and hashes were rechecked on the host. Together with source
inspection, the served connection rules out failure to select the composer,
missing View focus, editor classification, input-connection creation, Rust
state routing, or provider execution as the immediate cause. The robust
boundary is instead a real transparent in-window editor anchor plus a request
made only after that anchor has a visible layout and window token.

**Bounded host evidence / failures / rejected approaches:** Compat-configured
offline Gradle Java compilation initially failed after 5.82 seconds because a
diagnostic used the unavailable `Rect.flattenToShortString()` method. Replacing
it with the API-compatible `Rect.toShortString()` fixed the only compilation
error; the repeat
`:app:compileDebugJavaWithJavac` passed in 5.10 seconds with only the existing
SDK-XML, Java 8 source/target, and deprecated-API warnings; the final compile
after the nonzero-alpha visibility and Activity-identity guards also passed in
4.59 seconds with the same warning classes. `git diff --check` passed. No
focused JVM test was added because the bridge behavior depends on
Android ViewRoot/IMM/window-insets lifecycle and this Gradle project has no
Robolectric or instrumentation test harness; the bounded Java compilation and
static diff check are the available host checks. Leaving the editor off-window
and adding more timing delay was rejected because the corrected retest already
shows a served connection at invalid geometry. `SHOW_FORCED` was rejected
because it can outlive the editor lifecycle. `SHOW_IMPLICIT` was rejected for
this direct user interaction in favor of Android's ordinary explicit flags.
An opaque or touch-consuming proxy editor was rejected because GPUI remains the
sole visible editor and pointer target. Changing the Rust composer, composing/
selection/commit routing, generated Luau, provider runtime, shared agent
contract, or Core keyboard was rejected because none is implicated by the
served bridge and no provider request began.

**Host artifact result:** A bounded non-device transaction then ran
`./tools/a33xctl build-compat1`, which exited 0 in a measured 212.628470
seconds, and `./tools/a33xctl inspect-compat1`, which exited 0 in a measured
16.993600 seconds. Total measured wall time was 229.622070 seconds. The
inspector passed, and the resulting OTA was preserved byte-identically outside
mutable product output at
`/home/carlid/dev/lineage-a33x/sos-compat-ime-anchor-artifacts-20260817/lineage_sos_compat_a33x-ota.zip`.
Its revision is `sos.compat1.0212677d7038.20a182effe2d`; it is 1,042,130,049
bytes with SHA-256
`e667611d46f6b319775f899cc71cdb63c5b2ec82c648a0b3327cf530e05dc4f2`.

The finalized host evidence is:

| Path | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/sos-compat-ime-anchor-artifact-20260817/build-compat1.raw.txt` | 1,541,725 | `941a46db0fff3b66f580b09c60b55b766f10ea6c5ede511aeb7466966fc81d8e` |
| `/tmp/sos-compat-ime-anchor-artifact-20260817/inspect-compat1.raw.txt` | 27,796 | `bb3c304736d93e79f3ba44005859d9683b4e7ee64411e07ca870349e6ed6deb7` |
| `/tmp/sos-compat-ime-anchor-artifact-20260817/manifest.tsv` | 841 | `33aea4eb0d24afe8858e5ba5879e8a44c8e95de4248a3b42b547a1e2005bd979` |
| `/tmp/sos-compat-ime-anchor-artifact-20260817/manifest-verify.raw.txt` | 107 | `adb8f42fe1e07e72a3c9fd075e6e17ab79c9280006ba209d5927130ded821aca` |

The deterministic manifest excludes itself and temporary outputs, and its
listed paths, byte sizes, and SHA-256 values were independently verified.
Initial and final Git status were identical, and `git diff --check` passed.
The transaction performed no device action, credential access, or live-provider
request. These host results approve only the exact preserved artifact for a
later authorization decision; hardware IME acceptance remains open.

**Decision / remaining risk / next gate:** Adopt the transparent in-window
anchor and layout/token-aware explicit show as the narrow Compat repair. Host
compilation plus exact artifact build and inspection do not prove OEM IME
presentation, committed-text delivery, request execution, lifecycle behavior,
latency, stability, or soak, and no hardware PASS is claimed. The next gate
requires fresh authorization naming the exact preserved artifact above and the
exact target serial. The device gate must use the live semantic composer
bounds; require the new in-window bridge bounds, `ime_activate`,
`ime_input_connection_created`, and `ime_show_requested` diagnostics,
`GpuiImeBridge$BridgeConnection` served state, `mInputShown=true`, a visible
keyboard, and the caret in the lower composer; and audit crashes and relevant
enforcing AVCs. It must not enter text, access the credential, or invoke a live
provider until focus-only presentation passes and a separately authorized
continuation permits one nonsecret prompt.

## 2026-08-17 — Restore the Compat system IME service

**Goal / corrected root cause:** Restore text entry without restoring Android's
system experience. The autonomous `sideload-auto-reboot` transaction passed for
installed Compat revision `sos.compat1.0212677d7038.20a182effe2d`, and the exact
composer retest proved that the in-window `GpuiImeBridge` was focused and served
at `0,0-1,1` with `GpuiImeBridge$BridgeConnection`. The authoritative
`dumpsys input_method` state instead had an empty `Input Methods:` list,
`mSelectedMethodId=null`, empty `mLastEnabledInputMethodsStr`, `No input method
service`, `mInputShown=false`, and `mImeHiddenByDisplayPolicy=true`. No prompt or
model request ran and the configured credential was untouched. The finalized
evidence supplied to this source change is:

| Path | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.20a182effe2d/ime.raw` | 1,165 | `fa87bd492bcc9a25a9ef2bef51b943101e0716dc73ca4e77d2af8bab0f196ade` |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.20a182effe2d/ime.after` | 6,306 | `82974b56e1320d5b1189b8018de3f283c9c8357eb30d7164f15967803322a02a` |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.20a182effe2d/ime-visible.png` | 200,397 | `cbbd23fb9595456ed43f1085dd63beb860749a405ff1a88c4fd2ba4ba8933d2f` |

The regression is in product composition, not editor presentation. LatinIME
and the original bridge both existed when the bridge landed in `d2844f2` on
August 8. Commit `74a6c23b` moved Compat onto the shared native-host/UI-removal
composition on August 16, and the Core 0B/Core 1 split around `f4d7800` retained
that shared inheritance. The `sos-ui-removal-marker` overrides `LatinIME`, so
Compat 1 silently lost its only installed IME while Compat 0 retained Android
ceremonies and LatinIME. This explains why repeated bridge geometry, request
timing, provider, model, and credential hypotheses could not create a keyboard:
there was no input method service for Android to bind.

**Changed / architecture:** The native host common fragment now owns only the
host, runtime, and pre-unlock property. Compat 1, frozen Core 0B, and active
Core 1 explicitly select their removal marker in their product makefiles. Both
marker modules share one defaults list, but Core's stable
`sos-ui-removal-marker` additionally overrides LatinIME; Compat selects
`sos-compat-ui-removal-marker`, installed under its distinct natural module
filename, and retains exactly LatinIME from the inherited Android UI list. Compat 0 is
unchanged. `inspect-compat1` now requires
`PRODUCT/app/LatinIME/LatinIME.apk`, records its size/hash, and uses `aapt2` to
verify package `com.android.inputmethod.latin`, application direct-boot
awareness, absence of a shared UID, and the exported
`android.permission.BIND_INPUT_METHOD` service plus `android.view.InputMethod`
action. It still rejects every other removed UI APK. Core 0B/Core 1 inspectors
continue to reject LatinIME with the full removal list.

The speculative in-window/deferred-show implementation is removed in favor of
the proven `d2844f2` bridge behavior. Only independently useful lifecycle
safety remains: destruction drops the static Activity-bound bridge and resets
the inset observer so a recreated Activity installs a fresh observer, and a
stale Activity cannot deactivate the new bridge. Composing, selection, commit,
submit, and inset delivery are otherwise unchanged.

**Platform review / rejected framework change:** At boot, Lineage's
`InputMethodManagerService` user initialization queries the installed IME
services into the method map. `onUserReadyLocked` passes
`resetDefaultEnabledIme=true` when `DEFAULT_INPUT_METHOD` is empty, enabling the
default system IME; `updateInputMethodsFromSettingsLocked` then calls
`chooseNewDefaultIMELocked` when no method is selected. Its package monitor has
the corresponding rebuild/reset/select path for a package change after system
ready. Therefore the only restored system IME is auto-enabled and selected even
when secure settings were empty. No settings-writing app is justified.
LatinIME's manifest marks its application direct-boot-aware and its exported service requires
`BIND_INPUT_METHOD`; it declares no `sharedUserId`. Compat's WindowManager
membrane matches only `session.mUid == SYSTEM_UID` for system-window types, so
LatinIME's application-UID IME window is not silenced. No guessed
`TYPE_INPUT_METHOD` exception or framework marker was added.

**Bounded host checks / decision / next gate:** `bpfmt -d` produced no diff;
`bash -n tools/a33xctl` and `git diff --check` passed. A static product-graph
check proved that the Core and Compat removal sets differ only by LatinIME and
that each profile explicitly selects the intended module. The new `aapt2`
assertions passed against the locally built LatinIME intermediate. Compat
`:app:compileDebugJavaWithJavac` passed in 0.631 seconds with only the existing
Java 8/deprecated-API warnings. No full product build, device action,
credential access, or live-provider request ran, and no hardware PASS is
claimed. Next run, from `/home/carlid/dev/sos`, exactly:

```text
./tools/a33xctl build-compat1
./tools/a33xctl inspect-compat1
SOS_ENABLE_LEGACY_CORE0B_BUILD=1 ./tools/a33xctl build-core0b
./tools/a33xctl inspect-core0b
./tools/a33xctl build-core1
./tools/a33xctl inspect-core1
```

Preserve each inspected OTA outside mutable output with revision, byte size,
and SHA-256 before requesting a new serial/artifact-specific hardware gate. The
hardware gate must first prove that LatinIME is installed, automatically
enabled/selected, bound under its application UID, visibly shown for the exact
GPUI composer, and commits nonsecret text with no Android system Activity,
crash, or relevant enforcing AVC; live-provider execution remains a later,
separately authorized continuation.

## 2026-08-17 — Separate Compat and Core removal-marker install paths

**Goal / failed build evidence:** Complete the LatinIME profile split without
allowing two globally defined Soong modules to emit the same installed output.
The bounded host-only `build-compat1` attempt failed after 130.561 seconds while
generating Kati install rules. Its finalized raw output is
`/tmp/sos-ime-profile-split-artifact-20260817/build-compat1.raw.txt`, 670,454
bytes, SHA-256
`72336f2c91b374a74b6381f68fd38f2ec0f8b6ae9aaaa2ad70d9679eb3c00714`.
The generated `installs-lineage_sos_compat_a33x.mk` had duplicate rules at
lines 279262 and 279298 because both `sos-compat-ui-removal-marker` and the
globally defined Core `sos-ui-removal-marker` resolved to
`system_ext/bin/sos-ui-removal-marker`. The build stopped before producing or
inspecting an artifact; no Core build or device action occurred.

**Diagnosis / correction:** Product selection controls which marker is
packaged, but it does not make globally visible module install-rule definitions
mutually exclusive. The Compat marker no longer overrides its stem, so it now
installs naturally as `system_ext/bin/sos-compat-ui-removal-marker`; Core keeps
`system_ext/bin/sos-ui-removal-marker`. `inspect-compat1` requires the Compat
path and rejects the Core path, while the Core 0B/Core 1 inspector requires the
Core path and rejects the Compat path. The shared override defaults still omit
LatinIME, only the Core marker adds it, Compat 1 explicitly selects only the
Compat marker, Core 0B/Core 1 explicitly select only the Core marker, and
Compat 0 plus the shared native-host fragment select neither. A bounded
`check-product-graph` preflight enforces those stem, override, and selection
invariants before any expensive profile build work.

**Completed host validation / rejected preservation attempt:** The static
`check-product-graph` gate passed. The first resumed transaction also completed
the Compat build and inspection, but its preservation step searched for the
generic Lineage 23 package name and targeted an absent destination parent.
That attempt was rejected and no artifact from it was accepted. The corrected
resume selected the exact `lineage_sos_compat_a33x-ota.zip` output and preserved
it before the Core `installclean`, preventing the profile transition from
invalidating the accepted Compat result.

The corrected Compat build exited 0 in 350.663 seconds and `inspect-compat1`
exited 0 in 17.5302 seconds. Inspection required LatinIME and the distinct
Compat marker, rejected the Core marker and every other removed Android UI
package, and passed the remaining Compat package checks. The preserved result
is:

| Product | Stable artifact | Revision | Bytes | SHA-256 |
| --- | --- | --- | ---: | --- |
| Compat 1 | `/home/carlid/dev/lineage-a33x/sos-ime-profile-split-artifacts-20260817/compat/lineage_sos_compat_a33x-ota.zip` | `sos.compat1.0212677d7038.553ffbcc2487` | 1,065,840,611 | `c6c7a2d15645dee16e3477cec5ff5a769ea7326175f0e4e3ccaf81c5924edcca` |

The Core 1 build then exited 0 in 348.825 seconds and `inspect-core1` exited 0
in 16.6857 seconds. Inspection required the Core marker, rejected both the
Compat marker and LatinIME, and passed the no-Zygote lifecycle, native
credential ceremony, shared runner, pinned model, and SELinux gates. The
preserved result is:

| Product | Stable artifact | Revision | Bytes | SHA-256 |
| --- | --- | --- | ---: | --- |
| Core 1 | `/home/carlid/dev/lineage-a33x/sos-ime-profile-split-artifacts-20260817/core1/lineage_sos_core1_a33x-ota.zip` | `sos.core1.0212677d7038.4d2f2592bb98` | 1,022,150,772 | `0a12fb9abefed4f5dd465d57d763f51034189f3ba91b26b06db24ac53cd48e7b` |

The finalized host evidence is:

| Path | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/sos-ime-profile-split-artifact-20260817/resume-attempt-2/build-compat1.raw.txt` | 1,559,910 | `3028dfedbd1b9bdb4b0a27bba28231a97875b7bb7f0da87d5dafabaaa43f5d7f` |
| `/tmp/sos-ime-profile-split-artifact-20260817/resume-attempt-2/inspect-compat1.raw.txt` | 28,223 | `50cfeccc0e574adc3ef3fd209deff82b039ed19b2282720b32a8677516d5bac7` |
| `/tmp/sos-ime-profile-split-artifact-20260817/resume-attempt-3/build-core1.raw.txt` | 3,122,545 | `7ca74017fec38b24f61dee92c74ef83edcc691ca55a7f8b0a83f2398d1f1730f` |
| `/tmp/sos-ime-profile-split-artifact-20260817/resume-attempt-3/inspect-core1.raw.txt` | 24,028 | `2abbc7363b86b76aac16c80d05e8c6e0f187297986a22735ba0280fe84590475` |
| `/tmp/sos-ime-profile-split-artifact-20260817/resume-attempt-3/manifest.tsv` | 1,494 | `f04b4277489e53bc89d0cc69cf7a2902f4018357878812e012041f5231d10707` |

The deterministic manifest is sorted, excludes itself and temporary outputs,
and was independently verified. Initial and final Git status were identical,
and `git diff --check` passed. No device action occurred.

**Decision / remaining risk / next gate:** Host validation now accepts the
distinct marker filenames as the narrow graph correction; the LatinIME
root-cause solution and every other Compat/Core removal override remain
unchanged. This does not establish hardware IME presentation, text commit,
lifecycle stability, crash/AVC absence, or soak, and no hardware PASS is
claimed. The next gate requires fresh authorization naming the exact target
serial and preserved artifacts above. Its product order is Compat 1 followed
by Core 1, leaving Core 1 as the final installed state.

## 2026-08-17 — Combine the Compat IME service and in-window editor repairs

**Goal / conclusive installed evidence:** Close the source diagnosis for the
remaining Compat composer keyboard failure without changing Core's native
keyboard. The autonomous monitored transport for the preserved Compat artifact
above passed through `device→sideload→device`, including the inherent reboot,
with `Total xfer: 1.00x`; the installed revision was exactly
`sos.compat1.0212677d7038.553ffbcc2487`. This establishes the monitored
transport transition only. The earlier device-gate manifest was not
independently verified and therefore cannot support an overall PASS.

The first focus attempt targeted the wrong, upper editor and is rejected as an
IME acceptance gate. In the corrected retest, the runner selected the exact
lower composer node from its live bounds and tapped `(565,2335)`. The resulting
screenshot places the caret in that lower composer. Android reports selected
and current `com.android.inputmethod.latin/.LatinIME`, serves
`GpuiImeBridge` through `GpuiImeBridge$BridgeConnection`, and has
`mInputStarted=true`; LatinIME reports `mIsInputViewShown=true`. Nevertheless,
system-server reports `mInputShown=false`, no keyboard is visible, and the
served bridge remains outside the window at `-2,-2--1,-1`. Thus the two
independent regressions are now demonstrated together: Compat had removed its
system IME, and the restored IME still rejects the zero-alpha, off-window
editor under the current server/display policy. No text, prompt/model request,
credential action, or soak occurred.

| Corrected evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.553ffbcc2487/monitored-retry/correct-composer-retest/preflight.txt` | 456 | `f5c8eb58e26dada0afa0d537d160041c90461f48f42aed1587bbb54ed5240581` |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.553ffbcc2487/monitored-retry/correct-composer-retest/pre-tap.xml` | 5,148 | `d33983b97459cf21281479ef0e064a7f222709ad670d8483d470ea31823bedb5` |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.553ffbcc2487/monitored-retry/correct-composer-retest/tap-event.txt` | 109 | `ee2b7bcbce30bdff432635d288adcac9ce8d22e6edfe1f04ea7f3f138992cba4` |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.553ffbcc2487/monitored-retry/correct-composer-retest/post-tap-state.txt` | 1,191 | `d42dc71605b09e72ba59141c07ca0bd61f8eab60de289aec6bb1780080080333` |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.553ffbcc2487/monitored-retry/correct-composer-retest/post-tap.png` | 180,343 | `86249d49dff6dce9d4d1970949a8bf3c6a1675ea6ca6e79804e0664df3611457` |

**Changed / combined decision:** Keep the validated product split: Compat
retains the application-UID, direct-boot-aware LatinIME service while Core's
removal marker continues to exclude it. Restore the bounded in-window bridge
solution now justified by the corrected hardware evidence. `GpuiImeBridge`
uses an empty, transparent, non-interactive 1×1 editor at visible origin
`(0,0)` with nonzero framework alpha. It never consumes touch dispatch. A show
request is valid only for the singleton's current Activity, node, focus, and
activation generation, after attachment, non-null token, layout, visible
bounds, window focus, a generation-matched input-connection handoff, and
`InputMethodManager.isActive`. Activation and attachment restart input;
replaceable callbacks from activation, attachment, layout, window focus, and
connection creation issue the ordinary explicit `showSoftInput(..., 0)` plus
the owning Activity window's `WindowInsetsController` request. No
`SHOW_FORCED` or unbounded repost loop is used. Deactivation and Activity
destruction advance the generation, cancel queued work, clear the connection
generation and focus, hide the IME, and destruction also removes the inset
observer. Existing bounded composing, selection, commit, deletion, submit, and
`MAX_UTF16` routing is unchanged.

Nonsecret `ime_activate`, `ime_input_connection_created`, and
`ime_show_requested` logs expose request acceptance and controller request
state plus the anchor's attachment, token, shown/alpha/layout/focus, size,
visible bounds, and IMM-active state; they never include editor text.
`inspect-compat1` now requires those diagnostics and the explicit
`in_window_nonzero_alpha_noninteractive` bridge contract in the packaged
Compat APK, so the stale bridge cannot pass inspection. Core inspection and
the shared agent/provider implementation are unchanged.

**Bounded source checks:** Compat-configured Gradle
`:app:compileDebugJavaWithJavac` passed in 0.76 seconds with only the existing
Java 8/deprecated-API warnings. `:app:assembleDebug` plus extraction of every
new inspector marker from `classes*.dex` passed in 1.30 seconds.
`bash -n tools/a33xctl` passed in 0.00 seconds,
`./tools/a33xctl check-product-graph` passed in 0.02 seconds, and
`git diff --check` passed in 0.01 seconds. No Blueprint file was changed by
this repair, so `bpfmt` was not required. No full product build, device action,
credential access, provider request, or hardware PASS was performed or is
claimed for the new source.

**Final host artifact evidence:** The bounded product transaction then ran the
static product graph, the Compat build, and the Compat inspector. The product
graph exited 0 in a measured 0.021397 seconds, `build-compat1` exited 0 in
225.993626 seconds, and `inspect-compat1` exited 0 in 17.437225 seconds. The
inspector required and passed LatinIME, the distinct Compat removal marker,
Core-marker exclusion, and the packaged in-window bridge contract plus all
nonsecret diagnostic markers.

The exact product OTA was preserved byte-identically outside mutable output:

| Product | Stable artifact | Revision | Bytes | SHA-256 |
| --- | --- | --- | ---: | --- |
| Compat 1 | `/home/carlid/sos-final-compat-artifacts-20260817/lineage_sos_compat_a33x-ota-compat1.0212677d7038.46f7415f7285.zip` | `sos.compat1.0212677d7038.46f7415f7285` | 1,065,887,744 | `797e94d06ca7f23ca442c72d078c9f6ca2cf2bfe25b0d5490f1b00d5411fb1db` |

Finalized evidence is rooted at
`/tmp/sos-final-compat-artifact-20260817/`. Its deterministic,
self-excluding `evidence.manifest` is 3,140 bytes with SHA-256
`9e9d1145928d04d524d984cfef882c7ae7a1c43cc2fbe5ce0001b83a37e01541`
and was independently verified. The separate verifier output is
`/tmp/sos-final-compat-manifest-verify-20260817.stdout`, SHA-256
`afdfc799c80103650c09ec6a07ddf19fcbacfa01de651692af331b67ec586f17`.
Initial and final Git status and diff captures were identical, and
`git diff --check` passed. This transaction performed no device action or live
provider request and does not establish hardware IME acceptance.

**Remaining risk / next gate:** Obtain fresh authorization naming the exact
artifact above and target serial, then use monitored autonomous
`device→sideload→device` transport. The device gate must derive the exact lower
composer tap from live bounds; require selected/current LatinIME, an in-window
`0,0-1,1` served bridge connection, all three bridge log families with viable
anchor/IMM state, `mInputShown=true`, and a visibly shown keyboard; then perform
the separately bounded nonsecret live request, credential clearing, crash/AVC
audit, and soak. No hardware or live-provider PASS is claimed here.

## 2026-08-17 — Repair exact 0731 agent execution and Compat input lifecycle

**Goal / installed finding:** Turn the physical composer result from installed
Compat revision `sos.compat1.0212677d7038.46f7415f7285` into one bounded
Compat/Core runtime repair. Physical touch focused the lower composer, opened
LatinIME, and allowed the user to type and submit `Set darkmode`. One trusted
agent effect committed, but the capture contains no
`android_agent_pi_response`, provider completion, or enduring Node child, and
the UI briefly presented a Pi error. The credential was subsequently removed
through the trusted UI; the post-clear state has no child or agent service.
This proves physical text focus/input/submission and a failed request, but it
does not prove an HTTP response or status.

The confirmed configuration defect is the shipped OpenRouter model ID. The
user required `deepseek/deepseek-v4-flash-0731`, while the shared runner,
Compat bridge, Core contract/UI, inspectors, tests, and product documentation
all pinned the distinct older bare `deepseek/deepseek-v4-flash` ID. All product
paths now use only the exact `-0731` ID. Exact equality and exact-line artifact
inspection prevent the older ID, `latest`, `:free`, arbitrary models, or a
prefix/suffix variant from satisfying the contract.

**Failure observability / secrecy decision:** The shared runner now converts
provider exceptions into an allowlisted stage/category, fixed safe message,
exact nonsecret model, and optional validated numeric HTTP status. Compat and
Core propagate only that structure. Effect dispatch, agent-thread/request
start, child start/exit/response type, successful response provider/model, and
sanitized failure stage/category/model/status have nonsecret lifecycle
markers. Stderr remains drained/discarded and never surfaced. No credential
bytes, headers, prompt or request source, response/candidate source, raw
provider body, stderr, or arbitrary exception text enters error UI or logs.
Routine two-second status polling now preserves a surfaced request error; an
intentional new request, provider action, or credential change clears it.
Local launch, child/linker-or-exit, response I/O/protocol, timeout,
credential/provider rejection, tool-sequence, model, and candidate-validation
failures are distinguishable without weakening credential secrecy.

**Compat input and automation decision:** A capture-phase left-tap policy now
checks all live text-session bounds. A real tap outside every session blurs
GPUI focus, which runs the existing bridge deactivation, clears native input
ownership, and hides LatinIME. Tapping the active editor keeps focus; tapping
another editor is left to normal GPUI focus transfer and keeps/reopens the IME.
This policy is excluded from `core-native`, preserving Core's explicit native
keyboard. Editable Android virtual nodes now expose `ACTION_CLICK` as a
deterministic input-focus route in addition to `ACTION_FOCUS`;
`ACTION_ACCESSIBILITY_FOCUS` no longer doubles as input focus. The existing SOS
overlay Back still injects `KEYCODE_BACK`, so Android gives an open IME first
consumption before app navigation; the packaged marker records this platform
key-dispatch precedence and no persistent Android navigation bar was added.

The earlier automated focus evidence establishes only a boundary. The capture
does not contain the actual `adb input` argv or exit status. Runner-described
coordinates `(565,2376)` and later `(565,2300)` were respectively at/near the
reported composer bounds, both post-states were unfocused with no IME, and no
native `scene_pointer` marker arrived. No accessibility `ACTION_FOCUS` or
`ACTION_CLICK` was attempted; only UI hierarchy dumps were taken. Edge/inset
placement, gesture interception, overlays, and Android injection behavior
remain hypotheses. The later physical touch did produce pointer down/up plus
`native_text_focus agent-prompt=true`. Consequently this change does not claim
an Android InputDispatcher root cause and semantic automation remains a
separate acceptance path, not a substitute for physical touch.

| Focused evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.46f7415f7285/physical-composer-touch/post-owner-submit/request-window.txt` | 30,866 | `762279d535e751566cf14eb5a397515b23ce24438b40728822484ac30c505fdf` |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.46f7415f7285/physical-composer-touch/post-clear/post-clear-state.txt` | 1,520 | `ba7a4261d7079449247789a3cf7c91b787ffd094997011376ceacbdbef6ab96e` |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.46f7415f7285/physical-composer-touch/post-clear/post-clear.png` | 182,550 | `b163385543a34a5aaca037bf0a2188efcd32aae96b4d0eaf1544eb05997432e7` |
| `/tmp/sos-live-agent-compat-sos.compat1.0212677d7038.46f7415f7285/physical-composer-touch/post-clear/post-clear.xml` | 5,154 | `7ce3cc0766df940a6dae8440a390de8a356aaf0f5c05659bb7b9ee93e3714784` |

**Bounded host evidence:** `npm test` rebuilt the shared runner and passed all
12 tests, including exact-model rejection and a secret-shaped provider failure
whose output retained only category/status. Compat-configured Gradle
`:app:compileDebugJavaWithJavac` passed with only the existing Java 8 and
deprecated-API warnings. `cargo check -p sos-experience --tests` passed. The
two exact-model/persistent-error tests and the keep/transfer/outside-blur test
then ran and passed with temporary empty host linker stubs for this
workstation's absent `libxkbcommon` and `libxkbcommon-x11`. Both ARM64 checks
passed: `cargo ndk -t arm64-v8a -P 31 check -p
sos-experience --locked --no-default-features --features aosp-system` and the
corresponding `core-native` check. The ordinary host `cargo test -p
sos-experience` reached final linking but could not run because this
workstation still lacks `libxkbcommon` and `libxkbcommon-x11`; the focused
changed contracts passed with the temporary stubs, and both Android variants
compiled. Compat ARM64 clippy with `--no-deps -- -D warnings` passed. Core's
same strict clippy invocation reached an existing unrelated
`android/provider_client.rs` `needless_return`; ordinary Core checking passed
and that unrelated file was not changed. Compat `:app:assembleDebug` passed,
and direct DEX-string checks found the semantic-click, platform-Back
precedence, bridge request, child start/exit/response-type, and sanitized
failure markers. `cargo fmt --all -- --check`, `bash -n tools/a33xctl`,
`./tools/a33xctl check-product-graph`, and `git diff --check` passed. No full
product build, device action, credential use, live provider
request, or hardware PASS occurred in this implementation phase.

**Remaining risk / next gate:** Build and inspect fresh exact Compat 1 and Core
1 artifacts and record each revision, byte size, and SHA-256. Authorize and
sideload them separately. On Compat, require both physical touch and semantic
editable `ACTION_CLICK` to focus, compose/commit/submit correctly; outside tap
must emit blur/deactivation and hide LatinIME, another editor must transfer
focus without losing the IME, and overlay Back must first dismiss an open IME.
For each profile, configure one credential through its trusted ceremony, issue
one live `deepseek/deepseek-v4-flash-0731` request, require request/child/error
or completion markers with the exact model, successful candidate validation
and activation, then clear the credential. Finish with required revision and
surface/process readiness, crash and enforcing-AVC scrutiny, deterministic
evidence manifests, and soak. Separate artifact/serial authorization envelopes
remain mandatory; no new artifact or device acceptance is claimed here.

## 2026-08-17 — Correct the bundled-runner model inspector

**Goal / failed host gate:** Repair an inspection false negative discovered
after a fresh Compat 1 build, without changing the already-correct packaged
agent runtime. The build completed in 218.904 seconds and produced revision
`sos.compat1.0212677d7038.9f8b4d9f48e4`. The following inspection ran for
18.395 seconds and then rejected the shared runner's OpenRouter model check.
No device, credential, provider request, sideload, reboot, or other hardware
operation occurred.

**Diagnosis / rejected inference:** This was not stale packaging. The packaged
`/system_ext/etc/sos-agent/agent-runner.cjs` is byte-identical to
`services/sos-agent/dist/agent-runner.cjs`, and the bundle contains the exact
assignment
`var PINNED_OPENROUTER_MODEL = "deepseek/deepseek-v4-flash-0731";` plus the
decoder guard that rejects an OpenRouter request whose model differs from that
constant. `tools/a33xctl` incorrectly piped a JavaScript text bundle through
`strings` and required the model slug to occupy an entire output line with
`grep -Fx`; the actual output line is the complete assignment, so a correct
bundle could never satisfy that assertion. Conversely, globally forbidding
the older bare slug in this bundle is invalid: Pi's legitimate provider model
catalog includes metadata for other OpenRouter models even though SOS request
decoding and execution enable only the exact pin. Catalog presence is not
request authority.

**Changed / decision:** `check-agent-runner-contract` now accepts either the
repository bundle or an explicit packaged-bundle path. It requires exactly one
`PINNED_OPENROUTER_MODEL` assignment, requires that complete assignment to be
the exact `-0731` value, explicitly rejects an assignment to the older bare
value, and requires the compiled OpenRouter request guard to compare against
the pinned constant. Compat inspection still first proves byte identity with
the repository bundle, then invokes this assignment/guard check. It no longer
mistakes unrelated catalog metadata for an enabled SOS request model.

The separate gate manifest failure was operational: the runner recipe will use
a fresh evidence root and keep the independent verifier output outside the
manifested tree. No gate-generated `/tmp` evidence was edited, and no new
repository manifest generator was added.

**Bounded host evidence / next gate:** `bash -n tools/a33xctl`, direct
`./tools/a33xctl check-agent-runner-contract
services/sos-agent/dist/agent-runner.cjs`, and `git diff --check` pass. Re-run
`inspect-compat1` against the already-built revision above, record its exact
artifact path, byte size and SHA-256 in a fresh finalized evidence root, and
independently verify that root from an external verifier output. Only after
that host PASS should the gate continue with the planned fresh Core 1
build/inspection and separately authorized Compat/Core device campaigns. This
entry does not claim a product-inspection, device, live-provider, or hardware
PASS.

## 2026-08-17 — Correct the Core Rust-rodata model inspector

**Goal / r2 gate result:** Continue the non-device agent-runtime product gate
after correcting the bundled JavaScript inspector. Compat 1 inspection passed
in 18.219 seconds. The verified stable artifact is:

| Product | Stable artifact | Revision | Bytes | SHA-256 |
| --- | --- | --- | ---: | --- |
| Compat 1 | `/home/carlid/sos-agent-e2e-artifacts-20260817-r2/compat1/lineage-23.0-20260817-UNOFFICIAL-sos_compat_a33x.zip` | `sos.compat1.0212677d7038.9f8b4d9f48e4` | 1,066,479,708 | `f46e9959c705c09494cef84ac0bace3287c3634ecc18709762250dcb20176810` |

The subsequent Core 1 build passed in 226.212 seconds and produced revision
`sos.core1.0212677d7038.4db4c5c7d680`. Core inspection then failed after
15.712 seconds with `GPUI runtime omitted the pinned OpenRouter campaign
model`. No device, credential, provider request, sideload, reboot, or other
hardware operation occurred, so neither product has a new hardware PASS.

**Diagnosis / rejected inference:** The fresh ARM64
`libsos_core_experience.so` does contain the exact literal
`deepseek/deepseek-v4-flash-0731`. Raw byte matching finds it. Rust placed
adjacent constants in the same rodata run, so `strings` emits the model inside
longer lines such as the concatenated OpenAI/Codex/OpenRouter model table and
the credential-dialog copy. The inspector's `strings | grep -Fx` therefore
rejected correct bytes. A raw absence assertion for
`deepseek/deepseek-v4-flash` is also invalid because that bare text is a prefix
of the required `-0731` literal; it cannot establish which model is enabled.

**Changed / decision:** Core artifact inspection now uses raw fixed-byte
matching for the complete `deepseek/deepseek-v4-flash-0731` literal and also
requires the compiled wrong-response-model rejection marker. It makes no
standalone-string assumption and removes the invalid prefix-absence check.
Exact enablement remains enforced at the authority boundary by the Rust
`model_is_exact` equality contract and its rejection tests, the Core request's
fixed `OPENROUTER_MODEL`, response-model equality, and the shared bundled
runner assignment/decoder guard. This changes artifact observation only; it
does not weaken runtime equality.

**Evidence / next gate:** The r2 manifest at
`/home/carlid/sos-agent-e2e-artifacts-20260817-r2/manifest.tsv` is 1,892 bytes
with SHA-256
`9fb55eb4ac94a93749275e421b18e98231e2ca52539a16f6e524a84bd33ed6e3`.
Independent verification passed for its 20 finalized files; the external
verifier output `/tmp/sos-agent-e2e-artifacts-20260817-r2-manifest-verify.txt`
is 112 bytes with SHA-256
`29c47091ff245061575805700996c172f51b6c71575ef02be846ca411b2276de`.
No gate evidence was edited. `bash -n tools/a33xctl`, the direct Core runtime
contract check against the current built ARM64 library,
`check-agent-runner-contract` against the current bundle, and `git diff
--check` pass. Re-run `inspect-core1` against the existing r2 build, then
preserve and identify the Core OTA and produce a new finalized evidence root
plus external verifier output. Only a complete host PASS may advance to the
separately authorized Compat and Core device campaigns described above.

## 2026-08-17 — Accept exact Compat/Core agent-runtime host artifacts

**Goal / r3 gate result:** Close the host artifact phase with stable,
identity-checked and product-inspected Compat 1 and Core 1 OTAs after the
host-only Core checker correction. Core identity passed in 0.634 seconds and
Core inspection passed in 16.714 seconds. The accepted artifacts are:

| Product | Stable artifact | Revision | Bytes | SHA-256 |
| --- | --- | --- | ---: | --- |
| Compat 1 | `/home/carlid/sos-agent-e2e-artifacts-20260817-r2/compat1/lineage-23.0-20260817-UNOFFICIAL-sos_compat_a33x.zip` | `sos.compat1.0212677d7038.9f8b4d9f48e4` | 1,066,479,708 | `f46e9959c705c09494cef84ac0bace3287c3634ecc18709762250dcb20176810` |
| Core 1 | `/home/carlid/sos-agent-e2e-artifacts-20260817-r3/core1/lineage-23.0-20260817-UNOFFICIAL-sos_core1_a33x.zip` | `sos.core1.0212677d7038.4db4c5c7d680` | 1,022,134,360 | `581085630aec57adfd93a93cc6fd428080d192447c403ba9856293f45246d2ef` |

Compat was rechecked as the already accepted r2 artifact; Core passed after
replacing the invalid standalone-`strings` assumption with the corrected
compiled-runtime check described above. This is a host artifact PASS only. No
device or live-provider operation occurred, no credential was used, and no
hardware or provider success is claimed. Model cost was unavailable.

**Evidence / decision:** The finalized r3 manifest
`/home/carlid/sos-agent-e2e-artifacts-20260817-r3/manifest.tsv` has SHA-256
`84718809aec189b71e4a1eb08c877bfa16b6c3668f76a867185f514b2a95031b`.
Independent external verification passed; verifier output
`/tmp/sos-agent-e2e-artifacts-20260817-r3-manifest-verify.txt` has SHA-256
`6aac729ae6c6456f4e3bc90d15e6f0258928de7088ddd5a5310563dca46956b0`.
No evidence file was edited. Accept both exact OTAs for the next gated phase;
do not substitute a rebuilt or differently identified artifact.

**Next gate:** Obtain separate, artifact-exact device authorizations for
serial `RFCT50EGFCN`, Compat first and Core second. Each authorization covers
only its named OTA and one sideload attempt with its inherent reboot and
readiness/soak observation. Re-establish the product-specific readiness and
crash/AVC criteria, verify physical and semantic composer focus plus outside
tap blur/focus transfer/Back behavior, run one exact
`deepseek/deepseek-v4-flash-0731` live request, confirm successful candidate
activation, clear the credential, and complete the required soak. Until those
separate device gates pass, hardware behavior and provider completion remain
open risks.

## 2026-08-17 — Invalidate the Compat device gate at the input boundary

**Goal / result:** Exercise the separately authorized Compat 1 artifact on
serial `RFCT50EGFCN`. The exact installed revision was
`sos.compat1.0212677d7038.9f8b4d9f48e4`. The runner autonomously entered
Recovery, performed the single authorized sideload, observed its inherent
reboot, and passed exact revision, Android boot-complete, SOS HOME, LatinIME
selection, and relevant enforcing-AVC readiness criteria. The measured gate
duration was 390.256 seconds; model cost was unavailable.

The gate is invalid because execution crossed its instructed pre-owner input
boundary. After the synthetic tap, the runner issued
`adb -s RFCT50EGFCN shell input text TEST` and
`adb -s RFCT50EGFCN shell input keyevent KEYCODE_DEL` despite the instruction
not to enter prompt text. Both commands exited 0 and no text appeared. No
credential was entered, no provider request or prompt submission occurred,
and no extra reboot or soak occurred. Device ownership was released.

**Focus evidence / confidence boundary:** The exact synthetic tap was
`adb -s RFCT50EGFCN shell input tap 565 2376`; it exited 0. The composer
accessibility bounds were `[132,2352][998,2400]`, but afterward the IME still
reported `mInputShown=false`, there was no served editable view, and the
fallback/served view was the DecorView. The evidence contains neither a
semantic accessibility `ACTION_CLICK` attempt nor a native pointer-delivery
log. Therefore it does not establish an Android injection, InputDispatcher,
or SOS scene-routing root cause. Bottom-edge clipping or interception is the
leading hypothesis because the composer occupied the final 48 pixels, but it
remains unresolved rather than a product conclusion.

**Evidence / decision:** The finalized evidence root is
`/home/carlid/sos-agent-e2e-device-compat-20260817-r1`; its manifest has
SHA-256
`d1909dc93dde03b5ab5f206298b31e271f787189d443e3c268d5f594c2c6ac4c`.
Independent external verification passed; the verifier output has SHA-256
`2e304ef3829abd23a4d68155cc64512ccc906d468c7a4aef115f5055987a692c`.
No evidence file was edited. Preserve the readiness observations, but reject
this run as acceptance evidence for focus, IME behavior, provider completion,
credential handling, or soak.

**Next gate:** Obtain new explicit authorization for the same installed
revision on `RFCT50EGFCN`. Before owner interaction, allow only the precisely
listed input commands needed to capture pointer/InputDispatcher logs and
compare a mid-screen note tap with a fully scrolled, safely interior composer
tap; synthetic text and key commands are forbidden. Do not infer delivery
from command exit status alone. If that bounded diagnostic completes without
crossing the boundary, request owner-approved physical focus, outside-tap,
focus-transfer, Back, exact `deepseek/deepseek-v4-flash-0731` request,
candidate activation, credential-clear, crash/AVC, and soak checks.

## 2026-08-18 — Preserve Compat IME focus transfer and bind Pi evidence to authority commit

**Goal / terminal Compat evidence:** Close two gaps found on exact Compat
revision `sos.compat1.0212677d7038.9f8b4d9f48e4`. The finalized evidence root
is `/home/carlid/sos-agent-e2e-device-compat-20260817-r2`. One live exact
`deepseek/deepseek-v4-flash-0731` response took 128.017 seconds and produced a
validated/submitted candidate plus a visible dark UI. Physical field focus,
outside-tap dismissal, and SOS Back dismissing the IME first passed. Transfer
between two native fields failed because the old field's blur hid/deactivated
the IME before the new field's focus reopened it. The capture also lacked
explicit ordered `get_experience_context -> validate_experience ->
submit_experience` markers and a durable activation-commit marker, so neither
action order nor authority commit is promoted to PASS from the visible UI.
The credential was cleared, no agent child or service leaked, the measured
soak was 300.006615154 seconds, and no crash, ANR, or relevant enforcing AVC
was present. Model cost was unavailable. The manifest SHA-256 is
`c767e3ed8b1abc79b007f582ef79042a2bf47dfd2632511e007108ef663216e4`;
the external verifier SHA-256 is
`521f0a6e6761aa2a7ff701964e6cb0325bf74bfb62773ad9cbe4f12937313601`.

**Changed / decision:** Compat input now uses a small epoch state machine at
the GPUI focus boundary. Blur schedules end-of-effect-cycle resolution;
same-transition focus of another field invalidates that blur epoch and updates
the existing Android editor/input connection without hiding the keyboard. A
blur with no succeeding field focus resolves to bridge deactivation and
keyboard hide. The outside-tap classifier and Android-first Back dispatch are
unchanged, and the state machine is excluded from `core-native`, leaving the
fixed Core keyboard behavior unchanged. Contract tests cover active-field
retention, transfer, genuine outside blur, wrong-owner blur, and stale epochs;
an immediate hide with a guessed delay was rejected as timing folklore.

The shared Rust agent boundary now accepts action evidence only when the
entire fixed three-step allowlist is present exactly once and in order. Only
after that verification does it emit bounded ordinal action markers containing
provider/model/action names and no prompt, response body, candidate source, or
credential. Agent-origin candidates carry a typed submitted/validated/staged/
committed evidence state. The validated marker follows the Luau worker's
compile/render/scene acknowledgment; the staged marker follows revision
installation plus provider-state staging; the commit marker follows exact
revision/source/state reconciliation from the system authority after the
presented frame. Manual and reload candidates cannot emit these agent markers,
and failed, missing, reordered, extra, merely validated, or merely staged
flows cannot claim commit. Compat and Core share these Rust authority paths.
`tools/a33xctl` now requires all action/validation/stage/commit markers in both
packaged runtimes and requires the Compat focus-transition markers.

**Bounded host evidence:** Seven focused contract tests passed using temporary
empty host linker stubs for this workstation's absent `libxkbcommon` and
`libxkbcommon-x11`: four model/action/activation tests and three Compat
tap/focus-epoch tests. `cargo check -p sos-experience --tests`, ARM64 Compat
`aosp-system` checking, and ARM64 `core-native` checking passed. Compat ARM64
strict clippy (`--no-deps -- -D warnings`) passed. Core strict clippy reached
only the pre-existing unrelated `android/provider_client.rs:142`
`needless_return`; ordinary Core checking passed and that file was not changed.
Compat-configured Gradle `:app:compileDebugJavaWithJavac` passed with the
existing SDK XML, Java 8 source/target, and deprecated-API warnings. Final
`cargo fmt --all -- --check`, `bash -n tools/a33xctl`, `./tools/a33xctl
check-product-graph`, `./tools/a33xctl check-agent-runner-contract
services/sos-agent/dist/agent-runner.cjs`, and `git diff --check` passed. The
debug ARM64 Compat binary build passed, and raw compiled-library checks found
both focus lifecycle markers plus every ordered-action/validation/stage/commit
marker; the corresponding Core build and marker checks also passed. The
full 14-test host run passed 13 tests but retained the unrelated dirty-worktree
failure in `embedded_experience_is_valid`: the current provider contract
rejects `audio.set_volume`; all seven focused changed contracts passed. No
full AOSP/Soong build, device action, live provider request, or hardware PASS
occurred in this implementation phase.

**Remaining risk / runner recipe:** Build and inspect fresh ARM64 Compat 1 and
Core 1 artifacts, record each exact path/revision/byte size/SHA-256, and create
separate authorization envelopes for serial `RFCT50EGFCN`; do not reuse the
old artifacts as acceptance for this change. Run Compat first with one
authorized sideload and inherent reboot. Require exact Android/Compat
readiness, then physically transfer focus in both directions between two
native fields without any intervening inactive/hide/inset-zero marker or
visible IME close, while outside tap still deactivates/hides and Back still
dismisses the IME before SOS navigation. Issue one exact 0731 request and
require ordinals 1/2/3 for the allowlisted actions, then validation, staging,
authority commit for one request in order; reject missing, duplicate,
reordered, or commit-before-ack evidence. Clear the credential, prove no child
or service leak, complete crash/ANR/AVC scrutiny and the specified soak, close
all files, generate the deterministic manifest atomically, and independently
verify it. Only after Compat closes, authorize the separately identified Core
artifact and repeat the exact-model/action/validation/stage/authority-commit,
credential-clear, process, crash/AVC, manifest, and soak gate using Core 1's
no-Zygote readiness predicates. Hardware acceptance remains open until those
fresh, separate gates pass.

## 2026-08-18 — Accept r4 host artifacts for focus/evidence device gates

**Goal / result:** Build, identity-check, and product-inspect fresh Compat 1
and Core 1 artifacts containing the focus-lifecycle and authority-bound Pi
evidence changes above. Both host artifact gates passed. This is a host PASS
only: no device, credential, live-provider, sideload, reboot, or hardware
operation occurred, and model cost was unavailable.

| Product | Stable artifact | Revision | Bytes | SHA-256 | Build | Inspect |
| --- | --- | --- | ---: | --- | ---: | ---: |
| Compat 1 | `/home/carlid/sos-agent-e2e-artifacts-20260818-r4/compat.ota.zip` | `sos.compat1.0212677d7038.f3ccf618c623` | 1,066,449,144 | `0c2574caa1577f800095aed13190e1b9c9ee0ea492d57b3a3c8423cd32443b49` | 306.134556 s | 19.443815 s |
| Core 1 | `/home/carlid/sos-agent-e2e-artifacts-20260818-r4/core.ota.zip` | `sos.core1.0212677d7038.26ce1cb8445d` | 1,022,145,556 | `b14e4579ec12aa60673b63c225f7bc2ad3031c667aac0ece6ad54a2671de6b21` | 301.832817 s | 17.398108 s |

**Evidence / decision:** Compat inspection passed retained-LatinIME, exact
`deepseek/deepseek-v4-flash-0731`, Compat focus-lifecycle markers, ordered Pi
action-sequence markers, candidate validation/stage/commit markers, native
runtime, packaging, and boot-contract checks. Core inspection passed the same
exact-model/action/activation evidence boundaries, required LatinIME absence,
the native credential path, native runtime/packaging contracts, and Core 1
no-Zygote gates. The finalized r4 manifest at
`/home/carlid/sos-agent-e2e-artifacts-20260818-r4/manifest.tsv` has SHA-256
`6e2910978c4f3037c2ccbf00440decb93faea2562c6e5923e2f1f04f7b15d4eb`
and passed independent external verification. Repository state remained
identical across the gate and `git diff --check` passed. No evidence file was
mutated. Accept only these exact artifacts for the next gates; a rebuilt,
renamed-with-different-content, or otherwise differently identified artifact
requires a new host gate and authorization envelope.

**Remaining risk / next gate:** Obtain separate artifact-exact authorization
for serial `RFCT50EGFCN`, Compat first and Core only after Compat ownership is
released. The Compat envelope must name `compat.ota.zip`, revision
`sos.compat1.0212677d7038.f3ccf618c623`, byte size 1,066,449,144, and SHA-256
`0c2574caa1577f800095aed13190e1b9c9ee0ea492d57b3a3c8423cd32443b49`;
it covers one sideload attempt, its inherent reboot, Android/Compat readiness,
physical bidirectional field-transfer/outside-blur/Back checks, one exact 0731
request with ordered action/validation/stage/authority-commit evidence,
credential clearing, process-leak and crash/ANR/AVC scrutiny, manifest
verification, and soak. After that gate closes, separately authorize
`core.ota.zip`, revision `sos.core1.0212677d7038.26ce1cb8445d`, byte size
1,022,145,556, and SHA-256
`b14e4579ec12aa60673b63c225f7bc2ad3031c667aac0ece6ad54a2671de6b21`
for one sideload attempt, its inherent reboot, Core 1 no-Zygote readiness, the
same exact-model/action/authority evidence, credential clearing, process and
crash/AVC scrutiny, manifest verification, and soak. Neither hardware gate is
yet claimed.

## 2026-08-18 — Pass loaded-runtime Compat focus and exact Pi authority E2E

**Goal / result:** Exercise the exact accepted Compat 1 runtime revision
`sos.compat1.0212677d7038.f3ccf618c623` on physical hardware. The loaded
runtime E2E gate passed: exact SOS/Android readiness and retained LatinIME were
present; traced focus transfer in both directions between the two GPUI native
fields kept the IME lifecycle active; an outside tap resolved to deactivation
and keyboard hide; and SOS Back retained Android-first IME consumption.

One live `deepseek/deepseek-v4-flash-0731` request completed with child exit
code 0. The bounded evidence then recorded the exact verified sequence
`get_experience_context`, `validate_experience`, `submit_experience` as
ordinals 1, 2, and 3; candidate validation and staging explicitly remained
noncommitted; the system authority subsequently acknowledged commit; and the
result became visible. Full request latency was unavailable, so it is not
inferred. The measured child-exit-to-authority-commit interval was 0.138
seconds. Model cost was unavailable. The credential was cleared, no Node, Pi,
or agent child/service leaked, and no crash, ANR, or relevant enforcing AVC
was present.

**Soak / evidence:** The monotonic soak ran from `1787035095.107690333` to
`1787035398.454924965`, a measured 303.347 seconds. The finalized 22-file
evidence root is
`/home/carlid/sos-agent-e2e-device-compat-20260818-r3`; its manifest has
SHA-256
`a26ae630d8e367243f39ba4a52b21424c832236411773f49887258611e72ca33`.
Independent external verification passed, and the verifier output has
SHA-256
`3e3291750ae33c809297e28c08b0b3e87bcfc2d5545885395b14661eea4b3ac1`.
No evidence file was edited. Accept this as the terminal hardware PASS for the
loaded Compat runtime's focus lifecycle, outside dismissal, Android-first
Back, exact-model action order, validation/staging confidence boundary,
authority commit, visible activation, credential clearing, leak checks, and
soak.

**Install-transport confidence boundary:** The autonomous install transaction
is PARTIAL, not PASS. Its capture omitted both the initial Recovery-entry
command and the final sideload exit/`Total xfer` evidence. The owner manually
entered sideload transport and later booted the OS. Exact installed revision
identity proves which runtime produced the functional evidence, but it does
not retroactively prove an autonomous Recovery/sideload/reboot lifecycle.
Do not reuse this run as transport-automation acceptance.

**Decision / next gate:** Preserve the loaded-runtime Compat PASS while
keeping install transport separately open. Audit the actual autonomous
procedure and exercise it only within a future exact artifact/serial
authorization envelope that captures Recovery entry, sideload exit and
transfer completion, inherent reboot, and readiness without owner-assisted
transitions. Independently obtain new explicit authorization for the accepted
Core artifact `/home/carlid/sos-agent-e2e-artifacts-20260818-r4/core.ota.zip`,
revision `sos.core1.0212677d7038.26ce1cb8445d`, byte size 1,022,145,556, and
SHA-256
`b14e4579ec12aa60673b63c225f7bc2ad3031c667aac0ece6ad54a2671de6b21`;
run its separate one-sideload Core 1 no-Zygote readiness, exact Pi authority,
credential-clear, leak/crash/AVC, manifest, and soak gate. No Core hardware
claim is made here.

## 2026-08-18 — Harden Core Node startup and reduce credential transcription risk

**Goal / terminal Core r1 result:** Diagnose and remediate the failed exact
Core 1 live-agent request without granting executable-memory authority or
introducing remote secret injection. The installed artifact was
`/home/carlid/sos-agent-e2e-artifacts-20260818-r4/core.ota.zip`, revision
`sos.core1.0212677d7038.26ce1cb8445d`, 1,022,145,556 bytes, SHA-256
`b14e4579ec12aa60673b63c225f7bc2ad3031c667aac0ece6ad54a2671de6b21`.
Sideload, Core no-Zygote readiness, and credential cleanup passed, but the
exact live request did not. Request handling started at `09:15:34.679`; the
Node child as `sos_core_host` was denied reading `/proc/meminfo` at `.783`,
then denied `{ execmem }` on its own process at `.787`, and the old generic
`stage=protocol category=request_failed` marker followed at `.845`. That is a
measured 0.166 seconds from request start to failure marker, not provider
latency. The finalized evidence root is
`/home/carlid/sos-agent-e2e-device-core-20260818-r1`; `manifest.tsv` has
SHA-256
`cb562456f7d2b40368d48c46b655bf15a7943f720b83ddf3a8aea9b64c2ab0c2`.
The external verifier `/tmp/sos-core-manifest-independent-20260818-r1.txt`
recorded SHA-256
`f9a3c9047a4b0a205305aec48a541dc6a9c655023f6e989458e83b3bf9e7b6d8`.
This remains a terminal Core request failure, not a hardware or provider PASS.

**Changed / decision:** The immutable AArch64 `sos-node` prebuilt contains
the supported `--jitless` option and V8's `wasm_jitless` path. Core now places
`--jitless` before the fixed runner script in a tested argument contract and
emits a sanitized `hardening=jitless` launch marker. This avoids V8's JIT and
WebAssembly executable-code path rather than allowing `execmem`. Node also
reads the system memory total during fixed startup, so Core policy adds only
`open read` on `proc_meminfo`; it adds no map, ioctl, write, append, watch, or
executable-memory permission. A new source check and built-artifact inspector
require the argv ordering, prove the packaged Node supports `--jitless`,
require the hardened runtime marker and exact meminfo rule, and reject any
compiled Core-host process `execmem` allow. A dedicated child domain was
rejected for this milestone: the fixed signed Node/runner already inherits the
intended Core host boundary, and jitless removes the exceptional authority
that caused startup to fail. Hardware evidence must still prove this inherited
domain is sufficient without another relevant AVC.

Core child handling now maps launch, timeout, request I/O, response I/O,
observation I/O, unsuccessful numeric exit, signal, empty successful response,
and invalid response to distinct machine-readable safe categories. Exit and
signal reports contain only numeric `child_exit` or `signal` metadata. A valid
sanitized runner error still preserves its allowlisted provider category and
bounded HTTP status. Child stderr remains discarded, and no stderr, provider
body, request or response body, key, payload, argv secret, or environment
secret is logged or returned.

The trusted Core OpenRouter ceremony remains memory-only, non-persistent,
masked, and zeroized on cancel/exit. It now prefills and protects the fixed
`sk-or-v1-` prefix, groups suffix mask characters in fours, and displays the
suffix character count, reducing long-key transcription and deletion errors
without reveal, clipboard, history, network, ADB/USB injection, or any change
to generic text fields or Compat's IME. Broad `execmem`, remote secret entry,
temporary plaintext reveal, and the separate Compat InputMethodService
milestone were explicitly rejected.

**Bounded host evidence:** `cargo check -p sos-experience --tests` passed.
Direct isolated Rust test binaries passed all six agent-contract tests and all
four credential-ceremony tests, including exact argv order, distinct safe
failure mappings, prefix protection, grouped counting, replacement, cancel,
clear, and zeroized lifecycle behavior. ARM64 Android
`cargo check -p sos-experience --target aarch64-linux-android
--no-default-features --features core-native` passed with the pinned NDK 29
Clang/LLVM tools supplied through target-specific compiler, archiver, and
linker variables. `bash -n tools/a33xctl`, `./tools/a33xctl
check-core-agent-hardening`, `cargo fmt --all -- --check`, and `git diff
--check` passed. The ordinary host `cargo test -p sos-experience --lib`
compiled the changed crate but could not link because this workstation lacks
`libxkbcommon` and `libxkbcommon-x11`; the focused standalone tests above ran
the changed contracts instead. No Soong/product build, device operation,
credential use, or live provider call occurred.

**Remaining risk / next gate:** Rebuild and inspect Core 1 only: the runtime,
Core credential UI, and Core SELinux policy changed, while all additions to
shared Rust contracts are gated to `core-native` and no Compat packaged input
changed. Run `./tools/a33xctl check-core-agent-hardening`, then
`./tools/a33xctl build-core1` and `./tools/a33xctl inspect-core1`; preserve the
new OTA and record its exact revision, byte size, and SHA-256 in a finalized,
independently verified host evidence root. Do not reuse the r4 Core artifact.
After a new artifact/serial authorization envelope, run one Core sideload and
inherent reboot, require exact Core no-Zygote readiness, enter a real key only
through the physical trusted ceremony, and issue one exact
`deepseek/deepseek-v4-flash-0731` request. Require the jitless launch marker,
successful child/response/action/validation/stage/authority-commit evidence,
no relevant AVC (especially `execmem` or `proc_meminfo`), credential clearing,
no child leak, crash scrutiny, deterministic manifest verification, and the
specified soak. This implementation does not claim that hardware PASS.

## 2026-08-18 — Core r2 DNS remediation and development-only credential injection

**Goal / terminal r2 evidence:** The exact Core artifact
`/home/carlid/sos-agent-e2e-artifacts-20260818-r2/core.ota.zip`, revision
`sos.core1.e05f91bb6f0b.f73060a1ee5a`, 1,022,153,785 bytes, SHA-256
`c2bb7b842c4bfde49205fb474e272dc25d164f7a61d79c4af24a29aacc0ee96e`,
passed its one-sideload transport and Core no-Zygote readiness gate. The
sideload transfer took 77.093500 seconds, return-to-ADB took 87.813800 seconds,
and readiness inspection took 0.828757 seconds. The exact
`deepseek/deepseek-v4-flash-0731` request then failed from request start at
`10:32:36.127` to the sanitized failure marker at `10:32:36.693`, a measured
0.566 seconds. No required action, validation, stage, authority-commit, or UI
marker followed and no child remained. The old `execmem` and `proc_meminfo`
denials were absent, but the Node `MainThread` in `sos_core_host` was denied
`{ read }` on `net_dns_prop` at `10:32:36.659`. The runner incorrectly surfaced
that terminal connection startup failure as `stage=protocol
category=tool_sequence`.

The finalized evidence root is
`/home/carlid/sos-agent-e2e-device-core-20260818-r2`.
`post-request-audit-corrected.txt` is 4,813 bytes with SHA-256
`f27a546508832acd3f93bfcbea0ca3462b69a9670f39f1d330045c1a1c3b2480`;
`failure-followup-audit.txt` is 3,868 bytes with SHA-256
`c163a8488156f91716c3af751bd8843295d3566f9dd390ebaba8759ba5fcf03e`.
The credential entered for r2 remains only in the running old Core process and
still requires cleanup by the device owner; no host-side credential or device
operation was performed during this remediation.

**Changed / decision:** Core SELinux now uses only
`get_prop(sos_core_host, net_dns_prop)` for Bionic/Node resolver startup. It
adds no DNS-property write, broader property family, data/network authority,
or executable-memory permission. The shared Pi runner now inspects Pi's
terminal assistant error before deciding whether the authoring tool sequence
is structurally missing and performs a fixed OpenRouter hostname lookup before
provider execution so resolver startup has an unambiguous category.
DNS-resolution and connection failures are separate
sanitized `transport/dns_failure` and `transport/connection_failure`
categories; HTTP/provider failures remain provider categories, Rust-owned
process launch/I/O/exit/signal failures remain child categories, malformed
wire responses remain protocol categories, and `tool_sequence` is used only
when there is no prior terminal transport error and the actual bounded tool
order is invalid or incomplete. Raw error text, provider body, stderr,
request/response body, payload, and credential remain excluded from logs and
UI errors.

Long manual entry of a disposable OpenRouter key is no longer required for
development artifacts. `build-core1-dev` enables a dedicated Rust feature and
the precise `SOS_ENABLE_CORE_DEV_CREDENTIAL_BUILD=1` product switch; the
product makefile rejects that switch outside `userdebug`/`eng`. It alone
packages `sos-core-dev-credential` and `ro.sos.dev_credential=1`. The endpoint
also requires both that property and `ro.debuggable=1`, binds one dedicated
abstract local socket, authenticates exact ADB shell uid 2000 plus
`SO_PEERSEC=u:r:shell:s0`, accepts only a versioned set/clear frame of at most
512 credential bytes with two-second I/O timeouts, serializes updates through
the existing `CredentialState` mutex, sets the existing changed marker, and
zeroizes bounded key buffers. Clear does not depend on request success and is
available while the Core experience remains running.

`core1-dev-set-openrouter-key` validates the active Core 1 revision/product and
development gates, selects exactly one ready ADB device unless `--serial` is
given, disables terminal echo, accepts one line through stdin, and streams it
to the dedicated client. The key is never placed in process argv, shell
history, environment, properties, files, clipboard, screenshots, or normal
logs. Set/clear return fixed secret-free acknowledgements only.
`core1-dev-clear-openrouter-key` uses the same fail-closed product and peer
boundary. Ordinary `build-core1` omits the Rust endpoint feature; ordinary
inspection rejects the endpoint marker, client binary, and enabling property,
while the development inspector requires all three. SELinux grants shell only
`connectto` in userdebug/eng and carries an explicit user-build neverallow.
`adb shell input text`, temporary/persistent files, clipboard transport,
system properties containing the key, build-time embedded credentials, and a
general-purpose debug control plane were rejected.

**Bounded host evidence:** `npm test` rebuilt the shared agent bundle and
passed all 14 tests, including secret-safe DNS/connection classification.
`cargo check -p sos-experience --tests` passed. The exact ARM64 Android feature
combination passed `cargo ndk -t arm64-v8a -P 31 check -p sos-experience
--no-default-features --features core-native,core-dev-credential`. The pinned
AOSP Clang `-std=c++20 -Wall -Werror -Wextra -fsyntax-only` check passed for
the dedicated client. `cargo fmt --all -- --check`, `bash -n` for the CLI and
host fixtures, and `git diff --check` passed. A focused host `cargo test`
compiled the changed Rust tests but could not link because the workstation
lacks unversioned `libxkbcommon` and `libxkbcommon-x11`; no test assertion
failed. No Soong/full product build, device operation, or provider call was
performed, so no hardware PASS is claimed.

**Remaining risk / next gate:** A Core-only rebuild is sufficient: only the
Core runtime, Core product composition, its dedicated client, shared runner,
and Core policy changed. First run the focused host checks and host CLI mock,
then `./tools/a33xctl build-core1-dev` and
`./tools/a33xctl inspect-core1-dev`; preserve the OTA with exact revision,
size, and SHA-256 in a finalized independently verified evidence root. Also
build/inspect ordinary Core 1 once to prove the client, property, and endpoint
are absent. After a new exact artifact/serial authorization envelope, perform
one sideload and inherent reboot, require exact Core no-Zygote readiness, then
use the hidden one-paste set command and one exact live request. Require the
secret-free set marker, no credential bytes in argv/properties/files/logs,
successful child/response/action/validation/stage/commit/UI evidence, no
relevant AVC (especially `net_dns_prop` or `execmem`), no child residue, then
run the dedicated clear command even if the request fails and require only its
secret-free clear marker. Finish crash/leak scrutiny, deterministic manifest
verification, and the authorized soak.

## 2026-08-18 — Confine Core agent DNS and reject obsolete net.dns policy

**Goal / failed host gate:** Make the Core DNS remediation compatible with
Android's platform SELinux invariants without weakening enforcement or
granting network authority to the trusted render/compile/activation host. The
first ordinary Core product build stopped at the platform neverallow before
producing an OTA; no artifact or device operation followed. Evidence is under
`artifacts/host-gates/core-dev-20260818-01`. `build-core1.txt` is 530,178
bytes, SHA-256
`75278daed28fd61517bf457134501354772354f0dd8f06b1ed67290b17bd6abb`,
and records a measured 211.570464128-second build. Lines 344–370 show
`system/sepolicy/private/domain.te:2037` rejecting the expanded
`allow sos_core_host net_dns_prop:file { read getattr map open }` in both
Recovery and system_ext policy compilation. `process-preflight.txt` is 4,063
bytes with SHA-256
`6f8f3a2d2d6813f343db0eb8470e06067a8927e4b9be0d8fa9fd6e38c9360cca`.
The finalized `host-evidence-manifest.tsv` was independently verified.

**Architecture / decision:** The platform rule says that `net.dns*` is being
removed and permits file reads only to init, system_server, vendor_init, and
dumpstate. `sos_core_host` was already a `coredomain`, so adding or moving the
same property rule to another ordinary coredomain cannot satisfy that
contract. Core now transitions only the immutable `sos-node` entrypoint from
`sos_core_host` into the dedicated `sos_core_agent` coredomain. The coredomain
attribute is required for a system_ext entrypoint; it is not used as a DNS
exception. The child receives only its inherited bounded stdin/stdout pipes,
the exact `proc_meminfo { open read }` startup grant, Android's supported
dnsproxyd and fwmarkd Unix-socket paths, a TCP client socket, and name-connect
to an SOS-labelled TCP 443 port. It does not receive `net_domain`, raw/UDP
networking, bind authority, arbitrary destination ports, host data, Binder,
graphics, input, platform-adapter, authority, or development credential
socket permissions. The trusted UI host loses `net_domain`, direct Node
execution, and the obsolete TCP provider/revision grants. Policy neverallows
structurally prohibit IP sockets in the host, `net_dns_prop` access in either
domain, and `execmem` in both domains.

Granting the obsolete property to a coredomain, transitioning Node while
retaining that grant, leaving `net_domain` in the host, inheriting the host
domain with `execute_no_trans`, patching the platform exception list, using a
permissive domain, or hiding the denial with a custom `dontaudit` were
rejected. The standard `domain_auto_trans` boundary retains Android's normal
secure-exec behavior; no SOS source policy adds a permissive or dontaudit rule.
The existing `--jitless`, no-`execmem`, stdin-only credential, no persistence,
peer authentication, Core-dev-only packaging/runtime gates, ordinary-Core
exclusion, and transport-before-tool-sequence error ordering are unchanged.

**Bounded host evidence:** `bash -n tools/a33xctl`,
`./tools/a33xctl check-core-agent-hardening`,
`./tools/a33xctl check-core-dev-credential`,
`bash tests/a33xctl-host-test.sh`, and `git diff --check` passed. The hardening
check now requires the coredomain transition, inherited-pipe and exact meminfo
rules, supported DNS/fwmark proxy path, TCP-443 label, and child cleanup; it
rejects host network sockets, Node `execute_no_trans`, any SOS
`net_dns_prop` allow/get/set, permissive/dontaudit source, and Core host/agent
`execmem`. The built-artifact inspector requires the compiled transition,
entrypoint, exact meminfo and DNS-proxy allows and rejects compiled host IP,
net.dns, or executable-memory authority. A focused in-memory expansion of the
changed system_ext rules passed the tree's `secilc -v -m -M true -G -c 30`
against the existing platform CIL in 4.8 seconds; no lunch, Soong, product
build, device operation, or live provider call was run. Rust, TypeScript, and
ARM64 application sources did not change in this bounded remediation, so the
prior milestone's passing focused application/ARM64 checks remain applicable.

**Remaining risk / next gate:** The focused CIL replay proves syntax and the
platform neverallow relationship but is not an ordinary product build. Both
ordinary Core and Core-dev must be rebuilt because they share this SELinux
transition; preserve and inspect each output separately. Run the source gates,
then `./tools/a33xctl build-core1` and `./tools/a33xctl inspect-core1`, followed
by `./tools/a33xctl build-core1-dev` and
`./tools/a33xctl inspect-core1-dev`. Require both builds to pass Recovery and
system_ext policy compilation, require the compiled agent transition/DNS
proxy/TCP-443 confinement, and re-prove ordinary exclusion plus development
inclusion. Finalize independently verified host manifests containing each
artifact's path, revision, byte size, and SHA-256. No hardware PASS is claimed;
only after a fresh artifact/serial authorization envelope should the device
gate test exact Core no-Zygote readiness, the development set/request/clear
lifecycle, successful live DNS/TLS/request/action/commit/UI evidence, no
relevant enforcing AVC or crash, no leaked child, and the specified soak.

## 2026-08-18 — Compile the Core-dev credential client against Bionic

**Goal / failed host gate:** Complete the paired ordinary/Core-dev host build
gate without weakening bounded secret handling. The finalized evidence root is
`artifacts/host-gates/core-dev-20260818-02`. Its 529,822-byte
`build-core1-dev.txt`, SHA-256
`69c07b68df2c678da05e3099b21d818826e1323c0c1adc767bc1e7f8456cc0ac`,
records a 203.183324584-second failed development build. Lines 399–405 are the
first actionable failure: Android clang, using `-nostdlibinc`, the Bionic
headers, and target `aarch64-linux-android10000`, rejects undeclared
`explicit_bzero` in the `SecretBuffer` destructor. The later lines 920–923
`ninja: Missing restat` observation (`Image` older than `.config`) occurred
after that compile failure while other work was still draining and is
secondary fallout, not an independently established root cause.

Ordinary Core built and inspected successfully. The preserved exact artifact
is
`artifacts/host-gates/core-dev-20260818-02/artifact-set/sos.core1.e05f91bb6f0b.294faad24721-ordinary-ota.zip`,
revision `sos.core1.e05f91bb6f0b.294faad24721`, 1,022,136,198 bytes, SHA-256
`baf89a7f8a5bc1c4fac3f67116d87684eca9a8580a6db0d613f8dbcc8bc6991`.
No development OTA was produced. The host manifests were independently
verified; no device operation or hardware PASS followed.

**Changed / decision:** The Android client now calls Bionic's
`memset_explicit`, declared by `<string.h>` since API 34, over the complete
fixed-capacity secret array in its destructor. Unlike ordinary `memset` or
`std::fill`, that API guarantees the erasure is not optimized away; the current
platform module targets API 10000, where the declaration and libc symbol are
available. The fixed 513-byte allocation, maximum 512-byte credential,
stdin-only read, short-lived stack ownership, argv/property/build gates,
secret-free output, and absence of file or log transport are unchanged.

`check-core-dev-credential` now resolves Soong's selected clang version from
the configured Lineage checkout and compiles the exact client source as an
AArch64 Android platform target with `-nostdlibinc`, platform libc++, Bionic,
libbase, the module's C++20/no-exceptions/no-RTTI configuration, and warnings
as errors. This catches missing or API-incompatible Android declarations that
a desktop-header syntax check cannot. No SELinux or product-composition file
changed in this remediation: the dedicated `sos_core_agent` DNS proxy/TCP-443
confinement, no-`execmem` invariant, ordinary production exclusion, and
explicit development injection contracts remain intact.

**Bounded host evidence:** `./tools/a33xctl check-core-dev-credential` passed
the strengthened exact-source Android/Bionic compile, and
`./tools/a33xctl check-core-agent-hardening` passed. The mock-only
`bash tests/a33xctl-host-test.sh`, `bash -n` over that test and `tools/a33xctl`,
the pinned AOSP clang-format dry run over the client, and `git diff --check`
passed. No lunch, Soong, complete product build, device operation, or provider
call was run in this remediation.

**Remaining risk / next gate:** The focused Android compile establishes the
correct declaration and frontend/module compatibility but does not produce or
link a product artifact. Rebuild and inspect both ordinary Core 1 and Core-dev
for the fresh artifact set: although the fixed client is packaged only in the
development image, the revision identity hashes the complete shared staged
device tree, so the preserved pre-fix ordinary artifact must not be paired
with the new development artifact as one post-fix source set. Run the source
gates, then `./tools/a33xctl build-core1` plus `inspect-core1`, followed by
`build-core1-dev` plus `inspect-core1-dev`; require ordinary exclusion,
development inclusion, the shared compiled policy invariants, and separately
finalized, independently verified artifact identities and manifests before a
new hardware authorization envelope.

## 2026-08-18 — Give Core-dev an immutable product identity

**Goal / terminal r3 host evidence:** Diagnose the reported Core-dev packaging
failure without accepting or installing an ambiguous artifact. The finalized
host root is `artifacts/host-gates/core-dev-20260818-03`; its 2,028-byte
`host-evidence-manifest.tsv` has SHA-256
`7738021863c33b97f375fdf7df9e6b64a9145578aaa04c56f073aedaf5c94a2c`.
The independently verified 350-byte artifact manifest has SHA-256
`40c3c4a921f41e068d5b01116ab6219d47ada10e772da6f6895b9c22c9e3c027`.
Ordinary Core built in 381.147699930 seconds and inspected in 18.361229205
seconds. Its accepted artifact is
`artifacts/host-gates/core-dev-20260818-03/artifact-set/sos.core1.e05f91bb6f0b.85ca93719b2b-ordinary-ota.zip`,
revision `sos.core1.e05f91bb6f0b.85ca93719b2b`, 1,022,166,846 bytes, SHA-256
`86d046d024075fa4f375d34accdbd6cfee09dd23be62b09565083d28b5bbc8eb`.
Core-dev built in 350.978178612 seconds and its inspection failed in
18.041304196 seconds at line 337 with the message that the client was absent.
The rejected artifact remains preserved only as
`artifacts/host-gates/core-dev-20260818-03/artifact-set/sos.core1.e05f91bb6f0b.45c3e250577d-dev-ota-rejected.zip`,
revision `sos.core1.e05f91bb6f0b.45c3e250577d`, 1,022,152,827 bytes, SHA-256
`fc30518bfca5eaea44db47195696439167c9ae4fdbde8de6d5f665b8af7b1d27`.
No device operation or hardware PASS followed.

**Root cause / decision:** The failure text was a false absence diagnosis, not
proof that Soong omitted the module: `build-core1-dev.txt` line 11342 records
`SYSTEM_EXT/bin/sos-core-dev-credential` being added to target-files. The
inspector used host `-x` on an unpacked target-files entry; those files do not
preserve the final image execute bit as their host mode. The authoritative
`META/system_ext_filesystem_config.txt` records the installed mode. The
inspector now requires a regular file at the exact system_ext path and exact
`0 2000 755 capabilities=0x0` image metadata, while ordinary inspection
requires both the file and metadata entry to be absent.

The old design still had a real reproducibility defect: ordinary and dev used
the same `lineage_sos_core1_a33x` product, target-files directory and OTA name,
with an ad hoc shell environment switch and the same `sos.core1` revision
namespace. It could overwrite one variant with the other and made product
identity depend on caller intent. Core-dev is now the registered, non-shipping
`lineage_sos_core1_dev_a33x-userdebug` product. Its makefile unconditionally
selects the dedicated client and `ro.sos.dev_credential=1`, rejects `user`, and
sets `ro.sos.build_variant=core1-dev-credential`; ordinary Core sets
`core1-ordinary` and never references the client. The CLI selects the distinct
product and `sos.core1dev.<source>.<inputs>` revision namespace in the same
branch that enables the `core-dev-credential` Rust feature. Target-files,
package names and AVB product fingerprints are consequently disjoint. The
Samsung no-Zygote selection now explicitly includes both product names.
Keeping the shared product plus a late environment variable, trusting host
mode bits, or mutating a completed image were rejected.

**Inspector and bounded host evidence:** Core inspection derives the variant
from target-files contents: exact `ro.sos.build_variant`, revision namespace,
and AVB product fingerprint, then checks the runtime feature markers, client,
filesystem metadata, development property, and the existing compiled policy
confinement. Device readiness likewise requires a consistent ordinary or dev
revision/property tuple, and credential commands accept only `sos.core1dev`
with the exact dev property plus `ro.debuggable=1`. Direct GNU Make expansion
passed for both product files, proved ordinary exclusion and exact dev
package/properties, and rejected a `user` dev expansion. The incremental
Samsung source patch applies cleanly over the existing no-Zygote patch.
`./tools/a33xctl check-product-graph`, `check-core-dev-credential` (including
the exact AArch64 Bionic compile), `check-core-agent-hardening`, the mock host
test, Bash syntax checks, `cargo check -p sos-experience --tests`, `cargo fmt
--all -- --check`, and the exact ARM64 Android
`core-native,core-dev-credential` Cargo check passed. No lunch, Soong/full
product build, device operation, credential, or provider call was run in this
remediation.

**Remaining risk / next gate:** Both images require a fresh rebuild: product
composition, image identity, inspector rules, and the staged device-tree hash
changed. Do not reuse or rename either r3 artifact, especially the rejected
dev OTA. Run the source gates, then `build-core1` plus `inspect-core1`, followed
by `build-core1-dev` plus `inspect-core1-dev`. Preserve the two distinct OTAs
and target-files roots; require ordinary exclusion, dev inclusion and 0755
filesystem metadata, exact ordinary/dev property and revision namespaces, AVB
product identity, compiled no-Zygote/policy invariants, and independently
verified manifests with path, byte size and SHA-256. Only a fresh accepted
Core-dev artifact may define a later device authorization envelope; no
hardware claim is made here.

## 2026-08-18 — Make the a33x source patch bootstrap series-aware

**Goal / terminal r4 host evidence:** Preserve the immutable ordinary/Core-dev
product split while repairing the source bootstrap failure before another full
build. The finalized and independently verified failed-gate root is
`artifacts/host-gates/core-dev-20260818-04`. Its 1,740-byte
`host-evidence-manifest.tsv` has SHA-256
`227ce91c6f8e9fa8f428ef3174673029aa9f6de2d93dc3c44518a370a7190492`;
the 217-byte artifact manifest has SHA-256
`574da9b08c3cce0523423c974e6b5149613ea7d2fb0e30880fbab65d9b1660`.
The source revision was `e05f91bb6f0b0a9299b914138d6cd0966b9c82d5`.
Product-graph, Core hardening, development-credential/Bionic compile, host
mock, Bash syntax and diff checks all passed in respectively 0.020677657,
0.029229189, 0.595108485, 0.303495013, 0.004776570 and 0.009277818
seconds. Ordinary Core built in 349.654816592 seconds and inspected in
18.077148404 seconds. Its accepted r4 artifact is
`artifacts/host-gates/core-dev-20260818-04/artifact-set/sos.core1.e05f91bb6f0b.036045ca4e01-ordinary-ota.zip`,
revision `sos.core1.e05f91bb6f0b.036045ca4e01`, 1,022,158,595 bytes,
SHA-256
`4994eaae194175a6e34610606cca9c880d39218adf6bbc15960c0ed5b29905a9`.
Core-dev then stopped before artifact creation in 0.418578584 seconds when
patch 0005 failed at `common.mk:15`. No device access or hardware claim
occurred.

**Root cause / decision:** Patches 0005 and 0008 are a valid dependent ordered
sequence: 0005 introduces the ordinary Core no-Zygote conditional and 0008
extends that exact line to the distinct Core-dev product. After the ordinary
build had applied the complete sequence, bootstrap tested 0005 alone. Neither
its forward form nor its reverse form matches the post-0008 file, so the old
per-patch reverse-check misclassified the correct final tree as a conflict.
Patch fuzz, rejects, skipped failures, ad hoc checkout edits, and order changes
were rejected.

Bootstrap now pins all three affected Lineage project revisions and all eight
patch SHA-256 values, constructs each complete per-project result in a
temporary Git index from `HEAD`, and compares only patch-owned paths with the
pinned baseline and canonical final result. A clean project receives patches
in the exact global 0001–0008 order; a complete project is an idempotent no-op;
any mixed or foreign state fails before mutation. A second preflight requires
the complete result after application. Repository product composition remains
single-owned by the checksum/delete staging overlay under
`aosp/device/sos/a33x` (`AndroidProducts.mk`, the two Core product files and
`sos_core1_common.mk`); the ordered patches own only their external Samsung,
vendor and framework checkout paths. Ordinary and development invocations
therefore share one canonical source bootstrap without overwriting or
conditionally creating product files.

Patch 0008 now records its explicit dependency on 0005; its diff is unchanged
and its new provenance SHA-256 is
`842d80ad1d2e18c2199f253f9d645cb1b92685f6633d1c2cc299278369b03c0a`.
Because build identity hashes the ordered patch inputs, both ordinary and dev
must receive fresh revisions and artifacts; the otherwise accepted r4
ordinary OTA cannot be reused as post-remediation evidence.

**Focused evidence:** `./tools/a33xctl check-patch-series` passed against the
real complete checkout and reported all eight exact ordered hashes.
`tests/a33xctl-patch-series-test.sh` sparse-cloned the pinned baseline paths
into disposable worktrees, applied the complete stack, applied it a second
time, and proved a byte-identical diff plus final canonical state. Two direct
`./tools/a33xctl apply-patches` calls also passed as no-ops on the real final
tree. `./tools/a33xctl check-product-graph`, `check-core-agent-hardening`,
`check-core-dev-credential`, the existing host mock, Bash syntax checks and
`git diff --check` passed. No lunch, Soong/full build, device command,
credential use or provider call ran in this remediation.

**Remaining risk / next gate:** The temporary-index and sparse-worktree gates
prove applicability and idempotence, not image production. Start a fresh host
evidence root and run `check-patch-series`, the patch-series test,
`check-product-graph`, `check-core-agent-hardening`,
`check-core-dev-credential`, the host mock and shell/diff checks. Then rebuild
and inspect ordinary Core with `build-core1` / `inspect-core1`, followed by
Core-dev with `build-core1-dev` / `inspect-core1-dev`. Require new distinct
`sos.core1` and `sos.core1dev` revisions, ordinary development exclusion, dev
inclusion and 0755 metadata, exact product fingerprints/properties, compiled
no-Zygote and policy invariants, and finalized independently verified
manifests before defining any hardware authorization envelope.

## 2026-08-18 — Bind Core-dev capability to its hardened product contract

**Goal / terminal r6 device evidence:** Remediate the Core-dev readiness-only
failure without enabling broad Android debugging. On serial `RFCT50EGFCN`, the
exact OTA
`artifacts/host-gates/core-dev-20260818-06/artifact-set/sos.core1dev.e05f91bb6f0b.de1c4db550bb-dev-ota.zip`,
revision `sos.core1dev.e05f91bb6f0b.de1c4db550bb`, was 1,022,156,750
bytes with SHA-256
`99f63dfac0c20b4dfd772d8522aaa9d7facd1139d81388fd1691bae7dcaaaea`.
The exact sideload passed with exit 0 and `Total xfer: 1.00x` in
85.145654875 seconds; the inherent return to the device transport passed in
79.335597623 seconds. Readiness then failed in 0.467502212 seconds only at the
Core-dev product tuple in `tools/a33xctl` lines 2208–2211. No credential set,
clear, or provider request was attempted.

The independently verified read-only diagnostic root is
`artifacts/device-gates/core-dev-20260818-06-rfct50egfcn-diag-01`.
Its 458-byte `diagnostic-manifest.tsv` has SHA-256
`2b9d9bd915bffa84f3300ce042439cb088b3ebe7218e1936842b7d9027d2dae6`.
The 4,145-byte `device-snapshot.txt` (SHA-256
`40ce97f603fda00742a42fe2eac32a5303d33cce64ad180d9c34830eef32bcc8`)
records the exact safe values: correct `sos.core1dev` revision,
`ro.sos.build_variant=core1-dev-credential`,
`ro.sos.dev_credential=1`, `ro.build.type=userdebug`, and
`ro.debuggable=0`. It also proves the client existed as executable
`0755 root:shell`, although the old image still gave it the generic
`u:object_r:system_file:s0` label. The 2,908-byte
`readiness-source-context.txt` has SHA-256
`114d20cad182a7aaaef31122cd1ae029b77d995ca8ce735973877ff9231fd246`.

**Root cause / decision:** The global value is deliberate, not a malformed
Core product. Pinned Lineage `vendor/lineage/config/common.mk` sets
`PRODUCT_NOT_DEBUGGABLE_IN_USERDEBUG := true`; the platform
`gen_build_prop.py` consequently emits `ro.debuggable=0`, disabling broad
target debugging and adb root while retaining the `userdebug` build type.
The Rust endpoint, installed client, host set/clear validation, and readiness
had incorrectly treated `ro.debuggable=1` as the development capability.
Setting that global property to 1, weakening verified boot or SELinux,
granting adb root, broadening Android debugging, or trusting caller flags and
runtime-writable properties were rejected.

Core-dev now has one exact product contract shared by the endpoint semantics
and mirrored at its C++/host boundaries: a strict
`sos.core1dev.<12-lower-hex>.<12-lower-hex>` revision, immutable
`ro.sos.build_variant=core1-dev-credential`, immutable
`ro.sos.dev_credential=1`, the registered `ro.build.type=userdebug`, and the intentional
`ro.debuggable=0` hardening assertion. Global debuggability is no longer an
enable switch. The endpoint remains compile-time gated by the distinct
`core-dev-credential` Rust feature; only the registered distinct product
packages the stdin-only client. The client now carries the dedicated
`sos_core_dev_credential_exec` image label, and readiness plus set/clear
validate its fixed path, `0755 root:shell` metadata and exact label before any
request. Ordinary `sos.core1` continues to exclude the package, feature and
property and fails every dev command.

Every product mismatch now reports only a safe marker name plus expected and
sanitized actual value, localizing revision, build type, immutable marker,
debugging posture, client presence, mode, owner, group or label failures
without printing a secret. The hidden terminal prompt, stdin-only transfer,
bounded memory-only set/clear, zeroization, peer uid plus `SO_PEERSEC`,
production policy exclusion, dedicated `sos_core_agent` DNS proxy/TCP-443
path, no host networking or `execmem`, and transport-before-tool-sequence
classification are unchanged.

**Focused host evidence:** `./tools/a33xctl check-patch-series`,
`check-product-graph`, `check-core-agent-hardening`, and
`check-core-dev-credential` passed; the last includes the exact AArch64
Bionic client compile and source proof for Lineage's deliberate debugging
posture. `tests/a33xctl-host-test.sh` passed the observed
`userdebug + ro.debuggable=0` readiness, pseudo-terminal hidden set, and clear
paths, ordinary exclusion, and safe diagnostic rejection for a wrong
revision, wrong build type, wrong marker, and missing client. A standalone
Rust product-contract binary passed 2/2 focused tests. `cargo check -p
sos-experience --tests` and the pinned NDK 29 AArch64 Android check with
`--no-default-features --features core-native,core-dev-credential` passed.
`cargo fmt --all -- --check`, Bash syntax checks, and the mock host test
passed. The ordinary `cargo test -p sos-experience --lib
core_dev_credential::tests` compiled the changed crate but could not link
because the workstation lacks `libxkbcommon` and `libxkbcommon-x11`; the
link-free check and standalone focused contract tests cover the changed Rust
logic. No product build, device command, credential, or provider call occurred
in this remediation.

**Remaining risk / next gate:** Rebuild and inspect both ordinary Core and
Core-dev. The shared system-ext SELinux type/context inputs changed even
though ordinary Core still excludes the client, and the development runtime
and client contract changed; neither the r6 OTA nor any earlier artifact can
be reused. Run `check-patch-series`, the patch-series test,
`check-product-graph`, `check-core-agent-hardening`,
`check-core-dev-credential`, the host mock, Bash/format/diff checks, then
`build-core1` plus `inspect-core1`, followed by `build-core1-dev` plus
`inspect-core1-dev`. Preserve fresh distinct OTAs and target-files evidence
with exact revisions, byte sizes, SHA-256 values and independently verified
manifests. Require ordinary exclusion and `ro.debuggable=0`; require Core-dev
feature/client inclusion, `0755 root:shell` plus the dedicated label, the
exact immutable markers and `ro.debuggable=0`. Only then authorize one fresh
Core-dev sideload/inherent reboot/readiness transaction and proceed through
set, request, clear, crash/AVC scrutiny, manifest verification and soak. This
implementation does not claim a hardware PASS.

## 2026-08-18 — Repair Core target-files SYSTEM-property inspection

**Goal / failed r7 host gate:** Preserve the hardened `ro.debuggable=0`
artifact assertion for both ordinary Core 1 and Core-dev while making the
inspector read the property from the actual staged SYSTEM partition data. The
r7 source gates and ordinary build passed, with `build-core1` exiting 0 in
299.410686949 seconds. `inspect-core1` then exited 1 after 17.447966941
seconds at `tools/a33xctl` line 1864 under `set -u`:
`system_properties: unbound variable`. Core-dev build and inspection did not
run, no artifact was accepted, and the device was untouched.

The finalized evidence root is
`artifacts/host-gates/core-dev-20260818-07`. Its independently verified
1,212-byte `host-evidence-manifest.tsv` has SHA-256
`2624b105401e801c2db9ba419207fc893ee165be70ccfab7fb7d60fb79033c91`.
The 23,454-byte `inspect-core1.txt` has SHA-256
`4a3563f0967207a63225d0eb05aca032fb9e4c9ddb3525291753396f797707cc`;
the 521-byte `transaction-result.txt` has SHA-256
`4c45018c99a9a1d1cf3307aac842097de04df1579171fd2bcb626f4c987acb30`.
The failed inspection identified the unaccepted ordinary output as revision
`sos.core1.e05f91bb6f0b.a33768b9e2f9`, 1,022,163,055 bytes, SHA-256
`eac70a0543dba71e8072223339e20249ff34621483967abf56a5a91587fdcd58`.

**Root cause / decision:** `inspect_core_stage` added a grep against
`$system_properties` but never initialized that local, so nounset stopped the
inspection before it could evaluate the image. The shared validator now
resolves exactly `SYSTEM/build.prop` below the selected target-files root,
rejects a missing or empty file with a fixed secret-safe diagnostic, copies
that staged image input into a disposable inspection directory, initializes
the checked path before use, and always removes the directory. It requires
exactly `ro.debuggable=0` for both ordinary and development variants and also
retains Core-dev's exact `ro.build.type=userdebug` assertion. It never falls
back to `SYSTEM_EXT/etc/build.prop` or a source-tree property file. The large
inspector and focused host harness source the same narrow validator.

**Focused evidence:** `tests/a33xctl-host-test.sh` passed under
`set -euo pipefail`. Its matrix exercised ordinary and Core-dev against
missing, empty, exact `ro.debuggable=0`, and weakened `ro.debuggable=1`
SYSTEM data, using target and temporary paths containing spaces and proving
the Core-dev wrong-build-type rejection and the disposable directory empty
after every pass and rejection. A direct call
of the same validator passed against r7's generated ordinary target-files.
`./tools/a33xctl check-product-graph`, `check-core-agent-hardening`, and
`check-core-dev-credential` passed. `bash -n` over the CLI, host test, and ADB
fixture, `cargo fmt --all -- --check`, and `git diff --check` passed. No full
build, complete package inspection, device operation, credential, or provider
call ran during this repair.

**Remaining risk / next gate:** This inspector-only change does not alter the
staged device tree, patch series, packaged binaries, or content-derived
revision inputs. The already-generated r7 ordinary source output can therefore
be re-inspected to prove the immediate regression without rebuilding and must
retain the same revision. It was not accepted or copied into the r7 artifact
set, however, and the paired host-gate policy may still require a fresh full
ordinary/Core-dev transaction. The runner should execute the source gates,
then `inspect-core1` against the existing ordinary output if permitted;
otherwise run `build-core1` then `inspect-core1`. Follow with
`build-core1-dev` and `inspect-core1-dev`, requiring both inspectors to prove
actual SYSTEM `ro.debuggable=0`, all ordinary exclusion and Core-dev inclusion
contracts, distinct finalized artifacts, and independently verified
manifests. This implementation does not claim an artifact or hardware PASS.

## 2026-08-18 — Give the Core-dev credential client an executable capability boundary

**Goal / terminal r8 evidence:** Repair the false Core-dev readiness failure
without broadening ADB shell or weakening the hardened product. The accepted
r8 sideload artifact was revision
`sos.core1dev.e05f91bb6f0b.98fc6094998a`, 1,022,167,071 bytes, SHA-256
`86194432936d6aa34cd344062a578549b560d403e0b4c77ad4532ed0ae7abf88`.
The transport transaction in
`artifacts/device-gates/core-dev-20260818-08-rfct50egfcn` passed, but
readiness stopped before any credential or provider request with the safe
marker `client.presence expected=present actual=missing`.

The independently verified, read-only diagnostic root is
`artifacts/device-gates/core-dev-20260818-08-rfct50egfcn-diag-01`. It proves
the exact client does exist on dm-4 at inode 29 and is byte-identical to the
host target: 51,344 bytes, SHA-256
`2c09ca31a722e51fe7a7f62b778318bf9dfa95ceabe18dc3188879f11995b5c8`.
Enforcing SELinux correctly denied shell `getattr`/`test -e` against the
dedicated `sos_core_dev_credential_exec` type. The same evidence retained the
intended global `ro.debuggable=0` and exact immutable Core-dev markers. No key
was entered, no set/clear request ran, and no provider request ran.

**Root cause / rejected approaches / decision:** Readiness incorrectly used a
shell-domain file-stat as a presence test after the image had gained its
dedicated executable label. Granting shell broad system-ext/file access,
relabelling the binary as `shell_exec`, executing the client in shell,
`permissive`/`dontaudit`, or weakening production neverallows would turn the
diagnostic around by destroying the intended boundary and were rejected.

The immutable entrypoint now performs a `userdebug_or_eng` automatic
transition from shell into the dedicated `sos_core_dev_credential` domain.
That uid-2000 domain receives only its entrypoint, inherited ADB stdio, its own
Unix stream socket, and `connectto` for the Core host. It receives no explicit
network, filesystem, property, log, Binder, service-manager, or capability
authority, and shell is unconditionally forbidden from connecting directly.
The endpoint now accepts only uid 2000 plus exact
`u:r:sos_core_dev_credential:s0`; the former shell-context allow and peer
contract are gone. The client no longer links libbase or reads properties.
The feature-gated endpoint remains the owner of the immutable product check,
so ordinary Core excludes the binary, endpoint, property, and any effective
transition while user policy omits the transition itself.

Protocol v1 gained a zero-payload `probe` operation. It authenticates the
transitioned peer, decodes a bounded frame, reaches the product-gated
endpoint, returns only `core_dev_credential=READY`, and does not call either
credential mutation path. Core-dev readiness and both set/clear commands now
run this executable handshake; ordinary readiness relies on immutable runtime
markers while the target-files inspector proves actual binary/endpoint/
property exclusion. Safe host diagnostics separately classify missing
executable, SELinux execution denial, endpoint unavailable, rejected
peer/product, and protocol mismatch. Set still reads one hidden host-stdin
line only after a successful probe, set/clear payloads remain bounded and
memory-only, and both C++ and Rust buffers retain explicit/RAII zeroization.

**Focused host evidence:** `./tools/a33xctl check-core-dev-credential` passed,
including user/userdebug macro expansion, a focused matching CIL compile with
the pinned `secilc`, policy-negative checks, exact product expansion, and the
Bionic AArch64 C++ compile. `tests/a33xctl-host-test.sh` passed readiness,
preflighted hidden-stdin set and clear, ordinary denial, and all five safe
probe-error categories. The link-independent protocol crate ran 5/5 tests,
including probe non-mutation, clear after rejected set, set/clear decoding,
exact uid/domain peer acceptance, and malformed/bounded-frame rejection. Host Rust test compilation
passed, and the pinned NDK 29 AArch64 check with
`core-native,core-dev-credential` passed after supplying the target LLVM
archiver. Bash syntax, `cargo fmt --all -- --check`, and `git diff --check`
passed. The full desktop experience test binary still cannot link because
this workstation lacks unversioned `libxkbcommon` and
`libxkbcommon-x11`; the focused executable tests and host compile cover the
changed protocol without that unrelated GUI dependency. No full build or
device operation ran for this implementation, and it does not claim hardware
PASS.

**Remaining risk / next gate:** Both ordinary Core and Core-dev require fresh
builds because the shared system-ext policy changes; Core-dev also changes the
client and endpoint payload. Run `check-patch-series`,
`tests/a33xctl-patch-series-test.sh`, `check-product-graph`,
`check-core-agent-hardening`, `check-core-dev-credential`, the protocol test,
host mock, syntax/format/diff checks, then `build-core1` plus `inspect-core1`
and `build-core1-dev` plus `inspect-core1-dev`. Require ordinary target-files
to exclude the binary, endpoint and property so no transition can be effective,
and require Core-dev target-files to contain the exact 0755 root:shell
entrypoint label, dedicated domain transition, no shell socket allow, and
probe strings. Record
fresh artifact revisions, sizes, SHA-256 values and independently verified
manifests. Only after those host gates pass should a newly authorized
Core-dev sideload/inherent reboot/readiness transaction prove
`core_dev_credential=READY` with crash/AVC scrutiny before any set/request/
clear/soak sequence.

## 2026-08-18 — Make the Core-dev v1 credential protocol one cross-language contract

**Goal / terminal r9 evidence:** Remediate the first device execution of the
dedicated Core-dev client without relaxing protocol or peer validation. The
paired r9 host gate at `artifacts/host-gates/core-dev-20260818-09` passed and
preserved the exact Core-dev OTA
`artifact-set/sos.core1dev.e05f91bb6f0b.cd7b7f965926-dev-ota.zip`, revision
`sos.core1dev.e05f91bb6f0b.cd7b7f965926`, 1,022,171,787 bytes, SHA-256
`d163849f5f314403b6d55e7ba0954d820e88a639728e2847e30d401dd7639f48`.
On serial `RFCT50EGFCN`, Recovery entry passed in 2.241820492 seconds, the one
authorized sideload passed with exit 0 and `Total xfer: 1.00x` in
78.761798839 seconds, and the inherent wait for the device passed in
79.951946469 seconds. Core-dev readiness then exited 1 after 0.559423817
seconds at the zero-payload client handshake with the safe aggregate
`endpoint.protocol expected=v1 actual=mismatch`. No credential was entered;
set, provider request and clear did not run. The finalized device root is
`artifacts/device-gates/core-dev-20260818-09-rfct50egfcn`, whose independently
verified 997-byte `device-evidence-manifest.tsv` has SHA-256
`0f98c7fe9cb213d6f76c018d337a82f203502e40a549dd326a64690c65d2d128`.

The independently verified read-only follow-up is
`artifacts/device-gates/core-dev-20260818-09-rfct50egfcn-diag-01`. Its
289-byte `device-diagnostic-manifest.tsv` has SHA-256
`3128aaac41c8fe99452c73529575ff99de8863440ea7721d5500917a8940f93f`.
The 626-byte `device-protocol-snapshot.txt` (SHA-256
`afc9eb4d8b72a1ba4dc0faf99dcfec60f9a1de2298001f1e1f68e5b54104f0e2`)
confirmed the expected Core-dev product, `userdebug`, `ro.debuggable=0`,
no-Zygote lifecycle, enforcing SELinux and required native process domains in
1.403296775 seconds. It found no endpoint/protocol/peer/socket/AVC rejection
marker and intentionally did not rerun the probe or mutate credential state.
Thus the preserved r9 evidence proves that execution reached the protocol
boundary, but cannot identify whether the aggregate came from request
rejection, bad acknowledgement magic/version/status, or short I/O.

**Root-cause boundary / rejected relaxation / decision:** A byte audit found
the checked-in C++ and Rust v1 happy-path values nominally equal—`SOSK`,
version 1, operation byte, big-endian unsigned 16-bit payload length, and a
six-byte `SOSK/version/status` acknowledgement—but found that those values and
layouts were independently declared. The Rust server also treated all bytes
through peer EOF as one frame, while the client constructed the frame through
independent writes, and the Rust-only include test never compiled or executed
the production C++ exchange. Finally, the client collapsed bad acknowledgement
magic, version, protocol-rejection status and short I/O into two aggregate
outcomes. This combination allowed the r9 boundary failure to escape host
testing and left no preserved branch-level evidence. The unavailable r9
branch is not guessed here. Accepting version 2, accepting unknown
magic/opcodes/status, changing status meanings, retrying, weakening the exact
uid/SELinux peer check, or merely relabelling the old aggregate was rejected.

`dev_credential_protocol_v1.h` is now the sole numeric wire definition and
documents explicit golden request and acknowledgement vectors. Both the
production C++ client and generated Rust constants consume it; either Cargo
package fails its build if a required definition is missing, duplicated or
non-numeric. Compile-time assertions bind the C++ array/layout sizes and
maximum payload. The client constructs and explicitly erases one bounded
contiguous request frame, writes it with partial-write handling, half-closes
its write side, reads exactly one acknowledgement with partial-read handling,
and safely distinguishes unavailable connect, short I/O, bad magic, bad
version, protocol-rejection status, unknown status, wrong peer and rejected
request. It never prints a frame or payload.

The Rust endpoint now reads an exact eight-byte header, validates magic and
version, decodes the big-endian length, bounds allocation before reading an
exact payload, requires EOF with no trailing byte, and maps malformed or
short input to the canonical protocol-mismatch acknowledgement. Probe remains
zero-payload and invokes neither set nor clear. Set remains a bounded,
zeroized, visible-ASCII `sk-or-v1-` value received only from the dedicated
client's hidden stdin; clear remains payload-free. The feature/product gate,
uid 2000 plus exact `u:r:sos_core_dev_credential:s0` peer check, dedicated
domain, no-network policy and ordinary production exclusion are unchanged.

**Focused host evidence:** `cargo test --locked -p
core-dev-credential-protocol-test --lib -- --test-threads=1` compiled the
actual production C++ client logic and ran it against the actual Rust parser
and request dispatcher over Unix streams; all 9 tests passed. They cover exact
probe/set/clear golden bytes, a synthetic non-secret set fixture with no
payload output, probe non-mutation, rejected set followed by clear,
fragmented request and acknowledgement writes, zero payload, bad
magic/version/opcode/status, oversized and short frames, every defined status
and disconnects. `./tools/a33xctl check-core-dev-credential` passed the same
cross-language suite, pinned Bionic AArch64 C++ compile, policy compile and
source/product exclusions. `tests/a33xctl-host-test.sh` passed the expanded
safe diagnostic matrix. `cargo check --locked -p sos-experience --tests` and
the pinned NDK 29 `cargo ndk -t arm64-v8a -P 31 check -p sos-experience
--locked --no-default-features --features
core-native,core-dev-credential` passed. Cargo format, Bash syntax and
`git diff --check` passed. No product build, device command, credential or
provider operation ran in this remediation, and it does not claim hardware
PASS.

**Remaining risk / next gate:** The r9 branch was not retained, so only a
fresh device run can prove the remediated binaries and, on failure, preserve
the new exact secret-free category. Rebuild and inspect both ordinary Core 1
and Core-dev because the shared Rust build input and development client/runtime
payload changed. Run `check-patch-series`, the patch-series test,
`check-product-graph`, `check-core-agent-hardening`,
`check-core-dev-credential`, the cross-language protocol test, host mock,
format/syntax/diff checks, then `build-core1`/`inspect-core1` followed by
`build-core1-dev`/`inspect-core1-dev`. Require fresh distinct revisions,
ordinary endpoint/client/property exclusion, exact Core-dev package/feature/
policy inclusion, and independently verified manifests. Only after that
paired host gate passes should a newly authorized transaction for serial
`RFCT50EGFCN` perform one exact Core-dev sideload, inherent reboot, no-Zygote
readiness and zero-payload probe. If probe passes, continue once through
hidden synthetic/disposable set, provider request, mandatory clear, crash/AVC
scrutiny, deterministic manifest verification and soak; if it fails, stop
before any credential and preserve the exact new client category plus the
server's safe rejection category. No r9 artifact may be reused.

## 2026-08-18 — Accept the exact framed Core-dev status line without rebuilding r10

**Goal / r10 gate evidence:** Determine whether the r10 readiness aggregate
was a product failure or a host acceptance failure, then repair only the
failing layer. The exact installed OTA is
`sos.core1dev.e05f91bb6f0b.45b6c9467985`, 1,022,178,758 bytes, SHA-256
`286d6ab695b1ed2871b7dbbea12d9982d14dcf4e075569fcf0cfc162c6ac0b4f`.
The finalized root
`artifacts/device-gates/core-dev-20260818-10-rfct50egfcn` records Recovery
entry in 2.359517643 seconds, one sideload with exit 0 and `Total xfer: 1.00x`
in 78.760980319 seconds, and inherent device return in 81.807627777 seconds.
Readiness stopped in 0.528203630 seconds at
`endpoint.protocol expected=v1 actual=mismatch`; no credential, provider
request, clear, extra reboot, or second sideload ran. Its independently
verified 810-byte manifest has SHA-256
`c159c07697006ab8a01e42a254f6135bf70b03f0f2205a2cac07323e5415f09c`.

The independently verified read-only root
`artifacts/device-gates/core-dev-20260818-10-rfct50egfcn-diag-01` has a
484-byte manifest with SHA-256
`0099a3c9aa8887a4f5b7edc0c2a46980c5c56740c004399d3a50cb2df4d7f72e`.
Its 1.922600322-second snapshot proves the exact revision, `userdebug` with
`ro.debuggable=0`, enforcing SELinux, expected native domains, and the
installed client and experience library byte-identical to the r10 targets:
the root:shell 0755 client labeled
`u:object_r:sos_core_dev_credential_exec:s0` is 51,296 bytes with SHA-256
`d7956d941bbb1d9c4a5e5dabff9a0c590a1933302e17bb392518512fdbbb44b9`;
the root:root 0644 library is 14,710,536 bytes with SHA-256
`c4465419d6dfae0c9b6d96b513d73eef44c719be312aea1ef6018fa7b4680a93`.
Exactly one direct zero-payload production probe returned the canonical
newline-terminated `core_dev_credential=READY`, exit 0, in 0.042109757
seconds. Its 111-byte record has SHA-256
`89fc15792bbfdbf3887c89bd1dc1553015896073e649d653de9d57576cc9f270`.
That probe is a PASS for the client/endpoint handshake only, not completion of
the remaining hardware gate.

**Root cause / change:** `tools/a33xctl` converted client output to a shell
scalar and compared it with an unframed token. That implicit trailing-newline
stripping could neither validate the protocol-required terminator nor safely
distinguish the canonical deployed reply from CRLF, missing termination,
extra lines, or binary bytes; r10 therefore became a false negative at the
host harness. One shared parser now captures stdout and stderr separately,
checks the raw stdout byte count, and accepts exactly one ASCII status token
terminated by LF or CRLF. It rejects a missing terminator, additional newline
or output, prefix/suffix/whitespace changes, NUL/binary data, wrong status,
and a nonzero client exit without printing captured output. Probe, readiness,
hidden-stdin set, and clear all use that parser. Probe failures retain the
existing secret-safe execution, presence, availability, peer, magic, version,
status, I/O, and aggregate categories.

**Host evidence / decision / next gate:** The byte-level ADB mock reproduces
r10's exact LF reply and passes it through full readiness, also accepts the
intentional CRLF equivalent, and rejects missing/double newline, prefix,
suffix, whitespace, extra output, NUL, wrong status and nonzero-exit cases.
It additionally passes CRLF probe preflight plus set/clear acknowledgements
and rejects extra clear output. `tests/a33xctl-host-test.sh`, the product,
hardening, Core-dev and cross-language protocol checks, Bash syntax, format,
and diff hygiene pass; ShellCheck remains unavailable on this host. Only host
tooling, mocks, tests and this ledger changed, so the image payload, OTA bytes
and content-derived revision are unchanged and no rebuild or sideload is
required. Reuse the currently installed exact r10 image: a runner should run
`./tools/a33xctl inspect-core1-readiness --serial RFCT50EGFCN
--expected-revision sos.core1dev.e05f91bb6f0b.45b6c9467985`, preserve and
manifest its output, and require the complete no-Zygote readiness predicates
plus the zero-payload probe. The remaining live gate is the separately
authorized hidden disposable set, provider request, mandatory clear,
crash/AVC scrutiny, deterministic manifest verification and soak; this host
repair does not claim that gate passed.

## 2026-08-18 — Move the Core-dev client to a no-PTY ADB shell transport

**Goal / continuation and diagnostic evidence:** Reuse the installed exact r10
image while determining why the corrected strict status parser still received
no acknowledgement. The independently verified continuation root
`artifacts/device-gates/core-dev-20260818-10-rfct50egfcn-cont-01` retried the
exact readiness command against revision
`sos.core1dev.e05f91bb6f0b.45b6c9467985`; it again stopped at
`endpoint.protocol expected=v1 actual=mismatch`, before any credential or
provider request. Its 449-byte manifest has SHA-256
`7a0aeb23991ccd4e4375fc78d0e2ca3eb522aca75ab97e2abe7ec9edd86e5c40`.
The continuation recipe exited under `set -e` before appending its monotonic
timing fields, so this ledger does not infer a readiness duration from the
evidence timestamps.

The independently verified diagnostic root
`artifacts/device-gates/core-dev-20260818-10-rfct50egfcn-diag-02` has a
616-byte manifest with SHA-256
`b517786c7e5beb6c5eb95827cab339818761c61d13a699cedca1ae3b31c619a7`.
The exact command `adb -s RFCT50EGFCN exec-out
/system_ext/bin/sos-core-dev-credential probe </dev/null` exited 0 in
0.041885260 seconds but produced zero stdout bytes and zero stderr bytes; both
empty files have SHA-256
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
The strict parser correctly rejected that empty response. In contrast, the
prior safe direct `adb shell` zero-payload probe returned the exact
LF-terminated `core_dev_credential=READY`, exit 0, in 0.042109757 seconds.
Together these measurements isolate the failure to `exec-out` output delivery,
not the installed r10 client, endpoint, protocol, policy, or parser.

**Change / safety decision:** Probe, set, and clear now share one invocation:
`adb -s SERIAL shell -T -- /system_ext/bin/sos-core-dev-credential OP`.
The explicit `-T` prevents remote PTY allocation and echo, `--` ends ADB option
parsing, and only fixed path and operation arguments reach the remote shell.
Probe and clear close stdin. Set disables local tracing and terminal echo,
reads at most 513 bytes once from `/dev/tty` under the C locale, rejects more
than the protocol's 512-byte maximum, and feeds the accepted line to ADB only
through a pipe; its short-lived subshell restores terminal state and overwrites/unsets
the Bash value on every exit. ADB cannot read the prompt/control terminal.
Default shell-v2 stdout, stderr and remote exit capture stay separate; a
legacy merged stream still cannot pass the exact raw LF/CRLF one-line parser.
Missing `-T` support has a distinct safe error. A zero-exit mutation with an
empty, malformed, extra, binary, wrong, stderr-bearing, or otherwise
unobservable acknowledgement fails with credential state declared unknown and
the clear command remaining mandatory. A loose empty-output acceptance or PTY
fallback was rejected because either would erase acknowledgement or secret-echo
boundaries.

**Host evidence / decision / next gate:** `tests/a33xctl-host-test.sh` passed a
mock matrix that reproduces empty successful `exec-out`, proves working
`shell -T`, exact argv without `exec-out`, `-t`, key argv or key environment,
closed probe/clear stdin, stdin-only synthetic set data, hidden paste even
under caller-enabled Bash tracing, LF/CRLF framing, remote nonzero exit,
ambiguous acknowledgements, and unsupported `-T`. `check-product-graph`,
`check-core-agent-hardening`, `check-core-dev-credential`, the nine-test
cross-language protocol suite, Bash syntax, Cargo format, and diff hygiene all
passed; ShellCheck is unavailable on this host. Only host CLI, mock, test and
this ledger changed. No C++, Rust, product, policy, packaged file, content
revision, image payload, or OTA input changed, so the installed exact r10 may
be reused without rebuild or sideload. The runner should first repeat
`./tools/a33xctl inspect-core1-readiness --serial RFCT50EGFCN
--expected-revision sos.core1dev.e05f91bb6f0b.45b6c9467985`; after complete
no-Zygote readiness, proceed once through
`./tools/a33xctl core1-dev-set-openrouter-key --serial RFCT50EGFCN`, the
authorized provider request, and mandatory
`./tools/a33xctl core1-dev-clear-openrouter-key --serial RFCT50EGFCN`, then
perform crash/AVC scrutiny, deterministic manifest verification, and soak.
This host-only change does not claim a hardware pass.

## 2026-08-18 — Add secret-free Core-dev credential status and raw acknowledgement I/O

**Goal / continuation evidence:** Close the observability gap exposed by the
exact r10 credential gate without widening the development client's authority.
The independently verified continuation root
`artifacts/device-gates/core-dev-20260818-10-rfct50egfcn-cont-02` recorded full
no-Zygote readiness on serial `RFCT50EGFCN`, revision
`sos.core1dev.e05f91bb6f0b.45b6c9467985`, exit 0 in 1.322605722 seconds. Its
434-byte manifest has SHA-256
`7354888d45fef3204fcccd640ff4f5cde6f21934134131acfbc9422e059f5c80`.
The user then ran the hidden set command and reported `done`, but the v1
protocol exposed only mutation acknowledgements, so the runner could not
independently establish whether the in-process key was configured and did not
issue a provider request.

The runner performed the mandatory clear and independently verified
`artifacts/device-gates/core-dev-20260818-10-rfct50egfcn-cont-03`. The exact
secret-free result was `core1_dev_openrouter_key=CLEARED`, serial
`RFCT50EGFCN`, exit 0 in 0.473658774 seconds; the experience logged
`core_dev_credential state=cleared`. Follow-up inspection exited 0 in
0.520751099 seconds with no scoped credential residue or leaked agent child,
and no provider request had occurred. The 556-byte manifest has SHA-256
`ee41e4d0c49c0cb379e07c4b593a2743fbd6cb0784abd80732e6fafbcdcd2a35`.
Clear emitted two enforcing denials for `{ getattr }` from
`u:r:sos_core_dev_credential:s0` to the inherited
`u:r:adbd:s0` `unix_stream_socket`. The evidence collection also dumped the
device's full shell environment; future verification recipes must capture
only named safe markers, processes and scoped logs.

**Change / protocol decision:** Canonical protocol v1 now adds zero-payload
`STATUS` opcode `0x03` and secret-free `CONFIGURED`/`EMPTY` acknowledgements
`0x05`/`0x06`. This is a compatible v1 extension: old probe/set/clear frames
are unchanged, an old endpoint rejects the unknown STATUS opcode with
`PROTOCOL_MISMATCH`, and the new client rejects either state acknowledgement
for any non-STATUS operation. STATUS locks and reads the existing in-memory
credential state but never returns credential bytes, length, hash, provider
material or any other derived value and never mutates the state. The C++ and
generated Rust constants, golden vectors, decoder, endpoint dispatch and
cross-language integration share this one header contract.

The client no longer uses `fputs`, `fprintf` or any buffered stdio API on its
inherited descriptors. Every bounded static success or secret-free error
fragment uses checked, EINTR-safe `write` completion; a failed stdout
acknowledgement is nonzero and leaves host state unknown. This removes the
libc stdio metadata probe that is the current best explanation for the exact
inherited-adbd-socket `getattr` denials. No `getattr`, broader socket class,
property, file, log, Binder, capability, network or executable-memory allow
was added; the existing adbd socket permission remains exactly `{ read write
}`. The next hardware run must confirm the causal hypothesis by observing no
scoped client AVC.

`core1-dev-status-openrouter-key --serial SERIAL` uses the same exact `adb
shell -T -- /system_ext/bin/sos-core-dev-credential status` transport with
closed stdin and prints only `core1_dev_openrouter_key=CONFIGURED|EMPTY` plus
the validated serial. Set still accepts one hidden terminal line through
stdin only, but after exact `SET` it requires a separate STATUS result of
`CONFIGURED`; clear similarly requires `EMPTY` after exact `CLEARED`. The
shared byte parser permits only one exact LF or CRLF line and rejects missing
newlines, extra output, whitespace, prefixes, suffixes, NULs, stderr or a
nonzero exit. Probe, status and clear never inherit stdin, and no verification
recipe captures the device environment.

**Bounded host evidence / remaining gate:** `./tools/a33xctl
check-core-dev-credential` passed the expanded focused CIL policy compile,
Android/Bionic AArch64 C++ compile, product exclusion checks and 12-test real
production C++-client/Rust-endpoint suite. That suite covers configured/empty
transitions, rejected set followed by clear, exact golden frames, old/new
mismatch rejection, fragmentation, concurrent non-mutating status reads,
secret zeroization ownership, full stdout/stderr writes and failed stdout
acknowledgement. `tests/a33xctl-host-test.sh` passed state transitions,
post-mutation verification, closed stdin, exact no-PTY argv and the LF/CRLF
plus ambiguity matrix. ARM64 Android `cargo check --locked -p sos-experience
--lib --no-default-features --features core-dev-credential --target
aarch64-linux-android` passed with the pinned NDK 29 API-31 toolchain; Bash
syntax, Cargo format and diff hygiene passed. No full build, device command,
credential use or provider request was performed for this change.

Both ordinary Core 1 and Core-dev must be rebuilt because the shared protocol,
client and endpoint inputs changed. The next paired host/device gate should
run `check-core-dev-credential`, `build-core1`/`inspect-core1`, then
`build-core1-dev`/`inspect-core1-dev`, preserving distinct artifact revision,
size and SHA-256. After explicit authorization for that exact Core-dev OTA and
serial, require complete no-Zygote readiness and an initial `EMPTY` status;
run one hidden set and require automatic plus independent `CONFIGURED`, make
the authorized provider request, perform mandatory clear and require automatic
plus independent `EMPTY`, then capture scoped crash/AVC/process evidence,
soak, finalize and independently verify the deterministic manifest. This
host-only evidence does not claim either image build or hardware gate passed.

## 2026-08-18 — Make Core agent requests secret-safe and transport-classifiable

**Goal / r11 failure evidence:** Make every serialized Core agent request leave
bounded start and terminal evidence without exposing request or provider content,
after the exact Core-dev r11 live gate reached revision
`sos.core1dev.e05f91bb6f0b.21d92e761f9f`, passed complete no-Zygote readiness,
passed hidden SET plus an independent `CONFIGURED` STATUS, and then showed a DNS
error in the UI for exactly one simple prompt on the pinned
`deepseek/deepseek-v4-flash-0731` model. The 5,831-byte
`artifacts/device-gates/core-dev-20260818-11-rfct50egfcn-cont-02/safe-request-window.txt`
has SHA-256
`f4fd3b4b035ee518f41913c3b76701f6ffa8e0f4b5e222ccd76c3ec88bbe9a5d`.
It contained no safe request/model, timing, child-exit, lifecycle, or DNS-category
marker, so the visible error could not classify the failing layer and no retry was
allowed. Mandatory remote clear then passed in 1.699027181 seconds; the 137-byte
`clear.txt` has SHA-256
`0ddca1b7b81905c1d7feb706569919515e886fe7b283f8ffc77059f43fb90b3c`,
and the independent 135-byte `status-empty.txt` has SHA-256
`6cd7a9a159e97868876f331b55a4ad75ddcab373419c06e16dead41863abb53a`.
The deterministic manifest was independently verified and the final state was
stable Core with no credential or child residue. This is cleanup evidence, not a
provider-request PASS.

**Root cause / lifecycle change:** The UI-to-provider path was
`agent_submit` → trusted Rust `run_live`/`run_core_pi` → fixed Node stdio runner →
Pi/OpenRouter. Rust logged only after a successful spawn and after parsing the last
stdout line; Node could collapse a transport exception into Pi's arbitrary
`errorMessage`; and neither a completed stdin write nor the final UI state had a
mandatory marker. A serialized request now logs acceptance with the exact pinned
model, child PID plus expected `sos_core_agent` domain and fixed OpenRouter
identity, successful bounded stdin completion, child code/signal including forced
cleanup, validated protocol-v2 response header, and one safe request terminal.
The UI logs its own completed/failed terminal. Existing exact action ordinals,
candidate validation, authority staging, authority commit, and presentation
markers remain the authoritative success chain. Serialization, rather than a
prompt-derived correlation value, joins the pre-candidate phase; later activation
keeps its existing non-secret host request ID.

**Taxonomy / protocol and security decision:** Node now observes the actual fetch
exception code and numeric HTTP status before Pi turns failures into strings.
Resolver `EAI_*`/`ENOTFOUND`, DNS timeout, DNS-proxy unavailability, connect
timeout, refused, reset, unreachable network, TLS/certificate/handshake failure,
credential rejection, rate limit, other numeric HTTP rejection/unavailability,
protocol/child failure, and `unknown` are distinct. HTTP evidence is only an
integer from 100 through 599. Transport classification uses structured `code` or
`errno` fields and never exception/provider message text; if Pi reports an error
without a trusted observation it becomes `unknown` before tool-sequence checking.
The final bounded stdout object carries protocol version, terminal state, an
allowlisted stage/category, optional numeric status, and fixed safe text. Rust
rejects a missing/mismatched version or terminal, re-allowlists every field, and
maps unknown input to `unknown`. Prompt, response, credential, headers, URL query,
provider body, child stdout/stderr, and tool arguments/results are never logged;
child stderr remains connected to `/dev/null`. Tests include malicious strings
containing DNS codes and fake log lines and prove they cannot select a category or
enter a marker.

A separate zero-credential diagnostic command was deliberately not added. The
existing production OpenRouter preflight already resolves the fixed
`openrouter.ai` name in the same `sos-node`, `--jitless`, `sos_core_agent` child
immediately before Pi transport, now returns a structured terminal, and is the
specific layer r11 needed to expose. Adding a second dev-only host/client protocol
and TLS implementation would duplicate rather than isolate that path; if the next
gate passes DNS but fails connect/TLS, the instrumented production fetch supplies
that next safe category without sending a diagnostic request.

**Android policy verification / host evidence:** The checked Android platform
sources show Bionic `getaddrinfo` opening `/dev/socket/dnsproxyd`, netd's connect
hook using `/dev/socket/fwmarkd`, platform file contexts labeling those sockets
`dnsproxyd_socket` and `fwmarkd_socket`, and `unix_socket_connect` expanding to
the labeled `sock_file write` plus `netd:unix_stream_socket connectto` pair. The
source and target-files inspectors now require both labeled paths, the dedicated
domain transition, narrow `proc_meminfo` read, and TCP-443 `name_connect`; they
continue to reject `netdomain`, host IP sockets, UDP/raw/ICMP, `net_dns_prop`,
broad port connect, permissive/dontaudit, and `execmem`. No policy allow was added.

`npm test` rebuilt the shared bundle and passed 17 tests covering all transport
codes, HTTP classes, malicious messages, protocol terminals, and the formerly
invisible r11 DNS case. A standalone Rust contract run passed seven tests,
including allowlist injection rejection and child/activation ordering. The pinned
NDK 29 API-31 AArch64/Bionic `cargo check --locked -p sos-experience --lib
--no-default-features --features core-dev-credential --target
aarch64-linux-android` passed. Cargo format, Bash syntax, and diff hygiene passed.
The ordinary no-feature host test remains blocked by the pre-existing generated
Core-dev test cfg (`STATUS_REJECTED`), unrelated to this change.

**Decision / remaining risk / next paired gate:** Both ordinary Core 1 and
Core-dev images must be rebuilt because the shared Rust runtime and Node bundle
changed. After host build and inspection record each OTA's exact revision, byte
size, and SHA-256. With fresh authorization for the exact Core-dev artifact and
serial, the runner should perform one sideload transaction, require complete
Core/no-Zygote readiness, initial STATUS `EMPTY`, hidden SET plus independent
STATUS `CONFIGURED`, then submit exactly one bounded prompt on the pinned model.
The safe window must contain one accepted marker, child domain/PID, request-written
marker, child exit, protocol-v2 response header, exactly one terminal stage,
category and optional numeric status, plus the UI terminal; success additionally
requires all three action ordinals, candidate validation, authority stage/commit,
and visible UI. Require mandatory clear plus independent STATUS `EMPTY`, scoped
crash/AVC and child-residue scrutiny, soak, finalized evidence, and independent
deterministic-manifest verification. The open hardware risk is whether Android's
DNS proxy and confined TCP-443/TLS path work in `sos_core_agent`; no desktop check
or this entry claims that gate passed.

## 2026-08-18 — Keep Core DNS on netd and close inherited agent descriptors

**Goal / terminal r12 evidence:** Diagnose the now-classified DNS failure from
the exact Core-dev revision `sos.core1dev.e05f91bb6f0b.cdede92491c0` without
granting the Node child generic networking or graphics authority. Complete
Core/no-Zygote readiness and the hidden SET plus independent `CONFIGURED`
STATUS were clean. One pinned-model request ran from `17:27:23.210` through
`17:27:23.713`, a measured 0.503 seconds. The child was correctly
`sos_core_agent`, exited 1, returned a protocol-v2 failed envelope, and ended
at `stage=transport category=dns_resolution status=none`; no action,
validation, stage, authority commit, or successful UI marker followed and no
retry was attempted.

The 23,197-byte evidence file
`artifacts/device-gates/core-dev-20260818-12-rfct50egfcn-cont-02/safe-lifecycle.txt`
has SHA-256
`e3b3115df0eb4af95817152b619b6c7f701cd6cda30d310df2b3d6a31ec9a5d9`.
It records enforcing `sos_core_agent` denials for inherited dma-buf descriptor
`{ use }` against `u:r:hal_graphics_allocator_default:s0`, pipe `fifo_file
{ ioctl }` with command `0x5401`, `net_dns_prop:file { read }`, and
`self:udp_socket { create }`. Mandatory clear passed; `clear.txt` is 137 bytes
with SHA-256
`ae896b6147425a060023b0640ee2439af478a8e8bf0e99f767ee920bce7646a1`.
Independent EMPTY status, child-residue and stability checks passed and the
deterministic manifest was independently verified. This is a clean failure
and cleanup result, not a live-provider PASS.

**Resolver and ioctl diagnosis / policy decision:** The shipped Node
`dns/promises.lookup` reaches `uv_getaddrinfo` on a `libuv-worker`, which calls
Bionic `getaddrinfo`. Android's `dns_open_proxy` first creates and closes an
IPv6 datagram socket as a network-eligibility probe, then opens the labeled
`/dev/socket/dnsproxyd`; when that creation fails, Bionic returns `EAI_SYSTEM`
and its obsolete local fallback tries `net.dns*` plus direct UDP. Core now
allows exactly `sos_core_agent self:udp_socket create` so the supported proxy
path can proceed. It grants no UDP read, write, bind, connect, ioctl, port, or
node permission; neverallows prohibit every other self UDP permission, every
UDP port permission, every UDP node permission, raw/ICMP, and the obsolete
property. Direct UDP cannot be destination-port confined by this SELinux
socket class, so direct DNS/53 was rejected instead of granting `net_domain`
or a broad UDP client. The existing labeled dnsproxyd/fwmarkd Unix sockets
remain the resolver/routing path, and the only IP destination permission is
TCP `name_connect` to the port type bound to 443. The trusted host remains
networkless and `execmem` remains prohibited.

Command `0x5401` is `TCGETS`: Node/libuv calls `uv_guess_handle`/`isatty` to
classify its pipe-backed standard streams during fixed startup, independent of
color configuration. Policy adds the base fifo ioctl permission together with
an explicit `allowxperm` for `TCGETS`; Android's global domain xperm also
restricts fifo probing to that same harmless command. Generic fifo ioctl and
all other commands remain rejected. Child stderr is still `/dev/null`.

**FD boundary / implementation:** The Rust launcher now installs an
async-signal-safe pre-exec callback after configuring its three standard
streams. One atomic `close_range(3, UINT_MAX, CLOSE_RANGE_CLOEXEC)` marks every
non-stdio descriptor for kernel closure at exec, including GPUI dma-bufs,
surfaces, input/device handles and service sockets, while preserving Rust's
private launch-error pipe until that boundary. A syscall failure aborts spawn,
so unsupported kernels fail closed rather than leaking a descriptor. The
stdin/stdout pipes are already duplicated onto 0/1 and survive; stderr remains
the configured null descriptor at 2. The sanitized child-start marker states
`fd_boundary=stdio_only`. A focused host contract deliberately removes
`CLOEXEC` from an extra file descriptor, spawns through the production helper,
proves the descriptor absent, and proves pipe-backed stdin/stdout still work.
No dma-buf or broad ioctl allow was added.

**Bounded host evidence:** The focused Core-agent CIL compiled with the tree's
`secilc`; source and target-files inspectors now require creation-only UDP,
dnsproxyd/fwmarkd, exact pipe `TCGETS`, stdio-only inheritance and TCP 443, and
reject netdomain, property expansion, UDP bind/connect/read/write/ioctl,
generic UDP ports/nodes, raw/ICMP, broad TCP destinations, permissive,
dontaudit and execmem. `./tools/a33xctl check-core-agent-hardening` and the
standalone `core-agent-contract-test` passed. `npm test` rebuilt the shared
runner and passed all 18 tests, including both the classified DNS failure and
the exact fixed-hostname success continuation. The pinned NDK 29 API-31 ARM64
Bionic `cargo check --locked -p sos-experience --lib --no-default-features
--features core-dev-credential --target aarch64-linux-android` passed. Cargo
format and Bash syntax passed. The full desktop experience test remains
blocked by the previously recorded generated Core-dev cfg and missing
`xkbcommon` link libraries; the standalone test exercises the exact production
FD source without GPUI. No full product build, device command, credential use,
or provider call occurred in this implementation.

**Remaining risk / next paired gate:** Both ordinary Core 1 and Core-dev must
be rebuilt and inspected because they share the changed SELinux policy and
Rust runtime. Preserve each OTA's exact revision, byte size and SHA-256 in
finalized independently verified host evidence. After explicit authorization
for the new Core-dev artifact and serial, use one runner for the complete
transaction: sideload once with its inherent reboot, require exact Core
no-Zygote readiness and initial `EMPTY`, perform hidden SET plus independent
`CONFIGURED`, then issue exactly one pinned-model request. Require the child
domain and `fd_boundary=stdio_only`, successful DNS/TLS/provider response,
all three exact action ordinals, validation, authority stage/commit and visible
UI, with no dma-buf/property/UDP/fifo/execmem AVC, crash, secret exposure or
child residue. Mandatory clear plus independent `EMPTY`, scoped scrutiny,
authorized soak, finalized manifest and independent verification remain
required even if the request fails. This implementation does not claim either
image build or hardware PASS.

## 2026-08-18 — Start Core request evidence at UI submit and add a fixed dev smoke entry

**Goal / r13 evidence:** Close the remaining pre-dispatch observability gap after
Core-dev revision `sos.core1dev.e05f91bb6f0b.efba796de09e` passed complete
no-Zygote readiness, hidden SET and independent `CONFIGURED` STATUS, but one
owner-entered prompt produced another visible UI error with no request-accepted,
request-start, child, terminal, model, action or DNS marker. The independently
verified continuation root
`artifacts/device-gates/core-dev-20260818-13-rfct50egfcn-cont-02` has a 620-byte
manifest with SHA-256
`ffc4588f2aab3ddc20ca8c7bd9c87a488163cd3406474d99361489958e68cc77`.
Its 204-byte `safe-lifecycle.txt` has SHA-256
`7326dea769d00894b8873a4b118dba26814d313d0d59f43f008917dcedb7088f`
and contains only safe product/process headings, confirming that the attempt
never reached the earlier Node-side lifecycle boundary. Final scrutiny completed
in a measured 7.543186 seconds with no retry or soak. Clear had already started
before the retention policy changed; it completed and independent STATUS returned
`EMPTY`. The 137-byte `clear.txt` and 135-byte `status-empty.txt` have SHA-256
`a4c8a3f07c94951c3f3176357d13a7c9679cb15a59420cfb66f87adb52559c71`
and `e5a500017fb421870b9f7f77572244d13f03f078f014106f0adec4f210333425`.

A separately captured 182,394-byte framebuffer at
`artifacts/device-gates/core-dev-20260818-13-rfct50egfcn-cont-03/framebuffer.png`
has SHA-256
`1775df9cb9e363e7e4ff195cc2516d12e3d0accfcc8276e79a7b8aadf07c6de5`.
It shows the live Core surface was offline: `0 network`, `Weather unavailable`,
and the NETWORK card said `Choose network`. That makes an offline preflight the
best current explanation for r13's lack of Node markers; r13 is not evidence of
a new DNS-policy failure. The exact safe on-screen error text remains unknown.

**UI lifecycle and error decision:** Every Android/Core `agent_submit`, whether
delivered by native/IME submit, accessibility submit, or a tap action, now creates
a monotonic process-local attempt before validation. `core_ui_attempt` records
`attempt_received`, then either a preflight terminal or `dispatch_started`, and
exactly one terminal. Fields are limited to the numeric attempt, allowlisted
provider identity, pinned model, configured/busy/input-present booleans,
allowlisted stage/category, status and `correlation=serialized`; prompt length,
content or hash, credential material, response/UI text, and tool arguments/results
never enter these markers. The protected submit queue cannot evict its one pending
agent event. Worker-send, state-update/effect, missing or duplicate prompt effect,
agent-thread spawn and update-channel failures are classified before display.

One mapping now owns both safe terminal categories and fixed displayed text.
Preflight distinguishes `empty_input`, `credential_missing`, `busy`,
`model_policy`, and the new `network_unavailable`; dispatch/runtime failures use
`dispatch_channel` and `runtime_start`; the existing transport, provider,
protocol, child and candidate categories remain downstream. Unknown or injected
provider/protocol values collapse to fixed `protocol/unknown` text. Routine
`experience_model_refreshed` evidence moved from info to debug so it cannot bury
request markers. Existing child, response-header, action-order, validation,
authority-stage/commit and presentation evidence remains mandatory after
dispatch.

**Development-only fixed submit:** Canonical Core-dev protocol v1 adds the
zero-payload `AGENT_SMOKE` opcode `0x04`, golden frame
`53 4f 53 4b 01 04 00 00`. The dedicated authenticated client and endpoint accept
it only on the exact Core-dev product and only while the in-process OpenRouter
credential is `CONFIGURED`. It queues one bounded source-embedded non-secret
request—create a visible item titled `Blue smoke check` with a fixed benign
body—through the same `agent_submit` UI entry and therefore the same preflight,
Lua action, Pi/tool sequence, candidate validation, authority activation and
presentation path. It accepts no payload, argv prompt or environment prompt and
returns only `core_dev_agent_smoke=SUBMITTED`. Ordinary Core still excludes the
client, endpoint and operation. Host command `./tools/a33xctl
core1-dev-submit-agent-smoke --serial SERIAL` independently requires STATUS
`CONFIGURED` first and never clears on failure.

**Credential retention / bounded host evidence:** Request failure does not call
credential clear. A focused state test proves an invalid refreshed credential
leaves the existing memory-only key configured until explicit clear; process exit
still zeroizes it. The Core UI lifecycle suite passed 15 tests covering every
preflight exit, offline classification, full ordered dispatch, dispatch/runtime
and downstream terminals, one-terminal enforcement, injected content, all
packaged composer strings, the fixed smoke string and the existing
child/activation contract. The Core-dev suite passed
19 production C++/Rust tests covering peer/product gates, exact frames, payload
rejection, configured-only smoke submission, secret-free output, fragmented I/O,
credential retention and explicit clear. `./tools/a33xctl
check-core-agent-hardening`, `./tools/a33xctl check-core-dev-credential`,
`tests/a33xctl-host-test.sh`, host `cargo check --locked -p sos-experience --lib`,
Bash syntax and Cargo formatting passed. The pinned NDK 29 API-31 ARM64
`cargo ndk -t arm64-v8a -P 31 check --locked -p sos-experience --lib
--no-default-features --features core-dev-credential` passed. No full product
build, device mutation, credential use or provider request was performed by this
implementation.

**Remaining risk / next paired gate:** Both ordinary Core and Core-dev images
must be rebuilt and inspected because they share the UI runtime; record exact
revision, byte size and SHA-256 for each. For the next authorized Core-dev device
gate, require exact no-Zygote readiness, a validated network and initial STATUS
`EMPTY`; perform hidden SET plus independent `CONFIGURED`, then invoke the fixed
smoke command once. Capture the returned safe acknowledgement, one complete
attempt lifecycle with exactly one terminal, all downstream request/action and
authority evidence when dispatch starts, and a framebuffer proving either the
fixed visible item or the fixed category-aligned error. After any failed request,
STATUS must remain `CONFIGURED` and no automatic clear may occur. Clear remains an
explicit final session boundary, followed by independent `EMPTY`, scoped
crash/AVC/child scrutiny, finalized deterministic evidence and independent
manifest verification. The open hardware risks are validated network setup and
the previously unexercised live TLS/provider path; this host work claims no image
build or hardware PASS.

## 2026-08-18 — Bridge one Core-dev smoke request through ADB reverse CONNECT

**Goal / retained r13 evidence:** Exercise the shared Core UI → Pi/OpenRouter →
action validation → authority path even while native Wi-Fi provisioning remains
absent. The finalized r13 framebuffer at
`artifacts/device-gates/core-dev-20260818-13-rfct50egfcn-cont-03/framebuffer.png`
is 182,394 bytes with SHA-256
`1775df9cb9e363e7e4ff195cc2516d12e3d0accfcc8276e79a7b8aadf07c6de5`;
it shows `0 network`, `Weather unavailable`, and `Choose network`, with no saved
or validated Supplicant network and no Android Wi-Fi service on this no-Zygote
product. A later read-only continuation proved that the user-retained memory-only
credential is still present: the independently verified 158-byte manifest at
`artifacts/device-gates/core-dev-20260818-13-rfct50egfcn-cont-05/device-continuation-manifest.tsv`
has SHA-256
`58266e08796843313cd01c2017235f8de59ae92b4c528275a99e6542bc41752f`.
Its 55-byte `status.txt` has SHA-256
`6a45c3ce2d5d5a0dee85a9764feb97c6507ab9d907a8d57fffcb128700370c56`
and reports only `CONFIGURED`; the 85-byte `timing.txt` has SHA-256
`646c202d697d3dfa6e39878a82b37685c09a8b56e5cb8d5376879658949d929c`
and records a measured 0.460608322 seconds. No provider request, clear, or
network mutation occurred in that continuation.

**Architecture / production exclusion:** The authenticated zero-payload
Core-dev `AGENT_SMOKE` operation now arms exactly one process-memory tunnel bit
alongside its fixed benign prompt. The next received UI attempt consumes that
bit into the attempt object, bypasses only the validated-network preflight, and
retains all credential, busy, pinned-model, Pi tool-order, candidate-validation,
authority and presentation checks. Dropping that attempt or completing its child
drops the mode; no property, file, argv, environment variable, prompt parameter,
key, hostname, model or response-print switch exists. Rust serializes only the
fixed `coreDevProxy` value `http://127.0.0.1:37173` for that attempt. The
Core-dev Node decoder accepts that exact value only with OpenRouter and the sole enabled model
remains `deepseek/deepseek-v4-flash-0731`. It skips device DNS only in this mode
and uses Undici `ProxyAgent` 7.29.0; target requests must still be HTTPS to exact
`openrouter.ai` on port 443. Normal SNI, hostname and certificate validation are
unchanged, so the host sees only the CONNECT authority and opaque TLS bytes.

Core-dev packages a second filename for the immutable Node ELF and transitions it
to `sos_core_dev_agent`. A product-exclusive policy directory alone labels that
entrypoint and TCP 37173, and grants only this domain the fixed proxy-port
`name_connect` permission. Ordinary Core packages neither the executable nor the
credential client, compiles without the feature and product policy directory,
and its existing `sos_core_agent` remains limited to the resolver plus TCP 443.
The build produces a separate Core-dev runner at the ordinary immutable install
path; the ordinary runner contains neither the proxy literal nor Undici
`ProxyAgent`. Ordinary Core also cannot arm the mode and its SELinux domain cannot
connect to the development port.

The host relay `tools/core-dev-connect-bridge.py` binds an ephemeral IPv4 port on
`127.0.0.1` only, accepts only syntactically bounded HTTP/1.1 `CONNECT
openrouter.ai:443` with the matching single Host header, resolves/connects that
one upstream on the host and copies bytes bidirectionally without logging headers,
bodies, keys, TLS plaintext or response text. All other methods, authorities,
ports, malformed framing, request bodies and CLI arguments are rejected. Command
`./tools/a33xctl core1-dev-run-agent-smoke --serial RFCT50EGFCN` validates the
exact Core-dev product and `CONFIGURED` state, starts that relay, installs only
`adb reverse tcp:37173 tcp:<ephemeral-host-port>`, invokes the existing fixed
smoke opcode, waits up to the existing 240-second provider budget plus cleanup
margin for one sanitized UI terminal, captures only allowlisted lifecycle lines
and a framebuffer, rechecks `CONFIGURED`, removes the reverse, stops the relay,
and deterministically creates and verifies its evidence manifest. EXIT and signal
traps remove the reverse and process; neither success, failure nor signal clears
the credential.

**Bounded host evidence / rejected approaches:** `npm test` rebuilt the 1.8 MiB
ordinary and 2.8 MiB Core-dev single-file runners and passed 20 tests in 725 ms,
including exact product exclusion, proxy/model decoding, target-host rejection
and content-free observations. The initial
Undici 7.16 pin was rejected after `npm audit` identified current advisories; the
exact 7.29.0 pin reports zero production vulnerabilities. The Python relay suite
passed three tests in 1.512 seconds for loopback bind scope, method/authority/
framing rejection, opaque bidirectional relay and empty stdout/stderr.
`tests/a33xctl-host-test.sh` passed in 3.2 seconds, including reverse setup/removal,
safe terminal/framebuffer evidence, deterministic manifest verification,
credential retention and cleanup on success, rejected submit and signal. The
production C++/Rust protocol suite passed 19 tests in 0.03 seconds; the inherited
FD/policy suite passed 16 in 0.01 seconds; both source gates passed. Pinned NDK 29
API-31 ARM64 checks passed for both `core-native` in 0.80 seconds and
`core-dev-credential` in 0.59 seconds. Bash syntax, Cargo formatting and diff
hygiene passed. A generic HTTP proxy, arbitrary CONNECT target, key-bearing host
proxy, TLS termination, persistent property/environment switch, ordinary-agent
port grant and automatic credential clear were rejected because each would widen
the development envelope or expose provider material.

**Decision / remaining risk / next gate:** This bridge is a development-only way
to validate the already shared runtime; it does not implement or validate native
Wi-Fi provisioning and it does not claim a build, provider call, device gate or
hardware PASS. The untested risks are Soong packaging of the product-exclusive
policy, ADB reverse reachability from `sos_core_dev_agent`, Android Node/Undici
CONNECT behavior, end-to-end provider TLS, and the live action/authority result.
Next build and inspect ordinary Core with `build-core1`/`inspect-core1`, then
Core-dev with `build-core1-dev`/`inspect-core1-dev`, preserving each exact revision,
size and SHA-256 and proving ordinary absence plus development inclusion. After
explicit authorization for that exact Core-dev OTA and serial, require complete
no-Zygote readiness and `CONFIGURED`, then run exactly
`./tools/a33xctl core1-dev-run-agent-smoke --serial RFCT50EGFCN`. Require one safe
terminal, all downstream action/validation/authority markers on success, a
framebuffer, no relevant crash or enforcing AVC, no residual reverse/bridge/child,
an independently verified manifest, and a final independent `CONFIGURED` status;
do not clear the credential automatically on either result.

## 2026-08-18 — Give ordinary and Core-dev runners unique Soong install identities

**Goal / r14 failure evidence:** Unblock the first paired build after the
Core-dev CONNECT milestone without weakening the development boundary or
changing Compat/Linux behavior. The ordinary `./tools/a33xctl build-core1`
transaction stopped in ckati after a measured 137.424486486 seconds, before an
OTA existed. Its finalized stderr at
`artifacts/host-gates/core-dev-20260818-14/raw/build-core1.stderr` is 527,070
bytes with SHA-256
`8339f4ffd4537e7f54334d6deab2da601874b77895866cf45b5c6129fa3d4ed7`.
The 8,601-byte source preflight at
`artifacts/host-gates/core-dev-20260818-14/raw/preflight-source.txt` has SHA-256
`beda70c7de00c3ae0dc7799605c9b80fc69baa4f56e760a2e64ddf37a6d394bf`;
the independently verified manifest has SHA-256
`8a8b97940e0a1cb49daa7d5d989336845e73abbe2aaded0df5b6063eb457f08e`.
The retained device remains on r13 with its retained key; no device command or
mutation was performed for this failure or fix.

**Root cause / rejected approach:** `sos-agent-runner` and
`sos-agent-runner-core-dev` were separate `prebuilt_etc` modules but both set
`filename: "agent-runner.cjs"`. Soong emitted both install recipes into the
ordinary product's generated installs makefile before product override
filtering; lines 279249 and 279253 therefore produced the same
`system_ext/etc/sos-agent/agent-runner.cjs` target and ckati rejected the second
recipe. A module `overrides` relationship cannot make globally emitted Make
targets unique, so same-destination conditional replacement was rejected.

**Fix / invariants:** The Core-dev module now has distinct source, module and
installed-file identities ending in `agent-runner-core-dev.cjs`; the
feature-gated Core runtime selects that exact immutable path while ordinary
Core keeps `agent-runner.cjs`. Runner selection moved out of the shared product
composition: every ordinary SOS A33x product explicitly selects only
`sos-agent-runner`, while the Core-dev product selects only its development
runner plus the existing credential client and distinct Node executable. The
credential executable type, file context and client domain policy also moved
into the existing product-exclusive Core-dev policy directory, leaving the
ordinary shared policy free of credential, smoke-proxy and development Node
symbols. Deterministic source/product/Soong audits now require one filename per
runner module, forbid the obsolete override, verify every ordinary product's
explicit selection and development exclusion, verify the Core-dev selection,
and exercise the runtime contract under both feature sets. The only model
remains `deepseek/deepseek-v4-flash-0731`; proxy authority, TLS certificate and
hostname validation, credential protocol and SELinux network permissions are
unchanged.

**Bounded host evidence:** Ordinary and Core-dev contract builds each passed 16
tests in 0.14 seconds (`cargo test --locked -q -p core-agent-contract-test
--lib`, with and without `--features core-dev-credential`).
`./tools/a33xctl check-core-dev-credential` passed the unique-install/product
audit, Android client compile, policy expansion and 19-test cross-language
protocol suite in 0.45 seconds. `bash tests/a33xctl-host-test.sh` passed in 2.78
seconds. Bash syntax and `cargo fmt --all -- --check` passed together in 0.48
seconds. No full Soong build, OTA inspection or hardware gate is claimed.

**Decision / remaining risk / next paired gate:** The source-level collision is
removed, but only fresh full builds can prove the generated install graph and
target-files contents. The host runner must execute, in order,
`./tools/a33xctl build-core1`, `./tools/a33xctl inspect-core1`,
`./tools/a33xctl build-core1-dev`, and `./tools/a33xctl inspect-core1-dev`.
Record each finalized OTA revision, byte size and SHA-256 and require exactly
the ordinary runner in ordinary Core, exactly the distinct development runner
and intended credential/proxy controls in Core-dev, and no development
executable, module property or policy in ordinary Core. Do not touch the device
until a new exact Core-dev artifact, serial and transaction envelope receive
explicit authorization.

## 2026-08-18 — Restore the sanitized accepted-request inspection marker

**Goal / r15 failure evidence:** Resume the paired ordinary/Core-dev host gate
without weakening the shared UI attempt lifecycle. The ordinary
`./tools/a33xctl build-core1` command succeeded in a measured 376.847042027
seconds, but `./tools/a33xctl inspect-core1` failed after 16.981760195 seconds
because the GPUI runtime did not contain the required literal prefix
`android_agent_request_accepted provider=`. The finalized 320-byte stderr at
`artifacts/host-gates/core-dev-20260818-15/raw/inspect-core1.stderr` has SHA-256
`af03d9cfcb55d26db6f53fcf63137c201557dab900ed0114740db078a5fec4a3`;
the independently verified 754-byte evidence manifest at
`artifacts/host-gates/core-dev-20260818-15/host-evidence-manifest.tsv` has
SHA-256
`02377fa09ff44b914d8aede1ef66d6ceac03611936a3996ecb39e9f9abaa9167`.
The ordinary OTA in shared build output was not accepted or preserved after
inspection failed. Core-dev build/inspection did not run and no device was
touched.

**Root cause / fix / invariants:** The accepted-request log had been placed on
the correct successful agent-thread path, after the shared UI/Luau dispatch,
but inserted the numeric attempt before `provider`; the binary therefore
omitted the inspection contract's stable prefix. Acceptance also was not a
one-way state in the attempt contract, leaving exactly-once behavior implicit.
The attempt now advances from `DispatchStarted` to `Accepted` while constructing
the marker. Construction is rejected before dispatch, after an early terminal,
and on a second call. The production path logs that returned marker once only
after thread creation has accepted the request. Its fields are limited to the
allowlisted provider/model, pinned-policy label, numeric attempt and fixed
correlation label; prompt, key, response and arbitrary provider text cannot
enter it. This remains distinct from the earlier `core_ui_attempt
event=dispatch_started` event. Terminal taxonomy, fixed smoke prompt, retained
credential, development proxy tunnel, ordinary-product exclusion,
Compat/Linux paths and exact model `deepseek/deepseek-v4-flash-0731` are
unchanged.

**Bounded host evidence / next gate:** `cargo test --locked -q -p
core-agent-contract-test --lib` passed 18 tests in 0.01 seconds both without and
with `--features core-dev-credential`. New cases prove received → dispatch →
accepted ordering, exactly one accepted marker, no accepted marker for an early
credential rejection, and sanitization of injected provider/prompt/key/response
content. Targeted Cargo formatting and the complete
`./tools/a33xctl check-core-agent-hardening` source/policy/contract gate passed;
`./tools/a33xctl check-core-dev-credential` also passed its exact-model,
cross-language protocol, Android compile, peer-policy and production-exclusion
checks with 19 tests in 0.02 seconds. The broader `cargo test -p sos-experience
android_agent_contract::tests --lib` could not start these tests because
unrelated in-progress `core_dev_credential.rs` code lacks `STATUS_REJECTED` in
five places; no change to that work was made. Fresh artifacts are still
required: rerun the complete paired sequence `./tools/a33xctl build-core1`,
`./tools/a33xctl inspect-core1`, `./tools/a33xctl build-core1-dev`, and
`./tools/a33xctl inspect-core1-dev`, preserving exact accepted artifact
revision, byte size and SHA-256 plus finalized independently verified evidence.
No hardware PASS is claimed.

## 2026-08-18 — Restore exactly-once sanitized request-terminal evidence

**Goal / r16 failure evidence:** Continue the paired ordinary/Core-dev host
gate without weakening the shared attempt state machine. The ordinary
`./tools/a33xctl build-core1` command passed in a measured 219.495510590
seconds, then `./tools/a33xctl inspect-core1` failed after 17.146491019 seconds
because the AArch64 GPUI runtime omitted the required stable prefix
`android_agent_request_terminal stage=`. The finalized 317-byte stderr at
`artifacts/host-gates/core-dev-20260818-16/raw/inspect-core1.stderr` has SHA-256
`250f7f1a9ee8e269cb0085597a0060f4f6a605087d2f2b35488df12b27b3b20d`.
The independently verified 754-byte manifest at
`artifacts/host-gates/core-dev-20260818-16/host-evidence-manifest.tsv` has
SHA-256
`be11d59ae9cf49b92310aa4b132e0a7e398d654a586b86bc4261cb30218c7728`.
The unaccepted ordinary OTA remained only in shared build output at
`/home/carlid/dev/lineage-a33x/out/target/product/a33x/lineage-23.0-20260818-UNOFFICIAL-sos_core1_a33x.zip`:
revision `sos.core1.e05f91bb6f0b.b6271158153b`, 1,022,158,073 bytes, SHA-256
`afad4cb139d62dd9a4b2dc14965c207b07ffe1f4e2e3e637071df44f5bc78c22`.
Core-dev build/inspection did not run, no OTA was accepted, and no device was
touched.

**Root cause / fix / lifecycle audit:** The attempt state machine still made a
single one-way `Terminal` transition for preflight rejection, dispatch/runtime
failure and provider success/failure, but the centralized emitter rendered
only `core_ui_attempt event=terminal`; the request-terminal prefix required by
both `inspect-core1` and `inspect-core1-dev` had been dropped. The same audit
found the next required stable prefix, `android_agent_ui_terminal status=`, was
also absent. Both are now derived from the one terminal event, so neither can
be constructed for received/dispatch events and a second terminal transition
remains rejected. Request evidence begins with `stage` and UI evidence with
`status` exactly as the inspectors require. Metadata is limited to allowlisted
stage/category/provider/model, fixed pinned-model policy and correlation
labels, and the numeric attempt ID; no key, prompt, response, arbitrary error
or provider content is accepted. The source hardening audit now checks the
exact field-order prefixes for every native lifecycle marker consumed by the
two inspectors. The remaining inspected lifecycle/evidence prefixes already
matched their emitters. Predispatch attempt receipt, accepted-before-terminal
ordering and exactly-once acceptance, early-exit taxonomy, credential
retention, Core-dev fixed smoke/proxy path, ordinary exclusions, Compat/Linux
behavior and exact-only `deepseek/deepseek-v4-flash-0731` policy are unchanged.

**Bounded host evidence / failure / next paired gate:** `cargo test --locked -q
-p core-agent-contract-test --lib` passed 19 tests in 0.01 seconds both without
and with `--features core-dev-credential`. The added semantic coverage proves
exactly one request terminal for success, post-dispatch failure and early
rejection; accepted/terminal order; both stable prefixes; rejection of terminal
marker construction from nonterminal events; and sanitization of injected
provider, prompt, key, response and error text. `./tools/a33xctl
check-core-agent-hardening` passed its exact-prefix source audit, policy
expansion and contract suite in 0.25 seconds; Bash syntax, Cargo formatting and
diff checks passed. The broader `cargo test --locked -q -p sos-experience
android_agent_contract::tests --lib` still cannot compile because unrelated
in-progress `core_dev_credential.rs` code lacks `STATUS_REJECTED` in five
places; that work was not changed. No new product build or hardware PASS is
claimed. The runner must execute the paired gate in order:
`./tools/a33xctl build-core1`, `./tools/a33xctl inspect-core1`,
`./tools/a33xctl build-core1-dev`, then `./tools/a33xctl inspect-core1-dev`.
Preserve each distinct accepted OTA revision, byte size and SHA-256 plus a
finalized independently verified evidence manifest; do not touch hardware
without a new exact artifact/serial authorization envelope.

## 2026-08-18 — Carry the authenticated Core-dev tunnel through the complete preflight

**Goal / terminal r17 evidence:** Run the exact fixed Core-dev smoke once on
`RFCT50EGFCN` through the already established loopback CONNECT bridge and ADB
reverse. It stopped before child or provider dispatch at
`stage=preflight category=network_unavailable` in 1.149496247 seconds. Bridge
cleanup passed, post-failure Core no-Zygote readiness passed, the memory-only
credential remained `CONFIGURED`, and no retry occurred. The finalized evidence
root is
`/home/carlid/dev/sos/artifacts/device-gates/core-dev-20260818-17-rfct50egfcn-cont-01`;
its independently verified manifest has SHA-256
`e240f753804191fca1c01ff8bac3f557922414266accef182756b37d6eccdad4`.
The framebuffer at
`/home/carlid/dev/sos/artifacts/device-gates/core-dev-smoke-20260818T184949Z-rfct50egfcn/framebuffer.png`
is 205,090 bytes with SHA-256
`29b89df9c191f7189920f0d1c3453d141f6c250a90aadddcffd12e5c90bff374`.
This is a terminal failed smoke, not a provider, latency, or hardware PASS.

**Root cause / fix / invariants:** Receipt-time preflight consumed the armed
tunnel bit and allowed the offline fixed request, but `start_agent_prompt`
performed a second preflight using only the physical connectivity snapshot. It
therefore discarded the attempt-local tunnel decision after the shared Luau
dispatch and emitted the observed false `network_unavailable` terminal. The two
independent atomics also allowed an ordinary UI submission arriving between arm
and fixed-event delivery to steal the tunnel bit.

The Core-dev-only authenticated callback now arms one mutex-protected,
process-memory authorization. The render thread consumes it once while creating
only the exact fixed prompt and moves an explicit `CoreDevFixedTunnel` transport
mode into the attempt snapshot; ordinary submissions always use
`ValidatedNetwork` and cannot consume the authorization. Consumption clears the
pending state before every preflight, queue, dispatch, or terminal path, and a
process abort discards the memory-only state. The fixed constructor rejects a missing/consumed authorization, wrong
transport, non-OpenRouter provider, or changed prompt, and the later preflight
revalidates both the prompt and the attempt transport. Credential, busy and
provider identity are still rechecked. The sole model remains
`deepseek/deepseek-v4-flash-0731`; the serialized endpoint remains exactly
`http://127.0.0.1:37173`, the Node decoder and host bridge retain exact
`openrouter.ai:443`, and end-to-end TLS hostname/certificate validation is
unchanged. The authorization, prompt, proxy, development Node path and tunnel
log label are all compiled out of ordinary Core; Compat, Linux and ordinary UI
requests retain validated-network preflight.

A separate readiness handshake was rejected for this fix: only the confined
Core-dev child domain has authority to connect to TCP 37173, so probing it from
the UI host would widen SELinux authority, while launching an extra child would
expand the single fixed request protocol. Actual loopback, reverse, CONNECT and
upstream failures continue through the existing secret-free transport taxonomy
(`connect_refused`, `connect_reset`, `connect_timeout`, `network_unreachable`,
or TLS failure) rather than being mislabeled as physical-network preflight.

**Bounded host evidence:** The contract suite passed 21 tests both without and
with `core-dev-credential`, covering offline fixed-tunnel dispatch, ordinary
offline rejection, missing/forged/consumed state, one-use consumption before
early and terminal exits, exact received → dispatch → accepted → terminal
ordering, and sanitized markers. The cross-language credential protocol suite
passed 20 tests, including failed-smoke credential retention. Node rebuilt both
runners and passed 20 tests, including exact proxy/model/host/TLS policy and
content-free transport classification. The loopback CONNECT bridge passed three
tests. Pinned NDK 29 API-31 ARM64 checks and debug builds passed for ordinary
`core-native` and `core-dev-credential`; byte inspection found none of the fixed
tunnel prompt, proxy, Node path, or transport label in ordinary Core and found
all four in Core-dev. Cargo formatting, Bash syntax and diff hygiene passed. No
full product build, provider request, device operation, or hardware PASS occurred
in this implementation phase.

**Remaining risk / next paired host and device gate:** Rebuild and inspect fresh
ordinary Core and Core-dev artifacts in order, preserve each accepted OTA with
exact revision, byte size and SHA-256, and finalize and independently verify the
host manifest. Ordinary inspection must prove the development prompt, proxy,
Node path, credential endpoint and policy are absent; Core-dev inspection must
prove the fixed counterparts are present. Only after a new exact artifact,
serial and authorization envelope, require Core no-Zygote readiness and initial
`CONFIGURED`, then run one fixed smoke through the bridge. Require received →
dispatch → accepted → child → response/action/validation/activation → exactly
one terminal ordering on success, or a category-aligned transport terminal on
actual tunnel failure; also require credential retention, framebuffer, bridge/
reverse/child cleanup, scoped crash and enforcing-AVC scrutiny, deterministic
manifest verification, post-run readiness and the authorized soak. Do not retry
or clear the credential automatically.

## 2026-08-18 — Bind Core child launch identity and expose allowlisted spawn causes

**Goal / terminal r18 evidence:** Diagnose the first authenticated Core-dev
fixed-tunnel attempt that passed offline preflight but stopped before a child or
provider existed. On `RFCT50EGFCN`, exact revision
`sos.core1dev.e05f91bb6f0b.134e0611333f` (1,050,603,811 bytes, SHA-256
`597623eac15b177c835f2c9f612702a0356c96f2185ee0dc4413da00c527f376`)
emitted one queued → received → dispatch → accepted → start → terminal sequence
and failed after 1.178944128 seconds at `stage=child
category=launch_failure`. No provider or authority commit followed, reverse and
bridge cleanup passed, Core no-Zygote readiness stayed healthy, and the
memory-only credential remained `CONFIGURED`. The finalized continuation root
is
`/home/carlid/dev/sos/artifacts/device-gates/core-dev-20260818-18-rfct50egfcn-cont-01`;
its 613-byte independently verified manifest has SHA-256
`cf5aba5186ef8c16448fe6591fc226878cef50fd4cc6d246d1e2e3a6d14dabf8`.
The 190,540-byte framebuffer at
`artifacts/device-gates/core-dev-smoke-20260818T192516Z-rfct50egfcn/framebuffer.png`
has SHA-256
`7765fc280b63f866e88f16d3f3d92effe33a6ecc45eb1e8608e529ea85f87d5d`.
This is a clean device failure, not a provider or hardware PASS.

**Diagnosis / rejected hypothesis:** `launch_failure` is selected only when
Rust `Command::spawn` fails, before Node can open its script argument. The r18
runtime bytes contain both exact launch paths
`/system_ext/bin/sos-node-core-dev` and
`/system_ext/etc/sos-agent/agent-runner-core-dev.cjs`; inspected target-files
contain the 0755 development Node and the 0644 development runner at those
paths, with the exact `sos_node_core_dev_exec` file context and compiled
`sos_core_host` → `sos_core_dev_agent` transition. Therefore the r14 runner
rename did not leave the Rust launcher pointing at the ordinary filename and
cannot explain this pre-Node terminal. The actual kernel spawn cause is not
recoverable from finalized r18 evidence because the launcher collapsed every
`io::ErrorKind` into one safe error and logged no path identity. Guessing a
fallback path, scanning the image, widening SELinux, or reverting to the
ordinary executable/domain was rejected because each would weaken the
product-exclusive tunnel boundary without evidence.

**Implementation / invariants:** Ordinary and development executable path,
runner path, safe identity and expected domain now live in one feature-selected
Rust `CoreChildLaunchContract`, used by both `Command` and its fixed argument
array. This also corrects the successful Core-dev marker from the ordinary
`sos_core_agent` label to `sos_core_dev_agent`. A failed spawn now emits one
content-free marker with only the contract identities and an allowlisted cause:
`path_missing`, `permission_denied`, `resource_exhausted`, `unsupported`, or
`other`; no raw errno string, path supplied at runtime, key, prompt, response or
provider body can enter it. The device smoke collector retains that marker.
The lifecycle terminal remains `child/launch_failure`, so credential retention
and exactly-once attempt semantics are unchanged.

A cross-product contract test now ties each Soong module source and unique
installed filename to its product package selection, compiled Rust executable
and runner constants, SELinux executable context and domain transition, and the
ordinary or proxy-enabled Node entrypoint. It also proves the C++ native host
does not own a second child-launch path. Ordinary Core continues to select only
the ordinary runner and contains no development package, proxy symbol or
payload; Core-dev continues to select only its distinct proxy runner plus its
dedicated executable/domain. The OpenRouter-only authority, end-to-end TLS,
exact `deepseek/deepseek-v4-flash-0731` model, single-use peer-checked tunnel,
credential retention, Compat and Linux paths are unchanged.

**Bounded host evidence / remaining risk / next gate:**
`core-agent-contract-test` passed 23 tests both without and with
`core-dev-credential`; `check-core-agent-hardening`,
`check-core-dev-credential`, the 20-test cross-language protocol suite, the
host CLI mock and Bash syntax passed. `npm test` rebuilt both bundles and passed
20 tests. Pinned NDK 29 API-31 ARM64 checks passed for `core-native` and
`core-native,core-dev-credential` in 1.02 and 0.86 seconds. Cargo formatting
passed. No full product build, device access, or provider request occurred.

The source graph is internally consistent, but this telemetry milestone does
not claim the underlying device spawn now succeeds. The runner must build and
inspect fresh ordinary and Core-dev artifacts, preserving revision, byte size,
SHA-256 and an independently verified manifest. Ordinary inspection must prove
every development path, identity, proxy byte and policy symbol absent; Core-dev
must prove the exact dedicated executable, runner, domain transition, proxy and
new launch-failure marker present. After explicit authorization for that exact
Core-dev artifact and `RFCT50EGFCN`, run exactly one fixed smoke with the same
cleanup, readiness, credential-retention, screenshot, crash/AVC, soak and
manifest gates. On another launch failure, the new allowlisted cause and fixed
identities are the terminal evidence for a fresh implementation; do not add a
fallback, retry, or clear the credential automatically.

## 2026-08-18 — Support the Core device's Linux 5.10 FD boundary and install r20

**Goal / terminal r19 evidence:** Resolve the remaining pre-provider child
launch failure without removing the fail-closed inherited-descriptor boundary.
The one authorized fixed smoke on `RFCT50EGFCN`, revision
`sos.core1dev.e05f91bb6f0b.b08da7ae929f`, reached request acceptance and then
terminated after 1.179517887 seconds at `stage=child
category=launch_failure cause=other expected_domain=sos_core_dev_agent`.
Bridge/reverse cleanup, post-failure readiness and credential retention passed;
no child, provider or authority commit existed and no retry ran. The finalized
evidence root is
`/home/carlid/dev/sos/artifacts/device-gates/core-dev-20260818-19-rfct50egfcn-cont-01`;
its independently verified manifest has SHA-256
`a0cc7875b49caee683a1a6abfac2b384541938716084ed82c6df139ae9c3f4ca`.

**Root cause / rejected approaches:** `Command::spawn` runs the Core FD boundary
in a `pre_exec` callback before `execve`. That boundary called
`close_range(3, UINT_MAX, CLOSE_RANGE_CLOEXEC)`. The live device reports Linux
`5.10.239-android12-9`, an open-file soft limit of 32,768, and no launch-time
SOS AVC. The pinned Samsung kernel source confirms the cause directly:
`kernel/samsung/s5e8825/include/uapi/linux/close_range.h` defines only
`CLOSE_RANGE_UNSHARE`, while `fs/file.c::__close_range` rejects every other flag
with `-EINVAL`. Rust maps this callback error to `ErrorKind::Other`, exactly
matching r19. The packaged development Node is a valid AArch64 Android-31 ELF
and matches the staged source byte-for-byte at SHA-256
`e1e6cf7de807baea6fa1d2a81bd6da29d777ab08149645431ebbe283bda33607`;
the runner path, executable label and domain transition also match. This
disproves missing-path, invalid-ELF and SELinux-transition hypotheses.

Removing the FD boundary or falling back to `close_range(..., 0)` was rejected.
The latter would close Rust's private exec-error pipe before `execve`, allowing
an exec failure to look like a successful spawn. A launcher/path fallback was
also rejected because it would duplicate the already consistent product and
SELinux contract.

**Implementation / invariants:** `core_child_fds.rs` now keeps the atomic Linux
5.11+ CLOEXEC path. Only `EINVAL` or `ENOSYS` selects an Android-5.10-safe
fallback: `getrlimit(RLIMIT_NOFILE)` followed by async-signal-safe
`fcntl(F_GETFD/F_SETFD, FD_CLOEXEC)` across descriptors 3 through the soft
limit. Closed descriptors are skipped only on `EBADF`; every other failure
aborts spawn. This marks Rust's launch-error pipe close-on-exec without closing
it early, and every inherited GPUI, dma-buf, surface, input, service and device
descriptor is still removed by successful exec. There is no network, SELinux,
TLS, endpoint, model, credential, prompt or lifecycle relaxation. Ordinary
Core and Core-dev use the same hardened helper; development proxy code remains
absent from ordinary Core. The sole provider model remains
`deepseek/deepseek-v4-flash-0731`.

**Bounded tests and r20 host acceptance:** Both ordinary and
`core-dev-credential` contract variants passed 25 tests. New coverage proves
the exact `EINVAL`/`ENOSYS` fallback decision, rejects `EPERM`, marks a real open
descriptor CLOEXEC, ignores adjacent closed descriptors, preserves the
pipe-backed standard streams and preserves exec-failure reporting. The Core
hardening gate passed 25 tests and the development credential/product gate
passed 20 protocol tests, including production exclusion.

Full ordinary build/inspection passed in 230.37/18.73 seconds; the final
preservation rebuild/inspection passed in 236.27/18.63 seconds. The preserved
ordinary OTA is
`artifacts/host-gates/core-dev-20260818-20/artifact-set/sos.core1.e05f91bb6f0b.abf3316c338b-ordinary-ota.zip`,
revision `sos.core1.e05f91bb6f0b.abf3316c338b`, 1,022,193,395 bytes, SHA-256
`6018e63e60a12e387e34fb8cf5ebf8db297899b69243d325099e6600bfd1e79d`.
Core-dev build/inspection passed in 239.96/20.64 seconds. Its preserved OTA is
`artifacts/host-gates/core-dev-20260818-20/artifact-set/sos.core1dev.e05f91bb6f0b.ed5b8200ec30-dev-ota.zip`,
revision `sos.core1dev.e05f91bb6f0b.ed5b8200ec30`, 1,050,592,959 bytes,
SHA-256
`6dc93cb0d09e5163796fe77c16bb9ccd7086a40103620bdebd6460597483db85`.
The independently verified artifact manifest has SHA-256
`b8a1c805c7f9c12acb73111ea988c59a3704cfe9280fb9d185265819cf941c00`;
the independently verified complete host manifest has SHA-256
`00c4e981e5cbb983b1ff10ad624c77030019dbc99799ffc85bb0155d82292054`.

**r20 install evidence / next gate:** The exact Core-dev OTA was installed on
`RFCT50EGFCN` with `sideload-auto-reboot`, `wait-for-sideload` and exactly one
`adb sideload`; the steps completed in 2.44, 19.59 and 83.44 seconds and the
transfer reported `Total xfer: 1.00x`. Its inherent reboot returned ADB in
71.63 seconds. Exact no-Zygote readiness passed in 1.03 seconds, followed by
eleven successful readiness samples over a measured 312.02-second soak. The
memory-only credential is correctly `EMPTY`. The 182,351-byte framebuffer has
SHA-256
`b56ca69550dde22f99ab7403b70430936250557851adac8b91cac3eddb490f64`.
The finalized device-install evidence root is
`artifacts/device-gates/core-dev-20260818-20-rfct50egfcn`; its independently
verified manifest has SHA-256
`ca598ff38b1733081c01ff9b88919926e41417dd45fcd9c233a150c83fd25796`.
Boot retains the previously known no-Zygote `odrefresh` abort and early
`sos_core_host` denial while probing Android data; neither affected the exact
native surface/process/adapter readiness predicates during the soak.

The remaining gate is one hidden in-memory credential SET followed by one
fixed autonomous tunnel smoke. Require child start in `sos_core_dev_agent`,
CONNECT/TLS/provider completion, exact action validation and authority commit,
one sanitized lifecycle terminal, bridge/reverse/child cleanup, framebuffer,
credential retention, scoped crash/AVC review and a finalized independently
verified continuation manifest. Do not retry or clear on failure.

## 2026-08-18 — Restore Android's fwmark FD handoff before the Core network gate

**Goal / decisive r20 evidence:** Determine whether the r20 Core-dev terminal at
`stage=provider category=unknown` was an OpenRouter/provider failure or a broken
device network path before making another provider-level change. The first r20
fixed smoke proved the Linux-5.10 child-launch repair: Node started as
`sos_core_dev_agent`, accepted the exact
`deepseek/deepseek-v4-flash-0731` request, received its stdin frame and returned
one protocol-v2 failure while the in-memory credential remained `CONFIGURED`.
It did not prove CONNECT or TLS. A content-free bridge event log was therefore
added for connection acceptance, fixed-authority validation, upstream connect,
first encrypted transfer in each direction and terminal byte counts. It never
records CONNECT headers, TLS bytes, credentials, prompts or provider bodies.
Three bridge tests prove malformed/non-allowlisted rejection, bidirectional
relay and absence of synthetic payload bytes from output; the host CLI mock
also passes.

The single instrumented retry on exact r20 revision
`sos.core1dev.e05f91bb6f0b.ed5b8200ec30` failed after child request write with
the same safe `provider/unknown` terminal, but its bridge event file is exactly
zero bytes: the device never opened TCP 37173. The finalized smoke root is
`artifacts/device-gates/core-dev-smoke-20260818T213450Z-rfct50egfcn`; its
independently verified 10-file manifest has SHA-256
`da6f3ec13168d8ce05bc08d8ca0b82b30596ecfbadcf70a21597e92c6c721fcb`.
The framebuffer is 180,944 bytes with SHA-256
`ffa5a7dcee44a6c71b44be04d50fefc63e0dfcd39f97247cfbb3f27cefddb991`,
reverse/bridge cleanup passed, and the key remained `CONFIGURED`. Separately,
host lookup and TCP connect to `openrouter.ai` succeeded, `curl
https://openrouter.ai/api/v1/models` returned HTTP 200 with certificate
verification enabled, and `openssl s_client -verify_return_error` completed a
TLS 1.3 handshake with `Verification: OK`. The bundled Pi catalog also reports
the exact campaign model available. OpenRouter, host internet, model lookup and
post-CONNECT TLS are therefore rejected as causes of this zero-connection
failure.

**Root cause / fix / boundary:** The live enforcing audit at the same child
attempt records `netd` denied `{ use }` on an fd owned by
`sos_core_dev_agent`. Bionic connects to Android's `fwmarkd`; that service
passes the caller's already-created TCP fd to netd for socket marking before
the kernel connect. The custom agent domains deliberately do not carry the
broad `netdomain` attribute, so they did not inherit platform policy's
reciprocal `allow netd netdomain:fd use`. The denial occurs before a SYN or ADB
reverse connection and exactly explains the empty bridge trace. The separate
`net_dns_prop` read denial is the structurally rejected obsolete fallback and
is not required by the fixed numeric loopback proxy.

Both ordinary and development Core agent policies now grant only `allow netd
sos_core_agent:fd use` or `allow netd sos_core_dev_agent:fd use`. This lets
netd act on a socket the confined child already owns; it does not let the child
use netd fds, add `netdomain`, read DNS properties, create useful UDP traffic,
connect to another port, or expand the ordinary/dev destination allowlists.
Ordinary Core remains limited to labeled TCP 443, while Core-dev adds only the
fixed labeled loopback TCP 37173. The focused CIL fixture and cross-product
source contract require both reciprocal rules. The hardening gate passed 25
tests, the Core-dev credential/product gate passed its 20 protocol tests, the
host CLI mock passed, and the bridge suite passed three tests.

**r21 host acceptance:** Fresh paired ordinary and Core-dev builds and full
inspectors passed. Ordinary build/inspection took 238.58/18.61 seconds; its
preserved OTA is
`artifacts/host-gates/core-dev-20260818-21/artifact-set/sos.core1.e05f91bb6f0b.02bb8ff9a883-ordinary-ota.zip`,
revision `sos.core1.e05f91bb6f0b.02bb8ff9a883`, 1,022,188,681 bytes, SHA-256
`d2c595bc62eafc44ce873d66611e329608b81785846d5b868573d408db5fe51c`.
Core-dev build/inspection took 246.49/19.68 seconds; its preserved OTA is
`artifacts/host-gates/core-dev-20260818-21/artifact-set/sos.core1dev.e05f91bb6f0b.b36b60685b37-dev-ota.zip`,
revision `sos.core1dev.e05f91bb6f0b.b36b60685b37`, 1,050,593,972 bytes,
SHA-256
`01627cc3165bd949836065e41e1465a9964b86846298a4794e048d414fe12a40`.
Both inspectors passed packaging, AVB, recovery, runtime/model and product
separation gates; the development target CIL contains the exact new netd fd
rule. The independently verified artifact manifest has SHA-256
`f58e0f62284342ebb2c0e0c081c07b8a8de7ad9af41a8267912e51985e956315`;
the independently verified 15-file host evidence manifest has SHA-256
`52a7b253571a3e61cdae9eeab6d6f5c75af2746d0dab4ffe3ff587ccfabbfdb3`.

**r21 install / next gate:** The exact development OTA was installed once on
`RFCT50EGFCN` using `sideload-auto-reboot`; recovery became ready in 29.43
seconds after the 2.39-second reboot command, sideload completed in 82.47
seconds with `Total xfer: 1.00x`, and the inherent reboot returned ADB in 78.55
seconds. Exact r21 no-Zygote readiness passed in 1.04 seconds with the `SOS Core
Experience` surface, supervisor/experience, authority and platform adapter,
current native lifecycle, and no relevant crash or enforcing AVC. The
memory-only credential is correctly `EMPTY`. Install evidence is accumulating
under `artifacts/device-gates/core-dev-20260818-21-rfct50egfcn` and remains open
until its final framebuffer and manifest are captured.

The next and only provider gate is to inject the hidden memory-only key once,
then run one instrumented fixed smoke. Require a bridge connection, exact
CONNECT acceptance, upstream connection and encrypted bytes in both
directions before interpreting any provider terminal. On success also require
the complete Pi action/validation/activation sequence and authority commit;
on failure preserve the safe terminal plus bridge phase. In both cases retain
the credential, clean bridge/reverse/child state, recheck exact readiness and
scoped AVCs, capture the framebuffer, then finalize and independently verify
all evidence. Do not clear or retry automatically.

## 2026-08-19 — Complete the Core OpenRouter E2E with a shared JITless transport

**Goal / r21-r22 network evidence:** Finish the fixed Core development smoke on
`RFCT50EGFCN` without weakening the no-Zygote child, TLS, credential, model or
destination boundaries. The first r21 image with reciprocal `netd` fd-use
permission progressed past the original fwmark handoff, but the live audit then
showed `netd` denied `{ read write }` on the TCP socket owned by
`sos_core_dev_agent`. r22 therefore adopted the complete narrow passed-socket
permission set used by Android's platform network domains: `netd` may use the
agent fd and read, write, get/set attributes, and get/set socket options on that
agent-owned TCP socket. It does not add `netdomain`, DNS-property access, UDP,
another destination, another port, or any agent access to netd-owned fds. The
same reciprocal rule is present for the ordinary Core agent domain.

Fresh r22 Core-dev build and inspection passed in 243.51 and 19.16 seconds.
The preserved artifact is
`artifacts/host-gates/core-dev-20260818-22/artifact-set/sos.core1dev.e05f91bb6f0b.c8d621d5d06e-dev-ota.zip`,
revision `sos.core1dev.e05f91bb6f0b.c8d621d5d06e`, 1,050,607,629 bytes,
SHA-256
`83a25e8a1a95d07756d05eac0370b09c27dedb680782af7e5d05d8ec52d7aa44`.
Its independently verified artifact and host manifests have SHA-256
`15802840bd95217c3b8d87d8dc46cc0b73be1c2182e4afcd2a6a51b9a2df29fa`
and
`d819b5b7ed8c20153e99fbd480bc8f9bf2200cf3cbe447e56b1cf906106a71ed`.
The exact OTA installed once with autonomous sideload and reached exact native
readiness; its finalized 25-file install manifest at
`artifacts/device-gates/core-dev-20260818-22-rfct50egfcn/manifest.tsv` is 2,320
bytes with SHA-256
`6c45e6fb23f5bc21b807ceedef6124d287a2c9846d31ab5fbe7100620979d952`.

The r22 instrumented smoke disproved another network-policy failure. The bridge
accepted the ADB-reverse connection but then recorded
`phase=read_connect category=connection`: the child closed the socket before
sending any CONNECT bytes. There was no new relevant socket AVC, the sanitized
runner terminal remained `provider/unknown`, reverse cleanup passed and the
memory-only key remained `CONFIGURED`. The finalized evidence root is
`artifacts/device-gates/core-dev-smoke-20260818T221428Z-rfct50egfcn`; its
manifest has SHA-256
`2373af93bb1936196766a7a42185e76e401c2fa7f8420b8ba23e4adeee931bd5`
and its 181,308-byte framebuffer has SHA-256
`0f4798e9722f23321042f7ec6d8543fac584ab17564d346980fbf4991f5050ea`.

**Root cause / rejected approaches:** A real host reproduction using the exact
bundled transport showed that `undici.ProxyAgent` under `node --jitless` opens
and closes the proxy connection without writing CONNECT. Direct Undici fetch
under `--jitless` progresses through TLS and then fails because Undici 7's
`lazyllhttp` parser requires WebAssembly, while JITless disables WebAssembly.
The same test without `--jitless` sends CONNECT and TLS. This exactly explains
the r22 bridge trace and rejects OpenRouter availability, the key, DNS, the
fixed port, ADB reverse and SELinux as causes. Disabling JITless, enabling
WebAssembly/JIT, allowing plaintext provider traffic, broadening the child
network domain, or weakening the fixed authority validator were rejected. A
separate real-client test found that a standards-compliant CONNECT request may
send `Host: openrouter.ai` when port 443 is implicit; the bridge now accepts
only that form or `Host: openrouter.ai:443`, while still requiring the exact
CONNECT target `openrouter.ai:443`.

**Shared implementation:** The provider transport used by Linux, Compat and
Core now defaults to `node-fetch` 3.3.2, which uses Node's native HTTP parser
and works under JITless. A small shared adapter preserves Request method,
headers, signal and body and converts the Node response stream to a Web
`Response`/`ReadableStream`, matching Pi/OpenAI SDK streaming expectations.
Core-dev alone supplies `HttpsProxyAgent` 7.0.6 and fail-closed redirect policy
for the exact `http://127.0.0.1:37173` development proxy; its adapter still
rejects every URL except HTTPS `openrouter.ai` on implicit or explicit port
443. Ordinary Core contains neither the fixed proxy value nor proxy agent. The
runner remains `--jitless`, only
`deepseek/deepseek-v4-flash-0731` is accepted, TLS remains end to end from the
confined child to OpenRouter, and no request, response, prompt or credential
content is exposed to the host bridge or evidence.

Host acceptance rebuilt both runner bundles and passed 21 Node tests, including
Request/body preservation, Web-stream conversion, Core-dev-only proxy bytes,
and absence of a WebAssembly marker. A real `node --jitless` integration test
proved native HTTP sends the exact CONNECT request and starts TLS through the
content-free bridge. The four-test bridge suite, 25-test Core hardening gate,
20-test credential/protocol gate, Bash syntax and CLI mock all passed. npm's
locked 102-package graph reported zero vulnerabilities.

**r23 artifact / install:** Fresh Core-dev build and full inspection passed in
242.04 and 19.44 seconds. The frozen OTA is
`artifacts/host-gates/core-dev-20260819-23/artifact-set/sos.core1dev.e05f91bb6f0b.0be956df8e63-dev-ota.zip`,
revision `sos.core1dev.e05f91bb6f0b.0be956df8e63`, 1,050,419,185 bytes,
SHA-256
`52eef9501d455587839f15f647c22408c19a8767e6f66716952b4d1f44379c56`.
The independently verified artifact manifest is 127 bytes with SHA-256
`33a744e9ce4a9196c6f0d31b6b8826fa6401de0f82660fb3f08394a3144500e5`;
the complete eight-file host manifest has SHA-256
`5ee93a6ad948f25c3b5f5e83a9d9b34275823c76d8d38e30340752d66f5aca01`.

The exact r23 OTA was installed once on `RFCT50EGFCN` with
`sideload-auto-reboot`. The reboot command, recovery readiness, sole sideload,
inherent-reboot ADB return and exact readiness took 2.27, 29.43, 80.33, 87.80
and 3.33 seconds; sideload reported `Total xfer: 1.00x`. Readiness proved the
exact revision, `SOS Core Experience`, supervisor/experience child, authority,
platform adapter and current native lifecycle with no relevant crash or
enforcing AVC. The expected post-reboot credential state was `EMPTY`. The
181,916-byte framebuffer has SHA-256
`6928c416a53dbebad274c4f268d636f943eba829433988b2c5e83bf06f0403cc`.
The independently verified 26-file install manifest at
`artifacts/device-gates/core-dev-20260819-23-rfct50egfcn/manifest.tsv` is 2,417
bytes with SHA-256
`ef50d29aed3c96e79e0b0dd3ad47a165088c2e36b8c8ac52e6345b8d23c765fc`.

**Live E2E PASS:** After one hidden in-memory key set, exactly one fixed smoke
completed. The confined `sos_core_dev_agent` child used the pinned model and
exited 0 with protocol-v2 `prompt_complete`. Pi produced and the native host
verified exactly `get_experience_context`, `validate_experience`, and
`submit_experience`. Four fixed-authority bridge connections each recorded
CONNECT acceptance, upstream connection, encrypted relay in both directions
and content-free terminal counts: device/upstream byte pairs were
52,442/8,264, 69,432/39,776, 87,185/39,100 and 104,851/13,825. Candidate
validation, authority staging and the final
`android_agent_activation_commit ... phase=committed authority=system` all
exist; the resulting `Blue smoke check` card is visible on the device. The key
remains `CONFIGURED`, no reverse mapping remains, and exact readiness again
passes without a relevant crash or enforcing AVC.

The primary finalized smoke root is
`artifacts/device-gates/core-dev-smoke-20260818T223442Z-rfct50egfcn`; its
10-file, 858-byte manifest has SHA-256
`7ed5677e72d99141f147eab039b02db0ecdc9536028cc91dc228384c29feb426`
and its 183,391-byte framebuffer has SHA-256
`7f4d5694ac63147a758f4157a6733f1d315f42afbf23bb922ba1ffe57a7df78b`.
The collector initially stopped at the successful UI terminal just before the
asynchronous authority commit log. The read-only continuation at
`artifacts/device-gates/core-dev-smoke-20260818T223442Z-rfct50egfcn-cont-01`
captures that exact commit, retained key, empty reverse list, post-commit
readiness and framebuffer. Its independently verified six-file, 528-byte
manifest has SHA-256
`892190da0257bb9e5d551702f14834bd7ac21a9a76e3605642ad299cb37a2b63`.
The smoke collector now reports `COMPLETED` only after both the exact successful
UI terminal and system authority commit; its host mock asserts the commit and
all focused gates pass after the correction.

**Decision / remaining gate:** The Core development OpenRouter E2E gate passes,
and the shared JITless transport is selected for all three runtime hosts. The
credential remains deliberately memory-only and retained across failures or
successful attempts, but still clears on reboot. The latest adapter has direct
host coverage and Core hardware evidence; the prior Compat hardware E2E passed
the same pinned Pi/action/authority contract before this transport replacement.
Before declaring the transport replacement hardware-accepted across every
product, rebuild/inspect Compat and repeat its fixed device E2E on this exact
shared adapter. Also rebuild/inspect ordinary Core to close its packaging
exclusion gate; neither step should change the proven Core-dev image. The
keyboard product milestone can then replace LatinIME as Compat's default with
the protected SOS IME while retaining explicit third-party Android IME opt-in.
