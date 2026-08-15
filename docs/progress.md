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
