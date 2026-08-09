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
