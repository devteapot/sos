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

## 2026-08-21 — Prepare a reversible Framework Laptop 12 Linux hardware gate

**Goal / environment:** Turn the completed virtual Linux envelope into a safe,
repeatable first physical campaign for a Framework Laptop 12 without installing
the boot-owned appliance target or making a hardware claim from this host. The
implementation host was the Framework Desktop (AMD Ryzen AI Max 300 Series),
Fedora 44 Server, kernel `6.19.10-300.fc44.x86_64`, at clean base revision
`d88b4d441282` plus this feature change. No Framework Laptop 12 was attached and
no GDM, seat, DRM, input, suspend, reboot, or other physical transition ran.

**Changed:** `tools/install-linux-login-session` now has a non-mutating doctor
with Fedora and Debian/Ubuntu direct-session package guidance, an explicit
offline install mode, exact source/toolchain/install artifact metadata, and a
bounded uninstall that preserves user state, GDM, packages, and the default boot
target. The selectable login can run the checked-in `daily-flow.luau` through
the real resident Pi/faux path without credentials or network access, while
retaining the same broker, validation, submission, activation, and monitored
lifecycle. Live mode still requires the existing credential ceremony. The
session also carries a private persistent `output.json` into the compositor so
the target can select a bounded mode, scale, and rotation without changing the
desktop entry.

Added `tools/linux-hardware-gate` with `prepare`, `collect`, `audit`,
`finalize-manifest`, and `verify-manifest`. Preparation refuses virtualization,
dirty or revision-mismatched installed artifacts, invalid output configuration,
the wrong requested DMI product, inactive display-manager state, and missing
offline/live agent prerequisites. It captures the exact installed artifact
identities, OS/kernel/BIOS/CPU/GPU/DRM/EDID/libinput/toolchain environment, and
a journal cursor. Collection preserves the bounded user and kernel journal
interval, monotonic duration, durable current/authority revisions, and fallback
display-manager state. PASS requires a recovery-view DRM page flip, direct
session readiness, the configured agent, physical keyboard/touchpad/touchscreen
classes, two distinct page-flipped revisions in one unchanged host lifecycle,
durable authority agreement, clean logout, restored GDM, and no matching SOS or
kernel GPU fault. Finalized evidence receives a deterministic path/byte/SHA-256
manifest and independent verification. The complete operator and evidence
contract is in `docs/linux-hardware-gate.md`.

**Evidence / measurements:** One clean ordered host campaign ran `bash -n` on
all six changed/new shell programs, both new host suites,
`desktop-file-validate`, ShellCheck 0.11.0, the agent TypeScript check, the full
12-test agent suite, `tools/linux-agent-e2e`, and
`git diff --check`. It passed in 9.53 seconds wall time (14.86 seconds user,
2.16 seconds system). The real faux agent used only
`get_experience_context`, `validate_experience`, and
`submit_experience`, then activated revision `2303ba94d140…` from
`31f8e1d31b6e…`. No live model ran, so live-model cost was zero. ShellCheck
used container image ID
`02e9c7c59449ae12d76eb53d4d32f2c428c22b28154833b579ad9ddef362cee2`,
and passed all six scripts with no diagnostics. The Fedora doctor intentionally
exited 1 on this non-GDM build host and named the exact missing `gbm`, `libinput`,
`libseat`, `libudev`, `wayland-client`, `xkbcommon`, and `xkbcommon-x11`
modules plus their Fedora packages. The ignored rebuilt agent bundle is
`services/sos-agent/dist/agent-runner.cjs`, 1,878,811 bytes, SHA-256
`3eee6e7922fb82e344277793a435bb8edd36a2c183050b638a3c6ca13d3bc99a`.

**Failures / fixes / decision / next gate:** The first audit test exposed a
Bash local-initializer dependency under `set -u`; separating dependent local
assignments fixed it. Expected tamper rejection initially left verifier scratch
files in `/tmp`; process-substitution comparison removed that failure path and
the six test-created scratch files were deleted. A mocked selectable-session
test now proves offline startup passes `--fake-source` without a credential path
and that live startup still rejects missing credentials. A synthetic evidence
test proves the full PASS contract, rejects missing touchscreen evidence, and
rejects a post-finalization byte change. Accept this as hardware-gate readiness
code only. Commit the branch, install its clean exact revision on Fedora
Workstation on the Framework Laptop 12, keep SSH/text-console recovery, and run
the documented clamshell `prepare -> physical interactions -> collect` campaign.
Physical Intel KMS/panel/input behavior, stylus, rotation, suspend, latency,
thermals, and soak all remain open until captured on that target.

## 2026-08-24 — Pin Linux hardware-gate evidence to one kernel boot

**Goal / environment:** Close the merge-readiness gap in the Framework Laptop
12 gate where `prepare` and `collect` used monotonic timestamps without proving
they came from the same kernel boot. Validation ran on the Framework Desktop
Fedora host at base revision `4e1323e72b08` plus the same-boot working change;
the exact diff SHA-256 was
`1f05ebaec08e5d4963c10ad6457527f816db1c8bfba7a32854228486989f4665`.
No GDM, seat, DRM, input, reboot, or other physical transition ran.

**Changed:** `tools/linux-hardware-gate prepare` now records the validated
lowercase kernel boot ID with its journal cursor and starting monotonic time.
`collect` rejects a missing or different boot ID before reading journals or
subtracting clocks, records the current ID again, and `audit` independently
requires the two IDs to match. The focused host test proves both the matching
PASS and cross-boot rejection. The operator contract now lists same-boot
preparation and collection as mandatory.

**Evidence / measurements:** One ordered host campaign ran Bash syntax checks
for all six gate-related shell programs, both host suites,
`desktop-file-validate`, ShellCheck 0.11.0, the agent TypeScript check, all 12
agent tests, `tools/linux-agent-e2e`, and `git diff --check`. Every criterion
passed in 23,027,708,497 ns measured monotonic wall time. The faux Pi path
activated revision `2303ba94d140…` from `31f8e1d31b6e…`; no live model ran, so
model-weighted cost was zero. The finalized log is
`/home/carlid/pr10-same-boot-acceptance-20260824/host-campaign.log`, 6,906
bytes, SHA-256
`b2133502b1aa21993558ce4b5e5aa274773ffa779f4cbafb32ee2907a35700a2`.
Its 136-byte evidence manifest is
`/home/carlid/pr10-same-boot-acceptance-20260824/evidence-manifest.tsv`,
SHA-256
`eace01fefc751365d46f798427ddd6932c7ebf0d94537a478503540244fe2357`.

**Failure / decision / next gate:** Review rejected a merely positive duration
as proof of one campaign because the monotonic clock resets at reboot and can
again exceed the earlier boot's timestamp. No implementation or validation
command failed after adding the boot-ID boundary. Accept the updated host code
as merge-ready evidence tooling only. Physical Intel KMS/panel/input, GDM
lifecycle, and exact-target evidence remain open until the clean merged
revision is installed and the documented same-boot campaign runs on the
Framework Laptop 12.
## 2026-08-23 — Add a Fedora live remix path for first Framework 12 evidence

**Goal / environment:** Make the first Framework 12 Linux loop rebuild ISO,
boot live, prepare, select SOS in GDM, collect on that boot, and copy evidence
off, without loosening the hardware-gate PASS contract or calling a live boot
an installed product. The implementation host was Ubuntu 24.04.4 x86_64, not
Fedora, and no Framework Laptop 12, GDM, seat, DRM, or live ISO bake ran.

**Changed:** `tools/linux-live-image` remixes an official Fedora Workstation
live ISO by staging the existing `install --offline --destdir` output, runtime
packages (not `-devel`), GDM, the SOS session, the offline agent, and the
hardware-gate harness. It does not compose Fedora from lorax/kiwi/kickstart
and does not enable the boot-owned appliance target. Image identity records
live-boot labels, source revision, base ISO identity, and squashfs/erofs
payload hashes. `tools/linux-hardware-gate` now classifies live-boot versus
installed-workstation, pins live-boot from baked identity, refuses stock live
media and install-to-disk of a live remix, and records live-versus-installed
fields without changing audit criteria. Operator docs and README state that
live-boot evidence is not an installed product.

**Evidence / measurements:** One clean host campaign ran `bash -n` on the five
changed/new shell programs, both hardware-gate and live-image host suites,
ShellCheck 0.9.0, `git diff --check`, `tools/linux-live-image doctor`, and
`classify-boot --sysroot` of an empty tree. It passed in 1.346 seconds wall
time. Doctor reported `host=ubuntu`, named the missing ISO/EROFS tools, and
still exited 0 for layout checks. Classify-boot labeled the empty sysroot
`boot_kind=installed`. Synthetic audit still emits the exact PASS line for
both boot kinds and still FAILs missing touchscreen input. No ISO was built
and no hardware claim is made.

**Failures / fixes / decision / next gate:** Payload hashes stay on the ISO
filesystem (`/sos-image-identity.env`) rather than inside the squashfs they
describe, so the pin is not self-referential. `readlink -f` against a destroot
resolved host paths and was replaced with raw symlink basenames. Accept this
as live-image tooling and harness labeling only. Bake the remixed ISO on a
Fedora x86_64 host from a clean revision, boot it on the Framework Laptop 12,
and run the documented same-boot `prepare -> physical interactions -> collect`
loop. A live-boot PASS remains live-boot, not installed product. Physical
Intel KMS/panel/input behavior, persistence, disk install, stylus, rotation,
suspend, latency, thermals, and soak remain open.

## 2026-08-24 — Pin the diskless Framework gate to real Fedora live media

**Goal / environment:** Keep the first Framework Laptop 12 Linux campaign off
its internal disk while correcting the unverified multi-format live-remix path.
The review host was Fedora 44 Server x86-64, kernel
`6.19.10-300.fc44.x86_64`, at PR revision `e5da24f1f60c` plus this rework. No
Framework Laptop 12, physical GDM session, DRM/input transition, or internal
target disk was used. The host had no non-interactive sudo authorization and
lacked the native SOS development modules, so no privileged rootfs mutation or
complete SOS ISO bake is claimed.

**Changed:** `tools/linux-live-image` now requires the SHA-256 obtained from
Fedora's signed CHECKSUM, a Fedora x86-64 build host at the ISO's exact Fedora
release, every image/build command, and every native compile module. Inspection
is pinned to a flat EROFS rootfs at `LiveOS/squashfs.img`, matching the official
Fedora 44 Workstation media despite that historical filename. The mutation path
uses privileged `fsck.erofs --xattrs --preserve`, runs the existing offline
destroot install, reapplies the image's SELinux file-context policy, repacks as
root, verifies the rebuilt EROFS, and re-implants and verifies the ISO media
checksum. It refuses another container/rootfs layout instead of silently losing
UID/GID, permissions, capabilities, xattrs, or labels. Build work must be new
and is removed only from the exact bounded output path after success.

The hardware gate now requires the rootfs and ISO-level identities to agree on
release, revision, source ISO, agent mode, payload, and bake time. Live prepare
requires and hashes the mounted payload rather than treating that identity as
optional. Prepare records `/proc/sys/kernel/random/boot_id`; collect rejects a
different kernel boot for both live and installed campaigns. Live instructions
now invoke the baked `/usr/local/libexec/sos/linux-hardware-gate` path. Docs keep
the result explicitly `live-boot` / `not_installed_product=true` and describe
the removable-media-only operator path.

**Evidence / measurements:** Fedora's signed
`Fedora-Workstation-44-1.7-x86_64-CHECKSUM` verified with key
`36F612DCF27F7D1A48A835E4DBFCF71C6D9F90A6`. The transient source artifact
`/tmp/sos-pr11-fedora44.T3r9Wc/Fedora-Workstation-Live-44-1.7.x86_64.iso`
was 2,851,612,672 bytes with SHA-256
`1620295f6a00c27c3208f0c00b8ece4eab1ec69b9002152d97488bf26a426ddf`;
its embedded media check passed in 3.22 seconds. Its
`LiveOS/squashfs.img` was 2,487,484,416 bytes and the new exact payload check
reported `container_format=erofs-rootfs`. Targeted official-image extraction
preserved `root:root`, mode `0755`, and SELinux type `netutils_exec_t`; a
user-namespace EROFS round trip separately preserved a `1000:1000` file, mode
`0750`, and a user xattr. This does not substitute for the pending fully
privileged whole-rootfs metadata audit. `fsck.erofs` also accepted the complete
official payload used to validate the rebuilt-payload integrity boundary. The
same ISO's `dmsquash-live-root` mounts its backing device at
`/run/initramfs/live` and resolves `LiveOS/squashfs.img` there, confirming that
the ISO-level identity and payload paths used by the boot classifier are on the
live medium retained by Fedora's initramfs.

An ISO replay probe preserved volume ID `Fedora-WS-Live-44`, BIOS and UEFI El
Torito images, protective MBR, and GPT in 1.31 seconds. After a new embedded
checksum was implanted, `checkisomd5` passed in 3.04 seconds. The transient
`/tmp/sos-pr11-fedora44.T3r9Wc/replay.iso` was 2,851,930,112 bytes with SHA-256
`045745cb6547e216cdfee1747ab62afe635aa0bc33cbb5a8a2bd873304db9221`.
Both focused host suites passed; Bash parsing, ShellCheck 0.11.0 from container
digest `b9389b73c8f26f710a7171cb7d8848a34a9c1e07a7865e727c9ec4ce99f9a83f`,
the official-payload `fsck.erofs`, and `git diff --check` also passed. The final
ordered host campaign took 2.04 seconds wall time. No model provider ran, so
live-model and model-weighted gate cost were zero.

**Failures / fixes / decision / next gate:** The first review found that normal
user extraction of flat SquashFS/EROFS maps root-owned files to the builder UID
and cannot restore SELinux xattrs. An initial correction narrowed support to a
nested SquashFS/ext4 layout; inspection of the real signed Fedora 44 ISO rejected
that assumption because its nominal `squashfs.img` is a flat EROFS rootfs. The
final path therefore uses privileged EROFS extraction/repack plus explicit
relabeling. Focused tests then caught a command-substitution failure that had
masked a rejected payload and were extended to require the outer identity,
identity agreement, boot ID, installed collect path, media checksum, and EROFS
rootfs classification. Keep the PR draft until a Fedora 44 host with sudo and
the documented build modules completes one whole ISO bake, verifies the final
rootfs metadata and ISO sidecar, and boots it on the Framework Laptop 12 for the
same-boot physical `prepare -> SOS -> collect -> copy off` gate. That run may
write removable media but must not install to or mutate the laptop's internal
disk.

## 2026-08-24 — Validate staged private live-user state through sudo

**Goal / failure:** Close the remaining real-bake blocker in live-rootfs
validation without relaxing the mode of offline agent configuration. Although
the first hardening pass added a sudo fallback for reading a private config, an
ordinary unprivileged `[[ -f ... ]]` still ran before that fallback. The staged
`/etc/skel/.local` tree is root-owned and mode `0700`, so the builder cannot
traverse it and a real bake would report the config as missing.

**Changed / evidence / decision:** `live_image_require_exact_line` now performs
both existence and content validation directly when readable, or performs both
through sudo when the builder cannot traverse the path. The skel and optional
liveuser callers no longer preflight private files unprivileged. The focused
test locks the staged `.local` tree against ordinary traversal, retains a mode
`0600` config, proves that validation invokes privileged `test -f` and `grep`,
and requires `check-rootfs` to pass. Where the host supplies subordinate IDs
and a setuid-capable workspace, the suite repeats that check as namespace UID
1000 against an actual namespace-root-owned mode-`0600` fixture. Keep private
state private; do not weaken directory or file modes to make the image builder
able to read them. The next gate remains the complete privileged Fedora 44 bake
and removable-media-only Framework Laptop 12 campaign described above.

**Verification / measurement:** On the Fedora 44 review host, the subordinate-ID
namespace fixture ran rather than skipping. `tests/linux-live-image-test.sh` and
`tests/linux-hardware-gate-test.sh` passed, as did Bash parsing of the five
relevant scripts, ShellCheck 0.11.0 from container digest
`b9389b73c8f26f710a7171cb7d8848a34a9c1e07a7865e727c9ec4ce99f9a83f`, and
`git diff --check`. The ordered campaign took 2.15 seconds wall time. No model
provider ran, so live-model and model-weighted gate cost were zero.

## 2026-08-24 — Fix fragment-packed Fedora EROFS extraction after the first bake

**Goal / environment / failure:** Run the first complete privileged live-image
bake before writing removable media for the Framework Laptop 12. The Fedora 44
Server x86-64 build host was at clean revision
`f25b44935d91cc203f6565acb4f5cec28df0de34`, with `erofs-utils-1.9.2-2.fc44`
and a strict `tools/linux-live-image doctor` PASS. The signed Fedora source at
`/home/carlid/dev/sos/artifacts/linux-live-source/Fedora-Workstation-Live-44-1.7.x86_64.iso`
was 2,851,612,672 bytes with SHA-256
`1620295f6a00c27c3208f0c00b8ece4eab1ec69b9002152d97488bf26a426ddf`.
`xorriso` restored all 355 ISO-tree files in one second, then privileged
`fsck.erofs --extract` stopped before rootfs mutation because it tried to open
the pre-created extraction directory as the image's hidden packed-fragment
inode. The finalized failure log is
`/home/carlid/dev/sos/artifacts/linux-live-bake-attempt1.log`, 1,729 bytes,
SHA-256 `9471415cca20e0273beb4e058baddf34c82d7cee0344028dee83de6bbf31431f`.
No remixed ISO was produced, no removable media was written, and no Framework
or internal laptop disk was involved.

**Changed / evidence:** EROFS extraction now selects the filesystem root
explicitly with `fsck.erofs --path=/` while retaining privileged xattr, owner,
and permission preservation. It also rejects a nonempty extraction destination
instead of adding `--overwrite` and concealing stale files. A full probe against
the official Fedora payload is recorded below; the probe deliberately ran as
the ordinary builder with owner, permission, and xattr restoration disabled, so
it proves traversal and decompression compatibility only. The next privileged
bake remains responsible for the metadata-preservation gate.

With `--path=/`, the complete official root tree extracted successfully in
998.82 seconds with 33,412 KiB maximum RSS. It contained 155,630 paths and
`du --bytes --summarize` reported 6,709,518,634 bytes. The finalized probe log
at `/home/carlid/dev/sos/artifacts/linux-live-erofs-root-path-probe.log` is 51
bytes with SHA-256
`d1e1ee4fbb95c6145a00ac75bdb4216b1761ff7c60d1f11600fbaa9ca4d1015a`.
The rejected absent-destination attempt log at
`/home/carlid/dev/sos/artifacts/linux-live-erofs-absent-destination-attempt.log`
is 222 bytes with SHA-256
`30d9fadb6c6d6e6733db264d240df5e8c785afb3cf688b5cc3e509990ac1e50b`.
The live-image suite also builds a small `all-fragments` EROFS and requires the
explicit-root extraction to reproduce its file exactly. The live-image and
hardware-gate host suites, Bash parsing of all five relevant scripts, ShellCheck
0.11.0 from container digest
`b9389b73c8f26f710a7171cb7d8848a34a9c1e07a7865e727c9ec4ce99f9a83f`,
and `git diff --check` passed in one ordered 1.92-second campaign with 46,556
KiB maximum RSS. No model provider ran, so live-model and model-weighted gate
cost were zero.

**Rejected approach / decision / next gate:** Merely leaving the destination
absent was insufficient: `fsck.erofs` wrote the packed inode as a
3,662,513,055-byte regular file and then rejected it as the root directory after
18.06 seconds. Keep the explicit root selector and fail-closed empty-directory
check. After retaining the three finalized logs above, the bounded partial
output at `/home/carlid/dev/sos/artifacts/linux-live-image` was removed; it was
generated failed work and is not recoverable. Commit and push the fix, then run
one clean privileged bake. A successful host bake still does not close the
physical gate; the next gate is removable-media boot and the documented
same-boot `prepare -> physical interactions -> collect` campaign on the
Framework Laptop 12, without installing to or modifying its internal disk.

## 2026-08-24 — Relabel Fedora EROFS from policy instead of compose xattrs

**Goal / environment / failure:** Retry the complete privileged Fedora 44 live
bake at clean revision `478a8ed97a1fe1b0e6e142498752267f1be0e159` after
fixing fragment-packed traversal. The strict doctor and ISO-tree extraction
again passed, then EROFS extraction failed while setting `security.selinux` on
inode 12114131 with `EINVAL`. The finalized second-attempt log at
`/home/carlid/dev/sos/artifacts/linux-live-bake-attempt2.log` is 1,734 bytes
with SHA-256
`4c761c3479111a0ab742aa8eaa012e909162da3d5c2261bf4b8f2254a465a899`.
No remixed ISO or removable media was produced, and no Framework or internal
laptop disk was involved.

**Causal chain / changed:** `dump.erofs` resolves the failing inode to
`/usr/bin/nbdkit`. A read-only FUSE view of the signed official payload reports
its source label as `system_u:object_r:fusefs_t:s0`, while the Fedora 44 policy
for `/usr/bin/nbdkit` requires `system_u:object_r:bin_t:s0`. Restoring the
compose-filesystem label is therefore neither portable to the staging
filesystem nor the desired final state. The bake now mounts the EROFS payload
read-only and uses privileged `rsync -aHAXS --numeric-ids` to retain content,
numeric ownership, modes, timestamps, hardlinks, sparse layout, ACLs,
capabilities, and all applicable non-SELinux xattrs. It excludes
`security.selinux` and, consistently with rsync's superuser default, the
`system.*` namespace; `rsync -A` separately retains POSIX ACLs. After all
package and SOS mutations, the existing `setfiles` phase applies the rootfs's
own Fedora policy to the complete tree. The mount is bounded under the bake
work directory and has an EXIT cleanup before the work directory can be
removed. Before unmounting, a second metadata-only rsync dry run must report no
size, timestamp, owner, mode, hardlink, or ACL difference; normalized manifests
must report no capability or other included-xattr difference.

**Focused evidence / measurements:** Copying the official failing file through
the filtered rsync path preserved its bytes and omitted the stale `fusefs_t`
label in 0.08 seconds with 5,840 KiB maximum RSS. A subordinate-user-namespace
round trip then preserved mode `0750`, a hardlink, a user xattr, and
`cap_net_bind_service=ep` in 0.05 seconds with 5,708 KiB maximum RSS. The
finalized combined probe log at
`/home/carlid/dev/sos/artifacts/linux-live-xattr-rsync-probe.log` is 571 bytes
with SHA-256
`09db4e06f22bad30144ee67cd115129aea43f89ed49aaa638618bb6950f96ce7`.
The focused live-image test constructs a hardlinked mode-`0750` fixture with a
user xattr, conditionally gives it the same stale SELinux type, and requires
the copy to preserve every requested attribute except that source label; its
post-copy metadata audit must also be empty. The strict doctor, live-image and
hardware-gate host suites, Bash parsing of all five relevant scripts,
ShellCheck 0.11.0 from container digest
`b9389b73c8f26f710a7171cb7d8848a34a9c1e07a7865e727c9ec4ce99f9a83f`,
and `git diff --check` passed in one ordered 2.18-second campaign with 46,420
KiB maximum RSS. No model provider ran, so live-model and model-weighted gate
cost were zero.

**Rejected approaches / decision / next gate:** Continuing
`fsck.erofs --xattrs` would repeatedly fail on a compose label. Disabling all
xattrs would silently destroy capabilities and was rejected. Copying source
SELinux contexts and relabeling only after that is also unnecessary and blocks
the build before the authoritative policy phase. Stop full bake retries until
the new mount/copy layer and nearby host regressions are green. Then delete
only the bounded partial attempt-two output, commit and push the correction,
and run one fresh privileged bake. That downstream run must still prove the
whole-rootfs mount/copy, package mutation, policy relabel, EROFS repack, ISO
checksum, and identities before removable-media hardware testing begins.

## 2026-08-24 — Separate logical metadata and raw-xattr audits

**Goal / environment / failure:** Run the fresh downstream bake at clean
revision `f6ad4e9f69d967ddeca517f2760cac5f0969934d` after replacing direct
EROFS xattr extraction. The strict doctor, ISO-tree extraction, read-only EROFS
mount, and complete privileged rsync copy passed. The new dry-run audit then
failed on exactly `.d........x var/log/journal/`. The finalized bake log at
`/home/carlid/dev/sos/artifacts/linux-live-bake-attempt3.log` is 1,720 bytes
with SHA-256
`1e02477374a3ca0d264b3f475d91bce695829fa7153549cd0fbdc575b15ab46c`;
the 29-byte raw audit at
`/home/carlid/dev/sos/artifacts/linux-live-rsync-audit-attempt3.log` has
SHA-256 `f8ab10a71d8144e2f0004a0c587e48fc2e4b46950f6b54bce660af97609951e3`.
No remixed ISO or removable media was produced, and no Framework or internal
laptop disk was involved.

**Diagnosis / evidence:** Rsync's itemized `x` flag combined its raw-xattr view
with an ACL-bearing directory even though the copy intentionally filtered the
source SELinux label and handled ACLs separately. The source and destination
`/var/log/journal` both had numeric owner `0:190`, mode `2755`, identical access
ACLs, and identical default ACLs; only the expected staging SELinux context
differed. More importantly, the completed privileged copy retained the real
`security.capability` on
`/usr/libexec/gstreamer-1.0/gst-ptp-helper` as
`cap_net_bind_service,cap_net_admin,cap_sys_nice=ep`. Its source and destination
bytes both had SHA-256
`f3849ca6c51675c7365eb7b0bb048bc11f256aa09d069818ae83ef44744066a5`.
Thus the copy boundary passed and the combined audit, not metadata
preservation, was the earliest broken layer.

**Changed / decision / next gate:** Keep the fail-closed audit but split its
semantics. A metadata-only rsync dry run without `-X` now checks content size
and time, numeric ownership, modes, hardlinks, and logical ACLs.
Separate sorted `getfattr` manifests compare exact `user.*`, `trusted.*`, and
non-SELinux `security.*` names and values, including capabilities, while
deliberately excluding `security.selinux` and ACLs already checked logically.
The focused fixture requires both the metadata audit and normalized xattr
manifest comparison to be empty. Do not whitelist `/var/log/journal`, discard
the audit, or accept arbitrary rsync `x` differences. After nearby checks pass,
commit and push the correction. The next operator command must remove only the
bounded partial attempt-three output, then run one fresh privileged bake
through relabel and repack.

The strict doctor, live-image and hardware-gate host suites, Bash parsing of
all five relevant scripts, ShellCheck 0.11.0 from container digest
`b9389b73c8f26f710a7171cb7d8848a34a9c1e07a7865e727c9ec4ce99f9a83f`,
and `git diff --check` passed in one ordered 2.16-second campaign with 46,900
KiB maximum RSS. No model provider ran, so live-model and model-weighted gate
cost were zero. The pending privileged bake remains the next gate.

## 2026-08-24 — Ignore XFS's internal ACL xattrs in the portable manifest

**Goal / environment / failure:** Run the next privileged bake at clean
revision `dd97da735effa4392891049fbcc4e9df0b85601a` with separate logical
metadata and raw-xattr audits. The strict doctor, ISO-tree extraction,
read-only EROFS mount, complete privileged copy, and metadata-only rsync audit
all passed; the latter produced a zero-byte audit. The exact xattr-manifest
comparison then failed with 10 source entries and 12 destination entries. The
finalized bake log at
`/home/carlid/dev/sos/artifacts/linux-live-bake-attempt4.log` is 1,798 bytes
with SHA-256
`b839aa84bf97e6cedd0f88c88d7101368f6846abe683d3139860dd8f4b587717`.
No remixed ISO or removable media was produced, and no Framework or internal
laptop disk was involved.

**Diagnosis / evidence:** All ten source `security.capability` entries were
present byte-for-byte in the destination manifest. The only destination-only
entries were `trusted.SGI_ACL_FILE` and `trusted.SGI_ACL_DEFAULT` on
`var/log/journal`; XFS synthesizes these trusted xattrs from the POSIX ACLs that
the independent logical audit had already proved identical. The preserved
source manifest at
`/home/carlid/dev/sos/artifacts/linux-live-xattr-source-attempt4.txt` is 736
bytes with SHA-256
`6d8e758528fc1f6ef7947bff6c1fc9fa92ad0e3a8280abf15e93135b9faccf79`.
The destination manifest at
`/home/carlid/dev/sos/artifacts/linux-live-xattr-dest-attempt4.txt` is 1,027
bytes with SHA-256
`d33751772586b3eaa47bb7582e107744f3fa14ea57022062a660fb08cc7e032c`.
The zero-byte metadata audit is retained at
`/home/carlid/dev/sos/artifacts/linux-live-metadata-audit-attempt4.txt` with
the empty-file SHA-256
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.

**Changed / decision / next gate:** Exclude only the target-filesystem-internal
`trusted.SGI_ACL_*` encodings from the portable raw-xattr manifest. Continue to
compare every source `user.*`, other `trusted.*`, and non-SELinux `security.*`
value exactly, and continue to require the separate owner/mode/hardlink/ACL
audit to be empty. Filtering the two SGI entries from the retained destination
manifest made it identical to the retained source manifest: ten capabilities
matched and no portable xattr was missing or changed. Do not exclude all
`trusted.*`, weaken capability comparison, or whitelist a filesystem path.
After nearby host checks pass, commit and push the correction; the next
operator command must remove only the bounded partial attempt-four output and
run one fresh privileged bake through package mutation, relabel, and repack.

The strict doctor, live-image and hardware-gate host suites, Bash parsing of
all five relevant scripts, ShellCheck 0.11.0 from container digest
`b9389b73c8f26f710a7171cb7d8848a34a9c1e07a7865e727c9ec4ce99f9a83f`,
and `git diff --check` passed in one ordered 2.20-second campaign with 46,152
KiB maximum RSS. No model provider ran, so live-model and model-weighted gate
cost were zero. The pending privileged bake remains the next gate.

## 2026-08-24 — Preserve the active rootfs across offline-home staging

**Goal / environment / failure:** Run the next privileged Fedora 44 bake at
clean revision `1a658a866d0f8b9300175d113f13ee82a7c6c91e`. Rootfs extraction,
metadata verification, runtime-package installation, all Rust release builds,
and the agent TypeScript bundle completed. Destroot installation then tried to
measure
`/usr/local/libexec/sos-agent/dist/agent-runner.cjs` as the normal builder but
could not traverse its root-owned mode-`0700` `dist` directory. Staging the
offline skeleton subsequently failed while removing
`/tmp/sos-live-skel.9LdFDl/etc`, which had unexpectedly become root-owned. The
finalized log at
`/home/carlid/dev/sos/artifacts/linux-live-bake-attempt5.log` is 27,375 bytes
with SHA-256
`49e3de11b0e3b271ff73be60a9baf61077988a17402a3b9c06f41972a1d74ce7`.
The bounded output contains only `work/`; there is no ISO or image identity,
and no removable media or Framework disk was involved.

**Diagnosis / changed code:** `write-offline-user-state` reused the global
`live_image_rootfs` variable. Bash's function scoping therefore replaced the
surrounding bake root with its temporary home path; every subsequent
`live_image_target` call addressed that temporary directory, and privileged
copy setup created its root-owned `etc`. Root-option parsing now returns a path
instead of mutating shared state, offline-home writing uses a local
`home_root`, and rootfs validation uses a function-local root. The regression
sources the real tool, sets an active-root sentinel, calls the nested helper,
and requires the sentinel to remain unchanged. Separately, destroot publishing
now normalizes the agent code tree to `u=rwX,go=rX` after root ownership and
fails explicitly unless every manifest artifact is a readable regular file
with a valid nonzero size and SHA-256. A focused copy of the actual build tree
proved all directories traversable and files readable; the 1,878,811-byte
runner became mode `0755` with SHA-256
`3eee6e7922fb82e344277793a435bb8edd36a2c183050b638a3c6ca13d3bc99a`.

**Rejected approaches / decision / remaining risk / next gate:** Running the
whole bake as root would violate the builder boundary. Hashing the private
bundle only through `sudo` would make the manifest succeed while leaving the
desktop user unable to load the agent, and cleaning the corrupted temporary
tree through `sudo` would conceal the wrong destination. Keep the bake
unprivileged, publish runtime code readably, and reserve privilege for rootfs
mutation. The strict doctor, live-image and hardware-gate host suites, Bash
parsing of all five relevant scripts, ShellCheck 0.11.0 from container digest
`b9389b73c8f26f710a7171cb7d8848a34a9c1e07a7865e727c9ec4ce99f9a83f`,
and `git diff --check` passed in one ordered 2.13-second campaign with 47,380
KiB maximum RSS. No model provider ran, so live-model and model-weighted gate
cost were zero. The remaining risk is the downstream privileged integration:
remove only the bounded partial attempt-five output, then run one clean bake
through staging, relabel, EROFS repack, ISO replay, media checksum, and final
identity generation before writing removable media.

## 2026-08-24 — Assign image-policy SELinux labels during EROFS creation

**Goal / environment / failure:** Run the next privileged Fedora 44 bake at
clean revision `b618608a93e707efd2764911aeaa6f9a81dd99fe`. Extraction,
metadata verification, package mutation, cached release builds, readable
destroot installation, and both offline-home staging paths passed. The first
whole-root relabel then stopped while loading the image's file-context rules:
the build host's currently loaded targeted policy rejected
`nbdkit_exec_t` and `nbdkit_unit_file_t`. The finalized log at
`/home/carlid/dev/sos/artifacts/linux-live-bake-attempt6.log` is 8,563 bytes
with SHA-256
`4e1a7e830a7e4a448e228b4ec0c7899f4865627acd5c6f7bdb4965cd4262a496`.
The bounded output again contains only `work/`; no ISO or image identity was
produced, and no removable media or Framework disk was involved.

**Diagnosis / evidence:** This was not an invalid Fedora image policy. Running
`setfiles -n -m -c ROOT/etc/selinux/targeted/policy/policy.35 -r ROOT
ROOT/etc/selinux/targeted/contexts/files/file_contexts ROOT/usr/bin/nbdkit`
passed, proving the rootfs's own binary policy accepts the expected
`nbdkit_exec_t` rule. The installed `setfiles(8)` documents `-c` specifically
for checking contexts against another binary policy. A focused EROFS probe
used the rootfs file contexts with `mkfs.erofs --file-contexts`, produced a
180,224-byte image whose `/usr/bin/nbdkit` inode carried a 60-byte xattr area,
and embedded `system_u:object_r:nbdkit_exec_t:s0`. Attempting to restore that
label onto the host filesystem reproduced `EINVAL`, confirming why staging
tree relabeling cannot be the cross-policy boundary. A separate valid-context
probe extracted `system_u:object_r:bin_t:s0` exactly from the rebuilt EROFS.

**Changed / rejected approaches / decision / next gate:** Validate all file
contexts read-only against the rootfs's highest compiled `policy.*`, then pass
the same context file directly to `mkfs.erofs`; verify the rebuilt filesystem
with explicit xattr inspection. This keeps the staging tree's incidental host
labels out of the artifact while preserving ownership, modes, ACLs,
capabilities, and other portable xattrs. Loading the image policy into the
enforcing build host would be a global and unsafe mutation. Continuing to use
the host policy would reject valid image-only types, while retaining compose
or staging labels would make the live image incorrect. The focused host suite
now constructs an EROFS with a supplied file-context rule, requires the label
string in the image, and, when SELinux is active, extracts and checks the exact
`security.selinux` value; it passed in 0.55 seconds with 30,444 KiB maximum
RSS. The next gate is one fresh privileged bake through policy validation,
EROFS creation, ISO replay, embedded checksum verification, and final identity
generation before removable-media testing.

The strict doctor, live-image and hardware-gate host suites, Bash parsing of
all five relevant scripts, ShellCheck 0.11.0 from container digest
`b9389b73c8f26f710a7171cb7d8848a34a9c1e07a7865e727c9ec4ce99f9a83f`,
and `git diff --check` passed in one ordered 2.11-second campaign with 47,492
KiB maximum RSS. No model provider ran, so live-model and model-weighted gate
cost were zero. The pending privileged bake remains the next gate.

## 2026-08-24 — Add the missing direct-session Linux dmabuf path

**Goal / environment / failed gate:** Boot the Fedora 44 live remix at clean
revision `24dc85c2e29891d4072cd9674e656fcc05c97686` on the Framework Laptop 12
(13th-generation Intel Core, Raptor Lake-P UHD `8086:a721`, `i915`) and run the
first physical selectable-session gate. The exact ISO at
`/home/carlid/dev/sos/artifacts/linux-live-image/sos-fedora-workstation-live-24dc85c2e298.iso`
is 3,056,205,824 bytes with SHA-256
`cbbf9cca1bb70858713a9ae2ab5c3a9203f295ceeb46e82ce0710a983c2d9570`.
The same-boot campaign used boot ID
`77187e2c-d323-4a52-b88d-24d22a87bc33` and ran for
1,303,847,671,263 ns. Recovery and direct DRM page flips passed, but the shell
never became ready; the remaining agent, input, activation, lifecycle, and
logout criteria consequently failed. The finalized 937-byte verdict at
`/home/carlid/dev/sos/artifacts/linux-live-image/evidence/framework12-20260824/framework12-first-gate/verdict.txt`
has SHA-256
`882faf47c47cb1c3518cf2418c9e6d7c182a4cd935f57d80a0e9526a2a67024a`.
The associated 614,603-byte user journal has SHA-256
`4e08e4e406022a6db00226960026a4049ec202f33bd68a176355e4642ac092c2`.

**Earliest failure / rejected approaches:** The compositor logged that
`EGL_WL_bind_wayland_display` was unavailable and exposed only `wl_shm` to the
client. GPUI therefore reached Mesa's software path despite an available Intel
render node; Mesa 26.0.3 and LLVM 22.1.1 aborted in `fs_variant_partial` while
`libvulkan_lvp.so`/llvmpipe compiled the shell fragment shader. The revision
supervisor timeout and return to GDM were downstream. Temporarily forcing
`VK_DRIVER_FILES` to Intel changed Vulkan enumeration but retained the
software-OpenGL presentation boundary and reproduced the LLVM failure. Do not
ship that environment override or treat a Mesa/LLVM upgrade as the fix: either
would mask the missing Wayland buffer-sharing protocol rather than provide the
hardware client/compositor path.

**Changed compositor / focused physical evidence:** The direct backend now
publishes Linux dmabuf feedback independently of the optional EGL Wayland
binding. It advertises only the intersection of formats importable by all
renderers with connected outputs, selects the corresponding render node,
validates every client buffer against each active renderer, and refreshes the
feedback across connector/device changes; `wl_shm` remains available as the
fallback. A unit fixture requires three renderer format sets to collapse to
their sole common format. A 5,699,056-byte focused release build with SHA-256
`4de0c2131339b3f8b9ec04a12aacdfede2ed73227e99132eccf31cf481974f70`
was copied only into the live overlay. With both the login script and running
shell free of `VK_DRIVER_FILES`, the compositor advertised 240 formats on
`renderD128`, the experience host held Intel DRM file descriptors, revision
`31f8e1d31b6e2c91a8a0b0829e5f29934440c64ed8f535bb86d81a5a836c49e5`
produced its first compositor-owned page flip 415,636 microseconds after host
start, the system session became ready, and the offline agent started. The
25,750-byte diagnostic directory is retained at
`/home/carlid/dev/sos/artifacts/linux-live-image/evidence/framework12-20260824/framework12-dmabuf-diagnostic`;
its verified 242-byte manifest has SHA-256
`245c161354c9ddaf03f0b47de19c0d6307dfb76f590552f6ff06d6c86535f0c7`.

**Logout diagnosis / changed lifecycle:** The focused clean logout recorded
`linux_login_session_stopped reason=user_logout`, then six milliseconds later
a supervisor host proxy observed an already-removed `host-launcher.sock` and
emitted `linux_session_failed`. The launcher was a local in
`start_and_monitor`, so Rust dropped its socket before `run_system_session`
could stop the still-running supervisor. Session ownership now retains the
launcher alongside the child processes and drops it only after the supervisor
has stopped. This removes the shutdown race instead of weakening the hardware
gate's process-failure criterion.

**Host checks / decision / remaining risk / next gate:** One ordered campaign
ran formatting and diff checks, the compositor's 12 direct-backend tests, the
Linux session's seven unit/integration tests, warning-denying Clippy for both
packages, and the hardware-gate host suite. It passed in 5.84 seconds with
609,852 KiB maximum RSS. The finalized 5,949-byte log at
`/home/carlid/dev/sos/artifacts/linux-live-image/evidence/framework12-20260824/host-checks.log`
has SHA-256
`75a605c8e4091487b76607d7f982edb09c5a99050d6f5e9e6ce73525fcd0a282`.
No model provider ran, so live-model and model-weighted gate cost were zero.
The in-place binary experiment is diagnostic evidence, not a PASS for the
baked revision; the shutdown-order change, multi-GPU selection, physical input,
transactional activation, and final clean logout still lack artifact-matched
physical proof. Commit the fix, rebuild a clean revision-pinned ISO, boot it
fresh, and repeat prepare, all observed interactions, collect, manifest audit,
and same-boot verdict before promoting the Framework gate.

## 2026-08-24 — Gate agent configuration controls by platform capability

**Goal / physical observation / evidence:** Continue the focused Framework 12
live-overlay diagnostic after dmabuf startup succeeded and exercise ordinary
interaction. The compositor independently recorded native
`relative_pointer`, `pointer_button`, and `keyboard` input; the Linux host then
recorded 33 bounded text edits in the agent prompt. Selecting `CODEX SUB`,
`FAKE`, and `CODEX SUB` again produced action requests 34–36, each of which
reached the worker but failed commit as unsupported `agent.configure_codex` or
`agent.use_fake` effects. The bounded 21,440-byte journal at
`/home/carlid/dev/sos/artifacts/linux-live-image/evidence/framework12-20260824/framework12-dmabuf-diagnostic/journal-interaction.txt`
has SHA-256
`3f414fb9e87251bf60e9d917c7c05095486dbb9d058d1397ed763ca2109c29ca`.
The expanded 47,280-byte diagnostic directory verifies through its 332-byte
manifest, whose SHA-256 is
`ca4f67a1d4a952be8883e36d88f45564b06a6e188f5fcc392b45ef1be5bd87e7`.
These are focused physical observations on a modified overlay, not a complete
artifact-matched input gate; touch, activation, and the final verdict remain
open.

**Diagnosis / changed contract:** Linux provider selection is intentionally a
pre-session operation: `sos-agent-login` writes the private provider/model
configuration, and a new GDM SOS session starts and monitors exactly that
resident agent. Restarting or replacing it from a generated experience would
currently terminate the graphical session. The shared reference experiences
nevertheless rendered Android/Core credential buttons unconditionally, while
the Linux host correctly rejected their effects. `model.agent` now carries a
typed `configuration_actions` allowlist. Linux leaves it empty; Android Compat
publishes its five trusted credential actions; Core publishes only its pinned
OpenRouter, fake, and clear actions. Default, Daily Flow, and Timeflow render
only listed controls and otherwise explain that provider changes are managed
before login. Generated Luau still cannot bypass the trusted host's effect
validation.

The same check exposed an older decoder mismatch: the stock experience could
render capability-granted System Providers v1 controls, but Luau's bounded
effect decoder omitted audio volume/mute, media transport, app launch, and
attention acknowledgement. The decoder now accepts exactly those documented
typed actions; platform capability and adapter checks still decide whether an
individual request is authorized and executable.

**Rejected approaches / checks / next gate:** Do not turn `use_fake` into a
silent Linux no-op, launch a credential helper behind the direct session, or
weaken the host's unknown-effect rejection. Each would make rendered state or
process ownership disagree with the actual resident provider. A measured host
campaign passed five experience-IR tests, 22 Luau runtime tests, 26 Linux-host
experience tests, warning-denying Clippy, and NDK 29/API-31 Compat and Core
cross-checks in 10.24 seconds with 2,482,232 KiB maximum RSS. The finalized
10,087-byte log at
`/home/carlid/dev/sos/artifacts/linux-live-image/evidence/framework12-20260824/provider-controls-host-checks.log`
has SHA-256
`6c4d43c4eace55f09f4b99f97709fbe2a818f3caaad7fca30b3e7234755790c3`.
No model provider ran, so live-model and model-weighted gate cost were zero.
Build the next ISO only from the resulting clean committed revision, then repeat
the complete Framework prepare, SOS input and deterministic prompt activation,
clean logout, collect, audit, and copy-off sequence.

## 2026-08-24 — Complete a focused Framework prompt activation

**Goal / environment / physical result:** Continue the same focused Framework
12 live-overlay session on boot ID
`77187e2c-d323-4a52-b88d-24d22a87bc33` and test the transactional path after
the user independently confirmed that pointer and keyboard interaction behaved
normally. The 16-byte deterministic prompt `make this calmer` was submitted to
the offline fake agent. The agent fetched context, validated its generated
experience, and submitted revision
`c6d87d5809bbdc3a859ea4fc634f49d0588c3a9248fcc821eca67dcf26293e7f`.
Preparation measured 83 microseconds queued, 3,609 microseconds compiling,
1,629 microseconds rendering, and 5,246 microseconds total worker time.

**Transactional evidence / lifecycle:** The compositor quiesced input with
zero held keys, buttons, or touches and dropped no events, armed after commit
sequence 159,723, then presented the revision at commit sequence 159,724 and
submit sequence 431 with direct DRM page-flip evidence. The host reported that
frame 383,880 microseconds after text submission; the complete offline-agent
turn finished 400,536 microseconds after submission. A later same-boot sample
showed the experience host still at PID 45,075, with its original 16:19:06
start time, and the revision supervisor still reported the presented revision
as current. No experience-host restart occurred during activation.

**Evidence / decision / remaining risk / next gate:** The finalized
15,995-byte activation journal at
`/home/carlid/dev/sos/artifacts/linux-live-image/evidence/framework12-20260824/framework12-dmabuf-diagnostic/journal-activation.txt`
has SHA-256
`d32c7d03d247089c0fbedfa0a58fb12769009bb808e5f6fdfb7abf938153cb93`.
The 495-byte lifecycle sample has SHA-256
`726817c87482f38fd5f81feb5a02f2ba0f28750ade773c506d841bca262ab8d9`.
All six bounded files in the 63,948-byte diagnostic directory verify through
its 510-byte manifest, whose SHA-256 is
`caf98c4374738e48a82a092ba8152174318ce07ea482fb031ab8bfa55bba1a72`.
This closes focused physical pointer, keyboard, agent, revision-commit, direct
page-flip, and stable-host-lifecycle diagnosis. It does not promote the baked
ISO or complete the hardware gate: the compositor was replaced only in the
live overlay, the provider-control and logout fixes are not present in that
artifact, and touchscreen, corrected clean logout, image identity, collection,
and final manifest audit remain unproved. No live model provider ran, so model
cost was zero. Build one clean revision-pinned ISO containing all fixes, boot
it fresh, and run the full same-boot prepare/interact/collect/audit gate.

## 2026-08-24 — Run the revision-pinned Framework gate and isolate touch focus

**Goal / artifact / complete gate:** Boot the clean revision
`fb704784d5b35860c49d44c028ceb3a7fe7daf63` from the baked Fedora 44 live ISO
`/home/carlid/dev/sos/artifacts/linux-live-image-fb70478/sos-fedora-workstation-live-fb704784d5b3.iso`
(3,056,205,824 bytes, SHA-256
`c043f5657c68ea39e91006cb83fcf4cdb1013fcdf19cc1ef464f502632d48a91`)
on the Framework Laptop 12 and execute one same-boot prepare, physical input,
offline-agent activation, clean logout, collect, copy-off, and manifest audit
campaign. The 350,565,261,856-nanosecond campaign passed live-boot identity,
recovery and direct-compositor page flips, session/agent readiness, keyboard,
touchpad motion/button, touchscreen observation, clean logout, transactional
activation, fallback display manager, and kernel GPU checks. It correctly
failed overall: the host launched twice, the durable pointer remained at
`32f4b2a9c26f632bc20a3139d06a1b59aa9073e6513fabb7698566b669847a5c`
while authority reached
`0560f50dc390dc20c97db99d4a16ee45b11f2956315f927408a7a7b3b5dafcf6`,
and process-failure evidence was present.

The copied 107,405-byte directory at
`/home/carlid/dev/sos/artifacts/linux-live-image/evidence/framework12-20260824/framework12-fb70478-gate`
independently verifies 36 files when checked with the campaign's
`en_US.utf8` collation. Its 3,370-byte manifest has SHA-256
`a0605f305798af3cd11ad59a8d6c56454e450f3bcaeddba5edad6498c7cfa35f`;
the 1,008-byte verdict and 53,574-byte user journal have SHA-256
`01533d6a6a2263a01904d305710e3f3e533768efd273921b879fa0eff17d565b`
and `bc93e8583ec9879b37f6784326f5c88d463db18eae0edb3e08099a55703b973b`.
Manifest ordering is locale-sensitive today: verification passes under the
collection locale but not the development host's `C.UTF-8`, which remains a
portability defect rather than evidence corruption.

**Failure and recovery diagnosis:** The compositor presented candidate
`0560f50d…` after a 4,867-microsecond prepare, but the first experience host
did not consume the compositor-fence notification on its GPUI thread. The
supervisor timed out and launched a replacement while the original host was
still resident; the replacement failed with `Resource temporarily unavailable`.
The original host later panicked during logout after the compositor surface
had disappeared. A focused second SOS login on the same boot used one host PID,
recognized the incomplete transaction, prepared the identical candidate in
5,674 microseconds, directly page-flipped it, aligned current and authority,
and removed the activation journal. This proves durable recovery and candidate
validity, but does not clear the intermittent first-session host stall or
overlapping-restart risk. The user also observed scroll lag during the degraded
first host lifecycle. No live model ran, so model and model-weighted cost were
zero.

**Physical focus reproduction / changed code:** In the recovered session, a
touchpad click focused `note-draft` and hardware keys edited it. Four subsequent
physical touchscreen contacts over the other editor produced no
`agent-prompt` focus event; later keys still edited `note-draft`. The bounded
13,258-byte journal and 1,738-byte runtime snapshot are stored with a verified
162-byte manifest at
`/home/carlid/dev/sos/artifacts/linux-live-image/evidence/framework12-20260824/framework12-fb70478-focus-diagnostic`;
the manifest SHA-256 is
`1344f8d5752a1f4f1f25d9d7fc98652440b57c3367bae27d8f39944aa6ae824d`.
The Linux GPUI backend exposed raw Wayland touch only to the scene-pointer
router, while native text inputs registered mouse handlers but no touch hit
bounds. Native fields now register their painted bounds; a raw touch-down picks
the topmost field, focuses its persistent entity, and positions the cursor.
The existing active-field restoration remains in place because the first gate
shows it restoring the prompt after compositor input quiescing. The nested
compositor workflow now explicitly transfers focus from the autofocused note
editor to the agent prompt and back, guarding that behavior against becoming
sticky during ordinary interaction.

**Checks / focused overlay result:** An initial release host build completed in
99.70 seconds with 2,010,256 KiB maximum RSS; the 17,071,928-byte diagnostic
binary has SHA-256
`4f797a3ebd7721fd7af460b338a17be1a766763c8e784aeaf30deb1c2508d12c`.
The nested gate was not claimed because this development host lacks Weston.
The release host was then installed as the only changed binary in the
disposable live overlay, with its hash verified before login. On the next SOS
login, mouse input transferred focus from `note-draft` to `agent-prompt` and
edited the prompt. Physical touchscreen taps then transferred focus from the
agent prompt to the note and back to the prompt; the host emitted bounded
`sos_linux_touch_focus` records with the painted node IDs, followed by the
matching blur/focus pairs. The experience remained on its original PID 17,545,
current and authority both remained at `0560f50d…`, and no activation journal
or process failure appeared. The verified 8,775-byte focused-result bundle is
at
`/home/carlid/dev/sos/artifacts/linux-live-image/evidence/framework12-20260824/framework12-fb70478-focus-fix`;
its 166-byte manifest has SHA-256
`0c7089c3d753ccf9dc1ff5a3a719f7599ebbbd96b73038e93d42b50791697fe5`.

**Rejected inference / final source / next gate:** The first implementation
also made active-field restoration one-shot. Review against the original
activation journal showed this was unproved and unsafe: repeated restoration
had correctly returned keyboard focus after compositor quiescing. That change
was removed; the final diff contains only native touch hit routing and its
coverage. All 27 Linux-host tests passed in 0.12 seconds and warning-denying
Clippy passed. The final 17,071,928-byte host built in 79.24 seconds with
2,011,288 KiB maximum RSS and has SHA-256
`ce3c8a486c03f47b1a08d7c2dcfea51dd1ff00c77bc938520327f2e76f394418`.
GDM powered the live system off before that narrowed binary could replace the
diagnostic build, so the physical result closes the touch-routing mechanism on
the overlay but is not exact final-binary or baked-artifact evidence. Commit,
bake a clean revision-pinned ISO, and repeat the complete Framework campaign;
do not promote the hardware gate until artifact-matched focus transfer and
single-host activation both pass.

## 2026-08-25 — Bake and audit the touch-focus Fedora live ISO

**Goal / clean source / host:** Build the first artifact containing the Linux
native touchscreen-to-text-field routing fix, without treating a host bake as
physical acceptance. The Fedora 44 x86-64 host passed strict
`tools/linux-live-image doctor`; the source worktree was clean at
`d9d783cd65b7e6faabacc5dc4c26e63d4bf0eca6`. All 27 Linux-host tests,
warning-denying Clippy, Bash parsing of the live-image and nested-compositor
workflows, and `git diff --check` passed before the privileged bake. The pinned
official Fedora Workstation 44 source remained 2,851,612,672 bytes with
SHA-256
`1620295f6a00c27c3208f0c00b8ece4eab1ec69b9002152d97488bf26a426ddf`.

**Bake result / measurements:** The complete privileged remix passed rootfs
extraction and metadata preservation, Fedora runtime-package mutation, the
release host and offline-agent builds, offline-user staging, rootfs identity
validation, SELinux policy assignment, EROFS repack, ISO replay, and the
embedded media check. It completed in 2,703.62 seconds wall time with
2,041,868 KiB maximum RSS. The finalized 8,924-byte bake log at
`/home/carlid/dev/sos/artifacts/linux-live-bake-d9d783c.log` has SHA-256
`ed22f055cbe8008aa0019185dcd69ad01bb1d4d62400d5c2176742562c110bd4`;
the 985-byte GNU-time record has SHA-256
`144c05d8f95dac71d0f29f78ecc6d7c040b1097e6dcf470626b4e9ab74517809`.
No model provider ran, so live-model and model-weighted cost were zero.

The resulting live-boot artifact is:

| Artifact | Revision | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| `/home/carlid/dev/sos/artifacts/linux-live-image-d9d783c/sos-fedora-workstation-live-d9d783cd65b7.iso` | `d9d783cd65b7e6faabacc5dc4c26e63d4bf0eca6` | 3,056,074,752 | `21332392b6564e4f286c527f79645d564270f287d5313a1243c3b040f37738f9` |

Its 821-byte sidecar has SHA-256
`b8d30725b4c10e5e4ca4860b9aaf9cd0bc96a2780eb230153a1626786515ee46`
and records `source_dirty=false`, Fedora/build-host release 44, offline agent
mode, `live-boot`, and `not_installed_product=true`. The payload is the expected
flat EROFS rootfs, 2,691,596,288 bytes with SHA-256
`dc3c28416007457a72548b1959653cd869585b378900cefde8bb2d1275810235`.

**Independent audit / decision / next gate:** A fresh SHA-256 computation
matched the sidecar; `checkisomd5` independently passed in 3.31 seconds; direct
extraction of the ISO-level identity matched the revision, source, payload,
release, agent, and live-boot claims; direct extraction and hashing of
`LiveOS/squashfs.img` matched its declared size and digest; both
`check-payload` and `fsck.erofs` passed. Xorriso confirmed volume ID
`Fedora-WS-Live-44`, bootable BIOS and UEFI El Torito entries, protective MBR,
and GPT. The finalized 5,336-byte audit bundle at
`/home/carlid/dev/sos/artifacts/linux-live-image-d9d783c-audit` verifies all ten
evidence files through its 834-byte manifest, whose SHA-256 is
`5eb95ac37cac70a7a83f14b7fb506a4db52364d9e10149cda85ecb2e747c3a64`.
The temporary 2.69-GB extracted payload used for this audit was removed after
hashing and remains reproducible from the preserved ISO.

Accept this exact ISO as flashable live-test media only. It is not an installed
product and the host bake does not prove physical DRM, focus transfer, stable
single-host activation, input, or logout. Write this exact hybrid ISO to the
removable USB, boot it on the Framework Laptop 12, and run one fresh same-boot
prepare, mouse/touch field-transfer, offline-agent activation, clean logout,
collect, copy-off, and manifest-audit campaign before promoting the gate.

## 2026-08-25 — Diagnose and close Framework touch-triggered host starvation

**Goal / media / complete-gate result:** Write the revision-pinned Fedora 44
ISO from the preceding entry to the removable device, run its complete
Framework Laptop 12 gate, and use focused live-overlay experiments to resolve
the remaining host-lifecycle and input failures without repeatedly baking the
3-GB image. The exact 3,056,074,752-byte ISO with SHA-256
`21332392b6564e4f286c527f79645d564270f287d5313a1243c3b040f37738f9`
was written to `/dev/sda` with
`sudo dd if=...iso of=/dev/sda bs=16M status=progress conv=fsync` and verified
byte-for-byte. The write took 5 minutes 15.20 seconds with 18,828 KiB maximum
RSS. The 276-byte write log, 852-byte time record, and 57-byte verification log
have SHA-256
`a189ed58d99c4c149a93cb8861fe6d7825f8ccb8fbdeca5f139436432de247f1`,
`5448b31022d418190f53f2b3e2c1016e6c7e8d9786994582f9ff22909141e9a4`,
and `741e2a6c327be22bc80654a2917206f5906792292ec605a7713abe79624219c5`.

The 357,403,147,177-nanosecond same-boot campaign on boot ID
`9ad78727-161a-45ca-b84c-57b95d020f59` passed live-image identity, recovery and
direct DRM page flips, session and agent readiness, keyboard, touchpad,
touchscreen, clean logout, transactional activation, fallback display manager,
and kernel GPU checks. It correctly failed overall: `stable_host_lifecycle`
observed two host launches, durable authority remained at candidate
`0560f50d…` while the current pointer stayed at `32f4b2a9…`, and SOS process
failures were present. All 36 files verify in the copied 149,313-byte evidence
directory
`/home/carlid/dev/sos/artifacts/linux-live-image/evidence/framework12-20260825/framework12-d9d783c-gate`.
Its 3,372-byte manifest, 1,008-byte verdict, 93,916-byte user journal, and
1,072-byte kernel journal have SHA-256
`d8e39496f33f5c171f6dfc4d51fa18bc3fa95ff107ec251f08be5c57022c71e3`,
`128a21ef749ee330f6d1fe427976e49870bfaa7aeec0bef0f21b4989229f0853`,
`6f3c2b625238cc310a15168d304827dc48bc7baafb881fdadaff6050540b0172`,
and `6c4c168e92364c18615ee7f1981ac9f1d6f3a047fa33708c0b5ea7f8ebdf1fc4`.

**Focused failures and rejected approaches:** Physical retests first proved
touch focus but exposed three independent symptoms: a native field reverted to
an older authority value on blur, foreground action results could stop draining
under continuous animation, and touch eventually made text, focus, and Submit
lag or stall. A local input-state shadow fixed the stale-value race and one-shot
focus restoration preserved activation focus without stealing ordinary field
transfers. Counting successful foreground sends and re-pinging calloop fixed
lost readiness; limiting each dispatch to 64 tasks prevented an endlessly
self-replenishing foreground queue from monopolizing the loop. Running tasks
directly inside the calloop callback and then bounding those direct runs were
both rejected after physical tests: they initially responded, then touch focus
could be delayed for seconds and Submit again stopped. Moving tasks to
calloop's idle list was also rejected because continuous frame traffic could
starve that idle queue.

The decisive `eu-stack` sample of the stalled exact host showed its main thread
in `ppoll`, `wl_display_dispatch_queue`, Wayland WSI present,
`anv_QueuePresentKHR`, and the SOS host, while the Luau worker was idle. Wayland
frame callbacks and raw touch callbacks were calling `window.frame()`
synchronously; Vulkan presentation could wait for a swapchain-buffer release
before the Wayland source callback returned, preventing staged action results
from running. The 8,675-byte stack and its 154,585-byte bounded journal have
SHA-256
`98d75d0f7916a584beb1917883d84b7c847dea61510dc89abeb4e66144f87a36`
and `380ff03462020db901b759b0ec9b589614afd33e1a1bc9c4a88baca34f04da06`.
The first queued-frame build correctly moved rendering out of protocol
dispatch but kept a `RefCell` borrow alive through `window.frame()` and
deterministically panicked during startup; separating the pop and render
statements fixed that rejected implementation.

**Changed runtime and lifecycle:** Raw Linux touch now supplies a bounded,
coalesced wake receiver to the experience host so touch-only input marks its
entity dirty. Native text state is shadowed until the serialized authority
catches up, and activation restores focus once rather than on every render.
The GPUI calloop bridge now tracks unmatched sends, re-wakes while work remains,
bounds foreground batches, stages them until all ready protocol sources have
run, and services foreground work before a deduplicated post-dispatch frame
queue. Frame-callback, touch, and tablet paths enqueue frames rather than
rendering synchronously inside Wayland dispatch.

The isolated host launcher also reaps the actual GPU host after an unexpected
proxy disconnect, closing the overlapping-restart failure from the complete
gate. Orderly logout needed a distinct path: the compositor now sends a private
`0600` Unix-datagram request and remains alive while the lifecycle owner shuts
down the supervisor, provider, and host first. Proxy EOF grants an
already-delivered Shutdown request 250 milliseconds to exit; `/proc` state
decides whether a still-live host needs SIGKILL. Tests cover both graceful exit
and forced orphan reaping. This replaced two rejected logout variants: exiting
the compositor first caused a supervisor recovery launch, and unconditional
SIGKILL on proxy EOF produced a false `linux_host_launcher_failed` during
intentional shutdown.

**Exact focused physical result:** The final 17,080,760-byte experience host
has SHA-256
`5214883604708ce504b8cdbdae7ec21399d655de6f25566e1f4c4027635bc9f6`;
its release build took 1 minute 27.98 seconds with 2,010,032 KiB maximum RSS,
and the 820-byte GNU-time record has SHA-256
`9ceac69443469ff3582dd80c92e02b1f96a710ba48dfdca3fb1ccda52b9fd79a`.
`/proc/<pid>/exe` was verified against that digest before interaction. Across
54 complete physical touchscreen contacts, the compositor recorded 54 downs
and 54 releases and the host routed 36 native field-focus changes. It drained
and durably committed all 115 action requests, including 95 text changes and
21 focus changes. The user reported that touch, typing, Submit, and scrolling
were much better and remained responsive. Submit request 115 completed in
2,840 microseconds, committed authority revision 498, prepared and committed
revision `90e852f54c9d07465f5986d19d1e68a18916edd9eaf7080d958d4d9018b5c699`
in 5,121 microseconds of worker time, presented it by direct DRM page flip, and
activated it under the same supervisor host PID.

The final 5,706,016-byte compositor and 1,632,504-byte session owner have
SHA-256
`7bd3b6a0f50969e80cd8369cf33d4b746eb286d5335fd550e95db6a4481516d8`
and `352543f9cbfafb8f7e3ffa2701f5d132e73be8d5f42c7bccc65c103b53e48fe3`.
The exact-source logout emitted the compositor request and handoff followed by
`linux_login_session_stopped reason=user_logout`; it emitted no SOS failure,
panic, Vulkan surface error, or recovery launch, and the post-logout process
table contained no SOS compositor, supervisor, proxy, or experience host. The
final session-owner-only release build took 8.05 seconds with 349,400 KiB
maximum RSS; its 801-byte time record has SHA-256
`b91284dc5e2733d9e8154146a04928312a645e90349f2746a9a4dba42e021778`.

**Evidence / checks / decision / next gate:** The copied focused bundle at
`/home/carlid/dev/sos/artifacts/linux-live-image/evidence/framework12-20260825/framework12-staged-frame-focused-20260825`
is 341,138 bytes. All eight evidence files verify through its 1,179-byte
manifest, SHA-256
`013b91b17c88582a42d241e62b16f34cb239d09a17655d5dca24e5eba3045ff7`.
The 107,152-byte interaction journal, 42,142-byte exact logout journal, and
1,112-byte kernel journal have SHA-256
`c0c99d2ddef86af1aa33fb45a2ff3eeb2fc40531272dd2a4c4b5feaa0e65b066`,
`020b595447d1efeeb90787117598334b53e63bbbde4420ea848c6ac7c09ea4f4`,
and `a19dd8f9f0fab4f4af068661169ece01717dc2eb995031f34b8cc757fb73bf94`.
Three GPUI dispatcher tests, 29 Linux-host experience tests, nine Linux-session
unit tests plus its authority integration test, and 11 compositor tests passed;
warning-denying Clippy passed for all three affected products and both direct
Linux feature sets. No live model ran, so model and model-weighted cost were
zero.

Accept the focused overlay as physical evidence for the diagnosed mechanisms,
not as promotion of the old ISO: the booted artifact still contains revision
`d9d783c`, and the final binaries were installed into its disposable overlay.
Commit these fixes, bake one clean revision-pinned ISO, then run a fresh full
prepare, touch/keyboard/scroll/Submit, clean logout, collect, copy-off, and
manifest-audit campaign. Only that artifact-matched campaign can promote the
Framework live gate.

## 2026-08-25 — Replace acceptance-live with a mutable development environment

**Goal / decision / rejected workflow:** Separate fast physical iteration from
future release promotion. Rebuilding the prior 3.06-GB Fedora remix took
2,703.62 seconds and writing it to USB took another 315.20 seconds, so requiring
that cycle after every SOS patch is disproportionate during diagnosis. The
intermediate acceptance-live artifact class was rejected: it adds almost the
same compose/flash cost as release without providing the immutable SOS-only
artifact that will eventually ship. SOS now has two image classes only:
`development-live`, which is mutable and always
`promotion_eligible=false`, and a future immutable `release`, whose composer
and artifact-matched promotion gate remain to be built. The existing focused
Framework overlay evidence remains diagnostic mechanism evidence; it is not
retroactively promoted.

**Changed environment and controls:** `tools/linux-live-image` now labels the
Fedora Workstation remix `development-live`, installs and enables
`openssh-server`, opens the Fedora firewall's SSH service, and requires a
private non-symlink `--liveuser-password-file`. Password authentication is
restricted to `liveuser`, root SSH is disabled, reusable host keys are removed
so Fedora generates them at boot, and GDM liveuser autologin is disabled so the
operator can choose GNOME or SOS. Fedora creates `liveuser` during boot, so the
builder derives a SHA-512 password hash and installs a root-owned mode-`0700`
`livesys-session-extra` hook that assigns it after account creation, relocks
Fedora's temporary passwordless root account, and disables GDM autologin after
the GNOME live hook enables it. SSH requires and follows `livesys.service`.
Rootfs validation checks that boot-time provisioning, SSHD
enablement/configuration, absent host keys, and the non-promotable/mutable
identity fields without exposing the password.

`tools/linux-live-deploy` builds any selected compositor, experience-host,
provider, supervisor, session, or authoring binary locally and deploys it over
one multiplexed SSH connection. It refuses targets whose baked identity is not
mutable/non-promotable development-live and refuses a running SOS session. It
records base/source revision and dirty state, installs root-owned files into the
RAM overlay, verifies their remote SHA-256 values, and preserves matching host
and target deployment manifests. `tools/linux-hardware-gate` verifies that
manifest, snapshots the current bytes against the baked install manifest, and
emits only `DIAGNOSTIC_PASS promotion_eligible=false` or `DIAGNOSTIC_FAIL` for
development-live. Installed-workstation criteria remain available, but no
current environment is labeled a release artifact.

**Evidence / failures / measurement:**
`./tests/linux-live-image-test.sh` exercises identity fields, password-file
rejection, mocked password-hash/livesys/systemd/firewalld provisioning,
root-owned private rootfs validation, component selection, the complete mocked
SSH deployment, remote installation, metadata, and digest verification. It
passed with
`linux_live_image_host_tests=PASS`. `./tests/linux-hardware-gate-test.sh`
passed with `linux_hardware_gate_host_tests=PASS`, including the rule that a
complete development campaign is diagnostic rather than a normal PASS and a
missing touch observation is `DIAGNOSTIC_FAIL`. The combined suites, Bash
parsing, and `git diff --check` completed in 1.21 seconds with 30,316 KiB
maximum RSS. The test harness initially attempted root-owned fixture installs
without an effective-root mock and then exposed an EXIT-trap lifetime bug in
the deployer's SSH cleanup; the fixture now strips ownership flags while the
production path still uses sudo, and successful deployment explicitly cleans
up before function-local state goes out of scope. No model provider ran, so
model and model-weighted cost were zero.

**Remaining risk / next gate:** No new ISO or physical acceptance is claimed.
The rootfs tests use controlled command doubles; a real Fedora bake must still
prove the boot-time `livesys` password/root-lock/GDM hook, offline
`sshd.service` enablement and ordering, firewall persistence, GDM session
selection, and per-boot host-key generation together. Bake and flash
development-live once, boot the Framework Laptop 12, verify password SSH and
GNOME/SOS selection, deploy one changed binary with
`tools/linux-live-deploy`, verify its recorded digest on the laptop, and run a
same-boot diagnostic collect. Ordinary SOS patches can then reuse that base;
design and gate the immutable SOS-only `release` process separately.

## 2026-08-25 — Move development-live account setup to Fedora boot provisioning

**Goal / environment:** Run the first real `development-live` bake at clean
revision `659003a35635da8423a7393ddf8d9b109ac355e1` from the checksum-pinned
Fedora Workstation Live 44 x86-64 source
`artifacts/linux-live-source/Fedora-Workstation-Live-44-1.7.x86_64.iso`
(2,851,612,672 bytes,
SHA-256 `1620295f6a00c27c3208f0c00b8ece4eab1ec69b9002152d97488bf26a426ddf`)
and validate the new development access against a real Fedora rootfs.

**Failure / rejected approach:** The bake extracted and staged SOS, then failed
with `error: development rootfs has no liveuser account`. It exited 1 after
207.79 seconds with 2,001,264 KiB maximum RSS; no ISO was produced and no
physical-device result is claimed. The finalized failure log is
`artifacts/development-live-659003a-bake.log` (8,659 bytes, SHA-256
`90094fc77daa4aead345817069023a78c9449613e478db5f70716b699a0b42d7`),
and its timing record is `artifacts/development-live-659003a-bake.time` (92
bytes, SHA-256
`d7fc757d673677f5465d5d2ace3801d8afaa515f346a18e933fa44f246051645`).
Inspection of the extracted Fedora rootfs showed that
`/usr/libexec/livesys/livesys-main` creates `liveuser` at boot, temporarily
clears the root password, runs the GNOME hook that enables autologin, and only
then sources `/var/lib/livesys/livesys-session-extra`. Offline `chpasswd` was
therefore impossible, while editing GDM offline would be overwritten at boot;
both approaches were rejected.

**Decision / changed code:** `tools/linux-live-image` now verifies Fedora's
expected `livesys` contract, derives a salted SHA-512 password hash with
OpenSSL, and installs a root-owned mode-`0700` derived-spin hook. At boot the
hook assigns that hash to the newly created `liveuser`, relocks root, and
disables GDM autologin after Fedora's GNOME hook. An SSH unit drop-in requires
and follows `livesys.service`, failing remote access closed if provisioning
fails. Rootfs validation requires the hook, its ownership/mode and hash form,
the root relock, disabled autologin, and SSH ordering while confirming that no
pre-boot `liveuser` was fabricated. The Fedora-realistic test fixture now
models boot-time account creation rather than an offline shadow entry.

**Evidence / remaining risk / next gate:**
`./tests/linux-live-image-test.sh` and
`./tests/linux-hardware-gate-test.sh` passed with
`linux_live_image_host_tests=PASS` and
`linux_hardware_gate_host_tests=PASS`; combined with Bash parsing and
`git diff --check`, the measured run took 1.25 seconds with 30,428 KiB maximum
RSS. No model provider ran, so model and model-weighted cost were zero. These
host tests prove the generated files and fail-closed relationships, not Fedora
boot behavior. Commit the correction, rerun a clean bake in a new output
directory, independently audit the ISO, then boot it and verify root remains
locked, liveuser password SSH starts only after livesys, GDM offers both GNOME
and SOS without autologin, and reboot removes incremental deployments.

## 2026-08-25 — Activate development SSH from the completed livesys hook

**Goal / environment:** Complete the first real `development-live` bake and
boot it on the Framework Laptop 12 without touching its installed Omarchy
disk, then verify password-protected remote access before using the image as
the reusable SOS development base. The clean source revision was
`f057d251de7781622bb60a70c960d4bc01f8e37d`; the target identified itself as
Framework `Laptop 12 (13th Gen Intel Core)` revision A5, running Fedora 44
kernel `6.19.10-300.fc44.x86_64` in boot
`edb42181-55f8-4a36-a388-971f5db601e2`.

**Bake and media evidence:** The first bake invocation completed extraction,
package/runtime staging, rootfs validation, and EROFS repacking, but its cached
sudo authorization expired after 2,910.74 seconds; it exited 1 while waiting
to remove the work tree. The failure/resume inputs remain
`artifacts/development-live-f057d25-bake.log` (8,867 bytes, SHA-256
`a07f97f70385d288e05ced0a01613e1da81b3b4025fa2c7320d37b74bd54140a`)
and `artifacts/development-live-f057d25-bake.time` (92 bytes, SHA-256
`39ddd08d869807a026eecb18815989831e54582e1a1948f499f8f4b206390da8`).
Resuming from the already finalized payload produced
`artifacts/development-live-f057d25/sos-development-live-f057d251de77.iso`
(3,056,205,824 bytes, SHA-256
`c2232111ab8b4aa6d55907dfdf5830a688468bf4be7b8dd218f26c727925ffc0`).
Its embedded EROFS payload is 2,691,727,360 bytes with SHA-256
`c222e53420d88b7ac541e18629573660d2bc71f6174379cea1659ee37edc7f7e`.
An independent `checkisomd5` completed in 3.45 seconds with PASS. Uploading the
ISO to PiKVM virtual media took 138.62 seconds; the PiKVM copy matched the host
byte count and SHA-256 and remained connected read-only.

**Physical failure / rejected approach:** GDM required the configured
`liveuser` password instead of autologging in, and `livesys.service` completed
the password assignment, root relock, and GDM rewrite successfully. SSH did
not start: `sshd.service` was inactive/disabled with no port 22 listener, and
the boot journal contained no SSH start attempt. Direct EROFS inspection proved
the baked lower rootfs contained the offline
`multi-user.target.wants/sshd.service` link and the
`Requires=livesys.service` drop-in, while the initial running merged rootfs did
not expose the enablement link. Therefore offline enablement plus a dependency
on a successful but normally inactive oneshot service is rejected as the
development access boundary.

Before any live mutation, `findmnt` showed `/` as the writable
`LiveOS_rootfs` overlay with `/run/rootfsbase` as its lower directory and a
RAM-backed `/run/overlayfs` upper directory. `lsblk` showed the internal 1 TB
WD_BLACK NVMe with VFAT and LUKS partitions and no mountpoints. No installer
target was selected and no internal-disk write was performed.

**Focused proof / decision / changed code:** On that same disposable overlay,
disabling the dependency drop-in and running
`systemctl enable --now sshd.service` after completed provisioning made SSH
enabled and active with IPv4 and IPv6 port 22 listeners. A fresh independent
password SSH connection then passed; root reported locked, `liveuser` reported
a password, and the per-boot Ed25519 host private/public keys were root-owned
mode `0600`/`0644`. `tools/linux-live-image` now omits offline SSH enablement
and the `Requires=livesys.service` drop-in. The root-only Fedora hook makes
GDM configuration fail closed and performs `systemctl enable --now
sshd.service` as its final action, only after assigning the liveuser password
and relocking root. Rootfs validation requires that exact final action and
rejects any pre-provisioning SSH enablement. The fixture tests cover the new
metadata and reject a hook with any action after SSH activation;
`docs/linux-live-image.md` records the boundary.

Raw console screenshots, OCR, SSH audits, upload timing, ISO integrity, and
host test records are indexed by
`artifacts/pikvm-development-live-f057d25/evidence-manifest.tsv` (2,493 bytes,
SHA-256
`8c86900caa2b6d033610b760bf9e9271c064ccfd1701dc6f52788456d89694d5`).
`./tests/linux-live-image-test.sh` and
`./tests/linux-hardware-gate-test.sh` passed with
`linux_live_image_host_tests=PASS` and
`linux_hardware_gate_host_tests=PASS`; together with Bash parsing and
`git diff --check`, they completed in 1.30 seconds with 30,316 KiB maximum
RSS. No model provider ran, so model and model-weighted cost were zero.

**Remaining risk / next gate:** This physical boot proved the failure and the
focused live-overlay correction, not the newly generated hook on a fresh boot.
No hardware, latency, release, or promotion gate is complete. Bake the corrected
clean revision once, attach it read-only, cold-boot the Framework, and require
automatic SSH enablement only after successful `livesys` provisioning. Then
verify GDM offers GNOME and SOS, deploy one changed SOS component with
`tools/linux-live-deploy`, verify its recorded digest, and run a same-boot
diagnostic campaign before treating this image as the reusable development
base.

## 2026-08-25 — Add private Wi-Fi autoconnect to development-live

**Goal / environment:** Make the reusable Framework development image join its
lab Wi-Fi without console input so PiKVM can cold-boot directly into an
SSH-manageable environment. Reuse the NetworkManager profile created by the
running Fedora 44 development boot instead of reconstructing or printing its
secret. The profile was copied over the already authenticated SSH channel to a
private host file, validated as mode `0600` and 288 bytes, and intentionally
excluded from Git, logs, hashes, and evidence manifests because it contains a
network credential.

**Security decision / changed code:** This is an optional development-only
facility. `tools/linux-live-image` accepts
`--networkmanager-profile-file`, rejects symlinks and group/world-readable
inputs, and validates a Wi-Fi connection UUID, autoconnect, WPA-PSK/SAE with a
stored boot-time PSK, and automatic IPv4 without printing the SSID or PSK. It
installs the profile as root-owned mode `0600` under
`/etc/NetworkManager/system-connections`; rootfs validation rechecks the
profile and its root-owned mode `0700` parent. Matching rootfs and outer image
identities record only `wifi_autoconnect=true` and
`network_credentials_embedded=true`. Omitting the option records both fields
as false and embeds no SOS-owned profile. The standalone
`check-networkmanager-profile` command validates a prospective private input
before a long bake.

Runtime file permissions do not make the ISO secret: anyone holding the image
can extract an equivalent Wi-Fi credential offline. Documentation now says the
credentialed ISO must remain private, its network credential must be rotated if
custody is lost, and the future immutable release must exclude the profile.
The network name and credential are not present in source, identity metadata,
progress records, or test output.

**Evidence:** The fixture suite covers credentialed and uncredentialed identity
fields, private-file and non-symlink enforcement, disabled autoconnect,
missing PSK, installed ownership/mode, metadata agreement, and the no-profile
case. The actual private Framework profile emitted only
`linux_live_image_network_profile_checked=PASS wifi_autoconnect=true
network_credentials_embedded=true`. Bash parsing,
`./tests/linux-live-image-test.sh`,
`./tests/linux-hardware-gate-test.sh`, and `git diff --check` all passed in
1.69 seconds with 30,308 KiB maximum RSS. ShellCheck was not installed. The
finalized output is
`artifacts/development-live-network-profile/host-tests.log` (227 bytes,
SHA-256 `196ce54c4aa66c4c4b7a78d5842cee7c98b9f947edd3866b2c7275bda684c09f`)
and its timing is
`artifacts/development-live-network-profile/host-tests.time` (1,133 bytes,
SHA-256 `32a7d3f5a0339bf818ea300a36cac7849c60007fee89548071c2d8c66f919b05`).
No model provider ran, so model and model-weighted cost were zero.

**Remaining risk / next gate:** No new ISO or unattended network boot is yet
claimed. Commit the builder change, bake one clean credentialed
`development-live` ISO, attach it read-only through PiKVM, and cold-boot the
Framework without network HID input. Require the identity classification,
automatic Wi-Fi activation, the post-`livesys` SSH listener, password SSH, and
the still-unmounted internal Omarchy NVMe to pass together before adopting the
image as the reusable base.

## 2026-08-25 — Bake the credentialed development-live image

**Goal / environment:** Produce the first clean reusable development image
that combines the post-`livesys` SSH activation with private Wi-Fi
autoconnect. The clean source revision was
`28cf8fffee8e2492fc4f2b69fcfe27db3baf7b36`; the source media was
`artifacts/linux-live-source/Fedora-Workstation-Live-44-1.7.x86_64.iso`
(SHA-256
`1620295f6a00c27c3208f0c00b8ece4eab1ec69b9002152d97488bf26a426ddf`).
The private NetworkManager input remained outside Git and build evidence.

**Bake evidence / decision:** `tools/linux-live-image bake` staged SOS,
configured the development account and post-provisioning SSH activation,
installed the private NetworkManager profile, validated the rootfs, repacked
EROFS, and passed the embedded media check. It completed in 2,640.49 seconds
with 704,264 KiB maximum RSS. The result is
`artifacts/development-live-28cf8ff/sos-development-live-28cf8fffee8e.iso`
(3,056,205,824 bytes, SHA-256
`a346369a50cf5d1b32610fcf1c55c95ea7238172a46a2b7c6c1618428f4ed152`).
Its identity is
`artifacts/development-live-28cf8ff/image-identity.env` (954 bytes, SHA-256
`7685dab93216d94d8e512d4108ea553e143bbe003fb9d01d5f7d46b68c596c6d`)
and records the exact source revision, `development-live`,
`promotion_eligible=false`, `wifi_autoconnect=true`, and
`network_credentials_embedded=true` without an SSID, password, PSK, or
passphrase field.

The finalized bake output is `artifacts/development-live-28cf8ff-bake.log`
(9,189 bytes, SHA-256
`a9faee06c622fad34dcaddd341f223ff5c5721afcbc54ddd775a17fd18d5f69a`)
and its timing record is `artifacts/development-live-28cf8ff-bake.time` (54
bytes, SHA-256
`faff5ef450ed1fa5cf4cba1221f6a68119d18409c7c6c9849acc8ced5ce6c1ca`).
An independent SHA-256, `checkisomd5`, identity-agreement, and secret-field
audit passed in 5.10 seconds with 3,456 KiB maximum RSS. Its output is
`artifacts/development-live-28cf8ff-audit.log` (309 bytes, SHA-256
`b5a9672ed9ca23127a13a479f49827f5d30ef9e878175081179baed3cb31ceac`)
and timing is `artifacts/development-live-28cf8ff-audit.time` (49 bytes,
SHA-256
`97667dd628bd7bd8574a391693c728971343807a3a576987419d2a5a65f3ecb5`).
No model provider ran, so model and model-weighted cost were zero.

**Remaining risk / next gate:** This is build evidence, not unattended-boot or
hardware acceptance. Keep the credentialed ISO private. Attach this exact hash
read-only through PiKVM, cold-boot the Framework without network HID input,
and require automatic Wi-Fi activation, post-`livesys` SSH availability,
password SSH, correct image identity, and an unmounted internal Omarchy NVMe
before adopting it as the reusable development base.

## 2026-08-25 — Prove development-live Wi-Fi and SSH on Framework 12

**Goal / environment:** Boot the credentialed revision
`28cf8fffee8e2492fc4f2b69fcfe27db3baf7b36` on the Framework Laptop 12 and
prove that it becomes remotely manageable without entering a network
credential while preserving the installed Omarchy NVMe. The exact ISO was
uploaded to PiKVM in 134.96 seconds with 5,978,736 KiB maximum host RSS. The
PiKVM-side SHA-256 matched
`a346369a50cf5d1b32610fcf1c55c95ea7238172a46a2b7c6c1618428f4ed152`,
and the selected 3,056,205,824-byte virtual CD-ROM reported connected,
complete, read-only, and non-writable.

**Boot failure / boundary:** PiKVM's ATX state did not track or change the
laptop's power state, and remote HID did not reliably catch the Framework
firmware boot-menu window. The first reboot therefore entered the installed
Omarchy lock screen rather than the virtual CD-ROM. No installer or
block-device writer was invoked. The user then selected the already attached
read-only PiKVM CD-ROM and booted Fedora. This rejects fully unattended remote
cold boot with the present console wiring/configuration; it does not reject the
image's development-network path.

**Physical evidence / decision:** PiKVM captured Fedora startup and the GDM
`Live System User` chooser without autologin. The live boot obtained
`192.168.1.129` from the embedded Wi-Fi profile with no network HID input, and
password SSH became reachable. A 4.54-second SSH audit with 9,584 KiB maximum
host RSS reported `image_kind=development-live`, the exact source revision,
`promotion_eligible=false`, `wifi_autoconnect=true`,
`network_credentials_embedded=true`, enabled and active `sshd`, connected
Wi-Fi, `LiveOS_rootfs` overlay root, and boot ID
`c1e6564e-b427-471d-946e-9d83f2d8efde`. It also proved no
`/dev/nvme0n1` source was mounted. The credentialed ISO is accepted as the
reusable development base when selected explicitly at boot; it remains
private and is not a release candidate.

The finalized upload, stored-image state and digest, boot/GDM screenshots,
OCR capability state, SSH audit, and timing records are indexed by
`artifacts/pikvm-development-live-28cf8ff/evidence-manifest.tsv` (1,110 bytes,
SHA-256
`dd6f8ec86ce40b8cfb36509ae8b54c98bbe9d25fd2355b4a9fdcd6e80c9fe9ea`).
No model provider ran, so model and model-weighted cost were zero.

**Remaining risk / next gate:** Firmware boot selection still requires local
help until PiKVM power/boot-menu control is made reliable. The image has not
passed a release, promotion, latency, or full SOS interaction gate. Use this
boot for incremental `tools/linux-live-deploy` iterations, verify each deployed
digest and same-boot identity, and keep the internal NVMe unmounted. Later,
define a separate credential-free immutable release bake and physical release
gate.

## 2026-08-25 — Evaluate a rootless development-live bake boundary

**Goal / environment:** Determine whether the Fedora development-live remix
can move out of host `sudo` and into an unattended rootless Podman build while
retaining the ownership and extended metadata required by the existing image
gate. No product code changed. The Fedora host had Podman 5.8.4 in rootless
overlay mode, a 65,536-entry subordinate UID and GID range, and a world-usable
`/dev/fuse`. The probe used a Fedora 44 container and granted `SYS_ADMIN` only
inside its user namespace together with `/dev/fuse`; it did not invoke host
`sudo` or receive host root.

**Evidence / rejected approaches:** A focused rootless bind-mount probe showed
that container UID/GID `0:0` mapped back to the invoking host user, an arbitrary
`123:456` owner survived as subordinate IDs and was visible with the same IDs
through `podman unshare`, and `security.capability` survived. A subsequent
EROFS/FUSE round trip completed in 5.4 seconds and preserved numeric ownership,
hardlinks, a file capability, a portable user xattr, and a repack readable by
`fsck.erofs --xattrs` without host privilege.

Two naive extraction routes were rejected. `fsck.erofs --extract --preserve
--xattrs` retained the metadata model but aborted when rootless container root
could not set `security.selinux`. `erofsfuse` plus the builder's filtered
`rsync` avoided that SELinux write and preserved ownership, hardlinks,
capabilities, and portable xattrs, but Fedora 44's erofsfuse 1.9.2 did not
expose `system.posix_acl_access`, so it silently lost a test ACL. The stricter
finalized probe intentionally remained failed rather than accepting incomplete
metadata parity. Its log is
`artifacts/rootless-live-image-feasibility/probe.log` (2,647 bytes, SHA-256
`c3590ce2bf6a05916a6a52a0c4b28e9b66378530ced964515ca5b5961554c5b5`)
and its timing is
`artifacts/rootless-live-image-feasibility/probe.time` (5,342 bytes, SHA-256
`14c63b63de5352223717df8de55111526732f9c6b7a314d9bfdb0011a1f9990b`).
It ran for 4.67 seconds with 47,500 KiB maximum RSS and exit status 1. No model
provider ran, so model and model-weighted cost were zero.

**Decision:** A rootless container is a viable privilege boundary, but the
current bake is not a drop-in Podman wrapper. Prefer an explicitly pinned
EROFS extraction implementation that preserves every xattr except
`security.selinux` (which the existing builder deliberately regenerates from
the image's own policy at `mkfs.erofs` time). Keep the existing source/dest
ownership, hardlink, ACL, capability, portable-xattr, SELinux-policy, and
repacked-image audits. Do not accept the erofsfuse-only copy path unless its ACL
parity is independently proved.

**Remaining risk / next gate:** Implement the pinned Fedora build container and
the SELinux-filtered, ACL-preserving extraction path, then run the complete bake
from the checksum-pinned Fedora ISO with zero host privilege. Compare its
rootfs metadata audit to the accepted root-owned path, verify the finalized ISO
and identity sidecar, and boot that exact artifact in the disposable VM before
any physical diagnostic. Rootless baking does not make raw removable-media
writing rootless; that remains a separate udisks/device-authorization boundary.

## 2026-08-25 — Implement and complete a rootless Podman ISO bake

**Goal / environment:** Remove host privilege and human `sudo` involvement from
the development-live bake while preserving the existing EROFS metadata and
identity gates. The implementation is an uncommitted change over
`6d89f60350ed2ec53479d83676795417cd2cdea8`; the clean disposable test clone
used test-only revision `80cc320a865e8f3ae18bc1f58daaeafc8cbd9060`, which
must not be treated as a promotable source revision. The Fedora 44 x86_64 host
ran Podman 5.8.4 in rootless overlay mode with 65,536 subordinate UIDs and
GIDs. The source ISO remained
`/home/carlid/dev/sos/artifacts/linux-live-source/Fedora-Workstation-Live-44-1.7.x86_64.iso`
(2,851,612,672 bytes, SHA-256
`1620295f6a00c27c3208f0c00b8ece4eab1ec69b9002152d97488bf26a426ddf`).

**Changed code / boundary:** `tools/linux-live-image bake` now validates a
normal-user rootless Podman configuration, builds a Fedora image pinned by base
digest, and runs the existing builder with `--userns=keep-id` and no
`--privileged`, host `sudo`, host loop mount, or host root. Container-internal
`sudo` is confined to the rootless user namespace. The source ISO and password
are read-only mounts, and the external output directory is the artifact mount.
The exact build image ID,
`sha256:25fea39fe5a512315d228d3c42a30f593873607a9b69ef09f58d891430cdb900`,
is recorded in image identity. The new SELinux compatibility library suppresses
only `security.selinux` during userspace EROFS extraction, preserves a file
capability across the extractor's later `lchown`, hides ambient container
SELinux labels during repack, and lets `mkfs.erofs --file-contexts` regenerate
image-policy labels. Other ownership, hardlinks, ACLs, capabilities, and xattrs
remain fail-closed. The CLI adds `rootless-doctor`, `image`, and
`rootless-test`; documentation and the host suite cover the new default.

**Rejected approaches / failures:** The earlier erofsfuse copy route remained
rejected because it lost POSIX ACLs. During implementation, the first synthetic
round trip showed that `lchown` cleared `security.capability`; preserving and
restoring that xattr fixed it. A later round trip found that the rootless
container's ambient SELinux label could override file-context policy during
repack; filtering SELinux reads as well as writes fixed it. A shared-object
test clone was rejected because its Git alternates target was unavailable in
the container, so the full test used a self-contained clone. Three independent
audit-wrapper attempts were also rejected before the final audit: one had an
incorrectly quoted digest parser, one incorrectly required byte-identical
embedded and sidecar identities despite the unavoidable three final
`output_iso_*` self-reference fields, and one expected normal-verbosity
`fsck.erofs --xattrs` to print label names. None changed or invalidated the ISO.

**Focused evidence:** `tools/linux-live-image rootless-test` rebuilt the cached
image and passed owners, hardlinks, ACLs, file capability, portable xattr, and
SELinux-policy regeneration in 1.25 seconds with 48,272 KiB maximum wrapper
RSS. Its log is
`artifacts/rootless-live-image-implementation/rootless-test.log` (3,925 bytes,
SHA-256
`a4bcd648d15204ba51440a515dc6ea847ab0cb4be8cf4ccbe7af7a2a79650a51`)
and timing is
`artifacts/rootless-live-image-implementation/rootless-test.time` (763 bytes,
SHA-256
`d5546d9a4679553d96503c7844cce0b24e82ae437cb643154d620f6c252c30d1`).
Bash parsing, the compatibility library compiled with `-Werror`,
`./tests/linux-live-image-test.sh`, and `git diff --check` passed in 1.32
seconds with 30,316 KiB maximum RSS. The log is
`artifacts/rootless-live-image-implementation/host-tests.log` (196 bytes,
SHA-256
`02d88fe1ccc964a5e5dbedf690a3fbc3b15c9d98c8a89224e2180e1d27c7fecf`)
and timing is
`artifacts/rootless-live-image-implementation/host-tests.time` (1,232 bytes,
SHA-256
`3992ade2c41e702c9757986c72788b09e9d4a6fbff60ac03832f59d3cfc62273`).

**Full bake evidence / decision:** The rootless command completed without
interaction in 1:02:35 with exit status zero and 49,224 KiB maximum host-wrapper
RSS; the container's memory is outside that wrapper measurement. Rootless
userspace extraction took about 16.75 minutes, while the existing
deduplicating LZMA EROFS repack was the dominant approximately 42-minute,
single-core phase. The result is
`artifacts/rootless-bake-test-80cc320a865e/sos-development-live-80cc320a865e.iso`
(3,056,205,824 bytes, SHA-256
`8afa63410612af343e911ddd78b36b7ebf4a077faaaeaef59762d9ba9acc166a`).
It records `build_mode=rootless-podman`, the exact build image ID, the clean
test revision, `development-live`, `promotion_eligible=false`,
`wifi_autoconnect=false`, and `network_credentials_embedded=false`. The one-use
password and disposable clones were destroyed after the bake.

The finalized bake log is
`artifacts/rootless-bake-test-80cc320a865e/bake.log` (52,045 bytes, SHA-256
`be7097aefc9e09d31e656747ec252b265a9047d97a707646a231a520e94de85c`)
and timing is `artifacts/rootless-bake-test-80cc320a865e/bake.time` (1,241
bytes, SHA-256
`bcab6eca0315c7e95b277b6f744c45ad691dff84ba01fd3ac694e1fb9bda89e9`).
The sidecar identity is
`artifacts/rootless-bake-test-80cc320a865e/image-identity.env` (1,070 bytes,
SHA-256
`29f5c25a7e25bb2e7919cd0031683f0909c118178a871fadf0d2758cff232bf8`).
An independent 6.66-second audit with 7,588 KiB maximum RSS passed the embedded
media check, ISO and EROFS payload byte/digest agreement, EROFS xattr integrity,
normalized embedded/sidecar identity agreement, and secret-field absence. Its
log is `artifacts/rootless-bake-test-80cc320a865e/audit.log` (454 bytes,
SHA-256
`feb5ddc4538e927a51a294e516e8f7a4064b8a602a674ed893a44a7c7771d7e0`)
and timing is `artifacts/rootless-bake-test-80cc320a865e/audit.time` (2,423
bytes, SHA-256
`15d819832362d8429d701379ec2a676650439ec7bdde24bd00876f19b8e04f88`).
No model provider ran, so model and model-weighted cost were zero. The decision
is to accept rootless Podman as the default bake boundary; it removes the user
from the privileged loop, with slower userspace EROFS handling as the measured
cost.

**Remaining risk / next gate:** This proves build and artifact integrity, not
boot, graphics, networking, or physical hardware acceptance. The artifact is
from a test-only revision and is not promotable. Commit the implementation,
repeat the clean bake from that real revision if a distributable artifact is
needed, and boot its exact hash in the disposable Fedora VM. Only after that VM
gate should a physical Framework diagnostic run. A later performance change
may benchmark dropping EROFS `dedupe` to unlock parallel compression, but must
compare image size, boot compatibility, and metadata before adoption.
## 2026-08-25 — Start Linux System Providers v1 on development-live

**Goal / architecture:** Replace the Linux host's synthetic
`model.providers` value with the same System Providers v1 contract used by
Stock Base and generated Luau. The first Linux slice covers normalized
clock/locale/time-zone facts, UPower power/thermal, NetworkManager connectivity
and saved Wi-Fi, PipeWire/WirePlumber volume, MPRIS media sessions, and
freedesktop application discovery/launch. Existing file/iCalendar notes and
calendar plus sysfs display/input facts remain available through their current
typed compatibility fields while the remaining canonical display, session,
notification, clipboard, Bluetooth, and UDisks domains are designed as later
ABI slices.

`crates/providers-linux` now owns an in-process `SystemAdapter`. It uses typed
blocking D-Bus proxies for UPower, NetworkManager, systemd-timedated, and MPRIS.
The only command adapters are fixed-path `/usr/bin/wpctl` and
`/usr/bin/gio launch` calls with fixed argument vectors and no shell.
NetworkManager object paths, MPRIS bus names, desktop-file paths, and executable
commands never cross the provider boundary; Luau receives bounded labels and
SHA-256-derived opaque IDs. Capabilities are the intersection of revision
grants and resources observed in the current snapshot. Power ceremonies remain
ungranted and fail closed. The Linux host now routes the v1 audio, media,
network, application, and attention envelopes to this provider boundary and
polls provider fingerprints once per second.

**Development-live integration:** `sos-login-session` now creates a private
provider store and mode-`0600` wildcard grant manifest only when the image
identity is both `development-live` and mutable, or when the development escape
hatch is explicitly supplied. Stable sessions still require an exact private
revision grant. `tools/linux-live-deploy` can atomically overlay and hash the
new `provider-probe`, `login-session`, `stock-base`, and `api-doc` components in
addition to the six session binaries; `tools/linux-hardware-gate` accepts only
their fixed destinations. The read-only `sos-linux-provider-probe` exercises
the same `ProviderHub` and grant loader used by the host and has no action mode.

**Failures and corrections:** The first real probe found that an empty provider
store and absent media player caused the legacy music adapter to fail the whole
snapshot. Missing media now produces an inactive value so unrelated provider
domains remain available, with a regression test. A second probe found that
the first offline USB power-supply entry could mask an online AC entry; AC state
now aggregates all readable supplies, and charging remains absent when there is
no battery. Locale values are normalized to bounded language tags and the
time-zone identifier comes from timedated with safe fallbacks. Public app,
network, and MPRIS labels are UTF-8 byte-bounded and control-free, and action
IDs require the expected opaque syntax before resource lookup.

**Host evidence:** `cargo test --workspace --all-targets`,
`cargo test -p sos-experience --features linux-host --all-targets`,
`cargo clippy -p providers-linux --all-targets -- -D warnings`,
`cargo clippy -p sos-experience --features linux-host --all-targets -- -D
warnings`, `tests/linux-login-session-test.sh`,
`tests/linux-live-image-test.sh`, Bash parsing, formatting, and
`git diff --check` all passed. The provider crate has 17 passing tests and the
Linux-host feature has 29. A release provider probe built in 10.04 seconds and
is 1,738,120 bytes with SHA-256
`b0de0b6e928e70373d39402980535541e67b772b801c0a8d9408eb534fecfdc5`.
Its local live-service snapshot is
`artifacts/linux-system-providers-framework12-20260825/host-provider-snapshot.json`
(1,905 bytes, SHA-256
`2cea5568db36c690d8ea6e39fc996082b5031100dc2f6d07cc7d2232ed5db4f1`):
ABI 1, `Europe/Zurich`, validated Ethernet, AC power with no invented battery,
two opaque compatible applications, and only `app_launch` available because
this build host had no session audio or MPRIS resource. No model provider ran,
so model and model-weighted cost were zero.

**Physical state / next gate:** PiKVM still has the exact 3,056,205,824-byte
development ISO attached as a complete read-only virtual CD-ROM. During this
iteration its 1920x1080 stream remained byte-for-byte black, its ATX LED stayed
off after one bounded short power click, and enabling the unavailable keyboard
output did not create a live HID endpoint; no reset, long press, reboot, or
internal-disk action followed. The Framework appeared as `fw12` at
`192.168.1.123` and answered ICMP, but TCP/22 remained filtered through the
complete provisioning wait. Therefore no overlay was deployed and no physical
provider or interaction gate is claimed. Restore target SSH (or a working
PiKVM keyboard/video path), deploy `experience-host`, `provider-probe`, and
`login-session`, run the probe against live UPower/NetworkManager/PipeWire,
enter a fresh SOS login, exercise reversible volume and saved-Wi-Fi actions,
capture the stock provider UI, return to the original state, and only then
finalize and verify the physical evidence manifest.

## 2026-08-25 — Prove the first Linux System Providers slice on Framework 12

**Goal / environment:** Continue on the same mutable development-live Fedora
44 overlay instead of rebaking the ISO, then prove Stock Base and a generated
revision can consume and act through the Linux provider boundary on the
Framework Laptop 12. The physical campaign used boot ID
`9b1818f2-c6c3-4829-8109-c9b3320a02a3`; PiKVM retained the exact
3,056,205,824-byte development ISO as a complete, connected, read-only,
non-writable virtual CD-ROM. Every storage audit kept `/dev/nvme0n1` and its
partitions unmounted, with no installer or block writer present.

**Implementation and decisions:** The selectable host now keeps the real login
`HOME`, cache, D-Bus, and `XDG_RUNTIME_DIR`, while using an absolute compositor
socket in its private runtime directory. This lets the provider reach the user
PipeWire/WirePlumber service and lets ordinary native applications use portals.
The login publishes `SOS:GNOME` to the user manager and starts a dedicated
`sos-session.target`; a paired conflict target stops the standard graphical
session and portal services on SOS logout. Existing applications are therefore
indexed from bounded freedesktop entries and launched through a fixed
`gio launch` argument vector rather than rewritten or exposed to generated
code. A visible Calculator window, active generic/GNOME/GTK portal services,
and clean process removal proved that path. The GNOME portal backend still
warns that the Mutter ServiceChannel is absent; operations tied specifically
to that Mutter protocol remain open.

The power adapter originally selected a 0% HID peripheral ahead of the laptop
battery. It now excludes `scope=Device` power supplies, and the physical
snapshot reports the Framework battery at 80%. The host also creates a private
semantic socket and Stock Base gives its status, controls, networks, and
applications stable accessible roles and labels. The first horizontal status
layout overflowed at 1920x1080; stacking the three bounded cards made power,
network, and audio simultaneously visible. Stock exposes 24 bounded launch
rows while the canonical provider probe truthfully inventories 34 eligible
desktop entries.

Rapid absolute-volume actions exposed a stale-snapshot race: a second click
could derive its target from the pre-action model. System Providers v1 now has
`audio.adjust_volume(delta)`, authorized by the existing volume capability and
applied relative to current platform state. Linux uses fixed `wpctl` relative
arguments; the Compat framework and Core native adapters implement the same
closed action so the shared stock source remains portable. Zero and out-of-range
deltas fail before adapter access. The physical semantic round trip queued up
at `1787689676410221077` ns, reached 50% in 62.25 ms, queued down immediately,
and returned exactly to the 40% baseline 59.38 ms later (123.97 ms total), then
published `Volume 40%` again.

**Physical evidence:** Development overlay
`20260825T202116Z-da87df989f18-1889609` deployed the host, probe, Stock Base,
and API documentation in 105.077 seconds; final Stock Base overlay
`20260825T203215Z-da87df989f18-1893733` completed in 13.838 seconds and its
remote SHA-256 matched the source. Coordinated activations retained host PID
54637 and ended on durable revision
`acaad98458e6b7b566362f91ac6bef3d8b57c106edb416367dac1788e531e51c`.
The final canonical snapshot is
`artifacts/linux-system-providers-framework12-20260825/target-final-provider-snapshot.json`
(5,681 bytes, SHA-256
`516abee2f8367cd0ccf598f514c3b4d525bf379dae88ec1c301ba64f6da78be5`):
ABI 1, 80% battery, validated Wi-Fi, unmuted 40% audio, 34 applications, and
only audio, saved-Wi-Fi, and application actions granted by observed resources.
The atomic action record is
`target-final-audio-semantic-roundtrip.txt` (492 bytes, SHA-256
`25299a2de68cd2ded95540749ef7e98957acaa3b4d1aedd43fe3d248ac16a496`).
The final 1920x1080 PiKVM frame is
`pikvm/final-stock-system-providers-layout-fixed.jpg` (84,513 bytes, SHA-256
`4cb3e02516f399d109a3064963e1933281004630e13cb89e1778d677835f4cfd`).

One PiKVM coordinate test unintentionally selected Wi-Fi disconnect because
the pointer scale was wrong; a bounded saved-network provider action restored
the link without exposing a network credential, and raw coordinate input was
retired for action evidence. PiKVM video remained healthy, but its keyboard
reported no available output; a same-boot, non-secret-calibrated temporary
uinput helper was used only at observed GDM fields and then removed. Repeated
development logins made GDM reach its display-failure limit once; restarting
only GDM restored a fresh greeter. The final logout left all SOS processes,
both graphical targets, and the portal inactive, kept volume at 40%, and kept
the internal NVMe unmounted.

**Verification / verdict:** `cargo test --workspace --all-targets`, the
29-test Linux-host experience suite, the 17-test provider suite, the 11-test
Android authority suite, strict Clippy for the changed Rust boundaries,
formatting, Bash parsing, `tests/linux-login-session-test.sh`,
`tests/linux-live-image-test.sh`, and `git diff --check` passed. The shared
Compat Java and Core C++ relative-volume adapters were not rebuilt into Android
artifacts in this Linux campaign. No model request ran, so model and
model-weighted cost were zero.

The prepared same-boot hardware campaign correctly ended
`DIAGNOSTIC_FAIL`, not provider acceptance PASS: touchpad motion and touchscreen
input were not exercised, seven iterative host launches fail the stable-host
lifecycle criterion, and earlier failed login/session experiments remain in
the campaign journal. Same boot, recovery page flip, direct compositor,
session readiness, agent start, keyboard input, touchpad button, clean logout,
transactional activation, durable authority, fallback GDM, and kernel GPU
criteria passed. Its deterministic nested manifest has 38 files. The complete
campaign is indexed by
`artifacts/linux-system-providers-framework12-20260825/evidence-manifest.tsv`
(30,662 bytes, SHA-256
`389f2cbb34041bc23f3cacbd6b7ebc8c13ff911eab624961f3212cae29081479`),
independently verified with 275 files.

**Remaining risk / next gate:** This accepts the first canonical Linux slice,
not the whole requested matrix or a release. Normalize session/display/input,
notifications/attention, files/storage/clipboard, calendar/notes, Bluetooth,
and removable devices into subsequent provider ABI slices. Then run a fresh
single-login Framework campaign with physical touchpad motion, touchscreen,
suspend/resume, portal dialogs needed by stock applications, and no historical
process failures. Keep power confirmation, credentials, permissions,
emergency, and Recovery surfaces fixed native UI.

## 2026-08-26 — Make Stock Base the editable Linux integration target

**Goal / architecture:** Replace the small provider demonstration with a
substantial default experience while preserving the rule that stock has no
special native UI authority. `experiences/default.luau` is now a 35,843-byte
Scene ABI v3 revision with eight source-defined workspaces: Home, Agenda,
Notes, Media, Attention, System, Apps, and Agent. Navigation, agenda and note
composers, media controls, notification acknowledgement, connectivity/power/
audio controls, compatible-application launch rows, the resident-agent
conversation/composer, explicit unavailable/empty states, and the inline SVG
mark all use the same retained nodes, revision assets, typed provider effects,
and capability checks available to generated content. Its SHA-256 is
`02d7438ef81720eeb0640b376e503e43ecd70cacdee54cd7c5d9a3e200280428`.

Scene layout now has an explicit bounded `wrap` facet decoded by the Luau
runtime and implemented by both Linux and Android GPUI renderers. Stock opts
its navigation, cards, controls, notes, applications, and agent actions into
wrapping. The readable inner frame measures relative to its parent and is
bounded to 1,876 logical pixels; fixed card widths then reflow without a
device-name branch or render-time Luau callback. Tests identify the responsive
home grid and content frame by stable IDs and require wrapping, the maximum
width, relative measurement, all eight navigable workspaces, the agent
composer, typed audio/calendar/notes effects, one revision-scoped asset, and a
system-provider unavailable state. The final note transaction also exposes its
`Saved` result as an accessible status. The API and focused
`stock-experience.md` documentation define source-level replacement—not themes
or widget rearrangement—as the customization boundary.

**Host verification:** `cargo check -p sos-experience --features linux-host
--bin sos-experience-host`; `cargo test -p experience-ir -p runtime-luau -p
sos-experience --all-targets`; `cargo test -p revision-supervisor
--all-targets`; `bash tests/linux-login-session-test.sh`; formatting; and
`git diff --check` passed. The focused suites reported 5 experience-IR, 22
Luau-runtime, 16 experience, 1 revision-store signing, 10 coordinator, and 15
supervisor tests. No model request ran, so model and model-weighted cost were
zero.

**Framework 12 physical evidence:** The mutable development-live image at
base revision `28cf8fffee8e2492fc4f2b69fcfe27db3baf7b36` remained on the same
boot ID `9b1818f2-c6c3-4829-8109-c9b3320a02a3`, with overlay root and every
internal-NVMe partition unmounted. The final root-owned `/usr/share` source and
activated revision source had the same 35,843-byte SHA-256 above. Coordinated
activation committed revision
`40c4ec85c7577938d9e4a323f65e6c61e21f9197b7577e0a82eece8f3b99c76d`
while retaining stable host PID 62841. After logout, the exact-source
`stock-base` development overlay
`20260825T223150Z-5e7d3d2bda31-1923240` completed in 8,752,359,761 ns; its
111-byte deployment manifest has SHA-256
`9547249eebf5ca0d9292e52da4853eac929c49f5bf89db7c5f7eba7babcd66cb`.

The semantic interface navigated all eight workspaces. Agenda creation wrote
`09:30 Framework stock gate` through `calendar.append`; note creation wrote
through `notes.write`; and the final note test cleared the composer, published
an accessible `Saved` status, and found the provider-created file. Its
2,743-byte semantic snapshot has SHA-256
`1552885f53e2fdf4eb42a7d88e58b4c2dac553776cd6595943809589632a5b6c`.
The canonical audio action moved the live PipeWire sink from 40% to 50% and
back to 40%. PiKVM visually confirmed Home, Agenda, Notes, System, Apps, and
Agent; `pikvm/stock-home-bounded.jpg` is 79,074 bytes with SHA-256
`3a0dc32f05f1ca454f8a2878db320fb99e34a8fc72994903de9a5c3f67322094`,
and the final visible plus semantic note result is
`pikvm/stock-note-final-accessible.jpg` (70,811 bytes, SHA-256
`0df751168fa1fac57caa22ba9041d08063b926ec47cd9df7435eef74c7932092`).

**Failures / rejected evidence:** The direct compositor exposes PiKVM DP-1 and
the internal eDP-1 side by side as a 3,840-pixel global layout. The first Stock
revision stretched across that space; intrinsic growth and wrapping alone did
not constrain it. The retained maximum-width content frame fixed the visible
PiKVM output, while per-output workspaces remain host work. The first note UI
reported `Saving` after a committed effect and did not expose the status
semantically; transactionally committed state now reports `Saved` and has a
semantic regression test. One exploratory audio assertion looked for a
nonexistent system semantic value and is retained only as rejected evidence;
the saved `wpctl` 40/50/40 measurements are the acceptance record.

The clean lifecycle request stopped every SOS process and all three graphical
user targets, with GDM greeter session `c3` present. The sink reappeared at
100% after the graphical logout, so it was explicitly restored to the original
40% before finalization. The target then transiently withdrew its address and
PiKVM streamer; two screenshot requests returned HTTP 503. It returned at the
same address without power or HID input and accepted the final overlay, but the
PiKVM streamer remained unavailable, so there is no post-logout GDM frame.
HID, ATX, and read-only virtual-media state were byte-identical before/after;
the exact 3,056,205,824-byte development ISO stayed complete, connected,
CD-ROM, non-writable, and `rw=false`.

**Trust decision / next gates:** Android already loads this stock source from
AVB/OTA-protected `/system_ext`, pins the resulting revision separately from
the mutable current pointer, and falls back to it transactionally. Linux
development-live has content addressing and a read-only system source, but no
system-owned pinned stock revision or provisioned release-verification key.
The revision store's optional symmetric manifest HMAC is now documented
accurately and is not counted as an asymmetric signed-recovery boundary. A
Linux release still needs an immutable stock pointer plus asymmetric release
signature verification and a fixed recovery request.

This accepts Stock Base as the substantial editable integration target on a
physical clamshell, not as a release or physical tablet result. Run a
single-output nested portrait/tablet campaign, then add per-output host
surfaces for the dual-display topology. Exercise real MPRIS and attention
resources and an application launch/return under this final source. Continue
normalizing the remaining provider domains without moving credentials,
permissions, trusted power confirmation, emergency, lock, or Recovery above
the native boundary. The complete 75-file physical campaign manifest is
`artifacts/linux-stock-base-framework12-20260825/evidence-manifest.tsv`
(7,871 bytes, SHA-256
`5b60e5be27fa6e389d36f73a0d79f9923867fdd743dfa43f59164e8a5b93e059`),
independently verified after every evidence file was finalized.

## 2026-08-26 — Diagnose PiKVM pointer loss on the dual-output Stock session

**Goal / evidence:** Explain why Stock Base accepts touch and touchpad input on
the Framework but clicks from the PiKVM web console do not reach the visible
DP-1 surface. A same-boot, read-only SSH audit found the compositor initializing
eDP-1 first at 1,920x1,200, then positioning DP-1 at x=0 and eDP-1 at x=1,920
after DP-1 appeared. Libinput registered the PiKVM composite keyboard and two
mouse interfaces; sysfs reports its absolute mouse interface with ABS_X/ABS_Y
and the separate relative interface with relative axes. The integrated
touchscreen and touchpad remain distinct device groups.

**Cause / decision:** The direct compositor currently discards the device from
`PointerMotionAbsolute` and maps every absolute pointer through
`space.outputs().next()`. Touch uses the same first-output rule, and relative
pointer motion is clamped to that output. Because eDP-1 was inserted first on
this boot, PiKVM absolute coordinates target the internal panel even though
PiKVM video displays DP-1. This is an output-association bug below the Luau
revision, not missing Stock interaction handlers. No HID, power, media, disk,
session, or revision mutation was made during diagnosis.

**Next gate:** Preserve input-device identity through routing. Map configured
absolute devices to one named output (PiKVM to DP-1 and the integrated
touchscreen to eDP-1), let relative pointers traverse the complete output
layout, and fail safely when a configured connector is absent. Add deterministic
tests with reversed connector discovery order, deploy the compositor through
the development overlay, then prove the same visible Stock control from PiKVM
and the integrated panel before accepting multi-output interaction.

## 2026-08-26 — Route absolute input to explicit direct outputs

**Goal / change:** Remove connector discovery order from direct-session input
routing. The compositor now preserves each libinput device identity and resolves
absolute pointer, touchscreen, and tablet coordinates against a bounded
`input_outputs` map in the existing private `output.json`. An exact device name
selects one connector, a single connected output remains automatic, and an
unmapped multi-output device or absent configured connector fails closed with a
one-time diagnostic. Relative pointer motion now uses the complete logical
output layout and clamps to the nearest valid output rectangle rather than the
first inserted output. The mapping is reapplied with the existing direct-output
configuration refresh; generated Luau content receives no DRM, libinput, or
connector capability.

The Framework deployment configuration maps `PiKVM PiKVM Composite Device` to
DP-1 and all three observed `ILIT2901:00 222A:5539` touchscreen/stylus/mouse
device names to eDP-1. It also selects the new `mirror` layout: the direct
backend defaults to one logical canvas sized to the minimum connected-output
extent and centers that canvas on every physical mode. For the Framework this
keeps DP-1 at `(0,0)` on a 1,920x1,080 canvas and places the 1,920x1,200 eDP-1
at `(0,-60)`, yielding equal content with compositor-owned bars instead of a
second workspace. `extend` remains an explicit, connector-sorted horizontal
policy. The 221-byte configuration is
`artifacts/linux-multi-output-input-framework12-20260826/framework12-output.json`
with SHA-256
`9131a704962daf72ae15c9c9d0cc719552b17f0d84a158897712847d4396c0b8`.
Documentation now defines this mapping as direct-compositor policy and records
the safe ambiguous/unavailable behavior.

**Host evidence:** Deterministic tests reverse eDP-1/DP-1 discovery order and
still route PiKVM to DP-1 and the integrated panel to eDP-1. They also cover no
output, single-output automatic routing, ambiguous multi-output routing,
missing configured connectors, relative crossing into the second output, and
gap/outer-edge clamping. Mirror geometry tests require the 1,920x1,080 shared
canvas and centered panel placement independent of connector order; the
extended-policy regression still requires a 3,840x1,200 aggregate. Relative
input is separately bounded to the mirror canvas. `cargo test -p
sos-compositor --features direct-backend --all-targets` passed 21 tests; the
no-feature compositor suite passed 15; `cargo clippy -p sos-compositor
--features direct-backend
--all-targets -- -D warnings`, the 10-test `sos-linux-session` suite,
`tests/linux-login-session-test.sh`, formatting, and `git diff --check` passed.
The exact logs are under
`artifacts/linux-multi-output-input-framework12-20260826/host/`. The nested
verifier remains unavailable on this development host because `weston` is not
installed; its saved probe fails immediately with `error: required command not
found: weston`, so no nested-runtime claim is made.

The combined release compositor built in 33.98 seconds with maximum RSS
772,488 KiB. It is 5,745,112 bytes with SHA-256
`c1ee04496782b9db853c10d621843127b8ca4242fea882a2623566f4287b354d`.
No model request ran, so model and model-weighted cost were zero.

**Physical deployment / open gate:** The Framework first withdrew its known
address while PiKVM had no active streamer; the screenshot endpoint returned
HTTP 503 and no mutation was sent while target state was unknowable. It later
returned at 192.168.1.132 on the same boot ID
`9b1818f2-c6c3-4829-8109-c9b3320a02a3`. The audited root remained the live
overlay, the internal 1-TB NVMe and both partitions were unmounted, no installer
or block writer was present, and GDM was at its greeter with no SOS process.

`linux-live-deploy` then installed only the compositor in 20,538,381,724 ns as
deployment `20260826T055729Z-5e7d3d2bda31-1947278`; the target's root-owned
5,745,112-byte binary has the exact release SHA-256 above. The private 221-byte
mirror/input configuration was installed mode 0600 for `liveuser` and verified
against its local SHA-256. Deployment evidence is under
`artifacts/linux-multi-output-input-framework12-20260826/deploy/`.

The post-deployment session gate remains open. PiKVM again reported an online
1,920x1,080, 60-fps capture source, and both DP-1 and eDP-1 were connected, but
every captured frame stayed black while logind described the GDM Wayland
greeter as active and non-idle. GNOME logged repeated stage-view allocation,
EGL damage-region, and atomic cursor failures. One reset plus one harmless
keyboard wake and one absolute-mouse move were accepted by PiKVM without a
visible transition; both helpers were retired for this boot and no click,
credential entry, power action, or speculative input followed. A local login
is now required. After Stock starts, acceptance still requires the compositor
diagnostic to show mirror placement and PiKVM-to-DP-1 routing, one harmless
PiKVM click to change visible Stock state, and an integrated-panel/touchpad
regression check. The code is host-verified and physically deployed, but the
mirrored interaction claim is not yet accepted.

## 2026-08-26 — Default the Framework/PiKVM GDM pair to a cloned display

**Goal / cause:** Opening the Framework lid still appeared to extend the
desktop after the direct compositor had been changed to mirror. A same-boot
audit established that no SOS process was running: session `c3` was the active
GDM Wayland greeter. Mutter's `gdctl show --modes --properties` reported DP-1
as the primary 1,920x1,080 logical monitor at `(0,0)` and eDP-1 as a separate
1,920x1,200 logical monitor at `(1920,0)` with scale 1.3333. The observed
extension was therefore GDM policy below the login chooser, not a failure of
the new SOS direct-backend layout.

A verified and then applied `gdctl` clone briefly placed the PiKVM capture
monitor and built-in panel into one logical monitor, but no user
`monitors.xml` was written for the ephemeral `gdm-greeter` account. A later
readback after the lid event showed the original two-monitor extension again.
That runtime-only `gdctl --persistent` attempt is rejected as a boot/hotplug
default.

**Change / host evidence:**
`packaging/xdg/framework12-pikvm-monitors.xml` is now the explicit fallback for
the observed HJW `HDMI TO USB` DP-1 EDID and BOE `NV122WUM-N42` eDP-1 panel. It
uses Mutter's version-2 monitor schema and puts both monitors in one primary
logical monitor at `(0,0)`, scale 1, using their shared 1,920x1,080 modes. It
does not declare a system-only store policy, so a user's own monitor
configuration can still override the default. The SOS compositor remains
separately controlled by `output.json`.

The live-image baker now installs the 911-byte file root-owned at
`/etc/xdg/monitors.xml`, and `check-rootfs` rejects a missing or divergent
copy. `linux-live-deploy` exposes the same file as the `display-defaults`
component so this disposable overlay can be updated without rebaking the ISO.
The regression test parses the XML, requires one logical monitor containing
DP-1 and eDP-1 at 1,920x1,080, requires the absence of a locked system policy,
and verifies both hot-deployment and baked-rootfs identity. XML validation,
shell parsing, `git diff --check`, and `tests/linux-live-image-test.sh` passed;
the final complete live-image test took 1.43 seconds with maximum RSS 30,440
KiB. The packaged file SHA-256 is
`e194c48097e6c26039a572e11642075f54dee3d8530aa8d9bac7e57f4dd7c38a`.

**Physical evidence / decision:** `linux-live-deploy` first installed only
`display-defaults` as deployment
`20260826T062416Z-5e7d3d2bda31-1949972` in 8,559,035,868 ns. Before
finalization, the compositor and display default were deployed together as
`20260826T063120Z-5e7d3d2bda31-1951918` in 31,322,176,090 ns so the target's
current development manifest accounts for both patched artifacts rather than
superseding the compositor record. It names the 5,745,112-byte compositor SHA
`c1ee04496782b9db853c10d621843127b8ca4242fea882a2623566f4287b354d`
and the display default. The remote XML is root-owned, mode 0644, 911 bytes,
and has the exact packaged SHA-256. GDM was restarted without rebooting: the
unit returned in 1,212,987,561 ns and a new active greeter `c4` was ready in
1,755,857,538 ns on the unchanged boot ID
`9b1818f2-c6c3-4829-8109-c9b3320a02a3`. Fresh `gdctl` readback reports DP-1
at `1920x1080@60.000` and eDP-1 at `1920x1080@59.934` inside the same sole
logical monitor at `(0,0)`, primary, scale 1. PiKVM simultaneously recovered a
visible 1,920x1,080 GDM frame at 60 captured fps; the 49,316-byte JPEG SHA-256
is `9dcf87928b8007ac507c5ab37576a6491c2a58f980f497c7db657cd68d81b0ef`.
A later final snapshot was black again even though the active, non-idle greeter
still reported the same one-logical-monitor clone and no new stage-view, EGL,
cursor, or GNOME Shell crash was logged. The durable layout result therefore
does not close the separate intermittent PiKVM/GDM frame-blanking issue.

The root remained the writable live overlay, the internal NVMe had no mounted
partition, GDM remained active, and no SOS process ran during this greeter
change. The old GNOME Shell process logged a Mutter Cogl segmentation fault as
the greeter restarted; the replacement greeter is active and produced the
visible evidence frame, but that shutdown-path fault remains a diagnostic risk
rather than being hidden. No PiKVM HID, ATX, virtual-media, disk, or power
mutation was used for this fix. No model request ran, so model and
model-weighted cost were zero.

This accepts the Framework/PiKVM GDM mirror default for the currently connected
pair and future development-live bakes. It is intentionally hardware-specific:
different EDIDs or connector names fall back to Mutter discovery. The next
physical gate remains logging into SOS and proving its independently deployed
mirror geometry plus PiKVM absolute-click routing and integrated-panel input;
that Stock interaction gate is not implied by the greeter result. The finalized
75-file campaign manifest is
`artifacts/linux-multi-output-input-framework12-20260826/evidence-manifest.tsv`
(8,167 bytes, SHA-256
`9e227218d7b7db5def47b87b04044501076b89bc6161714d656405f43b21f09d`),
and every listed size and digest was independently rechecked after generation.

## 2026-08-26 — Accept the replaceable Stock Shell on Framework 12

**Goal / architecture:** Convert the substantial default revision into the
actual product shell without moving application ownership or platform
authority into Luau. Scene ABI v3 now has one additive, keyed `window_space`
content facet with a bounded gap, fallback and closed `floating`, `tiling` and
`scrolling` policies. GPUI measures its retained bounds and the permanent host
sends only integer geometry plus policy over the authenticated compositor
connection. Validation admits one such node per scene. Luau never receives a
surface identity, PID, Wayland object, socket, executable path or arbitrary
placement operation; Android renders the same primitive as unavailable.

The compositor admits at most eight application windows across native Wayland
and opt-in rootless XWayland, reflows them on configuration, map/unmap and output
changes, and constrains both rendering and hit testing to each assigned
rectangle. This hard clipping is required because an XDG client such as GNOME
Calendar may enforce a buffer minimum wider than its tile. Floating uses a
bounded cascade and click-to-raise; tiling uses a deterministic grid; the first
scrolling policy is an overlapping horizontal card stack rather than a claim of
Niri-style navigation. Existing applications remain opaque freedesktop launch
selections handled by the typed `gio launch` adapter and independent compositor
clients; none were rewritten or wrapped in generated shell commands.

[`experiences/default.luau`](../experiences/default.luau) now owns the top
status bar, bounded application container, collapsed command/agent rail, agent
FAB, reserving command center, application launcher, selectable window policy
and eight provider-backed workspaces. Bounded application status contributions
carry only ID, visible label/value and an optional opaque compatible-app
selection; Linux/Core publish none until the registration broker exists. The
host's `grow` mapping now permits flexible nodes to shrink below intrinsic
content, so a scroll list cannot enlarge the shell beyond its viewport. The
agent workspace is safely reusable in the main canvas and FAB rail through
scope-prefixed stable IDs. Top and side shell UI remains reserved outside the
application rectangle because a true transient overlay shell surface is not
yet implemented.

**Failures and fixes:** The first physical candidate
`988196dcf3c4b4122df51d9127f41951497db9f176795d4edb92166a7b212684`
placed its absolute canvas at an inherited flow offset and sent geometry beyond
the 1,920-by-1,080 canvas. The compositor rejected it correctly, but the host
treated that policy rejection as fatal and the coordinator recovered the
previous revision. Pinning the canvas at top-left, making the containing node
relative and policy rejections non-fatal restored rollback-safe operation.
Opening the command center then exposed the flex intrinsic-size bug: Luau sent
height 1,553, so the compositor retained Floating despite the selected Tiling
state. Zero automatic minimums for `grow` produced and acknowledged the correct
1,530-by-1,022 region. GNOME Calendar next demonstrated that an XDG size request
is not an enforcement boundary; per-window render/input clipping kept its
minimum-size buffer out of the Luau rail. Finally, the FAB initially produced
`interactive node requires a stable id`; the unnamed agent scroll region and
reused IDs were fixed and covered by the embedded-experience test. Forcefully
terminating repeated GDM sessions eventually reached GDM's display-failure
limit; restarting GDM recovered it, and clean `Ctrl+Alt+Backspace` session exit
remains the correct development gate path.

**Host evidence:** Final runs passed 2 compositor-control-protocol, 6
experience-IR, 23 Luau-runtime, 22 direct-feature compositor, 29 Linux-host
experience, 17 Linux-provider, 11 Android-authority library and 5 Android
authority binary tests. `tests/linux-login-session-test.sh` and
`tests/linux-live-image-test.sh` passed; strict clippy passed for compositor and
Linux experience host; format, shell syntax and `git diff --check` passed. The
nested compositor gate is `SKIP`, not PASS: `weston` is absent on this host.
No model request ran, so model and model-weighted cost were zero.

**Physical evidence:** The unchanged 3,056,205,824-byte development ISO
(`28cf8fffee8e2492fc4f2b69fcfe27db3baf7b36`) remained attached read-only.
Boot `9b1818f2-c6c3-4829-8109-c9b3320a02a3` used `LiveOS_rootfs` with an overlay
upper directory; the internal `nvme0n1` partitions remained unmounted. Wi-Fi
was connected to `Lino WiFi`, battery read 81%, and PiKVM captured the mirrored
1,920-by-1,080 canvas. Three exact hot deployments passed:

- experience host `20260826T092123Z-91b99efddca3-2015524`,
  108,048,939,075 ns;
- compositor `20260826T093317Z-91b99efddca3-2041459`, 75,573,250,184 ns; and
- Stock source `20260826T094053Z-91b99efddca3-2044852`, 9,379,079,456 ns.

The target experience host is 17,393,592 bytes, SHA-256
`36e766980614ef51032e3d5064515b366e97f2cd23265b2f54f91b529588ae8e`;
the compositor is 5,782,032 bytes, SHA-256
`c70de8ddb5bbd664c41d5f8b7c7a983f8e48bc42c2d237785b059e810b794c45`;
the login wrapper is 13,020 bytes, SHA-256
`fb0df2b872a3998c5f75cb1d980b9148625d39cc934d82769ca9cbf706e44ec3`;
and Stock source is 45,044 bytes, SHA-256
`6c5ccd60992cf64081237ebd8fdda1e37c8d784fecc89eacbd4de0813feddb2b`.
Normal install/stage/activate committed revision
`327459a9fb595be7db4183e4be31c671616842912bd75ed69e626980426e3eb8`
with transaction `linux-activate-103-327459a9fb595be7db4183e4be31c671616842912bd75ed69e626980426e3eb8`.
The compositor presented it from DRM page flip at commit 11,608 / submit 95,
then acknowledged the agent rail's 1,490-by-1,022 Tiling region.

PiKVM evidence
`artifacts/linux-stock-shell-framework12-20260826/pikvm/final-clipped-two-apps-tiling.jpg`
(SHA-256
`8c7d7438613908f61fa5fe3a23357842c7463f132f00021a0b3e622b9b70c333`)
shows Calculator and Calendar clipped side by side while the command panel is
visible. `final-clipped-panel-close.jpg` (SHA-256
`9daa3831dd760b5ce5629bc9636638eef8e3378740bc51ccb4e49bd2016cf98a`)
proves the Luau close action remained clickable outside native buffers, and
`final-agent-fab-panel-verified.jpg` (SHA-256
`b2f699682cfc97915c9a00868e4584cd5abcb6058a726b7848239916f83348b9`)
shows the independent agent rail and composer. The active login holds a block
inhibitor for `sleep:idle:handle-lid-switch`; the process tree confirms it wraps
the complete `sos-linux-session run-user` lifetime. With no PiKVM HID event
after the FAB action at 09:48:42, `final-idle-five-minutes.jpg` remained visibly
rendered more than five minutes later (76,689 bytes, SHA-256
`2fd6af21780fc15540c6f40dc7be6800daf4fdfa44848317a73d66a0e96c34eb`);
at 09:54:20 the same revision was active, the inhibitor remained held and
NetworkManager still reported `Lino WiFi` connected. The finalized 53-file
campaign manifest is
`artifacts/linux-stock-shell-framework12-20260826/evidence-manifest.tsv`
(5,626 bytes, SHA-256
`fcb0fbacd773e5ed930c6de7025c4a73f89be27e4b48f3a7673f9929c00c0cdc`);
independent verification passed after all evidence files were finalized.

**Decision / next gate:** The replaceable Stock Shell, authenticated window
space, provider launch path, PiKVM input, reserving command/agent rails, native
two-window tiling and active-session always-awake policy are accepted on the
Framework development target. Remaining product work is a real scrolling
navigation/focus model, app contribution broker, title/window list, keyboard
command-center/FAB shortcut, compositor overlay surface, physical portrait
tablet gate and immutable asymmetrically signed Linux stock-recovery pointer.
The session inhibitor is intentionally released at logout; if the always-on
development requirement also includes the logged-out GDM greeter, its separate
idle policy still needs an explicit image default.

## 2026-08-26 — Separate shell, native applications, and the movable agent overlay

**Goal / diagnosis:** Correct three boundaries exposed by manual Stock use on
Framework 12: closing an XDG application had to remove it from layout without a
later shell action restoring it; SOS-native content had to become a compositor-
managed application rather than shell-owned workspace paint; and the agent FAB
had to become a draggable surface above every application with an inline hover
composer and click-through to the full agent rail.

The lifecycle bug came from treating XDG role construction as mapping. The
compositor now registers a role and retained `Window` before the first buffer,
then maps or unmaps it from actual buffer presence on commit. Policy counts
change only on those transitions. Null-buffer unmap, later remap, destroy-after-
unmap, focus selection, and application reflow therefore share one state. The
same application count now includes `NativeApplication` and `Compatibility`
roles.

**Changed:** Scene ABI v3 gained three bounded facets: keyed
`shell_overlay(x,y,width,height)`, keyed `application_surface(title)`, and the
`hover_action` / `surface_drag` interaction fields. The runtime decoder,
validation, Android unavailable adapters, Linux GPUI host, control protocol,
Smithay policy, rendering, input clipping, and tests were extended together.
The trusted host now opens separate shell, transparent overlay, and native-
application XDG toplevels from one revision. The compositor classifies those
surfaces from the registered host PID, keeps the overlay above all other
content, and tiles/focuses/reflows the native application beside freedesktop
clients. The shell no longer paints the active Stock workspace inside itself;
that subtree is rendered only in the separate application surface.

The source-defined overlay expands from 64-by-64 to 430-by-146 on compositor
hover. Its composer is placed above or below the persisted bubble anchor based
on available vertical space and shares the keyed `agent_draft` session with the
full Agent workspace. Pointer press starts the trusted XDG move immediately.
Release with unchanged geometry emits `shell_overlay_activated`; changed
geometry emits only bounded `shell_overlay_moved`. Luau decides what those
events mean but never receives a surface handle or move authority.

**Failures / rejected paths:** An initial compatibility implementation mapped
toplevels before buffers and could not represent the normal null-buffer XDG
lifecycle. The first overlay made the complete 430-by-146 surface a drag
handle, which prevented focusing its composer; drag ownership was narrowed to
the bubble. A flex row then visually placed the expanded bubble at the wrong
edge, so composer and bubble now use explicit retained local positions. A
client-side move triggered only after pointer motion lost the gesture when a
large absolute step left the 64-pixel node; beginning the compositor move on
press and distinguishing stationary completion is the accepted path. One
intermediate handler dispatched on both press and release and toggled the pane
twice; the final compositor-owned activation emits exactly once. During PiKVM
acceptance, numeric mouse-button states returned HTTP success without producing
input; the API requires literal boolean `true` / `false`, and all final clicks
and drags used that form.

**Host evidence:** `cargo fmt --all -- --check`, `git diff --check`, strict
`cargo clippy ... -- -D warnings`, and
`cargo test -p experience-ir -p runtime-luau
-p compositor-control-protocol -p sos-compositor -p sos-experience --features
sos-experience/linux-host` passed: 7 IR, 24 Luau-runtime, 2 control-protocol,
18 compositor, and 29 Linux-host tests (80 total). New regressions cover keyed
primitive cardinality and decoding, native plus compatibility application
counts, first-buffer mapping, and null-buffer unmap without role destruction.
No model request ran, so model and model-weighted cost were zero.

**Physical evidence:** The same read-only development ISO and boot
`9b1818f2-c6c3-4829-8109-c9b3320a02a3` remained in use. `/` was the writable
`LiveOS_rootfs` overlay, while both internal NVMe partitions remained
unmounted. The final dirty-development deployment
`20260826T124920Z-c5581ad6160d-2154040` installed compositor, experience host,
and Stock source in 146,419,707,996 ns. The target compositor is 5,824,312
bytes, SHA-256 `b422bf8b1e9a190c3844e4792217c7cb2af84e31ca9a03198ee4d6a2f89fabf9`;
the host is 17,445,048 bytes, SHA-256
`5d8f6d87fe291ef2455179d0878b67da96f3b221040cbb212ed74e670756089a`;
and Stock source is 48,769 bytes, SHA-256
`41fb68ea03e3f5e41907f51848a396a0a3089422753e123db5f92fbe4cbe41e3`.
Transactional authoring activated revision
`ae84d67ee622eb148f79dceaec3c42ab420ae330c41c5114c6ed4b3de6611e36`.

Calculator and Calendar were visible as two Tiling clients; closing Calendar
removed it and Calculator immediately filled the available application space.
Opening the agent rail afterward did not restore Calendar. A separately
registered and mapped `NativeApplication` surface visibly carried the Stock
workspace. Typing `PROBE` through the hover composer appeared in both that
composer and the full Agent surface, proving one keyed editing state. On the
final binaries, a stationary bubble gesture completed at `(0,881)` with
`moved=false` and committed exactly one `shell_overlay_activated`. A later drag
completed at `(602,586)` with `moved=true`, committed only
`shell_overlay_moved`, and persisted the collapsed bubble at `(968,668)`.

The finalized physical captures include `calendar-closed-reflow.jpg`
(`55deb4b7...`), `agent-open-no-restore.jpg` (`b7de2765...`),
`native-application-surface.jpg` (`5cac0a1b...`),
`inline-composer-focus.jpg` (`8ee06480...`), `agent-bubble-click.jpg`
(`321ede74...`), and the moved expanded/collapsed pair (`798f7676...` /
`82454fbd...`). The complete target journal, binary identities, deployment
manifests, and images are indexed by
`artifacts/linux-shell-surfaces-framework12-20260826/evidence-manifest.tsv`
(1,832 bytes, SHA-256
`b07fb0750bc564b72fecf1602f8cddc3e591fc62c41b0ac0a954c1fc2a2032b6`);
independent manifest verification passed for all 15 files.

**Decision / next gate:** Accept XDG close/reflow, the bounded shell overlay,
and the first native-application composition boundary on Framework 12. Do not
call the application model complete: the current revision and permanent host
still cohost one active native application surface. The next product gate is a
real native-application registry/supervisor with independent content-addressed
revisions, namespaced state and lifecycle, multiple native application
surfaces, app-owned bounded status contributions, and command-center launch /
close/focus integration. Overlay follow-up should add a keyboard toggle and
exercise below-anchor placement and portrait/tablet geometry on hardware.

## 2026-08-26 — Stable agent anchoring, floating moves, lifecycle reflow, and full-panel mirroring

**Goal / diagnosis:** Resolve the next manual Framework findings without
moving shell policy back into native special-case UI: keep the inline agent
composer centered on its floating action except where the output edge requires
clamping; let its field receive focus independently of action activation;
remove hover/drag geometry flicker; support source-native and compatibility
window movement in Floating mode; confirm normal title-bar close/unmap/reflow;
and use the complete 1,920-by-1,200 laptop panel instead of the former
1,920-by-1,080 shared canvas with 60-pixel bands.

The composer focus bug was caused by the Linux host wrapping every
`surface_drag` node in a full-size GPUI element. That implicit wrapper captured
the complete overlay, including the text field. The visible bubble is now the
exact move handle. The flashing came from two independent geometry loops: the
expanded surface constrained movement, then a stale Scene notification could
reapply its old origin while the moved state commit was in flight. The first
edge probe confirmed a second consequence: a 430-pixel expanded surface could
stop at `x=1490`, so its centered 64-pixel action could not reach the physical
edge. The accepted interaction collapses the overlay to its action rectangle
for the duration of the compositor move, rebases the gesture when that bounded
configuration arrives, suppresses hover reconfiguration during the move, and
re-expands after release. The host holds the compositor's pending anchor until
the matching source state returns, eliminating the stale snap-back.

**Changed:** Scene ABI v3 `shell_overlay` gained an optional bounded
`anchor(x,y,width,height,above)`. The Linux host centers the surface on that
stable action rectangle, clamps the surface to the output, and repositions the
action locally when centering is impossible. Moved events report anchor rather
than expanded-surface coordinates. Runtime decoding, IR validation, tests,
Stock source, and API documentation changed together. The Stock bubble remains
the only overlay drag node; its sibling composer keeps normal text focus and
shares the existing `agent_draft` state.

The compositor now accepts `xdg_toplevel.move` for both `Compatibility` and
`NativeApplication` roles only while Floating and only with the primary button
held. It raises the selected window, tracks the pointer in logical coordinates,
and clamps the retained origin to `window_space`. Repeated Floating
configuration preserves moved positions; changing to Tiling or Scrolling
resets deterministic layout. Stock marks only its native application chrome as
`surface_drag`. Unmap and destroy also cancel an active move. Existing
first-buffer/null-buffer lifecycle handling remains authoritative, so an XDG
close immediately removes the surface and recomputes the remaining layout.

Mirror mode now selects an internal `eDP` connector as the canonical canvas,
falling back to the largest connected mode. Base render elements are projected
per output: identity on the Framework's 1,920-by-1,200 panel and a uniform 0.9
fit plus 96-pixel horizontal inset on the 1,920-by-1,080 PiKVM output. Absolute
PiKVM coordinates apply the inverse projection before hit testing, while the
internal touchscreen remains identity-mapped. Filling two different aspect
ratios simultaneously without crop, distortion, or unused pixels is
impossible; preserving the full laptop panel and the complete remote frame is
the chosen tradeoff, so the remote capture has side pillars instead of the
laptop having top/bottom bands.

**Failures / rejected paths:** Treating the expanded overlay origin as the
persisted action made the action visibly jump and constrained its range. A
full-overlay drag wrapper fixed neither focus nor composition and was removed.
Allowing Scene geometry to reconfigure immediately after a move recreated the
old-position flash and was replaced by pending-anchor suppression. During the
PiKVM gate, numeric mouse-button states returned API success but did not emit
clicks; literal JSON booleans are required. The first GNOME Calculator drag
probe hit its invisible upper resize margin and correctly emitted
`xdg_toplevel.resize(edge=top)`, not move; moving the probe lower into the
header emitted `xdg_toplevel.move` and exercised the new path. A process still
running as a GApplication service is not evidence that its window remains
mapped. Boxes also exposed two independent XDG toplevels (main window and
tutorial), so closing one can reveal the other without any lifecycle
resurrection.

**Host evidence:** Final `cargo fmt --all -- --check`, `git diff --check`, and
strict clippy for `experience-ir`, `runtime-luau`, `sos-experience`, and
direct-feature `sos-compositor` passed. Unit tests passed 2 compositor-control,
7 IR, 24 Luau-runtime, 25 direct-feature compositor, and 16 Linux experience
library cases. New checks cover anchor decoding/validation, centered and
edge-clamped resolution, canonical eDP mirror selection, and the exact 0.9 / 96
PiKVM projection. No model request ran, so model and model-weighted cost were
zero.

**Physical evidence:** The read-only development ISO remained mounted from
`sr0`; `/` remained `LiveOS_rootfs`, and both internal `nvme0n1` partitions
remained unmounted. Deployment
`20260826T133703Z-710d04d63585-2172861` installed compositor, experience host,
and Stock source in 131,024,294,235 ns. Final compositor deployment
`20260826T135507Z-710d04d63585-2179458` took 77,636,366,845 ns. The target
compositor is 5,836,424 bytes, SHA-256
`3013305f59675f7d2a7c7c37530fd69e462152ea824958e05148be9fdbe0a95d`;
the host is 17,447,864 bytes, SHA-256
`2be09044fdab6aa332872fb5db81a15ba2c886c191a917a60e85385de136c83d`;
and Stock source is 48,701 bytes, SHA-256
`b121999d2630574c3c38f90f8610f6ae505af71bcc8e55e183fad4fa9e25bb37`.
Normal transactional authoring activated revision
`a3c2acc91aad69b507f07faaa9d495d8cc04dd917ad14ab4f519c33128f678ba`.

The final session logged `eDP-1` at `(0,0)` / 1,920-by-1,200 and `DP-1` at
`(0,0)` / 1,920-by-1,080, then presented nonempty frames on both. PiKVM showed
the complete 16:10 scene with the expected side fit. Typing `abc` focused the
inline composer without opening the agent rail. A drag collapsed the action,
reached anchor `(0,523)`, emitted one moved action, and reconfigured the
expanded overlay at `(0,441)` / 430-by-146; the capture shows the composer
clamped left while the action remains at logical `x=0`. A normal Files
title-bar X removed the mapped client and the remaining native surface filled
the layout. Subsequent shell interaction did not restore it. In Floating mode,
the source-native surface moved to `(604,480)` and a traced GNOME Calculator
move finished at `(767,480)`, both bounded by the declared application region.

The finalized 16-file campaign manifest is
`artifacts/linux-shell-interactions-framework12-20260826/evidence-manifest.tsv`
(1,788 bytes, SHA-256
`0307ed05ab29c159b44fef786703b1f6f1270f41ca8f6819521972a839bf2b22`);
independent verification passed. Key captures are
`composer-focused-and-typed.jpeg` (`16f0b7da...`),
`edge-drag-after-clamped.jpeg` (`b3769dbb...`),
`titlebar-close-reflow.jpeg` (`850e1b74...`), the native move pair
(`b6782392...` / `929a7bbe...`), and the compatibility move pair
(`001dd9a0...` / `ba811660...`).

**Decision / next gate:** Accept anchored composer placement and focus,
flicker-free edge movement, XDG close/reflow, native and compatibility Floating
movement, inverse PiKVM input, and the full Framework panel as the current
physical development baseline. Remaining window-management work is resize,
maximize/minimize, keyboard move/focus, true scrolling navigation, XWayland
move parity, and independent native-application supervision. The next display
gate is physical portrait/tablet geometry and an explicit policy for whether a
remote mirror should fit, crop, or use an independently composed surface.

## 2026-08-26 — Tiled XDG close hit-testing and stock-preserving faux prompts

**Goal / diagnosis:** Reproduce and remove the remaining legacy-application
“close, then reappear” behavior reported from the Stock shell, and stop the
offline programmatic agent stub from replacing Stock with the older Daily Flow
demo. PiKVM reproduced the window bug directly: with Calculator in Tiling, the
pointer became GTK's north-edge resize cursor while centered on the visible X.
The compositor assigned sizes but never advertised XDG tiled-edge states, so
GTK retained its invisible client-side resize margin above the close control.
Those clicks requested `xdg_toplevel.resize` rather than closing the window;
the still-live mapped surface naturally appeared again after later shell
composition. This was a hit-testing/state-contract failure, not resurrection of
a correctly destroyed toplevel.

**Changed:** Application XDG toplevels now receive all four `tiled_*` states in
Tiling and Scrolling, both in their initial configure and every later relayout.
Floating clears those states without disturbing activation. Resize requests are
explicitly unsupported: managed layouts are restored from compositor policy,
while Floating keeps its existing geometry until interactive resize is
implemented. Tests prove managed layouts set every tiled edge, Floating clears
them, and unrelated activation state survives both transitions.

The default offline source is now the stock `default.luau` shell in
`sos-agent-login`, the selectable-session installer, and development-live image
state. A faux prompt still executes context, validation, and submission, but
submission becomes the existing `already_active` no-op instead of activating
Daily Flow. `daily-flow.luau` remains installed only as an explicit developer
fixture for mutation/activation tests; it is no longer selected by default.
The current live user's mutable configuration was changed to the same Stock
source without rebuilding the ISO.

**Failures / rejected paths:** The first resize handler resent the one-window
fixed size, which could distort a multi-window managed layout if a stale or
malicious client still requested resize. The accepted handler recomputes the
complete managed layout instead. Process presence was rejected as the mapping
oracle: GApplication processes may legitimately outlive a window, while the
decisive evidence is XDG destroy/null-buffer plus compositor unmap/destroy.
During the final direct-client trace, the first click followed a window mapping
under a stationary pointer and therefore retained the old pointer focus. Moving
away and back produced a real client `enter`/`motion`; the calibrated close
then emitted the expected protocol sequence. This diagnostic artifact was not
classified as an application lifecycle failure.

**Host evidence:** `git diff --check`, `cargo fmt --all`, the four focused
`sos-compositor` XDG tests, and direct-backend `cargo clippy ... -- -D warnings`
passed. `tests/linux-login-session-test.sh`,
`tests/linux-live-image-test.sh`, and
`tests/linux-hardware-gate-test.sh` all reported `PASS`; final `bash -n`
covered the changed packaging and image scripts. No model request ran, so model
and model-weighted cost were zero.

**Physical evidence:** The Framework 12 remained on the read-only development
ISO: `/` was `LiveOS_rootfs`, `sr0` supplied the live payload, and all internal
`nvme0n1` mountpoints were empty. No ISO rebuild or internal-disk write was
performed. Final dirty-development deployment
`20260826T153052Z-6fdd3b0a0db1-2205136` installed only the compositor in
47,145,816,366 ns. The root-owned target binary is 5,838,768 bytes with SHA-256
`4854c1f06a51c1993dffacc34ff271275332814899e0af87af7ff1984c6241d9`;
the target manifest matches it.

On that final binary, Calculator's close hover showed a normal arrow. The traced
client then received pointer motion at local `(891.31,30.08)` and primary-button
press/release, sent `xdg_toplevel.destroy` followed by `wl_surface.attach(nil)`,
and exited. The compositor logged `destroyed ... role=Compatibility
was_mapped=true`; the process was absent, and switching from Attention to Home
did not remap a surface. A final faux prompt executed
`get_experience_context`, `validate_experience`, and `submit_experience`; the
active revision stayed
`e2af4edc186d576187e8c205fdee6439bc8f9b5424a46538ff4b7640904e01a8`
before and after, and PiKVM still showed Stock Home.

The nine finalized artifacts are indexed by
`artifacts/linux-window-lifecycle-framework12-20260826/MANIFEST.sha256`
(860 bytes, SHA-256
`809c6b5dcc925ac01d0f0271c43f58b7cbee1b67e039cf7d33a8e43cd647bf4a`);
independent verification passed. Key captures are the pre-fix resize cursor
(`ccc1ee28...`), post-fix arrow (`724b9519...`), closed application
(`a17225b2...`), Home with no remap (`da78ab39...`), and Stock retained after
the faux prompt (`da78ab39...`).

**Decision / next gate:** Accept tiled/scrolling close hit-testing, ordinary GTK
destroy lifecycle, no-remap navigation, and Stock-preserving offline prompts on
Framework 12. The next window-management gate remains real interactive resize,
maximize/minimize, keyboard focus/move, scrolling navigation, and XWayland
parity; those should build on this XDG state contract rather than reintroducing
client-side ambiguity in managed layouts.

## 2026-08-26 — Stable managed-window identity and master-stack tiling

**Goal / diagnosis:** Reproduce the remaining report that legacy windows appear
to close but return when the Stock command rail opens, stop title-bar
double-click from overflowing a tile, and replace the sparse three-window grid
with conventional master-stack tiling. The untouched Framework state reproduced
the exact lifecycle symptom: one click made both Firefox and Files disappear,
but the compositor logged no null-buffer, unmap, or XDG destroy. Opening the
command rail made both mapped clients visible again.

The root cause was geometry identity being derived from stacking order.
`application_window_rectangles` zipped policy rectangles to
`Space::elements()`, while a pointer press raises the focused application and
therefore changes that order. The mapped locations did not move, but rendering
and hit testing immediately clipped each application against another window's
rectangle. A later rail resize performed a complete relayout, realigned those
two orders, and exposed the clients again. This was neither an application
restart nor a valid close followed by remap.

**Changed:** Managed Tiling and Scrolling now sort applications by their current
spatial position before assigning policy rectangles. Click-to-raise can still
change z-order, but it cannot change a window's geometry identity, clipping, or
hit region. Floating retains stacking order and its independently preserved
origins. A focused test proves the spatial order remains master, upper stack,
lower stack even when the upper stack item is presented first as the raised
window.

Tiling is now deterministic master-stack: the first application occupies the
full-height left half, and all later applications share equal rows in the
right-hand half. Two applications remain equal halves; with three, the master
is approximately twice the area of either stacked client. The calculation
absorbs integer remainder in the final stack tile and reduces an impossible
configured gap rather than placing any of the eight bounded windows outside
the declared window space. Exact three-window and maximum-count/oversized-gap
tests cover both policies.

Application maximize, unmaximize, fullscreen, and unfullscreen requests are
now denied by recomputing the complete active layout. Previously a title-bar
double-click used the one-window rectangle as the selected tile's new size
without moving its origin, which visibly overflowed the window space. Distinct
maximize/fullscreen semantics remain unimplemented; the current behavior is an
explicit no-op that preserves every assigned tile.

**Bubble decision / diagnostic:** No source or compositor behavior changed for
the agent bubble. Its composer intentionally collapses only while an XDG move
is active so changing overlay bounds cannot feed back into its drag origin. On
the final binary, PiKVM captured the 64-pixel action during the held drag and
the centered composer expanded again at the new anchor after release. One
initial absolute-pointer probe did not start a drag until a second small motion
established focus after the hover expansion; physical pointer motion naturally
provides that transition, but explicit focus synchronization for a stationary
pointer beneath newly configured surface geometry remains a follow-up.

**Host evidence:** `git diff --check`, `cargo fmt --all -- --check`, all 23
`sos-compositor` unit tests, and direct-backend
`cargo clippy --locked -p sos-compositor --features direct-backend --bin
sos-compositor -- -D warnings` passed. The no-feature test build retains its
existing cfg-dependent unused-`output` warning; the exercised direct build is
warning-free. No model request ran, so model and model-weighted cost were zero.

**Physical evidence:** The final compositor-only dirty development deployment
`20260826T161919Z-ddbe5b579a3b-2211914` took 71,982,020,066 ns. Its root-owned
mode-0755 compositor is 5,845,464 bytes with SHA-256
`6127583436eada53324ab602e40503b61c6bca6a3ba55f7ce70b8117771a37de`,
matching the target deployment manifest. The Framework remained on
`LiveOS_rootfs` supplied by the read-only `sr0` development ISO; both internal
`nvme0n1` partitions remained unmounted. No ISO rebuild, installer, internal
mount, or internal-disk write occurred.

On those exact bytes, Stock plus Calculator and Calendar visibly formed one
full-height master and two equal stacked tiles. Closing Calendar emitted
`destroyed compositor-managed XDG toplevel role=Compatibility was_mapped=true`;
Calculator immediately expanded into the remaining half. The Calendar
GApplication service was still resident, demonstrating again why a process is
not a mapping oracle. Closing and reopening the command rail configured the
window space from 1,530 to 1,844 and back to 1,530 logical pixels without any
Calendar map, and the final frame contained only Stock plus Calculator. A
Calculator title-bar double-click left it bounded in the same tile.

The 18 finalized files are indexed by
`artifacts/linux-window-manager-framework12-20260826/MANIFEST.sha256`
(1,780 bytes, SHA-256
`ede6f9e3f97b9f4a8519e98bd51b31694510e11116e6582f5c80bc2fd0782571`);
independent verification passed. Key frames are the pre-fix focus/raise hide
(`08839f58...`), pre-fix rail-triggered return (`201b2d32...`), final
master-stack (`7ecf92e2...`), final no-remap rail replay (`300af871...`),
contained double-click (`2dfc9985...`), active-drag collapse (`141e5070...`),
and post-release expansion (`15f0318a...`).

**Decision / next gate:** Accept stable managed geometry across focus raises,
ordinary close followed by rail reconfiguration, master-stack Tiling, bounded
state-request denial, and the temporary bubble-collapse drag policy on the
Framework 12. The next window-management gate is explicit maximize/minimize and
fullscreen policy, keyboard focus/move, true scrolling navigation, XWayland
parity, and stationary-pointer focus synchronization after compositor-driven
surface geometry changes.

## 2026-08-26 — Balanced recursive tiling and CSD-accurate close input

**Goal / diagnosis:** Replace the fixed master-stack policy with balanced
recursive tiling and explain why GNOME Files still ignored some visible close
clicks after Calculator and other compatibility applications had become
reliable. The untouched Framework state retained one visible Files window. A
PiKVM click on its visible close control produced no null buffer, XDG unmap, or
destroy, while earlier windows had emitted real destroys. Nautilus remaining as
a `--gapplication-service` process was therefore rejected as a lifecycle
oracle; the visible toplevel had not closed.

Refreshing Smithay's cached pointer target immediately before a press was a
useful stationary-pointer hardening but did not fix Files by itself. The first
development deployment proved both failed clicks were routed at canonical
`(1495.546875, 657.71484375)` to the live Nautilus compatibility PID, and an
ordinary Files folder selection worked at the same protocol boundary. A
focused `WAYLAND_DEBUG=client` launch then captured the earliest discrepancy:
Nautilus declared `xdg_surface.set_window_geometry(20, 20, 747, 553)`, but the
visible close click arrived as `wl_pointer.enter(..., 724.546875,
22.71484375)`. SOS rendered from the buffer origin (`mapped location -
window_geometry.loc`) while its custom prioritized hit test subtracted only the
mapped geometry origin. Input was consequently shifted 20 logical pixels up
and left into the GTK client-side-decoration margin. This explains the resize
cursor/gesture reports and why controls with larger hit regions appeared
intermittent. Activation-configure ordering was considered and rejected: a
second traced click emitted no configure between press and release and still
missed before the origin fix.

**Changed:** `window_under` now converts every mapped window geometry location
to the same surface render origin used by the renderer before calling
`surface_under`; a focused test covers the observed `(771, 635)` mapping and
`(20, 20)` client-side-decoration inset. Every ungrabbed pointer press also
re-evaluates its target at the current coordinates before dispatch, preventing
a compositor map, unmap, or relayout beneath a stationary pointer from leaving
stale focus. The minimal INFO trace records only logical coordinates, client
role, and PID.

Tiling now recursively splits each leaf along its longest edge. Branch length
is proportional to branch window count, so areas remain balanced: three
windows use three near-equal leaves appropriate to the current aspect ratio,
four form a 2x2 quad, and later windows continue subdividing instead of joining
an ever-longer right-hand stack. Returned rectangles are row-major so the
existing spatial identity rule remains stable across focus raises, close, and
relayout. Effective horizontal and vertical gaps are reduced only when needed
to keep all eight bounded leaves positive and inside the declared window
space. The compositor and Stock architecture documents now name this policy
and the render-origin input contract.

**Host evidence:** `cargo test -p sos-compositor --lib` passed all 25 tests,
including exact three-window splits, the four-window quad, maximum-count bounds
under an impossible gap, geometry-order stability, and CSD render-origin hit
testing. `cargo clippy --locked -p sos-compositor --features direct-backend
--bin sos-compositor -- -D warnings`, `cargo fmt --all -- --check`, and
`git diff --check` passed. The no-feature test build retains its existing
cfg-dependent unused-`output` warning; the exercised direct build is
warning-free. No model provider ran, so model and model-weighted cost were
zero.

**Physical evidence:** The final compositor-only dirty development deployment
`20260826T165827Z-6bf6dd9f741d-2216553` completed in 74,829,112,221 ns. Its
root-owned mode-0755 compositor is 5,854,592 bytes with SHA-256
`382888751be10c08b2981d7d0a9f476370f52907f06b4926b125ff4ab327fd5b`,
matching the target and deployment manifest. Source metadata records parent
`6bf6dd9f741d1ecba5cd2c5c07429ab41f656461` with the component code dirty; the
deployed binary contains the compositor diff now committed, while this evidence
ledger was finalized afterward. The development deployment itself remains
promotion-ineligible. The Framework remained on `LiveOS_rootfs` from the
complete 3,056,205,824-byte
read-only, non-writable `sos-development-live-28cf8fffee8e.iso`; both internal
`nvme0n1` partitions remained unmounted and no installer or block writer ran.

On those bytes, Stock plus Calculator and Calendar formed three balanced
recursive leaves, then Files opened as the fourth leaf of a visible 2x2 quad.
One click on Files produced
`destroyed compositor-managed XDG toplevel role=Compatibility was_mapped=true`
49.937 ms after the compositor's press-routing record; Nautilus then
disconnected. The remaining three leaves immediately rebalanced. Closing the
command rail expanded them to three aspect-appropriate columns, and reopening
it restored the earlier recursive geometry without any Files map or visible
return. This is the first clean downstream replay after the focused protocol
fix; repeated pre-fix full attempts were stopped at the runtime-debug circuit
breaker.

The 55 finalized files are indexed by
`artifacts/linux-window-manager-framework12-20260826/followup-balanced-close/MANIFEST.sha256`
(5,313 bytes, SHA-256
`789f5965c975c49be7caebcee0a6df07b20eba7945fadf791e84a7b61562e169`);
independent verification passed. Key evidence is the pre-fix Nautilus protocol
trace, the final four-window quad, the one-click Files close, both rail states,
the compositor lifecycle log, the storage/binary audit, and the exact
deployment metadata.

**Decision / next gate:** Accept balanced recursive Tiling, CSD-accurate pointer
coordinates, stationary-pointer press synchronization, one-click Files close,
and no-remap rail relayout on the Framework 12. The next window-management gate
remains explicit minimize/maximize/fullscreen policy, compositor-owned keyboard
close/focus/move commands, true scrolling navigation, and XWayland parity.

## 2026-08-26 — Close the large-experience authoring API gaps

**Goal:** Turn the friction observed while Codex built the large Stock
experience into explicit API support: actionable multi-state validation,
canonical Luau types, typo-safe decoding, responsive output placement,
bounded application-window observation/control, and revision-local modules.
Keep compositor and host authority closed and do not broaden the uncertain
provider-normalization item without a concrete failing use case.

**Changed:** Added a structured validation report that renders the default
state plus up to 32 declared `validation_scenarios`, gathers every failure, and
reports per-state node/input/image/paint/animation/semantics counts with the
runtime stage, consuming scene path, and message. The decoder now rejects
unknown keys through nested layout, content, paint/layer, interaction,
animation, and semantics tables. The local validator supports text or clean
JSON output.

Added the canonical API v3 Luau type prelude and pinned the official Luau
analyzer at tag `0.728`, commit
`ddcea05e1cc6f534e5eaac33325690c12f1ed274`. `sosctl typecheck` and `validate`
use it, and every checked-in experience now carries useful model/state/node/
event/effect annotations. Stock declares nine hidden states in addition to its
default state. Its first overlay position is output-relative end/end placement
with a logical margin; a persisted compositor anchor still wins after a move.

Revision manifest v3 now admits bounded namespaced `luau` sidecars. A sandboxed
cached `require` resolves only the current revision's package and rejects host
loading, cycles, reserved or un-namespaced IDs, invalid source, and `nil`
results. `sosctl validate` accepts repeatable `--module ID=FILE` inputs. The
Linux authoring broker and resident Pi tools exchange an exact optional module
package, preserve unexposed non-Luau assets, return the structured validation
report, and bind submission to the exact source and module bytes that passed.
The typed `stock.theme` module demonstrates the reusable token shape while the
cross-platform bootstrap retains an in-file fallback.

Added shell-model ABI 1 with a bounded logical canvas, at most 16 opaque
outputs, and at most 64 opaque native/compatibility windows. The compositor
publishes map/unmap/title/focus/output/resize changes without exposing
connectors, handles, application IDs, PIDs, or commands. Stock lists current
windows and emits only advertised `shell.focus_window` or
`shell.close_window` selections. The authenticated compositor re-resolves each
opaque ID, rejects stale/non-owned windows, and remains the focus/lifecycle
authority. This is deliberately distinct from the one revision-owned
`application_surface`; independent application revision supervision remains
future work.

**Evidence:**

- `cargo test --locked -p experience-ir -p runtime-luau
  -p revision-supervisor -p compositor-control-protocol -p sos-compositor
  -p sos-experience -p sos-linux-session` passed 117 tests across the selected
  unit, integration, and protocol targets. The no-feature compositor build
  retains its pre-existing cfg-dependent unused-`output` warning.
- `npm --prefix services/sos-agent test` rebuilt the packaged runner and passed
  14/14 tests, including exact module-package binding and invalid-report phase
  retention.
- `./tools/sosctl validate` passed all five top-level checked-in experiences.
  Stock passed the official analyzer and 10/10 runtime scenarios at 55,702
  source bytes. The repeatable-module JSON run reported `module_count = 1`,
  `valid = true`, and 10 scenarios for `stock.theme`.
- `cargo check` passed the seven changed Rust packages, and the explicit
  `cargo check --locked -p sos-compositor --features direct-backend --bin
  sos-compositor` passed. TypeScript `tsc --noEmit`, `bash -n tools/sosctl`,
  `cargo fmt --all`, and `git diff --check` passed. ShellCheck was not installed
  on this host, so no ShellCheck result is claimed.

**Failures and rejected approaches:** The first analyzer sweep found that only
the newly annotated Stock source passed; four older `--!strict` experiences
still depended on inference-heavy open tables. They were migrated instead of
adding a legacy checker bypass. The first module authoring fixture used stale
`text_session.text` syntax and omitted `state_key`; the strict decoder rejected
both at the exact child path, and the fixture was corrected. Initial `--json`
validation mixed human type-check status into stdout; status now goes to
stderr so stdout remains parseable JSON. A broad generic shell/process API was
rejected: the implemented window actions remain opaque, capability-advertised,
and compositor-owned. No provider API normalization was added because this
iteration produced no concrete ambiguity that justified another authority
surface.

**Decision:** Adopt the report, type, module, responsive placement, and shell
model additions as API v3 authoring support. Keep Rust decoding and
compositor/provider re-resolution authoritative even when static analysis
passes. This entry records host tests only and makes no new physical acceptance
or latency claim.

**Open risks / next gate:** Run the explicit SOS Linux stable-host acceptance
campaign with several real native and XWayland windows, exercise source-driven
focus and close, title/focus/map refresh, mirrored and independent output
layouts, and stale-ID rejection. Then exercise an actual resident-agent
multi-module revision through context → validation report → submission. True
scrolling navigation, minimize/maximize/fullscreen policy, independent
application revisions/lifecycle, per-output shell surfaces, and the physical
portrait/tablet responsive gate remain open.

## 2026-08-27: Add key-authenticated SSH to development-live

**Goal:** Let the private Fedora development ISO trust one explicit developer
Ed25519 public key so the Framework deployment loop can use SSH without a
remote password. Keep the local recovery password, per-boot SSH host keys, and
the non-promotable development-live boundary.

**Changed:** `tools/linux-live-image bake` and
`configure-development-access` now accept an optional
`--ssh-authorized-key-file`. They reject symlinks, multiple lines, malformed
keys, and every key type except Ed25519. The bake strips the input comment,
records the OpenSSH SHA-256 fingerprint in all image identity records, and
stages a root-owned restricted key. Fedora's post-user-creation `livesys` hook
copies that key to `liveuser` with mode 0600, restores its SELinux label, and
still starts SSH only as its final action. Key-authenticated images require
public-key authentication and disable password and keyboard-interactive SSH;
the password hash remains available to GDM and the local console. Agent, port,
and X11 forwarding are disabled on the authorized-key entry. Omitting the new
option preserves the previous remote password path.

The rootless Fedora build container now includes OpenSSH key tools. Rootfs
validation binds the staged key to the identity fingerprint, checks its owner,
mode, restrictions, boot provisioning, and SSH policy, and rejects undeclared
keys. The SOS policy uses the first SSH drop-in, and validation rejects a base
configuration that could assign authentication settings before loading it. The
current development machine's
`/home/carlid/.ssh/id_ed25519.pub` validated as a 256-bit Ed25519 key with
fingerprint `SHA256:53ddO6+sXQRlT4FrWSspHZtRsH424/cDB2yj2nAWcq4`.

**Host evidence:** `bash tests/linux-live-image-test.sh` passed with
`linux_live_image_host_tests=PASS` in 1.95 seconds and 30,240 KiB maximum RSS.
It covers key-only and password fallback identities, invalid type and encoding,
symlink and multi-key rejection, forwarding restrictions, fingerprint
tampering, SSH drop-in precedence, boot-hook order, and rootfs validation.
`./tools/linux-live-image rootless-test` rebuilt the pinned Fedora 44
environment and passed owner,
hardlink, ACL, capability, portable-xattr, and SELinux-regeneration checks in
88.68 seconds with 258,992 KiB maximum RSS. The resulting local build image is
`localhost/sos-linux-live-build:fedora-44`, 1,629,356,944 bytes, image ID
`sha256:f38110bd652e853e71dd7e8de3056e48c7aa9f455a996fec963e63322b1ddadc`,
from source parent `72846b325d88861de6487ede6db89b16fa3f5f57` plus the listed working-tree
changes. An in-container check found `/usr/bin/ssh-keygen`. A disposable Fedora
44 `openssh-server` inspection confirmed that the first active server directive
is `Include /etc/ssh/sshd_config.d/*.conf` and that Fedora's supplied drop-ins
start at `40-redhat-crypto-policies.conf`, so the SOS `00-` policy takes
precedence. A host `sshd -T` parse resolved `AuthenticationMethods publickey`,
`PasswordAuthentication no`, `KbdInteractiveAuthentication no`,
`AuthorizedKeysFile .ssh/authorized_keys`, `StrictModes yes`, root login off,
and only `liveuser` allowed. Both shell syntax checks and `git diff --check`
passed. ShellCheck was unavailable. No model provider ran, so model and
model-weighted cost were zero.

**Failures and rejected approaches:** The tests rejected RSA, a structurally
invalid Ed25519 key, a symlink input, a two-key file, and a valid replacement
key whose fingerprint did not match image identity. Automatic discovery of a
developer key under a home directory was rejected because it would make the
bake machine-dependent and hide which access authority entered the artifact.
A reusable baked SSH host private key remains forbidden; it would give every
copy the same host identity.

**Decision / next gate:** Keep the public key optional and explicit, and use
the current machine's Ed25519 public key for the next private development ISO.
No ISO or physical acceptance claim was produced in this change. After the
source has a clean revision, rebuild with `--ssh-authorized-key-file
/home/carlid/.ssh/id_ed25519.pub`, boot it on the Framework through the normal
disk-protected workflow, verify public-key login and remote password rejection,
and confirm that local password recovery still works.
## 2026-08-26: Formalize experience derivation and live composition

**Goal:** Define how SOS combines experiences without confusing source
derivation, runtime composition, ordinary window coexistence, revision-local
code reuse, or shared appearance. Preserve independent state, authority,
activation, failure, and custom visual systems across a live boundary.

**Changed:** Added `docs/experience-composition.md` as the architecture decision
for stable experience/revision/instance identities, fork and remix lineage,
published entry points, revision-bound dependency aliases, the proposed
host-owned `experience_mount`, typed properties and child events, data-flow
authorization, graph validation, locked and future tracked dependencies,
runtime containment, appearance resolution, authoring targets, activation,
rejected shortcuts, and the first acceptance gate. The document explicitly
marks derivation metadata, the contract package, global appearance model, and
mount content kind as unimplemented in Scene ABI v3.

Updated the vision's canonical artifact from one parent revision to exact
derivation parents plus exported contracts and dependencies. Updated the Stock
report to classify its current same-revision `application_surface` as
native-window coexistence rather than experience composition. The API documentation
now states the current limit and points to the future contract, and the README
documentation map links the focused decision.

**Evidence:** `git diff --check` passed for tracked edits. A no-index whitespace
check passed for the new untracked document. Focused relative-link checks
resolved every local link in the changed README, vision, Stock, and API files.
`docs/experience-composition.md` is 372 lines, 2,332 words, and 16,832 bytes.
This was a documentation-only architecture change, so no runtime, compositor,
device, or latency result is claimed.

**Failures and rejected approaches:** No documentation check failed. The first
framing used composition for independently presented windows; the decision now
calls that coexistence. A derivation-only model was rejected because it loses
child identity, state, independent updates, and failure isolation. Raw child
scene injection, cross-revision `require`, implicit permission union,
schema-only tracked updates, and a global executable style engine were also
rejected. Live composition instead keeps the child behind a host-enforced
mount, while a fork or remix emits a complete new revision.

**Decision:** Support both derivation and live composition. A fork has one exact
parent and a remix has several; both produce a self-contained experience with
new grants. Live composition resolves a declared child export through a
revision-bound alias and keeps parent and child VMs, state, grants, validation,
and activation separate. Appearance crosses the boundary as typed data;
styles remain revision-local code. Neither `application_surface` nor ordinary
window placement satisfies this contract.

**Open risks / next gate:** Fix numeric schema and graph limits, the exact
contract serialization, experience registry and manifest identity rules,
data-flow grant representation, child-event ABI, multi-VM lifecycle, global
appearance ABI, and tracked graph transaction semantics. Then implement the
Agenda and Media gate defined in `docs/experience-composition.md`: mount both
exports in a Dashboard, prove containment and appearance propagation, exercise
a tracked update, and create a self-contained remix from the same parents.

## 2026-08-26: Implement experience derivation, composition, and appearance

**Goal:** Build the complete shared and Linux-host milestone plan from the
experience derivation and composition decision. Preserve the API v3 edit path,
allow custom visual systems, keep authority at revision and experience
boundaries, and make graph activation recoverable rather than inferring
success from package installation.

**Changed:** Added the platform-neutral `experience-package` crate with stable
experience, revision, export, dependency, instance, and graph identities;
closed bounded value schemas; canonical contract digests; exact fork/remix
lineage; locked and tracked bindings; explicit boundary grants; typed global
and container appearance; canonical resolved graphs; and fixed limits for
exports, dependencies, values, depth, and instances. Revision manifest v4 now
hashes the canonical package, while v3 remains readable for the legacy path.
The durable registry owns stable experience identities and current/previous
pointers. The resolver verifies exact exports, digests, grants, roles, cycles,
and graph limits, and the graph store persists the exact accepted snapshot.

Added Experience API v4 named exports and host-owned `experience_mount`
content with bounded properties, container appearance, and declared child
events. `GraphRuntime` runs one sandboxed VM, state namespace, asset namespace,
and package per graph node. It validates every boundary value, contains a
failed child, shares durable state across repeated instances of the same
experience revision, and rolls back a partially failed child-event cascade.
Provider/state protocol v2 now supports per-experience resources, durable
revision-specific state, independent appearance generations, and atomic graph
state promotions. Ordinary mounted experiences cannot use shell-only content
or shell effects.

Extended the host protocol and supervisor with prepare, quiesce, present,
confirm, discard, and finalize graph operations. A durable activation journal
coordinates registry and graph pointers, retains the previous graph until
finalization, and selects a recoverable side after an injected crash. The Linux
host prepares graphs on a dedicated worker, renders and clips the composed
tree, routes input to one graph owner, namespaces text and accessibility state,
maintains revision-bound provider subscriptions, and can restore the previous
graph after a pre-commit failure.

Added bounded derivation and composition context, validation, and submission
tools to the trusted Rust broker and resident TypeScript agent. Validation is
bound to exact parents, dependencies, source, modules, contracts, grants, and
representative viewport and appearance scenarios. Submission installs the
candidate and resolved graph but reports activation as still required. A new
identity receives its initial registry pointer; replacing an existing identity
leaves its current pointer unchanged until graph activation. Authoring also
rejects replacement of a non-ordinary identity. Added Agenda, Media, Dashboard,
and self-contained Agenda-Media Remix packages plus a deterministic reference
installer and tracked-update/restart integration gate.

**Evidence:**

- `cargo test --workspace --locked --lib --bins --tests -q` passed every
  product unit and integration target. The focused
  `sos-linux-session` replacement-pointer regression passed, and
  `cargo test --locked -p sos-experience --features linux-host -q` passed 32
  Linux-host tests. `cargo test --workspace --locked --doc --exclude
  gpui-mobile -q` passed the remaining workspace documentation targets.
- `cargo check --workspace --locked --all-targets -q` passed. It retained the
  pre-existing cfg-dependent unused-`output` warning in `sos-compositor`.
- `npm --prefix services/sos-agent test` rebuilt the packaged runner and passed
  16/16 tests, including exact derivation and composition package binding.
- `./tools/sosctl typecheck` and `./tools/sosctl validate --json` passed all
  four checked-in composition examples.
- `cargo fmt --all`, `git diff --check`, and a focused relative-link check over
  the changed README and architecture documents passed.

**Failures and rejected approaches:** An unfiltered `cargo test --workspace
--locked` passed the product suites and then failed two pre-existing
`gpui-mobile` documentation examples: one refers to an illustrative
`MyVideoFactory`, and one omits imports for `Arc`, `IntoElement`, and `div`.
The vendored examples were not changed or counted as product failures. The
Android target probe stopped in `psm` because this host has no
`aarch64-linux-android-clang`; no Android build result is claimed. An early
submission path could make a replacement revision current during registration;
it was changed to leave existing pointers for activation and covered by a
regression test. Automatic activation from the authoring broker was rejected
because it would collapse validation, presentation, durable pointer commit,
rollback, and truthful status into one unverified response.

**Decision:** Adopt package and Experience API v4 for derivation and live
composition while retaining API v3 for legacy single-experience revisions.
Appearance is authority-owned typed data; executable styles stay local to the
experience. A fork or remix is a self-contained new revision. A live
composition preserves child identity, VM, state, grants, and failure ownership
behind a host mount. Installation and validation never imply activation.

**Open risks / next gate:** This entry completes the shared and Linux desktop
implementation milestones, not physical acceptance. Run the SOS Linux stable
host workflow to verify real pointer and text focus, accessibility focus,
compositor clipping, presentation latency, and recovery on hardware. Add graph
loading and rendering to the Android GPUI host, then run its build and physical
device gate with the Android NDK available. The current Stock shell remains an
API v3 top-level experience rather than a packaged v4 graph root. External
provider side effects are revision-authorized, but provider-wide idempotency or
compensation across a host crash remains future work.

## 2026-08-27: Accept composition and stable-host behavior in the Linux VM

**Goal:** Close the non-physical Linux loop for appearance, live composition,
the permanent host, compositor presentation, resident authoring, and packaged
boot recovery. Keep physical claims separate and leave the Framework target in
its existing development-live classification.

**Changed:** Added `tools/linux-compositor/verify-composition-nested`, which
installs the reference Agenda, Media, Dashboard, and Remix packages into a
disposable store and exercises a real graph supervisor, Linux host, and nested
Smithay compositor. It asserts Dashboard and child semantics, namespaced child
input and events, appearance generation, custom-child styling, unchanged host
PID across graph activation, exact graph recovery after host death, and three
compositor submit fences. Raw evidence preservation now fails the gate if its
destination is not new and writable.

The Linux host now creates `shell_overlay` and `application_surface` auxiliary
windows only while the active Scene contains those nodes. Reconciliation is
deferred until the current GPUI entity update completes. Revision handoff and
asynchronous model-refresh completion also re-merge the newest host-owned
agent, shell, and appearance channels, preventing an older worker snapshot from
erasing a resident-agent completion or appearance generation.

The direct compositor now distinguishes DRM access loss during a seat
transition from fatal rendering errors. Smithay `DeviceInactive` and
permission-denied frame errors pause submissions until libseat activation;
other errors still stop the compositor. The boot verifier records and restores
the selected memory-sleep mode, explicitly selects `s2idle`, supports focused
agent and lifecycle stops, and prints agent/service diagnostics on a semantic
completion failure. Nested and direct verifiers now check the current
registration/mapping log split, and the nested pointer probe targets trusted
shell chrome rather than an application surface. The session verifier waits
for a responsive supervisor socket and terminates the exact session PID on a
failed graceful stop. `linux-live-deploy` cleanup now remains safe under
`set -u` when the first SSH connection fails.

Reference composition semantics now identify the Dashboard root, Agenda
appearance generation, and Media title explicitly, making the cross-experience
assertions independent of incidental text.

**Evidence:** Evidence is under
`artifacts/linux-composition-acceptance-20260826/` (generated, not committed).
The composition gate passed in 1.544 seconds with graph
`f09068511e1c9d2c160fcc55583e9d347024fbf4a6ca2fa53ff2492a983ab287`,
activation PID 13140, recovered PID 13305, appearance generation 1, child event
`agenda.open`, and `nested_backend_submit`. The focused auxiliary-window gate
passed in 11.848 seconds (PID 6989, 152 suppressed compositor events), and the
complete nested gate passed in 10.888 seconds with activation PID 11324,
recovered PID 11985, revision `578c1f5a…`, native input/accessibility/IME,
conditional auxiliary surfaces, compatibility coexistence, and three exact
submit fences.

The Debian 13 direct-DRM gate passed in 22.198 seconds on kernel
`6.12.101+deb13-amd64`, activating revision `250b1573…` in PID 14478 and
recovering it in PID 14797 with VBlank-backed `drm_page_flip` evidence. The
focused packaged lifecycle gate passed in 43.902 seconds with the same lifecycle
PID across logind VT pause/resume, `s2idle` freezer suspend/resume, and output
remove/reconnect. The focused resident-agent gate passed in 43.009 seconds with
text-session input, typed `agent.prompt`, exact context/validate/submit tools,
Timeflow activation in the same host, a visible assistant completion, and DRM
evidence.

The final `tools/linux-vm/verify-boot-session` campaign passed in 57.952 seconds.
It reported session 1, lifecycle PIDs 878/1877/2146, host PIDs
1005/1764/1957, two intended systemd restarts, separated service identities,
revision `578c1f5a…`, and `drm_page_flip`. It rebooted back to
`graphical.target`; GDM and seatd were active and the disposable SOS install
tree was absent. The 33 direct-compositor tests and 33 Linux-host tests passed,
including the new live-channel handoff regression. Every acceptance run used
deterministic fixtures or the faux Pi provider, so external model-weighted cost
was zero.

**Failures and rejected approaches:** The first composition evidence-retention
attempt found an existing destination after the behavior had passed; cleanup
incorrectly preserved status 0, so cleanup failure now overrides success. The
first auxiliary-window implementation mutated GPUI windows reentrantly and was
replaced by deferred reconciliation. Early broad nested reruns clicked a valid
application instead of shell chrome and expected coordinates on the later map
log rather than the registration log; the probes were made explicit. The first
boot lifecycle assertion counted only `s2idle` while the guest selected `deep`;
after the verifier began selecting the intended mode, an immediate VT switch
exposed the real pre-pause DRM `EACCES` race. Treating every rendering error as
transient was rejected in favor of the two typed seat-transition errors.

Two agent reruns then activated Timeflow but lost the final assistant message.
Preserving live channels only at candidate commit was insufficient: an older
in-flight model-render result could still overwrite them. Re-merging at both
commit boundaries fixed the race, after which the focused and complete gates
passed. The physical redeploy attempt did not reach the target: ping failed,
SSH returned `No route to host`, and unauthenticated PiKVM status returned HTTP
401. No physical result is inferred from those failures.

**Decision:** Accept the composition and Linux stable-host milestones at
virtual-device scope. The reference graph has real host/compositor evidence,
and the packaged direct session has a complete cold-boot, resident-agent,
lifecycle, recovery, and restoration pass. Keep the physical result open: the
previous Framework deployment is a mutable, dirty `development-live`
diagnostic build, and the final host/compositor fixes were not deployed after
the target became unreachable.

**Remaining risks / next gate:** Power on the Framework 12, redeploy the final
`compositor` and `experience-host` artifacts, then run the physical hardware
gate from the fallback desktop and select SOS at GDM. Collect visible panel,
pointer/touch, suspend, hotplug, GPU, latency, thermal, clean logout, and
fallback evidence through an authenticated PiKVM session or an owner at the
machine. A development-live run can be only `DIAGNOSTIC_PASS`; installed-product
promotion still requires a clean revision-matched image. Android graph-host
integration remains separate.

## 2026-08-27: Prepare the Framework 12 composition diagnostic

**Goal:** Deploy the accepted Linux host/compositor fixes to the physical
Framework Laptop 12 and prepare the same-boot development-live hardware gate
without promoting mutable live evidence to an installed-product result.

**Changed:** Deployed the release `sos-compositor` and
`sos-experience-host` built from Git object `dcc9e2fc7ab9…`, then redeployed
those binaries together with the corrected `linux-hardware-gate`. The final
deployment is `20260826T224212Z-dcc9e2fc7ab9-2314801`, records a dirty source
tree, and remains `promotion_eligible=false`. The offline agent configuration
had drifted to `default.luau`; its mode-0600 original was preserved as
`config.env.pre-composition-gate`, and the configured source was restored to
the installed `daily-flow.luau` whose SHA-256
`09ccddca90f6d0a94ea8fbbb86204bbf8522123d2f73f21becfe964a2851a693`
matches the baked install manifest.

The physical gate's output-config validator was stale relative to the direct
compositor. It now accepts the documented `layout` and `input_outputs` fields,
keeps the closed key set, permits only mirror/extend, limits mappings to 32,
and applies the compositor's nonempty printable 128-byte bounds to both device
and connector names. A source guard makes that validator directly testable
without running a hardware campaign.

**Evidence:** The target reported Fedora 44, kernel
`6.19.10-300.fc44.x86_64`, bare metal, active GDM, and product `Laptop 12 (13th
Gen Intel Core)`. Focused validator tests passed `{}`, the target's four-device
mirror mapping, and rejection cases for an unknown layout, empty names, 33
mappings, a 129-byte name, and a control character. `bash -n` and
`git diff --check` passed. The final three-component development deployment
passed in 18.647 seconds; its local evidence is under
`artifacts/linux-live-deploy/20260826T224212Z-dcc9e2fc7ab9-2314801/`.

Hardware preparation then passed at
`/home/liveuser/framework12-composition-20260827` with exact revision
`dcc9e2fc7ab90d919afa63a9a1291a565717d505`, offline agent mode, and
`boot_kind=development-live`. Preparation captured the current same-boot
journal cursor and hardware/install identity before any SOS login.

**Failures and decision:** The first preparation stopped before evidence
creation because the offline source setting named `default.luau`. The second
stopped after nine preflight files because the gate rejected the compositor's
valid mirror/input mapping. That partial directory has no `prepared` marker and
was preserved as
`/home/liveuser/framework12-composition-20260827-failed-output-config`.
Changing the target configuration to fit a stale gate or bypassing validation
was rejected; the gate now checks the actual bounded compositor schema.

**Physical run and evidence:** A final clean campaign on the same boot started
at monotonic 69,977,359,677,901 ns and collected after 735,685,634,926 ns. The
recovery view reached a DP-1 DRM page flip, revision `e2af4edc…` reached both
panel outputs, and the semantic Stock composer submitted "Compose a calmer
daily flow" through `agent.prompt`. Context, validation, and submission tools
activated revision `6b3341ee…` at a DP-1 page flip. Supervisor host-proxy PID
534023 stayed constant, the journal recorded one experience-host launch at PID
534029, and the durable authority agreed with `6b3341ee…`. The session exited
through `Ctrl+Alt+Backspace`, GDM returned, and GNOME session 246 started on
`tty4`.

The target's original auditor printed `DIAGNOSTIC_PASS` for all criteria with
two presentations, two revision IDs, and one host launch. Its finalized 38-file
nested manifest is 3,595 bytes with SHA-256
`dad5cb62ab857ae76a2fc691f09e3954d9ffc4cbdb2feff1273ddae8bca18eff`;
target and controller verification both passed. The raw target evidence and
controller records are under
`artifacts/linux-framework12-composition-20260827/`.
The finalized top-level manifest lists 128 files, is 13,972 bytes, has SHA-256
`99731b64bbfb386a8184f32dfe34d6e01bbf397509a8a2f4445761ad8e86964d`,
and passed independent verification after every controller record was final.

**Failures, correction, and decision:** The first collection correctly failed
because SOS had never been entered. Its evidence was preserved as the
skipped-session attempt. Independent copying then found that manifest order
depended on locale: the Fedora target accepted `authority.json` before
`authority-revision.txt`, while bytewise controller verification rejected it.
Generation and verification now force `LC_ALL=C`; the host test generates under
`en_US.UTF-8`, verifies under `C.UTF-8`, and checks bytewise order.

Remote GDM password input reached PAM but failed authentication, so that helper
was retired. A bounded autologin attempt initially selected GNOME because GDM
reads AccountsService `Session`, not `XSession`; the exact GDM config was
restored and the uncollected attempt was archived. Setting `Session=sos` fixed
the focused reproduction. A later semantic check also needed to wait for the
post-`set_value` snapshot before submission. These attempts did not enter the
final journal cursor.

The final campaign did contain remote uinput. Journal markers for relative
pointer, pointer button, and touch followed devices named `SOS Remote
Diagnostic ...`, and the touch was explicitly reported as ambiguously routed.
The old auditor had no provenance check and therefore mislabeled those classes
as physical input. It now compares every session-added input device with the
libinput inventory captured at preparation. The corrected controller audit
fails this campaign with `input_device_inventory unexpected_devices=4` and
`DIAGNOSTIC_FAIL`. Keep the DRM, one-host lifecycle, authoring, durable-state,
and reversible-session results as development diagnostics, but reject the
physical-input claim. Supervisor status also recorded `active_graph: null`, so
this run does not close physical composition even though it exercised the final
graph-capable host and compositor binaries.

The stricter gate, compositor, and host were left installed together as
development deployment `20260826T231654Z-dcc9e2fc7ab9-2319572`, which passed in
24.700 seconds and remains dirty and promotion-ineligible. Temporary uinput,
semantic-client, and autologin files were removed after exact GDM-config
restoration. The final target state is the writable live overlay with active
GDM, GNOME on `tty4`, no SOS process, `Session=gnome`, empty `XSession`, and no
mounted internal NVMe partition.

**Next gate:** Run a fresh campaign with an owner at the Framework for the
integrated keyboard, touchpad, and touchscreen, without hot-added input devices.
Separately add a selectable-session graph root and physically boot the Dashboard
with its Agenda and Media mounts; require child-event, appearance, graph
recovery, and DRM evidence. Development-live can still produce only a
non-promotion diagnostic. An installed-product result requires a clean,
revision-matched immutable image.

## 2026-08-27: Remove the legacy Daily Flow experience

**Goal and cause:** Remove the old alternate experience and its instrumented
agent panel from every active product path. The August 26 change had made Stock
the default offline source, but the physical hardware gate still required the
old developer fixture. During the August 27 campaign that stale assertion was
mistaken for target drift, so the target config was changed back and a faux
prompt activated the obsolete experience. This was a gate and packaging bug,
not experience composition.

**Changed:** Deleted both legacy Luau sources. Linux now packages Stock and
Timeflow, uses Timeflow only as the resident agent's secondary prompt example,
and requires Stock as the deterministic offline source. The compositor's
native-input and accessibility checks now activate a test-only stateful fixture
with no agent UI. VM, authoring, login-session, live-image, faux-agent, Android
stress, README, and current design documentation references now use Stock,
Timeflow, the Android spatial candidate, or that test fixture. The installer
reconciles its previous manifest and removes packaged experience files that no
longer appear in the new manifest. Development-live deployment can now update
the agent-login helper, Timeflow, and both installed operator documents along
with the existing session components.

**Host evidence:** `rg` found no case-insensitive legacy name outside this
historical ledger and ignored generated artifacts. `cargo fmt --all`,
`git diff --check`, shell syntax checks, and validation of
`tests/fixtures/linux-stateful-experience.luau` passed; the fixture compiled to
16 nodes with one input, one image, one animation, and six semantic nodes.
`cargo test -p sos-experience --lib` passed 16 tests, and the focused
`sos-linux-session` authoring test passed. The login-session, hardware-gate,
and live-image host tests all reported `PASS`. `tools/linux-agent-e2e` traversed
context, validation, and submission, then changed revision
`82f5ddab…` to Timeflow revision `d5db162f…`. No model request ran, so model and
model-weighted cost were zero. The complete nested compositor command stopped
before setup because `weston` is not installed on this workstation; no nested
result is claimed. Crate-scoped
`cargo clippy -p sos-experience --lib --no-deps -- -D warnings` passed. The
dependency-wide form stopped on pre-existing
`derivable_impls` and `too_many_arguments` lints in `service-protocol` and
`experience-ir`; those unrelated composition changes were left untouched.

**Physical cleanup:** The Framework was reachable but an SOS session was still
active with the old source as both current revision and faux-provider input.
The running supervisor activated installed Stock revision `82f5ddab…` in the
same host PID 551202. The private mode-0600 offline config was rewritten to
`/usr/share/sos/experiences/default.luau`, then the session shut down cleanly.
The old installed source was 15,676 bytes with SHA-256
`09ccddca90f6d0a94ea8fbbb86204bbf8522123d2f73f21becfe964a2851a693`.
It, five inactive revisions containing the old root, and the eight-message
agent transcript containing a complete copy of that source were removed. The
final target scan found no old source or path under the executable directory,
installed experiences and current operator docs, or mutable user state. GDM
was active, no SOS process remained, and no internal NVMe partition was
mounted.

Final same-boot deployment
`20260826T234429Z-dcc9e2fc7ab9-2333753` updated six root-owned files in
7,520,181,781 ns and remains dirty and promotion-ineligible. Its ignored
evidence directory is
`artifacts/linux-live-deploy/20260826T234429Z-dcc9e2fc7ab9-2333753/`.
The 661-byte deployment manifest has SHA-256
`29619bdd9a594df5bc4b3985e6e49d1be568fb1e96f9ba9163326b9a61b72ebb`;
the 278-byte metadata has SHA-256
`c127adb6d120a1f2929a6ca4e2d0cd73ff9731ea0fdc8bd5b974a7cb3dc185c4`;
the 149-byte result has SHA-256
`392372eb35419e6d2bc4420eb4d4e6639779f678cc3b3db77f5fbf1b61e9ae8f`.

**Failures and decision:** Direct removal of immutable revision directories
first prompted and then failed because their contents were read-only. The
attempt was stopped, the five exact inactive directories were checked against
the current Stock revision, made owner-writable, and removed. Accept the source,
runtime, packaging, tests, and current-boot cleanup. Do not call the existing
development ISO clean: its immutable install manifest truthfully records the
old baked file, and a reboot discards the mutable overlay changes.

**Next gate:** Build a clean revision-matched development image, verify its
installed manifest and root filesystem contain only Stock and Timeflow, boot it
on the Framework, and repeat the Stock no-op faux prompt plus the physical
composition and input gates. Run the complete nested compositor regression on
a host with Weston before that image gate.

## 2026-08-27: Repair the GDM login bounce after revision cleanup

**Goal and cause:** Restore the physical SOS login after the Daily Flow cleanup
made GDM return immediately to its login page. Two attempts failed before the
provider socket existed, at journal monotonic times 72,911 and 72,915 seconds,
with `linux_session_failed` reporting `No such file or directory`. GDM and the
compositor were downstream of the failure. The cleanup had removed revision
`579f946f…` but left `previous` pointing to that absent directory. The session
runner reads both `current` and optional `previous` when it creates recovery
status, so the dangling rollback pointer aborted startup even though `current`
still named valid Stock revision `82f5ddab…`.

**Changed:** Removed only the invalid `previous` pointer on the live target and
kept the current Stock pointer unchanged. `RevisionStore::previous` now treats
a pointer whose revision directory no longer exists as unavailable. It still
validates the pointer shape and fully verifies any revision that does exist, so
a malformed pointer or corrupt rollback revision is not silently accepted. A
regression test removes the first of two installed revisions, asserts that
`previous()` returns `None`, and asserts that the second current revision still
verifies.

**Host evidence:** `cargo test -p revision-supervisor` passed 7 unit, 10
coordinator, 3 graph-supervisor, and 16 supervisor tests, including the new
dangling-pointer case. The five focused `sos-linux-session` system-session
tests passed. `cargo clippy -p revision-supervisor -p sos-linux-session
--all-targets --no-deps -- -D warnings`, `cargo fmt --all`, and
`git diff --check` passed. The first version of the regression fixture could
not delete the mode-0555 immutable revision as an ordinary user; the test now
models the privileged cleanup explicitly by making that exact fixture
directory owner-writable before removal.

**Physical evidence:** Development-live deployment
`20260826T235443Z-dcc9e2fc7ab9-2341916` installed the hardened session binary
in 11,385,200,513 ns. The installed 1,888,336-byte binary has SHA-256
`a32ef434b02eaa5fed52b2d68c578f219f94d65e218d65fcefa633a2bdb4e88a`.
The ignored evidence directory is
`artifacts/linux-live-deploy/20260826T235443Z-dcc9e2fc7ab9-2341916/`: its
114-byte manifest has SHA-256
`aeebf8d422a9df522f12c8e24c00aeda43a714f42fadf90d13abdf6fd5482170`,
its 278-byte metadata has SHA-256
`1f4b8eec40ff2f0ad471ad4ca589f878bdc8d31afce630693227bb367db2ec50`,
and its 83-byte result has SHA-256
`d19cd9f031bdba8b91759ef3a1a3ce9b9afc4938ae096c9b49fa8cccb0e4d425`.

A bounded GDM autologin used the already selected SOS session. PAM opened
physical session 258 at monotonic 73,343.919 seconds. Stock produced real
non-recovery DRM page flips on `eDP-1` and `DP-1` at 73,347.310 and 73,347.326
seconds, and `linux_system_session_ready` followed at 73,347.349 seconds: 3.430
seconds after session open. The supervisor, durable authority, and rendered
frame all agreed on Stock revision `82f5ddab…` and source SHA-256 `9f09372f…`.
The provider continued publishing through 73,387.318 seconds with the complete
session process set alive. The exact pre-test GDM config was then restored with
SHA-256 `87d6cc7eecc23565f361c46581b0fecf219eeef3791f088ea6e62943a1e66e36`;
GDM returned active, the temporary backup was absent, no SOS process remained,
and neither internal NVMe partition was mounted.

**Decision and next gate:** The current-boot GDM bounce is fixed and the
physical login-to-Stock path passes. This does not promote the dirty mutable
deployment or replace the previously required physical input and composition
campaigns. Bake the guard and Daily Flow removal into a clean image, reboot the
Framework from that image, and repeat manual SOS login plus the remaining
physical gates.

## 2026-08-27: Begin the complete v4 built-in and graph-activation migration

**Goal:** Resume the frozen experience-composition plan from its actual
implementation state, remove v3 as an authoring and built-in target, and close
the wire, migration, and atomic graph-state gaps before the remaining Linux and
Android gates.

**Changed:** The shared `experience-package` crate now defines opaque Instance
IDs, 256 KiB canonical package and graph wire limits, and strict canonical
decoders. One checked-in fixture covers a complete package, contract,
dependency binding, derivation, appearance profile, graph, instance identity,
and every frozen numeric limit. Rust, the Linux adapter, the Android authority
adapter, and the TypeScript resident-agent decoder consume that same fixture;
unknown fields, non-canonical bytes, and oversized payloads are rejected. The
canonical form is RFC 8785 JCS, including ECMAScript number formatting and
UTF-16 property ordering rather than a Rust-specific approximation.

Stock Shell and Timeflow now declare Experience API v4 exports and immutable
v4 packages. Stock loads its one revision-local `stock.theme` module instead
of duplicating that palette in the bootstrap source and maps authority-owned
semantic color tokens onto revision-local fallbacks at render time. The Linux
installer, development deploy tool, image checks, and selectable-session runner
carry both packages and the Stock theme module. A fresh session installs and
boots the reserved `sos.stock.shell` graph and registers `sos.timeflow`.

For an existing v3 store, `migrate-stock-v4` copies the verified durable state
into the v4 package, creates the Stock registry and graph records, and leaves
the old single `current` pointer unchanged. Once the provider starts, the
session seeds exact graph state through an explicit activation-mode graph
transaction and boots the supervisor with `--root-experience
sos.stock.shell`. Runtime graph state updates retain their prior exact-revision
semantics; only graph activation changes the stable per-experience current
state. The activation journal now places authority commit before registry and
graph pointer commits and records an authority transaction. A crash before
that commit aborts and returns to the old graph; a crash after it completes the
new graph. Resolved graphs also reject one Experience ID bound to multiple
revisions.

**Evidence:** `cargo test -p experience-package --test wire_model` passed three
wire tests. The shared Linux and Android wire fixture tests each passed, and
`npm --prefix services/sos-agent test` passed 19 tests. Focused package
validation rendered all ten Stock scenarios (43–125 nodes) and the 70-node
Timeflow scenario successfully. `cargo test -p provider-state-service -p
sos-linux-session -p revision-supervisor` passed the authority, registry,
resolver, coordinator, supervisor, authoring, and session suites. A new fault
test killed activation immediately after authority commit and recovered the
candidate state, registry pointer, and graph pointer together. A migration
test preserved `{"count":7}` under the v4 revision while proving the legacy
pointer still named the original v3 revision. `tests/linux-login-session-test.sh`
and `tests/linux-live-image-test.sh` passed, as did Rust formatting, focused
compilation, and the TypeScript build.

**Failures and decision:** The first authority change treated every graph
state batch as activation and broke the locked-revision test with `expected 1,
current 2`. The protocol now distinguishes activation batches from ordinary
state-update batches, preserving locked historical state while allowing an
activation to advance stable Experience state. A first package-install smoke
command omitted Cargo's binary selector; rerunning against the explicit
`sos-revision-supervisor` binary installed Stock revision `62374642…`,
Timeflow revision `1c3a0ed8…`, and Stock graph `ba056e2b…` successfully.

**Remaining risks and next gate:** This checkpoint has host evidence only and
does not close a hardware milestone. Finish persistent reverse-dependency and
multi-root tracked activation, convert ordinary authoring to v4 graph
activation, complete Instance-ID containment and appearance-write grants,
migrate Android and all current fixtures, then run full fault, fuzz,
performance, Linux physical, and Android physical campaigns. Retain the
legacy pointer and v3 activation reader until the migrated Stock graph has
booted, presented, restarted, and rolled back on the Framework.

## 2026-08-27: Move ordinary authoring and tracked updates onto v4 graphs

**Goal:** Remove the remaining new-authoring dependency on the v3 singleton
revision protocol and implement the tracked-child update path against stable
Experience identities.

**Changed:** Linux authoring context now names the active Stock Experience,
graph, and package. Validation accepts only API v4 candidates with the exact
Stock export contract, reads revision-exact authority state, validates every
scenario at the export's minimum and maximum viewports plus high-contrast
appearance, and requires the functional agent composer in agent workspaces.
Submission installs the immutable package, resolves a content-addressed graph,
and uses graph activation; it no longer stages or activates a singleton v3
revision. The resident-agent examples, authoring fixtures, Linux state/input/
failure fixtures, deploy helper, and curated generation guide now emit API v4.

The supervisor persists a canonical reverse-dependency index derived from
current package records and rebuilds it after registry changes and recovery.
The resolver can validate a candidate revision through an in-memory tracked
binding override without first changing any durable pointer. The new
`advance-experience` control operation leaves locked graphs pinned, but for an
affected tracked root it prepares the complete candidate graph, presents it,
commits authority state, then journals the child registry pointer and graph
pointer as one activation. Graph restart is now an explicit supported control
operation as well as automatic crash recovery.

**Evidence:** `cargo test -p sos-linux-session --lib` passed 14 authoring and
session tests. `npm --prefix services/sos-agent test` passed 19 tests. The
`tools/linux-agent-e2e` campaign started at Stock revision `62374642…`,
validated the v4 candidate across the full scenario and viewport matrix, and
activated revision `1767e067…` through the graph protocol. `cargo test -p
revision-supervisor` passed 39 tests, including locked child pinning, tracked
child graph advancement, exact restart, reverse-index persistence, activation
fault recovery, and existing legacy compatibility. `cargo check -p
revision-supervisor --all-targets` and Rust formatting passed.

**Failures and decision:** The first tracked test changed the child registry
before graph validation, which recreated the non-atomic ordering the feature
is meant to eliminate. The final flow keeps the current pointer untouched,
resolves with an ephemeral candidate override, and lets the activation journal
perform the durable switch only after presentation and authority commit.

**Remaining risks and next gate:** One supervisor currently owns one presented
root, so an update affecting multiple independently presented tracked roots is
still rejected rather than partially committed. Generalize the activation
unit across every affected root, then close Instance-ID namespaces, recovery
actions, authority appearance grants, and the remaining Android parity work.

## 2026-08-27: Enforce Linux Instance boundaries and graph-native recovery

**Goal:** Replace stable graph/revision identities at live isolation
boundaries, protect appearance mutation, and ensure Recovery operates on the
v4 graph that the session actually presents.

**Changed:** Each graph VM now receives a fresh opaque Instance ID when its
runtime starts; a second instantiation of the same content-addressed graph gets
different IDs. Runtime snapshots and provider effects carry that identity.
Linux namespaces rendered element and accessibility IDs, text/IME state,
pointer and hit-region capture, revision image/font/shader paths, provider
surfaces, provider frames, and provider-effect contexts by Instance ID. Stale
input, focus, gestures, and per-instance provider contexts are discarded or
cancelled when an instance leaves the active graph. Pending input is bounded
globally and per instance, child coordinates remain mount-local, and the graph
runtime enforces the frozen 8,192 aggregate scene-node limit.

The system-session Recovery status and rollback action now use the reserved
Stock graph's current and previous records in graph mode. An explicit restart
recreates the exact active graph host. Appearance writes now require a
separately provisioned `appearance-write` capability: the authority persists
only its SHA-256 digest, rejects socket clients without the exact capability,
and retains exact generation checks. The Linux session copies an optional
0600 capability file into the provider identity's private runtime credential
before starting the authority. The nested composition gate supplies that
credential explicitly.

**Evidence:** `cargo test -p runtime-luau` passed 31 tests, including fresh
instance identity, independent VM/state behavior, provider/asset namespacing,
scene authority, and graph rollback. `cargo test -p provider-state-service`
passed 14 tests, including denial before the appearance grant and persistence
after an authorized update. `cargo test -p sos-linux-session --lib` passed 15
tests, including a v4 graph-history Recovery test. `cargo test -p
sos-experience --features linux-host --lib` passed 34 tests, including
Instance-scoped accessibility/text state and pointer hit regions. Focused
all-target compilation, Rust formatting, shell syntax validation, and
`git diff --check` passed.

**Failures and decision:** The first embedded Stock test compiled the new
`require("stock.theme")` source without its revision-local module. The helper
now compiles checked-in Stock with the same module sidecar used by packaging.
The local nested Weston campaign could not start because this development host
does not have the `weston` executable; it failed before creating product
evidence and therefore closes no presentation gate.

**Remaining risks and next gate:** Instance namespaces are implemented on the
Linux host, but the multi-root tracked activation transaction and equivalent
Android host routing remain open. Run the updated nested composition campaign
on the Linux target with Weston available, then extend graph activation across
all affected presented roots before starting Android parity.

## 2026-08-27: Make tracked graph activation atomic across top-level roots

**Goal:** Close the remaining single-root supervisor assumption so one tracked
Experience update cannot leave independently launched consumers on different
child revisions.

**Changed:** The graph supervisor now owns a host and active graph per presented
root. A tracked update resolves every affected current graph against the exact
candidate revision, prepares all live hosts, stages the union of authority
state promotions once, and records all registry and graph-pointer changes in a
single durable activation journal. Inactive roots with current graph pointers
advance in the same transaction without creating phantom hosts. Recovery rolls
the entire set backward before authority commit or forward after it. Conflicting
revision bindings for the same Experience are rejected before staging.

Top-level presentation is now registry-addressed through `present-experience`
and `dismiss-experience`; the configured Stock root remains pinned. Each root
still has an independent host process, while each graph node retains its own
Luau VM and Instance ID. The supervisor enforces the frozen limit of eight live
instances across all simultaneously presented graphs and reports whether each
advanced root was live or inactive.

**Evidence:** `cargo test -p revision-supervisor --all-targets` passed 44 tests.
The focused graph suite passed 10 cases covering two live tracked roots, a live
plus inactive root, one authority transaction shared by two roots, rollback of
both pointers after an injected post-presentation fault, locked pinning, exact
restart, and rejection of a ninth aggregate instance. Rust formatting and
all-target compilation passed.

**Failures and decision:** Treating only roots owned by the running supervisor
as affected would have silently left a registered inactive graph stale. The
final transaction uses every current graph pointer from the reverse-dependency
index, but sends prepare, quiesce, present, and finalize only to roots with a
live host. Recovery no longer inserts inactive roots into the in-memory live
host map.

**Remaining risks and next gate:** The Linux control and transaction path is
complete at the supervisor layer, but compositor presentation evidence still
needs the physical Linux campaign. Audit and finish fork/remix authoring and
built-in v4 conversion next, then implement the same registry, graph, state,
appearance, and boundary behavior in the Android authority and host.

## 2026-08-27: Bind v4 authoring state and grants to explicit authority records

**Goal:** Remove the remaining implicit state/grant assumptions from v4
authoring and make provider access follow the frozen stable-Experience
authority model.

**Changed:** Package format v4 now carries an immutable state-migration record
with a fresh or exact Experience/revision source, source schema and state hash,
target schema, and result state hash. Derived and composed authoring requests
must choose that state source explicitly and declare bounded provider
capability requests. Exact revision migration reads the authority's retained
state; the revision store rejects a package whose migration result does not
match the installed durable state.

Authority format 3 adds capability-protected, generation-checked grant
decisions keyed by stable Experience ID. Decisions cover provider capabilities
and exact child Experience/export property and event flows. The graph
supervisor rejects unreviewed or overreaching graphs before preparation. In v4
graph mode the Linux host reads authority grants and intersects them with the
running revision's immutable requests; the legacy grant file remains only on
the singleton v3 rollback path. Trusted Stock and Timeflow packages receive
native bootstrap review, while agent-authored candidates report that review is
required and cannot activate until a native caller uses the private review
capability. Existing supersets are preserved and reused across revisions of
the same Experience.

**Evidence:** `cargo check --workspace --all-targets` passed. `cargo test -p
experience-package` passed 10 tests, `cargo test -p provider-state-service`
passed 15, `cargo test -p revision-supervisor` passed 47 after adding migration
digest rejection and stable-grant/revision coverage, `cargo test -p
sos-linux-session --lib` passed 15, `npm --prefix services/sos-agent test`
passed 19, and `cargo test -p sos-experience --features linux-host --lib --
--test-threads=1` passed 34. Rust formatting and
`bash -n packaging/libexec/sos-login-session` passed.

**Failures and decision:** The first grant draft keyed reviews by immutable
revision, conflicting with the frozen ownership rule. Moving the record to the
stable Experience ID exposed a second issue: returning the complete stable
superset to an older revision would exceed that revision's declaration. The
host now intersects both sets. Trusted review is idempotent and unions an
existing superset instead of silently revoking it.

**Remaining risks and next gate:** This closes explicit migration provenance
and Linux grant enforcement at the desktop-test boundary. Finish Stock's
top-level launch migration and optional worker-process isolation, then port the
registry, graph protocol, authority resources, and Instance-scoped routing to
Android before either physical acceptance campaign.

## 2026-08-27: Launch independent v4 Experiences from the Stock registry

**Goal:** Remove Stock's same-revision application subtree and make an
ordinary SOS application a separately identified, supervised, and
compositor-contained v4 Experience.

**Changed:** Graph boot and preparation now carry a bounded registry catalog
of ordinary-role Experiences. The typed Shell model exposes their stable IDs
and labels, and Stock emits closed `present_experience` lifecycle effects from
its command center and Applications workspace. The Linux graph host accepts
those effects only from a registry-authorized Shell package. The supervisor
then resolves and boots the target's exact current graph in its own permanent
host process; dismissal is restricted to the Shell or the ordinary Experience
itself. Eight live graph instances remain the global runtime limit, while the
non-live catalog is separately bounded to 64 entries.

The compositor control protocol now distinguishes authenticated Shell and
`NativeApplication` registrations. An ordinary graph host proves its own PID
with the existing peer-credential/token handshake but receives no quiesce,
presentation-fence, window-space, overlay, or window-control authority. Its XDG
toplevel participates in native application placement and containment. Closing
that window requests supervised dismissal instead of triggering crash restart,
and disconnecting an ordinary host no longer releases a Shell-owned input
quiesce. Checked-in Stock paints its workspace inside the shell and contains no
`application_surface`; that node remains only in the API v3 rollback reader.

**Evidence:** `cargo test -p compositor-control-protocol` passed 2 tests;
`cargo test -p sos-compositor` passed 27, including distinct authenticated
native application classification; `cargo test -p revision-supervisor --test
graph_supervisor` passed 14, including registry launch into an independent
host; `cargo test -p runtime-luau` passed 31, including rejection of the legacy
application primitive from every v4 graph role; and `cargo test -p
sos-experience --features linux-host --lib --
--test-threads=1` passed 35, including the native registration handshake and
Stock's typed stable-ID launch effect. `cargo check --workspace --all-targets`,
Rust formatting, `git diff --check`, and an explicit check that
`experiences/default.luau` contains no `application_surface` passed.

**Failures and decision:** The first Stock assertion looked for the registry
action on the Home workspace, although the product intentionally exposes it in
Applications and the command center; the corrected test opens the command
center before asserting the action and separately checks its typed effect. The
standalone `sosctl typecheck/validate experiences/default.luau` route is not
accepted as evidence because that utility still typechecks the entry source
without its revision-local `stock.theme` module. Package-aware Rust compilation
and validation use the real sidecar and pass.

**Remaining risks and next gate:** Independently presented v4 roots are now
separate host processes, while mounted graph children still share a process
with one Luau VM per Instance. Add the optional graph worker-process deployment
without changing the VM API, then close equivalent Android graph, state,
appearance, input, IME, accessibility, grant, activation, and recovery behavior
before the two physical acceptance campaigns.

## 2026-08-27: Isolate the v4 graph runtime in an optional worker process

**Goal:** Add the milestone 12 process boundary without making Experience code
or graph contracts depend on a Linux deployment detail.

**Changed:** `GraphRuntimeWorker` now has thread and process deployments behind
the same typed Rust API. Process mode re-executes `sos-experience-host` through
a private worker entry point and exchanges only length-prefixed, closed serde
messages. The 384 MiB frame cap covers the frozen eight-instance aggregate
asset and scene limits. Binary assets use base64 inside the private JSON frame
instead of unbounded integer arrays. Scene IR now has an explicit serde form so
snapshots can cross the process boundary without converting them to general
JSON values.

The worker reports readiness before the host can present the graph. Commands
remain request-ID matched, shutdown closes and reaps the child, and a broken
pipe returns a rejected operation or closes the results channel. The selectable
login session and direct system service choose `process` by default.
`SOS_GRAPH_RUNTIME_ISOLATION=thread` keeps the existing deployment for focused
debugging. Instance IDs, VM count, state ownership, grants, properties, events,
and scene limits are identical in both modes.

**Evidence:** `cargo test -p runtime-luau` passed 32 tests, including bounded
frame round trips. `cargo test -j 1 -p sos-experience --features linux-host
--test graph_process_isolation -- --nocapture` passed. That test compiled a v4
graph with a binary sidecar, rendered it in a different PID, completed a state
update, sent `SIGKILL` to the worker, and observed containment in the still-live
parent. `bash -n packaging/libexec/sos-login-session` passed.

**Failures and decision:** The first integration link filled the development
filesystem with regenerable Rust output and failed with `ENOSPC`; the linker
also exited with a bus error during that attempt. `cargo clean -p
sos-experience` removed 9.8 GiB of package build output. The identical
single-job test then passed. This is not product evidence, but it explains why
the test uses `-j 1` on this host.

**Remaining risks and next gate:** This process boundary isolates the complete
graph runtime from GPUI and compositor code. It does not assign one OS process
per mounted Instance, which the frozen deployment rule does not require. Each
Instance still owns a distinct Luau VM and no API observes process placement.
Android v4 parity is now the next implementation gate.

## 2026-08-27: Move the Android authority and host onto the v4 graph boundary

**Goal:** Replace Android's singleton v3 runtime ownership with the same stable
Experience, resolved graph, per-Instance runtime, durable state, appearance,
grant, and recovery boundaries used by Linux.

**Changed:** The Android authority now imports an existing singleton install as
the reserved Stock Shell Experience without moving or rewriting the legacy
pointer or state file. A packaged v4 Stock revision is installed beside it,
resolved into a content-addressed graph, and returned as a pending migration.
Only a host presentation confirmation moves the registry and graph pointers.
Rejecting that pending graph removes it and returns the untouched v3 artifact;
the fallback marker survives another authority restart. Fresh v4 installs use
the registry and graph store directly.

Android graph responses carry exact immutable package metadata, sources,
sidecars, per-Experience state resources, reviewed grants, and the
authority-owned appearance generation. Graph actions are checked against the
active graph identity, stable grant decision, immutable capability request,
provider action schema, expected state generation, state size, and shared
Experience state invariant. All affected Experience states replace one durable
composition document before provider effects execute. Appearance writes use a
separately provisioned bounded capability and generation compare-and-swap.

The Android GPUI host now starts one Luau VM per resolved graph Instance.
Mounts are clipped and rendered at host-owned bounds. Node IDs, assets, native
text sessions, pointer surfaces and capture, semantic bounds, focus targets,
and input shadow state are namespaced by Instance ID. Events are routed back to
the owning graph node with the namespace removed. Child failure renders a
bounded placeholder. Provider models are filtered by the intersection of the
revision request and reviewed stable grant, and only the registry-selected
role reaches the runtime. Graph state and effects commit through the authority;
failed commits restore the prior in-memory snapshot. Appearance generations
are polled and applied without changing revision identity. A first v4 boot is
confirmed after a rendered frame. A rejected startup graph is rolled back
before the app aborts, preventing a repeated bad-candidate boot loop.

Samsung and Cuttlefish product definitions now install
`default.package.json` and the `stock.theme` sidecar with the Stock source,
start the authority with that complete package, and publish revision and API
format 4. `ro.sos.legacy_revision_read=3` describes the remaining rollback
reader. Staging and Samsung target-files inspection compare both new artifacts
with repository sources.

**Evidence:** `cargo test -j1 -p android-authority-protocol -p
android-system-authority` passed 24 unit, integration, wire-fixture, and doc
tests. The new authority cases cover pending migration across restart,
presentation confirmation, rollback to v3 across restart, per-Experience graph
state persistence, and appearance persistence. `cargo ndk -t arm64-v8a -P 31
check -j1 -p sos-experience --features aosp-system` and the corresponding
`--no-default-features --features core-native` check both passed. `cargo fmt
--all`, `git diff --check`, and `bash -n tools/a33xctl tools/aospctl` passed.

**Failures and decision:** A direct `cargo check --target
aarch64-linux-android` could not locate `aarch64-linux-android-clang`. This was
a host invocation error, not a source or target failure. Re-running through
the repository's cargo-ndk path supplied the pinned NDK compiler and both
Android feature sets passed. The regular source-swap authoring entry point is
explicitly rejected while a v4 graph is active because installing a bare
source would recreate singleton v3 ownership.

**Remaining risks and next gate:** This is compile and authority fault-path
evidence, not an Android hardware verdict. Finish the packaged v4 Android
authoring and staged graph-activation API, add Android-side graph fixture
coverage, build and install the resulting ARM64 product, then run restart,
rollback, IME, accessibility, grant isolation, child-failure, appearance, and
composition acceptance on the Samsung target.

## 2026-08-27: Make Android authoring a staged v4 graph transaction

**Goal:** Remove Android's remaining singleton source-swap authoring path from
the active product and make a generated Stock revision use the same immutable
package, resolved graph, presentation, recovery, and rollback rules as every
other v4 activation.

**Changed:** The Android authority protocol now stages an immutable package
revision against an exact active graph and can discard the staged graph before
presentation. The authority checks the stable Experience ID and registry-owned
role, exact current state hash and schema, package contract, grants, resolved
graph limits, and dependency bindings before returning the candidate graph.
Candidate state stays separate until the host confirms a rendered frame.

Confirmation writes one graph-activation journal before it replaces the
composition state, Experience registry pointer, graph pointer, and legacy
fallback marker. Restart recovery completes that journal from every durable
phase. Whole-graph rollback restores the prior composition state and pointers.
Opening the v4 authority disables bare v3 installation, while the legacy
revision reader and rollback activation remain available for existing recovery
artifacts.

The Android host now compiles generated source with the active revision's
sidecars, requires API v4 and the exact active export set, migrates state with
an explicit exact-parent record, validates all declared scenarios and export
viewports, starts the candidate graph with one VM per Instance, and confirms it
only after GPUI presents a frame. Failed runtime preparation discards the
staged graph. The fake agent now makes a visible edit to the v4 Stock package
instead of replacing the shell with the unrelated Timeflow Experience. Both
agent preflight and host submission require the privileged Stock shell to keep
its `agent_submit` text session. Agent activation evidence advances through
validated, staged, and committed phases on the graph path.

**Evidence:** `cargo test -j1 -p android-authority-protocol -p
android-system-authority` passed 26 tests. The new authoring case covers stage,
discard, presentation, candidate state commit, whole-graph rollback, and v3
authoring rejection. The recovery case interrupted six consecutive journal
phases and proved that every restart selected the complete candidate graph,
removed pending intent, and could roll back to the original graph. `cargo test
-j1 -p sos-experience` passed 18 tests, including a fake-agent Stock edit that
retains API v4, the `main` export, theme module, and agent composer. Both
`cargo ndk -t arm64-v8a -P 31 check -j1 -p sos-experience --features
aosp-system` and `cargo ndk -t arm64-v8a -P 31 check -j1 -p sos-experience
--no-default-features --features core-native` passed. `cargo fmt --all` and
`git diff --check` passed.

**Failures and decision:** The first Stock agent test compiled the modified
source without its `stock.theme` module and failed at `require`. The built-in
compiler now derives sidecar need from the declared module import, matching the
package behavior. Android v4 authoring is accepted as the only creation path.
The v3 code remains a bounded read and rollback compatibility path.

**Remaining risks and next gate:** This is host compilation and authority fault
evidence, not a Samsung hardware verdict. Audit checked-in tools, fixtures, and
recovery scripts for any remaining v3 creation. Then complete the shared
desktop verification campaign and run the Android graph activation, restart,
rollback, IME, accessibility, appearance, grant, containment, and child-failure
campaign on the physical SM-A336B.

## 2026-08-27: Remove the remaining v3 creation paths

**Goal:** Make Experience API v4 the only built-in and authoring target across
repository tools, Linux boot setup, Android source delivery, and checked-in
examples, while retaining the explicit v3 rollback reader.

**Changed:** `android-exit-agent.luau` now publishes a v4 `main` export. The
Android development helper seeds generated work from Stock v4, preserves its
theme module and agent composer, waits for graph presentation, reports source
identity, and invokes live whole-graph rollback. The host accepts the rollback
request through a deep link, starts the previous graph, and reports success
only after GPUI presents a frame. Candidate failures now emit one stable log
record for device automation.

The revision supervisor CLI rejects bare `install`; new callers must provide a
complete v4 package. The direct-DRM boot campaign now installs packaged Stock
and Timeflow revisions, creates both registry graphs, provisions grant review
and exact trusted revision inputs, boots the graph supervisor, uses the
resident authoring broker for replacement, and checks graph-aware status and
rollback. Its systemd unit now passes the trusted product revision IDs from a
root-owned environment file. The type prelude, top-level status, architecture,
runtime, host, supervisor, and composition documents describe v4 as the active
contract and v3 as read-only recovery compatibility.

**Evidence:** `./tools/sosctl validate
experiences/android-exit-agent.luau --json` passed Luau type checking and the
runtime's complete scenario validation at 15,383 source bytes and 22 scene
nodes. `cargo test -j1 -p revision-supervisor --all-targets` passed 48 tests.
`cargo test -j1 -p sos-linux-session --all-targets` passed 18 tests, including
v4 authoring, graph authority, recovery history, and the retained legacy
migration fixture. `cargo test -j1 -p sos-experience` passed 18 tests. The
ARM64 `cargo ndk -t arm64-v8a -P 31 check -j1 -p sos-experience --features
aosp-system` check and the corresponding `--no-default-features --features
core-native` check passed. `tests/linux-login-session-test.sh`, `bash -n
tools/sosctl tools/linux-vm/verify-boot-session
packaging/libexec/sos-login-session`, `cargo fmt --all -- --check`, and `git
diff --check` passed.

**Failures and decision:** The audit found two active leftovers. The direct-DRM
VM still installed and swapped singleton v3 revisions, and the systemd unit did
not supply the trusted revision arguments required by graph mode. It also used
Timeflow as a fake Stock edit, which the v4 shell-composer check correctly
rejects. The rewritten campaign uses a visible Stock variant through the
authoring broker. No built-in, public CLI, or resident authoring flow creates
v3 now. Direct library fixtures still create v3 artifacts solely to prove
migration and rollback decoding.

**Remaining risks and next gate:** The rewritten direct-DRM campaign has passed
syntax and desktop component tests but has not yet rerun inside the Debian VM.
Run that gate before treating its old physical evidence as v4 evidence. Then
close fuzz, fault, performance, and documentation coverage before starting the
Framework and Samsung physical campaigns.

## 2026-08-27: Close v4 desktop boundary, recovery, and measurement coverage

**Goal:** Complete the platform-neutral verification required before rerunning
the Linux and Android physical composition campaigns.

**Changed:** Added deterministic generated-boundary tests without a new fuzzing
runtime dependency. They generate bounded closed schemas and resolved graphs,
check canonical round trips and stable graph identities, reject wrong types and
structural corruption, and mutate the shared package and graph fixture at the
byte boundary. Graph-supervisor fault coverage now interrupts every durable
activation cut point: intent, presented, authority committed, registry
committed, and graph committed. Each restart asserts both the registry and
graph pointer, not only the returned recovery decision.

Added an explicit release-profile desktop measurement for the reference
Agenda, Media, and Dashboard graph. It installs and resolves the immutable
packages, starts all three VMs through mounted-scene readiness, dispatches a
namespaced child event, propagates appearance, activates a new root graph,
recovers a committed journal, and reports Linux process RSS. Android pointer
containment tests now exercise local Router owners instead of racing through
the process-global production router when Rust runs tests concurrently.

**Evidence:** `cargo test -j1 -p experience-package --all-targets --
--nocapture` passed 13 tests. The generated corpus covered 10,000 schemas,
10,000 graphs, and 5,000 mutations of each canonical fixture in 2.74 seconds.
`cargo test -j1 -p revision-supervisor --test graph_supervisor
activation_journal_recovers_an_atomic_graph_at_every_durable_phase --
--nocapture` passed all five injected phases. Core Rust, Linux, Android, and
TypeScript shared-wire tests passed; `npm test` in `services/sos-agent` passed
19 tests.

`cargo test --release -j1 -p revision-supervisor --test composition_metrics --
--ignored --nocapture` measured 1.383 ms for package install and graph
resolution, 1.347 ms from graph start through all three mounted scenes ready,
0.693 ms from child event to composed snapshot, 0.250 ms for appearance to
composed snapshot, 0.809 ms for graph prepare/present/commit, and 0.610 ms for
committed-journal recovery. Process RSS changed from 7,496 to 8,268 KiB, a 772
KiB delta or coarse 257 KiB per Instance. After the Router isolation fix,
`cargo test --workspace --all-targets -j1` passed the complete Rust workspace.
Both ARM64 `cargo ndk` checks for `aosp-system` and `core-native` passed.

**Failures and decision:** The first workspace-wide run exposed two parallel
Android pointer-test failures. Both tests used the singleton production Router,
so an active surface capture and render order from one case could overwrite the
other. Router operations now have owner-local helpers used by the tests; the
production entry points retain one locked Router per host process. The desktop
latencies stop at a complete composed snapshot and are not labeled as physical
frame latency.

**Remaining risks and next gate:** Run the rewritten v4 direct-DRM VM gate and
the Framework stable-host composition campaign. Desktop RSS and snapshot
latencies cannot close compositor input, page-flip, suspend, thermals, or
device-memory gates. The Samsung v4 composition, IME, accessibility, grant,
failure, appearance, restart, and rollback campaign remains required.

## 2026-08-27: Preserve direct-DRM acceptance evidence before cleanup

**Goal:** Make the rewritten v4 Debian direct-session run independently
auditable under the Linux acceptance workflow.

**Changed:** `verify-direct-session` accepts an opt-in absolute
`SOS_DIRECT_EVIDENCE_DIR`. After stopping the disposable session and restoring
GDM, it copies finalized compositor, session, uinput, activation, and status
logs, records the source revision and guest environment, archives the
socket-free revision store, and writes measured monotonic duration and exit
status. The default remains destructive temporary cleanup, and an existing or
relative evidence path is rejected instead of overwritten.

**Evidence:** `bash -n tools/linux-vm/verify-direct-session` passed, and
`tests/linux-login-session-test.sh` reported
`linux_login_session_host_tests=PASS` through the v4 mock registry. The v4 VM
campaign itself is the next gate and will supply the retained artifacts,
hashes, and measured result.

**Failures and decision:** The prior verifier printed selected evidence but
removed the raw per-criterion logs on every exit. Console output alone is not
enough for the formal campaign ledger, so retention is explicit and opt-in
instead of weakening normal disposable cleanup. Preflight also found that the
selectable-session component mock still returned a synthetic singleton current
revision and lacked registry graph commands. The mock now persists isolated
current, Experience, and graph markers and exercises install-package,
bootstrap, bootstrap-graph, and experience-status through the active v4 path.

**Remaining risks and next gate:** Synchronize this exact revision into the
Debian 13 KVM guest, run the direct gate once with a fresh evidence directory,
copy the finalized artifact set back to the host, verify its manifest, and
record the verdict. VM evidence still cannot close Framework hardware claims.

## 2026-08-27: Remove the singleton pointer from fresh Linux v4 boot

**Goal:** Resolve the first rewritten direct-DRM failure and make stable
Experience registry state the only active ownership path in a fresh Linux v4
session.

**Changed:** The development launcher now installs and registers packaged Stock
and Timeflow independently, creates no global `current` pointer, bootstraps both
graphs into per-Experience authority state, provisions a private grant-review
capability, and reviews each exact trusted graph before starting the graph
supervisor. The Linux session CLI exposes graph-authority bootstrap and trusted
graph grant review for this path.

The installed selectable session now distinguishes migration from fresh boot.
An existing v3 singleton is imported without moving its pointer. A fresh store
creates only v4 Experience and graph pointers. In graph mode the system session
does not bootstrap global authority state; it initializes and reviews exact
Stock and Timeflow graphs. The direct verifier compares registry and authority
state for `sos.stock.shell` and asserts that a fresh run never creates the
legacy pointer.

**Evidence:** The first VM attempt is retained at
`.cache/evidence/linux-v4-69d58d7/attempt1`. Its nine finalized files total
25,511 bytes. `SHA256SUMS` is 990 bytes with SHA-256
`a304ffb23e2d9e6025245b8dec03905094e10ccd4646a85e4365fb4fa279709a`.
`result.json` records FAIL after 52.738361068 monotonic seconds. The compositor
completed its recovery DRM page flip, then the graph supervisor rejected Stock
because the authority had no grant decision for `sos.stock.shell`.

After the fix, `cargo test -j1 -p sos-linux-session --all-targets` passed 19
tests, including a fresh graph-authority bootstrap that leaves
`RevisionStore::current()` absent and a migration case that leaves an existing
v3 pointer unchanged. Shell syntax, `git diff --check`, and
`tests/linux-login-session-test.sh` passed. The selectable-session fixture now
also asserts that fresh v4 startup creates no mock singleton pointer.

**Failures and decision:** The failed attempt showed that package and graph
installation alone is insufficient: graph boot correctly fails closed until
the authority has both exact per-Experience state and a reviewed stable grant.
Keeping the old global bootstrap merely to make this path start would preserve
the ownership model v4 is replacing, so the launcher now uses the graph
authority APIs directly.

**Remaining risks and next gate:** Commit and synchronize the fix, then rerun
only the direct-DRM phase with a new evidence directory. This is the second
end-to-end attempt for the same objective; another failure triggers the runtime
debug circuit breaker before any further complete rerun.

## 2026-08-27: Keep the direct input activation inside the Stock v4 contract

**Goal:** Apply the repeated-gate circuit breaker to the second direct-DRM
failure and repair the earliest failing layer without another full attempt.

**Changed:** The direct verifier no longer activates the standalone minimal
input Scene as if it were the privileged Stock shell. It derives a visible
candidate from the complete checked-in Stock source, verifies that the stable
text changed and the `agent_submit` composer remains, then submits that exact
candidate through the v4 Stock package flow. The old fixture was removed. The
input criterion remains compositor-owned: it holds kernel keyboard, button,
touch, and stylus state across the graph activation and checks suppression at
the presentation boundary. The maintenance validator no longer runs the entry
source through a module-unaware standalone analyzer when revision-local modules
are supplied; it typechecks the sidecars and compiles and validates the complete
runtime package instead.

**Evidence:** Attempt two is retained at
`.cache/evidence/linux-v4-661bb29/attempt2`. Its 11 captured evidence files
total 42,555 bytes. `SHA256SUMS` is 1,357 bytes with SHA-256
`f98ef9816b84e3e0ad6ea463707c2d4da8a91f9c4b18a40c36a166c38f932ba8`.
`result.json` records FAIL after 5.565508283 monotonic seconds. It proves both
graph-authority bootstraps and grant reviews, Stock DRM presentation, stable
host startup, and all four initial compositor input classes before
`linux-script` rejected the incomplete fixture with `requires the Stock agent
composer`.

The focused package-aware regression
`validates_a_visible_complete_stock_edit_with_its_revision_local_theme` passed.
The matching `sosctl validate` run compiled the 57,116-byte candidate with the
`stock.theme` module and reported all ten scenarios valid, from 39 to 121 nodes
and zero to one text sessions per scenario. Linux session tests, shell syntax,
the selectable-session fixture, formatting, and diff checks pass.

**Failures and decision:** This was the second full failure for the direct-DRM
objective. The runtime-debug circuit breaker stopped complete reruns. Comparing
the attempts isolated distinct sequential preconditions: attempt one lacked
authority grants; attempt two passed that layer and failed at source extraction
and revision validation. Weakening the Stock composer check or attaching the
shell role to the small input fixture would violate v4 role and authoring
boundaries, so the gate now edits the complete Stock artifact.

**Remaining risks and next gate:** The focused layer passes. Run one fresh
downstream direct-DRM attempt. A PASS must still preserve raw evidence and prove
no singleton pointer.

## 2026-08-27: Prevalidate direct candidates before the timed input hold

**Goal:** Diagnose the post-circuit downstream failure without treating it as
an input or compositor product regression.

**Changed:** The direct gate now generates and package-validates the complete
Stock candidate before creating kernel input devices. It parses the retained
validation report and requires one revision-local module and a valid aggregate
result. Only then does it begin the bounded input hold and invoke the exact
submission path again. The hold is five seconds for both successful and
aborted activations, leaving measured scheduling margin without weakening any
required input count or suppression assertion.

**Evidence:** The failed downstream run is retained at
`.cache/evidence/linux-v4-17227eb/attempt3`. Its 13 captured evidence files
total 116,640 bytes. `SHA256SUMS` is 1,489 bytes with SHA-256
`72aa35b188317ffec229c0f4efe140e9e31e6beb4eee44fc57f93bacd1c37f93`.
`result.json` records FAIL after 101.873237232 monotonic seconds. Candidate
revision `27ddfcc9…` did reach a DRM page flip in unchanged host PID 10147, but
the log shows the input contacts ended at monotonic second 1011 while candidate
quiesce began at second 1107 with `keys=0 buttons=0 touches=0`.

**Failures and decision:** The first use of the pinned Luau analyzer cloned and
built it after the input helper had begun its 1.5-second hold. The gate then
correctly failed at the first held-input assertion. Prewarming outside the
timed interval makes compilation readiness an explicit prerequisite and keeps
the activation assertion about input containment rather than host build speed.

The focused prevalidation reported PASS for ten scenarios and one module. The
matching package-aware authoring regression, shell syntax, formatting, and diff
checks passed.

**Remaining risks and next gate:** Run syntax and focused package validation,
then one clean direct phase from a fresh store and evidence directory. Do not
accept cached success without the retained prevalidation report and nonzero
held-input counts.

## 2026-08-27: Bind Linux input release and rollback to the graph transaction

**Goal:** Diagnose the fourth direct-DRM result at the first incorrect product
boundary and make a rejected presented graph return to the accepted graph with
physical evidence.

**Changed:** The compositor no longer ends its input-quiesce epoch merely
because an armed candidate frame was presented. A graph candidate remains
non-interactive until the supervisor has promoted authority state, registry
and graph pointers and sends `FinalizeGraph`. Boot and bounded legacy revision
presentation still resume explicitly after their first proven frame.

If authority rejects after candidate presentation, the Linux host now restores
the previous graph while retaining the candidate's quiesced input epoch, arms a
new fence for the restored graph, and emits `GraphDiscarded` only after that
graph has compositor presentation evidence. The compositor permits this
trusted shell rollback fence to reuse the existing quiesce epoch without an
input-release gap. The direct verifier now requires the exact five-frame graph
sequence: boot old, committed new, rejected old, restored new, restarted new.

**Evidence:** Attempt four is retained at
`.cache/evidence/linux-v4-528a3e6/attempt4`. Its finalized files total 144,395
bytes. `SHA256SUMS` is 2,388 bytes with SHA-256
`f69773ffd9653af66f0936eef4b8a900421234e01324f0f463a5ebe6c48a4724`.
`result.json` records FAIL after 15.877643295 monotonic seconds. Before the
final count assertion, it proved complete v4 Stock boot and grant review,
successful candidate activation under held keyboard, pointer, touch and stylus
input, an authority rejection under the second held-input lifecycle, unchanged
registry/authority state, and host restart from PID 13025 to 13451. Its four
DRM frames exposed the missing fifth boundary: the rejected old graph was
presented, but discard restored the accepted new graph only in host memory and
returned without a compositor fence.

After the fix, all 28 compositor library tests pass, including explicit
finalization and rollback rearm policy cases. All 36 Linux/Android experience
library tests and all 15 graph-supervisor integration tests pass. The Linux
host compiles with `linux-host`; `bash -n tools/linux-vm/verify-direct-session`,
formatting, and diff checks pass.

**Failures and decision:** Treating the fourth frame as harmless preview and
changing the expected count would not prove visible rollback, and releasing
input at candidate presentation allowed interaction before authority commit.
The durable transaction already ended at `FinalizeGraph`; physical input and
discard evidence now use that existing boundary.

**Remaining risks and next gate:** Commit and synchronize this exact revision,
then run one fresh direct-DRM attempt. PASS requires five ordered DRM graph
frames, four explicit input-epoch resumptions, retained raw evidence and a
verified manifest. Framework integrated-input and Samsung physical campaigns
remain open.

## 2026-08-27: Correct the final direct verifier authentication assertion

**Goal:** Preserve and classify the first run with physically evidenced graph
rollback, then repair its exact non-product failure before one downstream run.

**Changed:** The final direct-session PID checks now match the compositor's
current structured authentication message, including the explicit
`role=Shell`. No product behavior or acceptance count changed.

**Evidence:** Attempt five is retained at
`.cache/evidence/linux-v4-769866b/attempt5`. Its finalized files total 147,484
bytes. `SHA256SUMS` is 2,388 bytes with SHA-256
`45a7f672e1cb20b03c8647b31caa2b3c10030393c542bf4b5b6714884c486d8e`;
`sha256sum -c` verifies all 19 listed artifacts. `result.json` records FAIL
after 31.726691694 monotonic seconds.

The run nevertheless passed the complete product boundary before line 434:
five ordered DRM graph frames were Stock boot `b8d0745f…`, accepted candidate
`27ddfcc9…`, rejected Stock `b8d0745f…`, physically restored candidate
`27ddfcc9…`, and restarted candidate `27ddfcc9…`. The compositor recorded four
explicit input-epoch resumptions, with the rejected and restored frames sharing
one quiesced epoch. Registry and authority revisions agreed, no singleton
pointer existed, both held-input lifecycles had nonzero keyboard, button and
two-touch state, host PID changed from 15336 to 15770 after the forced crash,
and compatibility mapping succeeded. GDM and seatd were active after cleanup.

**Failures and decision:** The gate stopped because it searched for the removed
phrase `authenticated SOS shell control connection`; both relevant log lines
use `authenticated SOS compositor control connection ... role=Shell`. The PID
values and shell role were present. The assertion now follows the actual stable
structured message instead of weakening authentication evidence.

**Remaining risks and next gate:** Run syntax checks, commit, synchronize and
perform one fresh direct phase. A PASS must retain and verify the same evidence
set. Physical Framework and Samsung acceptance remain open.

## 2026-08-27: Pass the v4 Debian direct-DRM graph gate

**Goal:** Close the virtual Linux physical-presentation phase with a clean,
auditable v4 graph verdict after the focused authentication assertion fix.

**Changed:** No product code changed during this attempt. The exact committed
verifier and rollback/input transaction implementation from source revision
`294fe67` were synchronized into the Debian 13 guest.

**Evidence:** `tools/linux-vm/verify-direct-session` reported
`linux_direct_session_passed` with activation PID 16868, restarted PID 17301,
revision `e094a24c…`, and `drm_page_flip` evidence. `result.json` records PASS
after 15.887747472 monotonic seconds. The retained bundle is
`.cache/evidence/linux-v4-294fe67/attempt6`; its finalized files total 125,684
bytes. `SHA256SUMS` is 2,388 bytes with SHA-256
`1e143313c4d2e39d5647baa26227a24e026f0ddc8c5100111250d5241309cda1`,
and `sha256sum -c` verifies all 19 listed artifacts.

The session contains exactly five DRM graph frames in the required order:
Stock boot `b8d0745f…`, committed candidate `27ddfcc9…`, rejected Stock
`b8d0745f…`, restored candidate `27ddfcc9…`, and restarted candidate
`27ddfcc9…`. The compositor recorded four explicit input-epoch resumptions, so
the rejected and restored frames remained in one closed epoch. Both successful
and faulted activations began with one held keyboard key, one held pointer
button and two held touch contacts, and their later releases were suppressed.
Stylus pressure traversed tablet-v2. Registry and authority revisions agreed,
the fresh store had no singleton `current`, the compatibility client mapped,
and GDM and seatd were active after cleanup.

**Failures and decision:** None in the accepted run. This closes the Debian
direct-DRM v4 graph gate, including visible rollback and host restart. It does
not make a Framework panel, integrated-input, suspend, latency or thermal claim.

**Remaining risks and next gate:** Run the stable-host campaign on the physical
Framework using only input devices present in the prepared inventory and an
actual Dashboard composition graph. Then complete the Samsung v4 composition,
IME, accessibility, grant, child-failure, appearance, restart and rollback
campaign.

## 2026-08-27: Create missing parent directories during live deployment

**Goal:** Deploy the complete v4 Linux stack to the Framework development-live
target without relying on directories introduced only by newer base images.

**Changed:** `linux-live-deploy` now creates the root-owned parent directory for
every exact component destination before installing that component. This
includes the revision-local Stock module directory and also makes selective
deployment independent of which sibling component happened to run first. The
mock deployment requires the module-directory creation command explicitly.

**Physical experiment and evidence:** The Framework at `192.168.1.132`
reported `Laptop 12 (13th Gen Intel Core)`, Fedora 44, kernel
`6.19.10-300.fc44.x86_64`, boot ID
`9b1818f2-c6c3-4829-8109-c9b3320a02a3`, active GDM and SSH, and
`LiveOS_rootfs`. No internal NVMe filesystem was mounted. The prior SOS login
was still a singleton session with no active graph; it was terminated cleanly
before deployment and all SOS processes exited.

Release builds from clean source `c217b8fdd58e…` completed and the deployer
transferred the v4 executables and package inputs. Installation then stopped at
`/usr/share/sos/experiences/modules/stock-theme.luau` because that older live
image did not contain the parent `modules` directory. It did not start SOS,
touch boot configuration, or mount the internal disk. The failed deploy ended
before emitting an accepted deployment record.

After the fix, shell syntax passes and `tests/linux-live-image-test.sh` reports
`linux_live_image_host_tests=PASS`, including the explicit missing-parent
contract.

**Failures and decision:** Precreating the module directory manually would make
this target pass while leaving the reusable incremental deployer dependent on
base-image age. Parent creation is now part of the exact root installation
transaction. The partially updated target remains at GDM and must not launch
SOS until the complete redeployment verifies every installed digest.

**Remaining risks and next gate:** Commit the deployment fix, redeploy every v4
component, verify the target manifest and start a fresh Framework graph
campaign. The mutable development-live result remains diagnostic and physical
integrated input still requires an owner at the laptop.

## 2026-08-27: Keep the Framework gate allowlist aligned with v4 deployment

**Goal:** Prepare the revision-pinned Framework campaign after the complete v4
deployment without broadening the hardware gate beyond known product paths.

**Changed:** The hardware gate's closed development-deployment allowlist now
includes the exact current destinations for `sos-agent-login`, the Stock and
Timeflow package manifests and sources, the revision-local Stock theme, current
agent and stable-host documentation, and the bounded Framework display
defaults. It still rejects every unlisted path. The host test pins each newly
accepted destination so deployment and acceptance cannot drift silently.

**Physical experiment and evidence:** The complete clean deployment from
revision `f2b2b2334edd…` passed on the Framework as deployment
`20260827T090419Z-f2b2b2334edd-2567577`. It installed and independently checked
all component digests in 23,115,719,197 ns. Generated deployment evidence is
under
`.cache/evidence/framework-v4-deploy/20260827T090419Z-f2b2b2334edd-2567577/`;
the image remains mutable `development-live` and
`promotion_eligible=false`.

The first hardware preparation then stopped before runtime launch with
`unsafe development deployment path: /usr/local/libexec/sos/sos-agent-login`.
This was the first of several valid v4 paths absent from the older allowlist;
no partial campaign was accepted and SOS remained stopped at GDM. After the
fix, `tests/linux-hardware-gate-test.sh` reports
`linux_hardware_gate_host_tests=PASS`, and shell syntax passes.

**Failures and decision:** Removing current components from the deployment
manifest or accepting `/usr/local/libexec/sos/*` as a wildcard would either
hide deployed product state or weaken the gate. The allowlist remains exact and
now matches the complete v4 deployment contract.

**Remaining risks and next gate:** Commit and deploy the corrected gate only,
prepare a fresh same-boot evidence directory, then launch the Stock shell and
reference Dashboard graph. Integrated keyboard, touchpad and touchscreen
criteria cannot be satisfied through PiKVM HID.

## 2026-08-27: Represent the removed legacy experience in live-image audits

**Goal:** Keep the old baked image auditable after the explicitly requested
legacy Experience removal, without restoring that source or allowing arbitrary
missing installed files.

**Changed:** Development deployment metadata now declares one exact retired
baked artifact, `/usr/share/sos/experiences/daily-flow.luau`, and deployment
idempotently removes that exact path. The physical gate requires that exact
metadata value. While snapshotting the immutable image's historical install
manifest, it records only that exact absent file as `retired`; every other
missing or mismatched baked artifact still fails closed. Both deploy and gate
tests pin the deletion command, metadata identity, and narrow exception.

**Physical experiment and evidence:** After the corrected v4 allowlist was
deployed in the complete clean component set, Framework deployment
`20260827T090741Z-53e8cc3c150b-2570383` passed all installed digest checks in
24,551,567,902 ns. Generated evidence is under
`.cache/evidence/framework-v4-deploy/20260827T090741Z-53e8cc3c150b-2570383/`.
Hardware preparation then reached the baked-artifact snapshot and stopped on
the exact old 15,676-byte source named by the immutable install manifest. The
overlay correctly reported that path absent; its baked SHA-256 is
`09ccddca90f6d0a94ea8fbbb86204bbf8522123d2f73f21becfe964a2851a693`.
SOS was not launched and the internal disk remained out of scope.

`tests/linux-live-image-test.sh` and `tests/linux-hardware-gate-test.sh` now
both report PASS, and shell syntax passes.

**Failures and decision:** A clean newly baked image would naturally omit the
old manifest row, but the current mutable physical diagnostic must remain
truthful about its immutable lower layer. Restoring the deleted source was
rejected. A wildcard missing-file exception was also rejected; the deployment
records and the gate recognizes one exact tombstone.

**Remaining risks and next gate:** Commit and redeploy the complete manifest so
the tombstone is target evidence, then prepare the Framework campaign again.
This compatibility tombstone can be removed with the old development image;
it is not an active experience or activation path.

## 2026-08-27: Stage the Framework v4 composition campaign without claiming runtime

**Goal:** Put the exact clean v4 deployment and reference graph on the physical
Framework 12 while preserving the distinction between preparation and live
composition evidence.

**Changed environment and evidence:** Clean source revision
`c4e9aec110982b27e82e63ba2158dad8aff259c9` was completely deployed as
`20260827T091047Z-c4e9aec11098-2574462`. The deployer independently verified
every installed digest and completed in 29,171,952,848 ns. Generated evidence
is under
`.cache/evidence/framework-v4-deploy/20260827T091047Z-c4e9aec11098-2574462/`;
the mutable live overlay remains `promotion_eligible=false`.

The hardware gate then prepared
`/home/liveuser/framework12-v4-composition-c4e9aec` on Framework Laptop 12,
Fedora 44, kernel `6.19.10-300.fc44.x86_64`, boot ID
`9b1818f2-c6c3-4829-8109-c9b3320a02a3`. The internal NVMe remained unmounted.
The content-addressed reference set was installed with Dashboard graph
`f09068511e1c9d2c160fcc55583e9d347024fbf4a6ca2fa53ff2492a983ab287`,
Dashboard revision
`6676864289356369df159b1e55f110e08878948c94272398ec0580765e5eee98`,
Agenda revision
`30e496f607393114c5a3963a25b91681c5a49d222db16801cbe6882b42b3ba3b`,
Media revision
`f6a9b11b2ce849ae5525319e6dd34e14ef38406ee27b8d1524bd03bff04c8bb6`,
and self-contained remix revision
`d5f061a7097eb069c83046679cc937287219fa475c6697da381abcb23eae2de3`.

**Failure and decision:** The laptop became unreachable before GDM or SOS was
reconfigured and before the graph runtime started. No autologin mutation was
made. Preparation is retained on the live overlay, but it is not presentation,
input, restart, or rollback evidence. If the boot ID changes after wake, the
same-boot evidence directory must be discarded and prepared again.

**Remaining risks and next gate:** Wake the Framework, verify the same boot ID,
then run Dashboard through physical presentation, mounted-child interaction,
appearance propagation, restart, rollback, and integrated keyboard, touchpad,
and touchscreen checks. PiKVM HID cannot satisfy the integrated-input gate.

## 2026-08-27: Recover Android build capacity and align the Core 1 patch contract

**Goal:** Resume a clean Samsung v4 build without accepting stale generated
images or a source patch whose declared target set differs from the applied
Lineage tree.

**Changed environment:** The accepted Debian VM was stopped cleanly before its
79 GiB generated `.cache/linux-vm` runtime was removed. An unrelated,
rebuildable 163 GiB `/home/carlid/dev/aosp-sos/out` tree was also removed.
Linux source, the retained acceptance evidence, the 417 MiB base image, and the
active `/home/carlid/dev/lineage-a33x` source/output tree were preserved. Free
space increased from 116 GiB to 331 GiB; `./tools/a33xctl doctor` then passed
with 350,166,740 KiB available.

The pinned patch
`0005-s5e8825-select-no-zygote-for-sos-core1.patch` now names both the shipping
`lineage_sos_core1_a33x` product and the non-shipping
`lineage_sos_core1_dev_a33x` provider-probe product in its make condition and
description. This matches the already-applied Lineage hardware tree instead of
failing patch idempotence on the second target.

**Evidence:** `git -C /home/carlid/dev/lineage-a33x/device/samsung/s5e8825-common
apply --reverse --check` passes for the corrected patch. A subsequent
`./tools/a33xctl apply-patches` classified all seven pinned patches as already
applied. The standalone ARM64 APK, GPUI Core runtime, native Node runtime, Pi
runner, and Android authority rebuilt successfully before the shadow Lineage
compile began.

**Failures and decision:** The first `build-core` stopped because patch 0005
could neither apply nor reverse cleanly: the Lineage tree already contained the
two-product condition while the repository patch described only Core 1
shipping. Reverting the provider-probe protection was rejected. Generated
Linux VM and old AOSP output were removed only after identifying them as
rebuildable and outside the retained evidence set. The shadow build launched
from the earlier dirty patch state is diagnostic only and cannot be selected
for a device.

**Remaining risks and next gate:** Finish the diagnostic shadow build, commit
this corrected source contract, then rebuild and inspect final Compat 1 and
Core 1 artifacts from one clean revision. No Android artifact may be installed
until its source identity, target profile, signatures, AVB graph, package
contents, byte size, and SHA-256 pass the matching inspector.

## 2026-08-27: Complete Android registry launching and dynamic child containment

**Goal:** Remove the last Stock-only Android composition shortcut before
building physical artifacts: install the reference Experiences as signed
product content, launch them through the registry, propagate appearance, and
contain a child that fails after activation.

**Changed:** The Android revision protocol and authority now stage
`PresentExperience` and `DismissExperience` against the exact presented graph.
Presentation writes a pending graph record, starts no pointer mutation, and
durably selects the top-level Experience only after the host confirms a
rendered frame. The selected Experience survives authority and host restart.
Stock is the only registry-owned role that may present another Experience;
dismissal returns the currently presented ordinary Experience to Stock through
the same staged path.

The signed Samsung and Cuttlefish authority services now idempotently install
Agenda, Media, Dashboard, and Agenda-Media Remix. All four receive independent
top-level `main` graph pointers, while Dashboard retains locked Agenda and Media
summary mounts. Authority restart does not reset later registry revisions.
Provider snapshots expose the bounded catalog only to the host; the runtime
still clears it from every ordinary Experience model. Stock now requests the
explicit `appearance_write` capability. Android checks that request and its
reviewed stable-ID grant, the exact presented graph, and the next generation
before changing authority-owned appearance. A write remains available while
an ordinary graph is presented without transferring the grant to that graph.
The request travels over the existing SELinux-restricted administrative
revision channel: only the fixed Core host or platform-privileged SOS host may
connect, while untrusted app domains have no `name_connect` permission.

The GPUI host separates `shell.present_experience` and dismissal from provider
effects, commits the initiating state, prepares the selected graph, and waits
for its frame before confirmation. Exact bounded deep links support physical
automation for presentation, dismissal, appearance toggle, and reference
events. Logs identify the graph root, every Instance/Experience/export, child
failure/recovery, root readiness, and appearance generation.

The graph runtime now contains both update-time and render-time failures from a
non-root Instance. It marks only that child failed, removes its scene so the
host renders the unavailable placeholder, and suppresses the failed update's
provider effects and output events. The root and siblings remain ready. The
reference Agenda exposes hidden acceptance failure/recovery actions and
Dashboard exposes a liveness counter. Media's invalid `music.toggle` fixture
was corrected to the closed `media.play_pause` ABI and its immutable package
now requests `music_control`.

**Evidence:** The focused reference graph test installs all four top-level
graphs, starts Dashboard's three VMs, routes `agenda.open`, contains both an
Agenda update exception and render exception while Dashboard remains ready,
commits a root liveness action, recovers Agenda, propagates appearance, and
checks the self-contained remix. All 32 `runtime-luau` library tests pass.

The Android authority suite passes 16 tests and the revision wire suite passes
5. The new authority case discovers four signed catalog records, stages the
three-Instance Dashboard, recovers the pending selection after restart,
confirms it, denies presentation from an ordinary root, accepts a Stock-granted
appearance generation while Dashboard is active, denies Dashboard the same
write, restarts on the exact Dashboard graph and appearance generation, then
stages and confirms dismissal to Stock. Both ARM64 checks pass:
`cargo ndk -t arm64-v8a -P 31 check -j1 -p sos-experience --features
aosp-system` and the `--no-default-features --features core-native` variant.
`./tools/a33xctl check-product-graph`, shell syntax, Rust formatting, and diff
whitespace checks pass.

The earlier diagnostic Shadow image completed successfully in 13m44s as
`sos.shadow.c4e9aec11098.b97282870441`, proving the corrected seven-patch
Lineage tree and full OTA pipeline. It was built before this change from a
dirty source identity and is explicitly rejected for installation.

**Failures and decision:** Android previously packaged only Stock even though
its runtime could decode a graph. Stock therefore received an empty Experience
catalog, and its valid `shell.present_experience` effect was rejected by the
system provider registry. Separately, startup rendering contained a broken
child, but a child that failed during an update caused whole-graph rollback.
Test-only direct pointer mutation and raw scene injection were rejected. The
same staged authority protocol and failed-Instance placeholder now cover the
real product path.

**Remaining risks and next gate:** Commit this slice, rebuild Compat 1 and Core
1 from the clean revision, run their complete inspectors, then install only an
exact inspected artifact on serial `RFCT50EGFCN`. Physical acceptance must
still prove touch, hardware keyboard/IME where applicable, accessibility,
namespaced Agenda interaction, appearance propagation, failure containment,
restart, rollback/dismissal, memory, thermals, and exact evidence hashes. The
local USB ACL and sleeping Framework remain external prerequisites for their
respective physical gates.

## 2026-08-27: Build and inspect exact v4 Android composition artifacts

**Goal:** Produce installable Compat 1 and Core 1 artifacts from the committed
Android composition implementation, prove their package and image contracts,
and preserve the exact inspected inputs before the shared Lineage output tree
changes product.

**Changed:** No product source changed during this gate. Both builds used clean
commit `52ce577ac0f46e9a22eec4edde93f136b648b894`. Compat 1 identified itself as
`sos.compat1.52ce577ac0f4.89321d76b350`; Core 1 identified itself as
`sos.core1.52ce577ac0f4.8ff80933e8b1`. The complete extracted target-files
trees were archived with Zstandard alongside each OTA and inspector log under
`.cache/evidence/android-v4-52ce577/` before the next product build reused the
Lineage output tree.

**Evidence:** `./tools/a33xctl build-compat1` completed successfully in 5m41s.
`./tools/a33xctl inspect-compat1` passed whole-package signature, compressed
data, PIT ceilings, boot/recovery/vendor-boot footers, the complete AVB and
verity graph, recovery init packaging, exact Stock source/package/theme,
SELinux labels and policy, provider runner, v4-only creation markers, and all
four signed reference Experience markers. Preserved Compat 1 artifacts are:

- `compat1/lineage-23.0-20260827-UNOFFICIAL-sos_compat_a33x.zip`:
  1,067,703,993 bytes, SHA-256
  `9d013db07b75c469ff4f8186c6ebb944a150a196ff924bbc11276e055c6daf76`;
- `compat1/lineage_sos_compat_a33x-target-files.tar.zst`:
  2,175,140,468 bytes, SHA-256
  `40c2e3134f3c9aea2c27b244cd989c513376a18e05ff10a41ac2c5083cb995a2`;
  and
- `compat1/inspect-compat1.log`: 29,237 bytes, SHA-256
  `99b078dbf0c71bcdbd46623650a17d98440d8f2dc34bb87d9d845accc9f6af38`.

`./tools/a33xctl build-core1` completed successfully in 5m23s.
`./tools/a33xctl inspect-core1` passed the same signature, PIT, AVB, recovery,
v4 package, theme, and composition gates plus native no-Zygote ownership,
pinned Core model validation, disabled user APK installation, and the exact
Core host/platform policy. Preserved Core 1 artifacts are:

- `core1/lineage-23.0-20260827-UNOFFICIAL-sos_core1_a33x.zip`:
  1,022,839,686 bytes, SHA-256
  `66cba1584be3a8a8d9e849a19e57c52ae4c1a45c22ff28183bc2f4f54c4a216c`;
- `core1/lineage_sos_core1_a33x-target-files.tar.zst`:
  2,079,179,951 bytes, SHA-256
  `887e2d0f3f9a0dc11408d2ffe4b1e463432063a006447cb5f7d83d6b7b347699`;
  and
- `core1/inspect-core1.log`: 25,466 bytes, SHA-256
  `7bd04f14962f24d9f84cf076bbe0cd87586f13a1651c5cf31c5e54f7752ba530`.

**Failures and decision:** The earlier Shadow artifact remains rejected because
it predated the committed composition source. Both final artifacts passed from
one exact clean commit. The physical Samsung is detected at USB path `1-1.1`,
but `adb devices -l` reports serial `RFCT50EGFCN` as `no permissions`; no image
was installed and no physical claim is made from the build inspectors.

**Remaining risks and next gate:** Refresh the Samsung USB ACL, install only the
preserved inspected Compat 1 artifact, and run the complete device campaign
before advancing to Core 1. Physical input, accessibility, restart, recovery,
thermal, memory, and composition behavior remain open. The Framework Linux
input campaign also remains open and may proceed independently over SSH while
the Android USB transport is unavailable.

## 2026-08-27: Recheck the v4-only authoring and Linux host boundary

**Goal:** Verify at the exact documentation head that no checked-in Linux
experience or creation command can return to Experience API v3, and distinguish
the remaining physical prerequisites from product failures.

**Changed:** No product or target state changed. The audit ran at commit
`9610dec`. Stock and Timeflow are the only checked-in general Linux experiences
and both have package-v4 manifests. `sosctl linux-script` requires
`api_version = 4`, installs the Stock package through the v4 package path,
resolves a graph, and activates that graph. Daily Flow has no source, package,
agent input, or activation path. Its remaining non-historical occurrences are
the exact retired baked-artifact tombstone and tests that require its deletion.
API v3 remains only the deliberately bounded activation/rollback decoder and
test fixtures that exercise that compatibility reader. Scene ABI v3 references
describe the retained scene model and are not Experience API v3 authoring.

**Evidence:** `tests/linux-live-image-test.sh`,
`tests/linux-hardware-gate-test.sh`, and
`tests/linux-login-session-test.sh` each reported `PASS`. The corrected command
`cargo test -j1 -p sos-linux-session -p revision-supervisor --all-targets`
passed 69 tests with one explicit desktop-performance test ignored: registry,
resolver, tracked update, graph journal, authority transaction, authoring,
legacy import, and shared v4 wire cases all passed. A case-insensitive source
audit found the Daily Flow name outside `docs/progress.md` only in the exact
live-image tombstone and its pinning tests.

**Failures and decision:** The first combined Rust invocation selected the
nonexistent Cargo package name `linux-session`; Cargo rejected it before
running tests. Repeating the unchanged test command with the declared package
name `sos-linux-session` passed. No hardware verdict is inferred. The Samsung
enumerates as USB `04e8:685d` in Download Mode, so adb cannot own it, and the
Framework address `192.168.1.132` returns `No route to host`. No OTA, target
file, session, or boot state was changed.

**Remaining risks and next gate:** Exit Samsung Download Mode into the existing
authorized Android system, unlock and replug it, then sideload the exact
inspected Compat 1 artifact through the established one-OTA recovery path.
Wake the Framework and redeploy the current clean v4 component set before its
integrated input and composition campaign. Retain the v3 rollback reader until
the pinned recovery artifacts have completed migration, as required by the
rolling and reversible plan; do not restore v3 authoring.

## 2026-08-27: Make child failure and timeout physically addressable

**Goal:** Let the Framework and Samsung campaigns prove child containment
through the shipped reference graph instead of substituting a direct runtime
unit-test call for physical host behavior.

**Changed:** Agenda's reference summary now has semantic controls that trigger
an update exception and an instruction-budget timeout. Dashboard has a parent
liveness control and displays its committed count. These are ordinary Scene
ABI interactions. Their Instance-owned targets pass through the same input,
graph worker, authority transaction, and fallback rendering paths as any other
child or parent action. No host-only event injection or cross-experience call
was added. The reference integration test now times out Agenda, verifies that
Dashboard remains Ready, commits a Dashboard action, recovers Agenda, and then
continues the existing render-failure proof.

**Evidence:** Both changed Luau sources passed `sosctl validate`. Agenda
validated at 2,882 bytes with five nodes and four semantic nodes; Dashboard
validated at 2,603 bytes with six nodes and two semantic nodes. The focused
reference composition test passed, including the bounded timeout interrupt,
and all 32 `runtime-luau` library tests passed. Both default and
`direct-backend` compositor checks also pass without the earlier feature-only
unused-parameter warning.

**Physical environment and failure:** Before this audit changed the reference
source, clean commit `205fc42548d529ec011f189a71e367092257a379` deployed all
Linux components to the Framework as deployment
`20260827T103016Z-205fc42548d5-3061033`. The deployer verified every installed
digest in 218,496,599,593 ns. The laptop remained at GDM on boot ID
`9b1818f2-c6c3-4829-8109-c9b3320a02a3`, with no SOS process and no mounted
internal NVMe. The newly installed reference set produced Dashboard graph
`d3a96aeec8cd8c290e38602ce91d41f4d05a459d3843fdd864e325a58ba24c3f`.
That deployment is now stale by design because the acceptance controls changed
after the audit. It is deployment evidence, not runtime acceptance.

**Decision and next gate:** Commit and redeploy the complete reference set,
install its new exact revisions, then prepare a fresh same-boot campaign. The
operator must still use the integrated Framework keyboard, touchpad, and
touchscreen; accessibility automation may inspect or activate composition
semantics but cannot satisfy those physical-input criteria.

## 2026-08-27: Freeze exact composition artifacts and redeploy Framework

**Goal:** Replace the now-stale Android and Framework artifacts with exact
outputs from the composition acceptance-control revision before either physical
campaign starts.

**Changed:** No product source changed during this gate. Compat 1, Core 1, and
the Framework deployment all use clean commit
`ed88d333d02f85b0c9f43ba237a60f3b67cecd8b`. Compat identifies itself as
`sos.compat1.ed88d333d02f.191469142323`; Core identifies itself as
`sos.core1.ed88d333d02f.a6e4c4bebd47`. The preserved Android inputs live under
`.cache/evidence/android-v4-ed88d33/`. This directory does not yet contain a
final evidence manifest because device evidence still has to be added.

**Android evidence:** `./tools/a33xctl build-compat1` reported a successful
Lineage build in 3m33s. Its preserved inspector rerun completed in 18.98s and
passed whole-package signature, compressed data, PIT ceilings, AVB and verity,
recovery init, exact v4 package and theme bytes, SELinux policy, provider
runner, and all four signed reference Experience markers. The exact Compat 1
files are:

- `compat1/lineage-23.0-20260827-UNOFFICIAL-sos_compat_a33x.zip`:
  1,067,692,779 bytes, SHA-256
  `49620a0d3115027341d7b08a77f387dd942841207bc3a237f695d88deb60d69f`;
- `compat1/lineage_sos_compat_a33x-target-files.tar.zst`:
  2,176,146,364 bytes, SHA-256
  `9ce371c3a360ae4dc54c55c006efe1c05f573a7726371b3c70a64c10c097af25`;
- `compat1/inspect-compat1.log`: 29,237 bytes, SHA-256
  `98362f1ff87f527957ff77c20ccee4cdcbab20504ce56bb1ed423be6b1ec34ff`;
  and
- `compat1/inspect-duration.env`: 33 bytes, SHA-256
  `906be7f1e8e3dc5af615ec5c82b46615f914011c7ba044cc48848f9ee3fe1aad`.

`./tools/a33xctl build-core1` reported a successful Lineage build in 3m28s.
Its preserved inspector rerun completed in 17.22s and passed the shared image
checks plus no-Zygote ownership, pinned Core model validation, disabled user
APK installation, native host and platform policy, and the signed reference
composition markers. The exact Core 1 files are:

- `core1/lineage-23.0-20260827-UNOFFICIAL-sos_core1_a33x.zip`:
  1,022,894,881 bytes, SHA-256
  `c5ef24a41e284bb93febe75d12dd45836f28720eb6e775f0e58594d0f5467b78`;
- `core1/lineage_sos_core1_a33x-target-files.tar.zst`:
  2,079,722,884 bytes, SHA-256
  `e1f15ad5a354987dc6faa2b7e642132b4d86d84671a94ec6ff1a91cc4246fa7a`;
- `core1/inspect-core1.log`: 25,466 bytes, SHA-256
  `7b9b167f1f0132df378eb1a52d58db772d620097fa9d0fbb51de50c70effc12a`;
  and
- `core1/inspect-duration.env`: 27 bytes, SHA-256
  `ca295f20f7f20f570d3cc87d28f9734474bf468184bd878ed59920e52fcecafe`.

**Framework environment:** `tools/linux-live-deploy` installed the exact clean
revision as deployment `20260827T103546Z-ed88d333d02f-3066003` in
72,958,871,510 ns. Its three local records are under
`artifacts/linux-live-deploy/20260827T103546Z-ed88d333d02f-3066003/`:
`deployment-result.env` is 348 bytes with SHA-256
`d5fe4d49ab74ee12ce8c64291f0689383de196d44b712a566f635d79a7b88ac2`,
`development-deployment-manifest.tsv` is 2,389 bytes with SHA-256
`66abe0f29472cd1e3fe545d4240d908ba686d4968df3e1553009a922d6904f1f`,
and `development-deployment.env` is 345 bytes with SHA-256
`ed89e9bb6e23cbfc56de8cc0f399b578bc2e480da4d6c9e9ec68b7218a715bbb`.
The target holds Dashboard graph
`be6025edc7f87c25167df07079612b78d34a999b4c94514f79d3df179d933582`
and a fresh same-boot gate at
`/home/liveuser/framework12-v4-composition-ed88d33`. It remains a
`development-live` diagnostic with `promotion_eligible=false`.

**Failure and decision:** The earlier `52ce577` Android artifacts and
`205fc42` Framework deployment predate the acceptance controls and are rejected
for these campaigns. The Samsung still has not received an OTA. The Framework
is reachable on the prepared boot, but it remains at GDM. The PiKVM API is
online and authenticated access is unavailable, so no remote HID was sent and
no Linux runtime or physical-input claim is made.

**Remaining risks and next gate:** Enter the prepared SOS session, collect the
integrated Framework input and live Dashboard composition evidence, and return
cleanly to GDM. Then install only the preserved Compat 1 OTA on serial
`RFCT50EGFCN`, complete its physical campaign, and advance to the preserved
Core 1 OTA. Generate manifests only after each evidence directory is final.

## 2026-08-27: Fix Linux appearance-grant startup schema skew

**Goal:** Diagnose the Framework SOS login that returned to GDM and repair the
earliest failed runtime boundary before another physical attempt.

**Failure evidence:** The exact `ed88d33` deployment authenticated through GDM,
initialized both DRM outputs, and reached a nonempty recovery page flip. It
then started the provider, authority, supervisor, permanent host, offline agent,
and authoring broker. At monotonic timestamp `110121.424685`, the host rejected
Stock's reviewed `appearance_write` capability as an unknown
`providers-linux::Capability` variant. The host exited, the supervisor and
session followed, and GDM returned. The failure happened before the first Stock
frame, so no input or composition interaction was attempted.

The finalized same-boot result is
`.cache/evidence/linux-framework-ed88d33-startup-fail/`. Its campaign wall time
is 1,083,931,528,324 ns and its verdict is `DIAGNOSTIC_FAIL
promotion_eligible=false`. The directory contains 38 manifested files and is
1,218,361 bytes. `evidence-manifest.tsv` is 3,597 bytes with SHA-256
`45acc4906d040b13e2bbf328881d0389c773ec5c89749e4b4e2e8909dfa42c94`;
independent verification passed. `journal-user.txt` is 18,016 bytes with
SHA-256
`3a339d83fb9651f9f8d3c3d4c02049c79f2f287e51634f0691c8923eda8338d7`.
`journal-kernel.txt` is 4,015 bytes with SHA-256
`8d5e52d076039ab0d6405f574ce9ca5232abf9fa386870e5ba22ba2be039f786`.
`verdict.txt` is 1,128 bytes with SHA-256
`1e241179d1f049d64f7e259308ce2fcf05b4e6e8eaad10ae90e8237b28dbf019`.
The gate passed same-boot identity, recovery page flip, direct compositor,
offline agent start, prepared input inventory, one host launch, GDM fallback,
and kernel fault scrutiny. It correctly failed session readiness, input,
activation, durable agreement, clean logout, and process-failure criteria.

**Changed:** `providers-linux::Capability` now includes `AppearanceWrite`. The
variant only decodes an authority-owned grant; it adds no Linux provider
operation. A regression test loads the shipped Stock v4 package and requires
every declared provider capability to decode through the Linux host type. This
closes the exact package-to-host schema boundary that escaped the earlier graph
and Android tests.

The laptop also lost network access while it waited at GDM. The existing
launcher inhibitor starts only after GDM launches SOS, so it correctly protects
an active SOS session but cannot protect the prepared-login interval. The Linux
hardware gate now starts the root transient unit
`sos-linux-hardware-gate-awake.service` at the end of `prepare`. Its logind block
inhibitor covers `idle`, `sleep`, and `handle-lid-switch`. `collect` requires
that exact recorded unit, captures its inhibitor row, and releases it before
manifest generation; its exit trap releases the unit after an earlier
collection failure. Preparation refuses a second active owner. The gate does
not change persistent GNOME or GDM power settings. The final audit now requires
the exact prepared inhibitor row, the row immediately before release, and the
recorded inactive unit after release as the `gate_awake_inhibitor` criterion.

**Inhibitor experiment:** A user-manager transient unit was rejected by logind
with `Failed to inhibit: Access denied` because a noninteractive desktop user
cannot acquire this block inhibitor. That approach was removed. The root-owned
transient unit then registered successfully as PID 863940 with
`sleep:idle:handle-lid-switch`, reason `Prepared physical acceptance campaign`,
and mode `block`; it remains active on the Framework while the fixed deployment
is prepared.

**Verification:** All 18 `providers-linux` tests pass. The `sos-experience`
library passes 18 tests, `sos-linux-session --all-targets` passes 20 tests, and
`revision-supervisor --all-targets` passes 49 tests with only the explicit
desktop composition metric ignored. The latter includes the reference graph,
activation journal, tracked update, restart, and host lifecycle cases.
`tests/linux-hardware-gate-test.sh` executes normal inhibitor release and the
early-failure cleanup path and passes. `tests/linux-login-session-test.sh` and
`tests/linux-live-image-test.sh` also pass. Shell syntax, Rust formatting, and
diff whitespace checks pass; `shellcheck` is not installed on this development
host.

**Decision and next gate:** The failed run is retained and cannot be promoted.
Commit and deploy the decoder fix, prepare a new same-boot directory from that
exact clean revision, and make one fresh physical login attempt. The Framework
integrated-input and live Dashboard composition criteria remain open.

## 2026-08-27: Isolate accessibility endpoints for presented Experiences

**Goal:** Resume the Framework composition campaign after the v4 grant decoder
repair and follow the first failed top-level Dashboard launch to its earliest
runtime boundary.

**Physical experiment:** The exact clean `05845c7` source deployed as
`20260827T111453Z-05845c792079-3342501` in 147,826,226,695 ns. The new gate at
`/home/liveuser/framework12-v4-composition-05845c7` acquired its root-owned
GDM-gap inhibitor before the temporary inhibitor was released. The SOS login
then reached physical DRM presentation, loaded Stock's reviewed
`appearance_write` grant, presented Stock, and kept the permanent host and
offline authoring agent alive. A console-driven prompt produced and presented
candidate Stock revision
`26f36f17f51251e5c8c911e9d8cbec0a34b5d9a2b2e2fb3db2ed03245fe30491`
without restarting the permanent host.

The first Dashboard catalog action was correctly denied because the reference
Media and Dashboard packages had not received independent authority decisions.
This was an acceptance-preparation omission, not permission inheritance: the
trusted `bootstrap-graph` and exact `review-graph-grants` commands initialized
the three-Experience graph and recorded one Media capability review plus one
Dashboard data-flow review. The retry advanced past grants and launched PID
871467, then failed at monotonic timestamp `111585.383886` with
`accessibility service is already listening` on the shell's canonical socket.
The supervisor retained Stock. Direct cross-root `activate-graph` was also
rejected as designed because the configured root did not match the Dashboard
graph; no pointer was forced.

**Changed:** The Linux host now reads the first graph request and determines
its registry role before starting accessibility. The shell continues to own
the canonical `SOS_ACCESSIBILITY_SOCKET`. An independently presented ordinary
Experience derives a fixed-length SHA-256 namespace from its stable Experience
ID and binds a sibling socket, while mounted child semantics stay namespaced in
the host tree. This prevents a top-level application host from colliding with
or replacing shell semantics and keeps the Unix path below the platform length
limit even for a maximum-size Experience ID.

**Verification:** `cargo test -j1 -p sos-experience --features linux-host
--lib` passes 37 tests, including the bounded, stable, distinct top-level
accessibility path regression. The default `sos-experience` library suite
passes all 18 tests, and Rust formatting passes.

**Decision and next gate:** Preserve the `05845c7` diagnostic interval, deploy
the namespaced-host fix from one clean revision, and relaunch Dashboard through
Stock after the exact independent grant reviews. Pass still requires visible
Dashboard, Agenda, and Media semantics, child event and appearance propagation,
host recovery, clean logout, same-boot collection, and manifest verification.

## 2026-08-27: Accept concurrent authenticated compositor clients

**Goal:** Retry the Framework Dashboard launch after separating top-level
accessibility endpoints and diagnose the next failed boundary without relaxing
the presentation protocol timeout.

**Physical experiment:** Exact clean revision `943ebba` deployed as
`20260827T113325Z-943ebba00bfa-3350997` in 120,954,048,218 ns. The prepared
same-boot directory is
`/home/liveuser/framework12-v4-composition-943ebba`. Stock recovered its
durable generated revision, and its catalog action launched ordinary Dashboard
host PID 881336. That host successfully bound the new namespaced accessibility
socket `accessibility-52cfa993b20dbe27.sock`, proving the preceding fix. It
then timed out before compositor application registration and was killed after
the supervisor closed its proxy.

**Root cause:** The mode-0600 compositor control listener accepted one
connection and called its complete connection loop inline. Stock's permanent
shell registration is intentionally long-lived, so the listener could never
accept the Dashboard registration behind it. Raising the five-second protocol
timeout would only delay the same failure and was rejected.

**Changed:** Each accepted control connection now runs in its own bounded
client thread. A maximum of 16 registering or registered connections covers
the eight-live-Instance limit with recovery overlap while rejecting an
unbounded same-UID connection fan-out. Authentication remains token plus
`SO_PEERCRED` PID. All policy and geometry mutations still serialize through
the single calloop channel and compositor state owner.

**Verification:** `cargo test -j1 -p sos-compositor --all-targets` passes all
29 tests, including recovery of a released connection slot at the hard bound.
Rust formatting passes.

**Decision and next gate:** Preserve the `943ebba` failure interval, deploy the
bounded concurrent-listener fix from a clean revision, and repeat the same
Stock catalog action. Require both authenticated control PIDs, the separate
semantic endpoints, a mapped Dashboard surface, mounted child semantics, and
the remaining physical and recovery criteria before collection.

## 2026-08-27: Filter Linux provider snapshots by reviewed grants

**Goal:** Continue the same Framework Dashboard launch after the concurrent
compositor-listener repair and stop at the first remaining failed boundary.

**Physical experiment and failure:** Exact clean revision `e024434` deployed as
`20260827T114243Z-e024434871c3-3355860` in 95,551,157,858 ns. PiKVM selected
the SOS GDM session and entered the prepared login without local intervention.
Stock presented on both physical DRM outputs and kept its permanent host alive.
Its `Open dashboard` semantic action launched ordinary host PID 887248. The
host bound `accessibility-52cfa993b20dbe27.sock` and authenticated a concurrent
`NativeApplication` compositor-control connection, proving both preceding
fixes. It then stopped before graph preparation because its empty, reviewed
provider grant could not pass the Linux snapshot reader's unconditional
`NotesRead` check. The compositor closed only the failed application's control
connection; Stock remained interactive.

**Root cause and changed code:** `ProviderHub::snapshot_with_frames` required
`NotesRead`, `CalendarRead`, `MusicRead`, and `SystemRead` before returning any
model. That made every ordinary v4 Experience request Stock's complete read
set, even when its package declared no provider access. Linux snapshots now
read each domain only when the authority-reviewed grant contains that domain's
read capability. Denied notes, calendar, music, legacy network/system, system
provider data, and provider surfaces arrive as empty typed values. Effects
retain their separate capability checks. The reference Media package now
declares both `music_read`, which its renderer consumes, and `music_control`,
which its toggle effect consumes.

**Decision and next gate:** Run the focused provider and reference-graph tests,
then redeploy one clean revision. The next physical attempt must render the
complete Dashboard graph rather than widening Dashboard's authority decision.
The current run is diagnostic evidence, not an acceptance result.

## 2026-08-27: Retire the secondary Linux experience and fix the v4 physical audit

**Goal:** Remove the obsolete secondary Experience from the Linux product and
make the Framework gate judge the multi-host v4 system instead of its former
single-revision assumptions.

**Changed:** The selectable login, direct system session, development runner,
installer, live-image check, and resident-agent unit no longer install,
bootstrap, review, advertise, or use the secondary Timeflow package. The agent
prompt builder now accepts one required Stock example; Android may still pass a
second example while its migration remains open. On an upgraded Linux state
directory, `retire-experience` atomically moves the old ordinary registry
record to `retired-experiences/<Experience ID>`. Its revision history remains
recoverable, but it no longer enters Stock's launch catalog. The pinned Stock
Shell cannot be retired. Development deployment also removes the two baked
Timeflow files and the older Daily Flow source, and records that exact retired
artifact list for the hardware gate.

The hardware collector now reads Stock with `experience-status` and compares
that v4 registry revision to
`authority.experiences["sos.stock.shell"].revision_id`. It no longer compares
the compatibility singleton pointer to v4 authority state. Stable lifecycle
now requires one unique authenticated `Shell` compositor-control PID while
allowing independently presented and intentionally recovered
`NativeApplication` hosts. Counting every isolated host as a shell restart was
rejected because successful Dashboard composition necessarily creates another
host. Deleting old revision data was also rejected; registry retirement keeps
history while removing the product entry point.

**Verification:** `cargo test -j1 -p revision-supervisor --all-targets` passes
50 tests plus the explicitly ignored desktop metric, including recoverable
retirement and the pinned-shell rejection. `cargo test -j1 -p
sos-linux-session --all-targets` passes 20 tests. `cargo test -j1 -p
sos-experience --features linux-host --lib` passes 37 tests. The login-session,
hardware-gate, and live-image shell suites pass. The agent TypeScript build and
all 19 tests pass, including the packaged runner with only one example. Shell
syntax, Rust formatting, and diff whitespace checks pass.

**Decision and next gate:** Commit and deploy this clean v4 Linux product set,
archive the old registry record on the Framework, and prepare a fresh exact
same-boot campaign. Physical Dashboard composition, child event, appearance,
application-host recovery, integrated input, and clean logout remain open.

## 2026-08-27: Deploy the resident-agent runtime with Linux source changes

**Goal / physical failure:** Run the fresh Framework campaign from exact clean
revision `99fc6edbb1b3f0a8b2f07abeab6e2dc93f7282e5`. The complete deployment
`20260827T120844Z-99fc6edbb1b3-3373758` passed in 221,202,268,279 ns and the
gate at `/home/liveuser/framework12-v4-composition-99fc6ed` acquired its
root-owned sleep inhibitor. PiKVM selected the SOS GDM session and submitted
the login remotely. GDM opened session 300 at monotonic timestamp
114591.240238, but the session returned to tty4 at 114594.773129, about 3.53
seconds later. The console selection, black transition, and returned console
frames are retained under
`.cache/evidence/linux-framework-99fc6ed-live/`; their SHA-256 values are
`082af38c44505a2fdc53ae274e13860673935c8bd78a2f4e54986d0dd5de3cb9`,
`8be18d70dd2ffbfbb73d3aec970541fec3c4dfb27e1dbc4c86736e6bc108af8b`,
and `f2e47040186ffa55231ea2c06479f40a08be91831c0a69c2802515e83e4271ea`.
This run is diagnostic and not an acceptance result.

**Diagnosis / evidence:** The compositor reached direct DRM on both outputs,
the provider and supervisor started, and Stock host PID 898012 authenticated
as the shell. The resident agent then exited with `missing required option
--example-secondary`; the login wrapper correctly treated the missing agent
socket as fatal and closed the graphical session. The target's bundled runner
was the baked 1,878,811-byte artifact with SHA-256
`3eee6e7922fb82e344277793a435bb8edd36a2c183050b638a3c6ca13d3bc99a`.
The current one-example-capable bundle is 1,890,551 bytes with SHA-256
`c98c35fefede6b9c5d53f5b01021e63ed7ccd790470db5c3f02a186305ae4b58`.
`linux-live-deploy` rebuilt and copied native binaries and checked-in assets,
but did not define the resident-agent runtime as a deployable component, so
the source/API change and runtime artifact diverged despite the agent tests
passing locally.

**Verification:** The agent package rebuilt the exact 1,890,551-byte bundle
and all 19 tests passed, including the packaged runner with only one example.
The complete Linux live-image host suite passed, the deploy component listing
contains `agent-runtime` at its installed path, Bash parsing and the Git
whitespace check passed. The ordered local campaign took 3.742 seconds.
ShellCheck was not installed in this checkout environment, so no ShellCheck
result is claimed for this change.

**Changed / decision / remaining risk / next gate:** Add `agent-runtime` to the
development deployment contract and its default component set. It runs the
pinned package build, stages the executable bundle, records its size and
digest in the deployment manifest, installs it at
`/usr/local/libexec/sos-agent/dist/agent-runner.cjs`, and verifies the remote
digest like every native component. The host regression now requires this
component and build step. Updating only the current target by hand was
rejected because the next full deployment would silently recreate the same
source/runtime skew. Run the agent and live-image host suites, commit the
change, deploy the exact clean revision, and repeat the PiKVM login. The full
Dashboard composition and physical acceptance gates remain open.

## 2026-08-27: Admit the agent bundle to the development gate allowlist

**Goal / physical preflight failure:** Deploy the resident-agent correction
and prepare a new exact Framework campaign. Clean revision
`418cb925952164b84d55b397d1d6a1288edc42b5` deployed as
`20260827T121856Z-418cb9259521-3380427` in 68,434,256,933 ns. Its manifest and
the target both recorded the corrected runner at 1,890,551 bytes with SHA-256
`c98c35fefede6b9c5d53f5b01021e63ed7ccd790470db5c3f02a186305ae4b58`.
Before SOS login, `prepare` rejected that new manifest entry as an unsafe
development deployment path. It created only the partial preflight directory
`/home/liveuser/framework12-v4-composition-418cb92`; no campaign environment,
graphical session, or acceptance result was created. A separate root-owned
handoff inhibitor remains active while the exact-revision campaign is rotated.

**Root cause / changed / decision:** The deploy tool and manifest now owned the
agent bundle, but the hardware gate's closed allowlist still named only the
previous deployment components. Add exactly
`/usr/local/libexec/sos-agent/dist/agent-runner.cjs`; do not widen the rule to
the complete agent tree. The live-deployment fixture now builds, stages,
installs, hashes, and compares this component and requires the 16-entry
selected-component manifest. The hardware-gate fixture requires the exact
allowlist entry. Bash parsing, both complete host suites, and the Git
whitespace check passed in an ordered 4.359-second campaign. Commit and
redeploy all default components from one clean revision, then prepare and
execute the PiKVM campaign. Physical composition remains the next gate.

## 2026-08-27: Provision the Linux appearance authority at every launch path

**Goal / physical diagnosis:** Continue the exact Framework v4 composition
campaign through the first unsupported authority operation. Clean revision
`c7558102e7049f4321434a1156ba0414c547a88c` deployed as
`20260827T122511Z-c7558102e704-3386411` in 74,809,654,404 ns. PiKVM selected
SOS at GDM, entered the login, and captured Stock and the composed Dashboard
without local console handling. Locked graph
`9a49ec819b8d0c83fa45566f16925498dbb95bd882bfc87d6b92526603e637f7`
mounted independent Agenda and Media instances. Agenda's structured child
event reached Dashboard in 33,807,189 ns. A forced child update failure left
the parent responsive and durable Agenda state intact; a forced timeout was
contained in 22,046,197 ns and recovered in 112,470,695 ns. Killing Dashboard
renderer PID 903265 left shell renderer PID 900864 alive and produced proxy
PID 908497 plus renderer PID 908502 with both child mounts and durable state
restored.

**Failure / root cause:** Live appearance propagation could not start because
the selectable login provisioned only `grant-review.capability`. The system
session already had a least-privilege path that copies an existing
`appearance-write.capability` into a provider-only runtime credential, but no
selectable, development, or direct-VM launcher created the source capability.
The live provider therefore started without `--appearance-capability-file`.
An attempted Media action also failed atomically because no MPRIS player was
active; authority and Dashboard state remained unchanged, which is the
expected provider-boundary behavior rather than a product defect.

**Changed / decision:** Provision a persistent random 32-byte appearance-write
capability beside the authority state for the selectable session and
development runner, with mode 0600. Pass it only to the provider authority.
The direct-system VM setup creates the equivalent `sos-compositor:sos-ipc`
0640 capability so the existing per-role credential copy can restrict runtime
access. Keep appearance authority separate from grant review instead of
reusing one bearer secret across two powers. The selectable-session regression
asserts both capability files are exactly 64 hexadecimal bytes at mode 0600.

**Verification:** All 20 Linux session tests pass, including graph-authority
binding and the shared v4 wire fixture. The selectable-session, complete live
image, and hardware-gate host suites report PASS. Bash parsing passes for all
four changed launch and test scripts; Rust formatting and Git whitespace checks
also pass.

**Remaining risks and next gate:** Commit and redeploy one exact clean revision.
A fresh Framework campaign must mutate appearance through the authority socket,
prove generation propagation without revision changes, and repeat composition,
containment, renderer recovery, Stock revision activation, physical input,
clean logout, manifest verification, and evidence collection.

## 2026-08-27: Preserve appearance generation across graph child recovery

**Goal / physical experiment:** Exercise live appearance and child containment
through one fresh exact Framework session after provisioning the missing
authority credential. Clean revision
`175137d366f9a72f3d3a379f5ef2dd0c23b5c72d` deployed as
`20260827T124231Z-175137d366f9-3396618` in 40,113,525,553 ns. PiKVM performed
the complete GDM SOS selection and login. The persistent appearance capability
was 64 bytes at mode 0600; the running provider received its separate mode-0400
runtime copy. Dashboard graph
`9a49ec819b8d0c83fa45566f16925498dbb95bd882bfc87d6b92526603e637f7`
presented both locked children.

The authority accepted appearance generation 0 to 1 in 4,964,227 ns and the
Agenda semantic result exposed generation 1 after 407,318,787 ns. Media's
custom scene remained byte-for-byte equal and the Agenda, Media, and Dashboard
revision IDs did not change. The target evidence is under
`/home/liveuser/framework12-v4-composition-175137d/composition`; the PiKVM
generation-1 frame is under
`.cache/evidence/linux-framework-175137d-live/dashboard-appearance-1.jpg`.

**Failure / root cause:** The ordinary Agenda update failure was contained and
Dashboard committed its next parent ping, but the recovered Agenda displayed
`Today · appearance 0` while authority state still recorded generation 1.
`child-recovery-appearance-regression.json` records the healthy parent,
preserved `Design review` state, recovered child, parent ping 4, authority
generation 1, and stale child label. The matching PiKVM frame is 92,474 bytes
with SHA-256
`987c0523a8be3345784e9c4d648e0bbc47697f35686347dda2c175da6b63e789`.

The Linux host sent a graph-wide appearance command to the runtime worker but
returned before advancing its own cached `ExperienceModel`. A later provider
refresh correctly preserved the host's supposedly authoritative appearance,
which was still generation 0, and that full model refresh rerendered the
recovered subtree with the stale value. Authority state, graph revision state,
and failure containment were not corrupted.

**Changed / decision / verification:** Install every newer appearance profile
in the host model before dispatching it to a graph worker. Provider refreshes
therefore inherit the same generation, and duplicate or stale profiles remain
ignored. All 38 Linux-host library tests pass, including the new regression
that recreates the provider-refresh boundary. The reference live-graph and
self-contained-remix test also passes. Rust formatting and Git whitespace
checks pass.

**Remaining risks and next gate:** Commit, redeploy, and repeat the focused
appearance/failure/recovery sequence before a new clean end-to-end Framework
campaign. Timeout containment, application-host restart, Stock activation,
integrated input, logout, and final collection remain open; this diagnostic
campaign is not an acceptance result.

## 2026-08-27: Complete the exact Framework v4 composition diagnostic

**Goal and environment:** Close the full locked-composition Linux campaign on
the Framework Laptop 12 from exact clean revision
`4f93f50e9e559329eb6120ec5dcbeccfd530a1e7`. The complete development payload
deployed as `20260827T125525Z-4f93f50e9e55-3402102` in 119,126,621,512 ns.
PiKVM performed the SOS GDM selection, login, graphical observation, agent
prompt entry, and clean logout. A root-owned block inhibitor protected the
entire prepared campaign from idle, sleep, and lid-switch handling. This was
Fedora development-live boot `9b1818f2-c6c3-4829-8109-c9b3320a02a3`, so it
is intentionally promotion-ineligible and does not claim installed-product
acceptance.

**Physical composition evidence:** Stock launched locked Dashboard graph
`9a49ec819b8d0c83fa45566f16925498dbb95bd882bfc87d6b92526603e637f7`
through its semantic catalog action. The graph mounted independent Agenda
revision `0fa593bb5dc1f655dfeacad3ff2a7fc457c98807ecf6800a42eb9accdbc7967d`
and Media revision `8befbf5c1ebcde560edbc1e3254ff92c4e11bd86bf6ed1e015e484da4c2ff930`
inside Dashboard revision
`6ced86a21146c706b78851bf8278e797169d88d7a9296a62ea8adbaab265f9f0`.
Agenda durable state survived restart from the preceding attempt. Appearance
generation 1 to 2 committed through the authority in 6,256,369 ns and reached
child semantics in 405,414,126 ns without changing any revision ID; Media's
custom scene remained byte-identical.

An injected Agenda update failure was contained in 21,994,463 ns and recovered
after a parent event in 68,844,068 ns. An injected child timeout was contained
in 22,575,946 ns and recovered in 90,349,829 ns. Both paths preserved Agenda
state, kept Dashboard interactive, and restored appearance generation 2.
Killing Dashboard renderer PID 926078 left Stock renderer PID 925154 alive;
the application proxy and renderer restarted as PIDs 928138 and 928142 in
197,354,046 ns with both mounts, state, and appearance intact. PiKVM then
submitted `Make the workspace feel calmer` through Stock's resident agent.
Stock atomically advanced from revision `26f36f17…` to
`f39a6050cb0e8fb444beb7d4b1d7f62f76f992336688330c59ef8de5b2124479`
and presented graph
`f226d9795e5a6a5ca0a8f4d14f1df686fc569820aca6dc0d324712d5f7a427f0`;
the pointer-visible change took 335,181,758 ns. Dashboard then dismissed
through the v4 experience action.

The exact prepared inventory and compositor journal proved the integrated AT
keyboard, PIXA3854 touchpad motion and primary button, and ILIT2901 touchscreen
contact, alongside physical DRM page flips on both outputs. PiKVM performed the
clean logout and GDM recovery. Collection measured 1,320,441,618,564 ns from
prepare through logout. All hardware-gate criteria passed: same boot, recovery
page flip, direct compositor, session readiness, resident agent, all four input
classes, prepared inventory, clean logout, awake inhibitor, transactional
activation, multi-host lifecycle, durable authority, fallback display manager,
process failures, and kernel GPU faults.

**Evidence and decision:** The sealed bundle is
`.cache/evidence/linux-framework-4f93f50-campaign/`, 2,968,676 bytes including
the manifest. Its 8,696-byte 87-file manifest has SHA-256
`0be17dc236149c9755c93b755314f8950e52a2199be20eb1cd08f9dcbbd7e800`.
Both target-side and independent local `verify-manifest` and `audit` runs
passed. The exact result is
`DIAGNOSTIC_PASS promotion_eligible=false`; composition and recovery are green
on physical Framework hardware, while installed-product promotion remains a
separate gate.

## 2026-08-27: Build and inspect exact v4 Android Compat 1 and Core 1 artifacts

**Goal:** Produce immutable Samsung SM-A336B candidates from the same exact
`4f93f50e9e559329eb6120ec5dcbeccfd530a1e7` source used by the successful
Framework campaign, then run every offline package gate before touching the
device.

**Compat 1 evidence:** `build-compat1` produced build identity
`sos.compat1.4f93f50e9e55.a55ec0763af3`. `inspect-compat1` passed in
19,343,480,782 ns, covering the whole-package signature, compressed-data
integrity, PIT ceilings, AVB consistency, recovery device init, ARM64-only
native payload, native Compat ownership, system IME, UI-removal policy,
authority and resident-agent payloads, reference composition markers, and
`ro.sos.revision_format=4` / `ro.sos.experience_api=4`. The preserved OTA is
1,067,689,645 bytes with SHA-256
`ac4d46e40b38ea8b59a655ad1afefec7b11b5366d33e712925c87a9e9dd3e7f2`.
Its deterministic target-files archive is 2,173,516,615 bytes with SHA-256
`fd763645f3a2d9d5307a8f044974a2225626565bb23979d6e22e188a0fcf965d`.

**Core 1 evidence:** `build-core1` completed in 251,728,919,131 ns with build
identity `sos.core1.4f93f50e9e55.45ba3d420456`. `inspect-core1` passed in
17,245,525,725 ns, covering the package and AVB gates plus Core's pre-unlock
native host, pinned-model rejection, signed reference composition, no-Zygote
UI ownership, disabled user APK installation, and v4 format/API markers. The
preserved OTA is 1,022,880,554 bytes with SHA-256
`f70e1bfbc264187363d81829abfc7edf1fd60dc9db32b494a30acd5365c8dd33`.
Its deterministic target-files archive is 2,076,956,250 bytes with SHA-256
`9600dd6ec5db97b32c1c83493156afa273fc1af4a1a07b05326079d0d0595641`.
Logs, durations, identities, packages, and target files are under
`.cache/evidence/android-v4-4f93f50/` and remain outside Git.

**Decision and remaining gate:** Do not install either artifact while serial
`RFCT50EGFCN` is exposed only as Samsung USB Download Mode (`04e8:685d`) and
`adb` reports `no permissions`. No device mutation was attempted. Physical
Compat and Core acceptance, including exact revision readiness, composition,
properties/events, grants, appearance, restart/rollback, IME, accessibility,
input containment, and final evidence manifests, remains open until an
authorized Android or recovery transport exists.

## 2026-08-27: Remove the retired secondary Experience from v4 product inputs

**Goal:** Close the audit gap between the v4 registry/graph architecture and
the artifacts actually shipped to Android. Stock must be the sole resident
agent example, and the Cuttlefish product gate must exercise v4 graph
presentation rather than the legacy singleton revision pointer.

**Failure / root cause:** Linux had already retired the former secondary
Experience from its catalog, but its source and package remained checked in and
were still copied into every Compat and Core artifact as a second Pi prompt
example. Both Android agent launchers passed that file to the runner. The AOSP
`verify-sos` path also pushed it as a candidate and judged activation through
`/data/misc/sos/revisions/current`, bypassing the registry and graph authority.
The previously preserved `4f93f50` Android packages therefore remain valid
offline evidence for that revision but are superseded for final physical
acceptance.

**Changed:** Deleted the retired source and package; removed the secondary
prebuilt module, product package, stage input, inspector expectation, Java and
Core launcher arguments, and runner option. The deterministic fake agent now
generates a visibly distinct revision of Stock while preserving its v4 package
and `stock.theme` sidecar. The legacy source-swap stress helper likewise uses
Stock revisions only and remains rejected whenever a v4 graph is active.
`aospctl verify-sos` now reads content-addressed graph pointers, presents the
signed `sos.example.dashboard` graph through the lifecycle API, requires the
composition authority and rendered-frame confirmation to agree, and preserves
that graph across independent authority and HOME process recovery. Current
documentation and authoring examples now point to Stock-v4 or the signed
reference composition. Linux keeps only bounded removal tombstones for old
installed registry records and baked files; they do not package or present the
retired Experience.

**Evidence:** `cargo test --locked -p sos-experience` passes all 17 tests;
`cargo test --locked -p revision-supervisor --test graph_supervisor` passes all
15 graph lifecycle, journal, tracked-update, authority, and state tests. The
shared agent build passes all 19 TypeScript tests, including its one-example
prompt contract. `tests/a33xctl-host-test.sh`,
`tests/linux-login-session-test.sh`, and `tests/linux-live-image-test.sh` each
report PASS. `bash -n tools/aospctl tools/a33xctl`, Rust formatting, and Git
whitespace checks pass. No Android device command was issued while the Samsung
remained in unauthorized Download Mode.

**Decision / next gate:** Active product and authoring paths are v4-only; API
v3 remains only as the bounded activation/rollback reader required for pinned
recovery artifacts. Build and inspect fresh exact Compat 1 and Core 1 packages
from this cleanup revision, then run their physical campaigns when an
authorized transport is available. Remove the v3 reader only after those
recovery artifacts have migrated and successfully rolled back.

## 2026-08-27: Seal cleaned v4 Android Compat 1 and Core 1 candidates

**Goal:** Rebuild both Samsung SM-A336B product profiles from the cleanup
revision so the candidates reserved for physical acceptance contain Stock and
the signed reference graph, but no retired Timeflow/Daily Flow package or
secondary agent example. Preserve immutable OTA and deterministic target-files
artifacts before any device mutation.

**Compat 1 evidence:** Exact source
`884ab4e0036e621f51ba0a3a6c147a5c591da81e` produced build identity
`sos.compat1.884ab4e0036e.9f7afadd0450` in 384,330,589,843 ns.
`inspect-compat1` passed in 19,390,410,550 ns, including package signature,
compressed-data, PIT, AVB, product ownership, authority, resident-agent,
reference-composition, v4 format/API, and retired-secondary-absence checks.
The sealed OTA is 1,067,612,833 bytes with SHA-256
`1e438726c96dfbdfa067b43a7ae4662b7e6b91ff56f4f2693d5c95470f0406ec`.
The deterministic target-files archive is 1,863,975,871 bytes with SHA-256
`e8dc8d7d41ddbfe7d6459cc63d4f7290bc9f6e147914382aca1f504d0261a4f2`;
its zstd integrity and absence of the retired secondary input pass.

**Core 1 evidence:** The same source produced build identity
`sos.core1.884ab4e0036e.1df2554bb5c5` in 373,140,225,319 ns.
`inspect-core1` passed in 17,584,174,377 ns, including the common artifact
checks plus pre-unlock host, pinned-model rejection, signed composition,
no-Zygote ownership, disabled user APK installation, v4 format/API, and
retired-secondary-absence gates. The sealed OTA is 1,022,887,729 bytes with
SHA-256
`0b0ce039e306c42292cd67eb6be19f15ee753f7ab2ff6b4059247210af4f17fb`.
The deterministic target-files archive is 1,782,249,070 bytes with SHA-256
`49ba5c7ace4df8bcd22605f2334da0fa34229b7fd227bf1f657b7f8785a1b863`.
Its 9,657-entry archive passes zstd integrity, contains the Stock package and
theme module, and contains no Timeflow, Daily Flow, or secondary example.

**Artifacts / failure / decision:** Logs, exact monotonic durations,
identities, OTAs, target-files archives, and verification transcripts are
under `.cache/evidence/android-v4-884ab4e/` and remain outside Git. The older
`.cache/evidence/android-v4-4f93f50/` pair is superseded because it predates
secondary-input removal. At 2026-08-27T16:00:09+02:00, serial `RFCT50EGFCN`
still reported `no permissions` through adb and USB `04e8:685d` Download Mode.
After every artifact and transcript was finalized, `a33xctl` generated and
independently verified the complete 17-file evidence manifest in
6,115,010,906 ns. The 1,775-byte manifest is
`.cache/evidence/android-v4-884ab4e/manifest.sha256`, SHA-256
`a8f056b6d8c77d56b2ed6b4f297802b5b726a3f4200573ab43859114690f7b93`;
an independent path-set comparison found no unmanifested file in its root.
No install, flash, reboot, or other device mutation was attempted. These are
offline-candidate passes, not physical-product verdicts. The next gate is the
exact Compat then Core device campaign once an authorized Android or recovery
transport exists; retained API v3 recovery activation cannot be removed until
that campaign proves migration and rollback.

## 2026-08-27: Audit every v4 composition milestone against its exit gate

**Goal:** Reconcile the implementation with the original milestones 0 through
12, run the broad cross-component verification program from the current
product tree, and distinguish missing code from physical gates that cannot be
claimed on desktop evidence.

**Changed:** Added an explicit milestone closure matrix to the composition
design. Repaired two `gpui-mobile` documentation examples: the platform-view
element now imports the types and GPUI traits it uses, and the registry example
accepts a real `PlatformViewFactory` instead of referring to an undefined
placeholder. No runtime or product behavior changed. A complete source audit
finds every built-in package and every active authoring path on v4. Remaining
API v3 literals are bounded migration, rollback, rejection, decoder, and test
fixtures; no v3 package is shipped or newly authored.

**Verification:** `cargo test --workspace --locked` passed 278 tests in
9,921,878,687 ns after the documentation repair. The 94 ordinary ignored tests
are 93 upstream-style GPUI examples plus the deliberately explicit performance
campaign. Running the Linux graph worker gate with `linux-host` passed its
process-crash-containment test in 6,482,634,425 ns. The explicit release
composition campaign passed in 21,230,414,230 ns and measured three Instances:
1,634 us cold mount readiness, 725 us child-event propagation, 382 us
appearance propagation, 1,139 us graph activation, 985 us committed recovery,
and 292 KiB coarse RSS delta per Instance. `services/sos-agent` passed all 19
build, bounded-authoring, derivation, composition, canonical-wire, and prompt
tests in 2,958,087,282 ns. The five a33x, Linux login/live-image/hardware, and
PiKVM host suites all passed in 7,037,440,280 ns.

**Failure / decision:** The first workspace run reached all product tests but
failed the two incomplete documentation examples; the corrected examples and
the full unchanged workspace then passed. Milestones 0 through 10 and the
implementation portion of 12 satisfy their exit conditions. Milestone 11's
shared Android implementation and offline products are complete, but its
physical exit gate remains open. The connected SM-A336B is still in Download
Mode, whose Odin session cannot be converted into the authenticated Recovery
sideload path from this host; previous evidence establishes that a physical
Side + Volume Down restart is required. No blind flash or device mutation was
attempted.

**Remaining risks and next gate:** Physically restart the SM-A336B out of
Download Mode, then run the exact sealed Compat 1 campaign followed by Core 1,
including input, IME, accessibility, appearance, grants, child containment,
restart, rollback, and independently verified manifests. Only after the
cleaned recovery artifacts migrate and roll back successfully may the bounded
v3 reader be removed. Installed Linux promotion remains a separate physical
product gate; the Framework development-live composition diagnostic is already
green.

## 2026-08-27: Add a v4 Cuttlefish lifecycle verifier regression

**Goal:** Exercise the rewritten `aospctl verify-sos` contract after removal of
the retired secondary Experience. The verifier must use stable Experience graph
pointers, present the signed Dashboard graph, bind authority state to confirmed
rendering, and distinguish authority recovery from HOME recovery.

**Changed:** Added a stateful explicit-serial adb fixture and
`tests/aospctl-host-test.sh`. The fixture models booted x86-64 SOS HOME,
enforcing SELinux domains, Stock and Dashboard content-addressed graph
pointers, composition authority state, confirmed graph logs, init-owned
authority recovery, and Android-owned HOME recovery. The positive case proves
Stock-to-Dashboard presentation without replacing the permanent GPUI process,
then preserves Dashboard through both independent restarts. It records every
transport command and rejects any query of `/data/misc/sos/revisions/current`.
The negative case deliberately changes the authority PID during HOME-only
recovery and requires the verifier to fail closed.

**Evidence:** `tests/aospctl-host-test.sh` reports
`aospctl_host_test=PASS`. Bash parsing passes for the verifier, fixture, and
test. The complete six-script AOSP, a33x, Linux login/live-image/hardware, and
PiKVM host set passed in 7,212,432,794 ns. No physical adb command is routed
through this regression: its adb binary is an explicit temporary path and its
serial is the Cuttlefish loopback endpoint.

**Failure / decision / next gate:** A real Cuttlefish rerun is unavailable on
this workstation because `~/dev/aosp-sos` is absent and the development volume
has 306 GiB free, below the harness's enforced 400 GiB source/build floor. Do
not delete unrelated data or weaken that resource gate. Retain the host
regression and run the real product when a provisioned AOSP workspace is
available. This improves Android lifecycle evidence but cannot replace the
still-open SM-A336B physical campaign; the phone remains in Download Mode and
requires its established physical key restart before Compat installation.

## 2026-08-27: Share and cross-check composed scenes across Linux and Android

**Goal / changed:** Remove the last platform copy of the scene-composition
transform before installing the cleaned Compat and Core candidates. Linux,
Compat, and Core now call one pure transform that prefixes node IDs with the
owning Instance ID, attaches a ready child only to the matching declared
dependency mount, and leaves a failed child's mount empty for the host-owned
fallback. The host regression deliberately gives Dashboard and Agenda the
same raw action ID, mounts Agenda, injects a failed Media sibling, and requires
all composed IDs to remain unique and Instance-prefixed.

**Failure / fix:** The first ARM64 `gate-strict` check exposed a separate
configuration regression. Plain non-AOSP builds still compiled appearance,
top-level lifecycle, and reference-event deep-link branches whose queues exist
only in the AOSP product. The shipping `aosp-system` and `core-native` checks
were already green. Non-AOSP builds now reject those product-only links with
an explicit log entry, while Compat retains the real queue behavior.

**Evidence / decision:** `cargo test --locked -p sos-experience` passes all 18
default tests. The `linux-host` library passes all 38 tests in 2.85 seconds.
ARM64 Android release checks pass for plain `gate-strict` in 0.71 seconds,
Compat `aosp-system` in 0.76 seconds, and `core-native` in 0.83 seconds. Keep
one composition implementation for all hosts. Rebuild and inspect the exact
Android candidates after this source change, then run the physical Compat and
Core campaign when the SM-A336B exposes an authorized recovery or Android
transport.

## 2026-08-27: Make Android v4 composition physically auditable

**Goal:** Close the missing procedure and user-reachable control surface for
the physical Compat/Core composition gate. The result must judge the complete
v4 graph, not a boot screenshot or a collection of unbound adb transcripts.

**Changed:** The AOSP Android host now owns a fixed `Theme`, `Rollback`, and
conditional `Home` strip above Experience content. It advances appearance
through the registry-authorized Stock identity, stages rollback through the
authority, and returns an independently presented ordinary root to Stock
through the normal lifecycle graph. Ordinary Luau receives neither those
actions nor their authority. Fixed host IDs expose the controls through the
Compat virtual accessibility tree and prevent Experience nodes from spoofing
their actions.
The mounted Agenda reference export now includes a text
session so physical IME evidence names the child Instance instead of an
unrelated Stock field. `tools/a33xctl` adds ordered
`capture-v4-composition-stage` and `audit-v4-composition-campaign` commands for
Stock, locked Dashboard, appearance, child update failure, child timeout,
recovery, mounted IME/accessibility, independent host and authority restart,
v4 Stock authoring, and v4-to-v4 rollback. Every checkpoint binds the exact
product, serial, revision and artifact digest and captures monotonic timing,
SELinux, processes, surfaces, authority state/grants, screenshot, logs,
per-process memory, and product readiness or accessibility. The final audit
checks state/grant isolation, structured child events, failure containment,
appearance without revision churn, namespaced IME, independent PID recovery,
authoring and rollback revisions, then generates and verifies its manifest.

**Failure / decision:** The existing Android deep links were Compat-only, so
Core had no user-reachable appearance, dismissal, or rollback path after an
ordinary Experience replaced Stock. A proposed worker-restart checkpoint
would also have required a test-only Core backdoor. The campaign instead uses
shipping host controls and independently restarts the real host child and
authority. The first audit draft treated generic Stock IME logs as composition
evidence; the mounted Agenda input and Instance-prefixed log requirements
replace that shortcut. PID sets are normalized before comparison, Core memory
comes from each `/proc/<pid>/status`, and audit is offline-repeatable rather
than depending on the device still being attached.

**Evidence:** `tests/a33xctl-host-test.sh` passes complete mock Compat and Core
campaigns, independently verifies both manifests, and rejects a campaign in
which the authority changes during host-only recovery; its measured wall time
is 6.92 seconds with 4,900 KiB peak host RSS. Bash parsing and `git diff
--check` pass. Focused `revision-supervisor`, `runtime-luau`, and
`sos-experience` suites pass 100 tests; the one explicit performance campaign
remains intentionally ignored. The `linux-host` profile separately passes 38
library tests plus its process-boundary integration test. ARM64 release checks
pass for plain `gate-strict`, Compat `aosp-system`, and Core `core-native`.
The complete a33xctl, aospctl, Linux login/live-image/hardware, and PiKVM
six-script host set passes in 14.14 seconds with 708,684 KiB peak host RSS.

**Remaining risk / next gate:** The previous `884ab4e` cleanup artifacts are
superseded by these source changes. Build, inspect, and seal final exact Compat
and Core artifacts, then run the staged campaign on SM-A336B serial
`RFCT50EGFCN`. The phone still needs an authorized Android or Recovery
transport before installation: at 2026-08-27T17:09:06+02:00, adb still reports
`no permissions` and USB ID `04e8:685d` still reports Download Mode. Retain the
bounded v3 rollback reader until the physical campaign has migrated Stock and
proved rollback between two v4 revisions; only then remove it and rebuild the
final no-v3 products.

## 2026-08-27: Seal exact Android v4 composition candidates

**Goal / changed:** Rebuild the Compat 1 and Core 1 products from the complete
physical-campaign implementation and seal the exact install inputs before any
Samsung mutation. Both builds use clean source
`cfe4ebb63eb3b7ffc9bf72c95a25f33152e1314c`. Compat identity
`sos.compat1.cfe4ebb63eb3.a6a42402ae5b` built in 269.80 seconds with
2,974,452 KiB peak RSS. Its offline inspector passed in 19.29 seconds with
47,512 KiB peak RSS. Core identity
`sos.core1.cfe4ebb63eb3.4d984b84b044` built in 260.94 seconds with
2,978,176 KiB peak RSS. Its offline inspector passed in 17.27 seconds with
47,860 KiB peak RSS. Both inspectors require revision format 4 and Experience
API 4, preserve `ro.sos.legacy_revision_read=3` only for the reversible
rollback window, and pass the v4 system-control, graph, signature, AVB,
ownership, and retired-secondary-absence checks.

**Artifact evidence:** Compat produced
`.cache/evidence/android-v4-cfe4ebb/compat1/compat1.ota.zip`, 1,067,699,297
bytes, SHA-256
`3f70274838d07d2aedeeea820b1bab549f628ed9032cc956e31c1a5bb07e1144`.
Its deterministic 9,673-entry target-files archive is 2,173,677,658 bytes,
SHA-256
`dd80b1332c37e3a385551e70da4b829d938ef1a3967dda0bc09582b76f97c631`;
archive creation took 3.88 seconds with 113,524 KiB peak RSS. Core produced
`.cache/evidence/android-v4-cfe4ebb/core1/core1.ota.zip`, 1,022,859,688 bytes,
SHA-256
`c183cba1d9cbc71d19ef91a960716c6a621b3272bb075ed3c016a498c868c454`.
Its deterministic 9,657-entry target-files archive is 2,077,042,185 bytes,
SHA-256
`95dcb14c5287fc33efb0006431559a81ad107901042cb976bcc5e2e0b29103ae`;
archive creation took 3.54 seconds with 114,052 KiB peak RSS. `zstd -t` and a
complete tar listing pass for both archives. Each contains Stock and its theme
and contains no Timeflow, Daily Flow, or retired example-secondary product.

**Failure / decision:** The first Core archive attempt used Compat's
`-target-files` directory spelling. Core actually generates
`lineage_sos_core1_a33x-target_files`; tar rejected the missing input and left
a 22-byte invalid output. No product input was altered. Replacing that output
from the resolved Core directory with the same sorted, epoch-zero,
numeric-owner archive recipe produced the verified artifact above. Keep the
variant-specific source path explicit rather than normalizing a path that the
Lineage products do not share.

**Manifest and transport evidence:**
`./tools/a33xctl evidence-manifest-generate --root
.cache/evidence/android-v4-cfe4ebb --output
.cache/evidence/android-v4-cfe4ebb/manifest.tsv` recorded 22 finalized files;
`evidence-manifest-verify` independently passed. The 2,129-byte manifest has
SHA-256
`697ae62cc3f66211eebb2e95e97decd4fef34ec4935fe7ede6784c5274939bc8`.
At 2026-08-27T17:24:36+02:00, serial `RFCT50EGFCN` still reported adb `no
permissions` at USB path `1-1.1`, and `lsusb` identified `04e8:685d` Download
Mode. `.cache/evidence/android-v4-cfe4ebb/transport-status.txt` records that
read-only observation. No phone mutation was attempted.

**Decision / next gate:** These are accepted offline candidates, not physical
product verdicts. When the Samsung exposes authorized Android or Recovery,
install the exact Compat OTA and run its complete staged v4 composition
campaign, then repeat with the exact Core OTA. Keep the bounded v3 rollback
reader through that campaign so Stock can migrate and prove v4-to-v4 rollback;
then remove the reader, rebuild, and rerun the relevant package and rollback
checks for the final no-v3 products.

## 2026-08-27: Reject headless Compat ADB authorization and add trusted SOS consent

**Goal / physical evidence:** Begin the sealed Compat campaign without erasing
the installed migration source. Sysfs identified USB `04e8:685d` as an
SM-A336B ADB interface despite the host `usb.ids` label “Download Mode.” After
the device node received an ACL, the installed Core reported revision
`sos.core1dev.e05f91bb6f0b.0be956df8e63`, profile `core`, revision format 3,
Experience API 3, and enforcing SELinux. This is the intended legacy import
case. A fresh inspection of the exact sealed Compat candidate passed in 18.61
seconds after its hashed target-files archive and OTA were restored to the
variant-specific build paths without changing their bytes.

Recovery did not preserve `adb reboot sideload-auto-reboot` from that legacy
Core build: it reached the “No command” recovery screen, exposed no sideload
transport, and the bounded 170-second host wait stopped without sending OTA
bytes. A plain recovery reboot followed by physical “Apply update / Apply from
ADB” produced the expected `18d1:d001` sideload endpoint. The exact
1,067,699,297-byte Compat OTA, SHA-256
`3f70274838d07d2aedeeea820b1bab549f628ed9032cc956e31c1a5bb07e1144`,
then transferred exactly once in 86.82 seconds with `Total xfer: 1.00x`; no
wipe or second transfer occurred. Evidence is under
`.cache/evidence/android-v4-cfe4ebb-physical/compat1/`, including the
5,252-byte sideload log with SHA-256
`901d1d686b5f88ff4c50796fbf8de2b47e6b81ec0e06aaffd27b118708611dc4`.

**Failure / root cause:** The installed Compat candidate booted and exposed
ADB but remained `unauthorized`. The product retains `ro.adb.secure=1` while
intentionally removing SystemUI; Android's default key-confirmation component
still pointed at the absent SystemUI APK. Compat's system-Activity membrane
would also have blocked an arbitrary replacement privileged Activity. This is
a product defect, not a transport timeout, so the sealed `cfe4ebb` Compat OTA
is rejected as a physical candidate. Automatically accepting a new host key
or injecting one through recovery was rejected because either path bypasses
physical user consent.

**Changed / decision:** `SosFrameworkBridge` now contains one fixed,
platform-signed ADB consent Activity. The Compat framework overlay binds both
owner and secondary-user confirmation resources to that component. The
surface validates bounded key input, hides untrusted overlays, requires the
owner profile to be unlocked, distinguishes Allow once from persistent trust,
offers an explicit denial path, denies disconnect/close, and never logs key or
fingerprint material. `MANAGE_DEBUGGING` protects the component and is
privapp-allowlisted. The framework membrane exception names this exact class;
all other Android system Activities remain blocked. The artifact inspector now
checks the compiled resource binding, single-Activity manifest, permission
boundary, membrane exception, lock/owner guards, and consent actions.
`tests/a33xctl-host-test.sh`, Bash parsing, patch reverse-application, and
`git diff --check` pass.

**Remaining risks / next gate:** Compile and inspect a superseding Compat
artifact, install it from Recovery, physically exercise Deny, Allow once, and
Always allow, and then run the full composition campaign. The workstation's
existing Samsung/Google USB udev rules still fail to grant remote-session ACLs
for `04e8:685d` and `18d1:d001`. The new root-only
`install-host-usb-rules` command installs an exact-PID, group-scoped 0660 rule,
reloads udev, and retriggers current nodes; its source contract is covered by
the a33x host suite. Run it once before the next install so device ownership no
longer requires repeated manual ACL repair. Core remains unmodified and
uninstalled. The v3 reader remains bounded to this migration window.

## 2026-08-27: Build and seal the trusted-consent Compat candidate

**Goal / evidence:** Compile the complete trusted ADB-consent change from
clean repository revision `f19c430223a83ecef60f4ac787613078432e8dcd`
before returning to Recovery. The end-to-end ARM64 Compat build passed in
334.33 seconds with 2,978,056 KiB peak RSS; Soong's product phase reported
4:49. The exact identity is
`sos.compat1.f19c430223a8.2e0169180ab5`. The strict offline inspector passed
again from the finalized outputs in 18.87 seconds with 48,272 KiB peak RSS,
including the compiled single-Activity bridge manifest, both framework
resource bindings, `MANAGE_DEBUGGING` boundary, exact framework membrane
exception, consent/lock markers, whole-package signature, PIT ceilings, AVB
hashes and hashtrees, v4 graph controls, API/format 4, and retired-secondary
absence.

**Artifacts / failure:** The sealed OTA is
`.cache/evidence/android-v4-f19c430/compat1/compat1.ota.zip`, 1,067,659,231
bytes, SHA-256
`d91704446867ca51b95d392a6b1f4a7056e9247d2b8d9280e7fdaf73e758b4e2`.
The deterministic 9,673-entry target-files archive is 2,173,369,839 bytes,
SHA-256
`33b2672cc0fa4c63bf4a6560f1ea7c55b2e0cb3634249d4923af73713ad1cc4b`;
zstd integrity and complete listing pass, Stock package/theme are present,
and the retired secondary is absent. The first archive command reused the
older hyphenated directory spelling, so tar rejected its nonexistent input
and left a 22-byte output. That exact failed output was removed and the
resolved Soong `_target_files` directory was archived; no product input or
sealed OTA changed.

The complete ten-file candidate manifest was generated and independently
verified. `.cache/evidence/android-v4-f19c430/manifest.tsv` is 978 bytes with
SHA-256
`9ca94f833d623513f308e0caeb9c9257146bc6078ea58c1fd7ccd6feff21b93e`.
This is an offline candidate pass, not a physical verdict.

**Decision / next gate:** Install the durable host USB rule once, manually
enter Recovery because the currently installed rejected build cannot accept
ADB, and sideload this exact OTA once. Physical acceptance begins only after
the fixed surface visibly denies an untrusted request and then accepts the
known workstation key through explicit user input. Continue the ordered Compat
composition campaign only on that verified identity.

## 2026-08-27: Prove durable remote ownership of A33x USB nodes

**Goal / environment change:** Remove the repeated manual `setfacl` step from
the Samsung acceptance lifecycle. The owner installed the repository rule with
`install-host-usb-rules --group wheel`. The root-owned mode-0644 installed
file is `/etc/udev/rules.d/70-sos-a33x-usb.rules`, 598 bytes, SHA-256
`fc5a2b0e69ead827004cee98566d137fdddbf7db24fc0629d55b067d3600e72d`.
`udevadm test` selects its exact `04e8:685d` entry, resolves group `wheel`, and
sets mode 0660 while retaining the desktop `uaccess` tag.

**Evidence / decision:** The current `/dev/bus/usb/001/089` node changed to
`root:wheel` mode 0660 with no named per-user ACL. A remote process can open it;
adb now reports the expected `unauthorized` state from the installed rejected
Compat candidate rather than `no permissions`. The old setup appeared to work
because it reused an authorized `04e8:6860` node or a desktop-seat ACL. SOS's
ADB-only `04e8:685d` gadget and Recovery's `18d1:d001` endpoint were not both
covered, and each reenumeration discarded the ephemeral node ACL.

A bounded `adb -s RFCT50EGFCN wait-for-sideload` completed without observing a
Recovery endpoint; the phone remained on `04e8:685d`. No sideload, reboot,
wipe, or other phone mutation occurred. The next gate remains the physical
Side + Volume Down restart, immediate Side + Volume Up Recovery entry, and
selection of Apply update / Apply from ADB. The host rule is now ready for that
new node without another ACL repair.

The first physical restart attempt subsequently returned to the rejected SOS
Compat image instead of Recovery. A second, explicitly measured
`wait-for-sideload` ran for 300.00 seconds and exited 124 with 5,372 KiB peak
RSS; USB remained the same booted `04e8:685d` endpoint. No OTA bytes or device
commands were sent. The next retry may use either the fixed lock surface's
Volume Up+Down recovery chord after a Side off/wake cycle, or the established
Side+Volume Down then immediate Side+Volume Up boot sequence.

## 2026-08-27: Bake and mount the revision 7c414374 development-live ISO

**Goal / environment:** Build the newest clean Linux development-live image
available to the bake, preserve private Wi-Fi autoconnect and password access,
then replace the PiKVM virtual media without booting or writing an installed
disk. The checksum-pinned Fedora 44 source ISO was 2,851,612,672 bytes with
SHA-256 `1620295f6a00c27c3208f0c00b8ece4eab1ec69b9002152d97488bf26a426ddf`;
its signed checksum verified against Fedora 44 primary-key fingerprint
`36F612DCF27F7D1A48A835E4DBFCF71C6D9F90A6`. The rootless doctor passed.

**Build evidence:** The worktree advanced from `cfe4ebb` to clean revision
`7c414374e450733c6541e1e88a70dbe94c15c1bc` before source identity capture.
The staged install metadata, rootfs check, image sidecar, and final filename all
agree on `7c414374e450`. The 1:03:37 rootless bake passed rootfs validation,
EROFS repack, ISO replay, and the embedded Fedora media check. The private
inputs remained mode 0600 and were removed after the finalized bake. The ISO
is `artifacts/development-live-7c41437/sos-development-live-7c414374e450.iso`,
3,057,975,296 bytes, SHA-256
`e2f5c5e9dd6315558e515dfdc25293b0db56e540c700175127f0fe1b4a5b3fad`.
Its identity records `development-live`, `promotion_eligible=false`,
`wifi_autoconnect=true`, and `network_credentials_embedded=true`.

**PiKVM evidence / decision:** PiKVM at `192.168.1.47` first reported the old
`28cf8ff` ISO connected read-only. It disconnected that drive before upload.
The first multipart request failed immediately with HTTP 400 and transferred
no image bytes; the documented binary-body request then uploaded the exact ISO
in 2:15.96. PiKVM reported the stored image complete and non-writable at the
expected byte size. A full 1:34.78 API read-back produced the same SHA-256 as
the local artifact. Final state selects the exact `7c414374` image with
`connected=true`, `cdrom=true`, `rw=false`, and `writable=false`. ATX remains
retired because earlier calibration proved its telemetry unreliable. The
inspected console frame was black, so target power and boot state are not
claimed. No HID, ATX, installer, boot-order, internal-disk, or target SSH
mutation occurred.

**Remaining risk / next gate:** This accepts the image build and read-only
PiKVM mount only. The ISO embeds a network credential and must remain private.
Physical boot still requires an observed one-time firmware selection because
remote ATX and the boot-menu window remain unreliable. After boot, verify the
live overlay identity, Wi-Fi, SSH, SOS session, and that every internal NVMe
partition remains unmounted before claiming a physical runtime result. Raw
build, upload, read-back, MSD-state, and console evidence is under
`.cache/evidence/pikvm-live-7c41437/`.

## 2026-08-27: Reject bridge-owned ADB UI and split consent presentation from privilege

**Goal / physical evidence:** Install and exercise the superseding
`f19c430` Compat candidate's trusted ADB consent surface. Recovery exposed the
durably group-owned `18d1:d001` sideload endpoint. The candidate manifest and
artifact digest passed immediately before one transfer. The exact
1,067,659,231-byte OTA, SHA-256
`d91704446867ca51b95d392a6b1f4a7056e9247d2b8d9280e7fdaf73e758b4e2`,
transferred once with `Total xfer: 1.00x` and exit 0 in 87.21 seconds. The
4,456-byte finalized log at
`.cache/evidence/android-v4-f19c430-physical/compat1/install/sideload.log`
has SHA-256
`3ef939c00174eee4a59534d39a84a6df649d05c44cb918f6af5fceef2369dec0`.
Recovery returned to its main menu, the user selected reboot, and the new
Compat product re-enumerated as the expected `04e8:685d` ADB-only gadget.

**Failure / diagnosis:** The bridge-owned consent Activity did not become
visible. Five fresh authorization checks after restarting the host ADB server
and ten more after a physical USB reset all remained `unauthorized`. Offline
inspection reconfirmed that the installed candidate compiled both overlay
resources to the intended component and included the enabled, exported,
direct-boot-aware bridge Activity with `MANAGE_DEBUGGING`; this rules out a
stale package or source-only manifest assumption but does not prove that the
shared-system-UID Activity reached presentation. The first `usbreset` attempt
passed the full device-node path, which Fedora's implementation rejected as
“No such device found.” Retrying its required `001/094` syntax succeeded and
did not change the result. No authorization, wipe, or second OTA transfer
occurred. The `f19c430` physical candidate is rejected.

**Changed / decision:** Presentation now lives in the already proven,
platform-signed SOS HOME package, while privilege stays in the headless
framework bridge. Android's custom ADB confirmation resources name the fixed
`SosAdbConfirmationActivity` in `dev.sos.experience`. That Activity is not
Luau-visible, validates bounded owner/unlock/key/fingerprint input, suppresses
untrusted overlays, keeps the display awake, and offers Deny, Allow once, and
Always allow. It sends only an explicit result to a bridge receiver protected
by the new signature permission `dev.sos.permission.REPORT_ADB_CONSENT`; the
receiver alone calls `IAdbManager`. The framework membrane returns to its
package-only SOS HOME exception, and the bridge exposes no Activity. The
artifact inspector now verifies the compiled HOME Activity, permission and
overlay binding, headless bridge receiver, signature boundary, consent markers,
and framework membrane. `tests/a33xctl-host-test.sh`, Bash parsing, patch
reverse-application, and `git diff --check` pass.

The first public-SDK Java compile rejected `UsbManager.ACTION_USB_STATE` and
`USB_CONNECTED`, which exist in the platform build API used by the earlier
bridge implementation but not in Android's application stubs. The HOME
surface now binds the stable framework action and boolean-extra strings
locally; it does not acquire a hidden-API dependency. The repeated release
Java compile passes.

**Open risks / next gate:** Build, inspect, and seal this second superseding
Compat candidate before another Recovery entry. Its first physical gate is
visible denial, followed by session-only and persistent authorization of the
known workstation key. The complete v4 composition campaign remains pending;
the bounded v3 reader remains only for that reversible migration gate.

## 2026-08-27: Split Android into a first-class Stock Mobile experience

**Goal:** Stop treating the phone as a responsive rendering of the Linux Stock
Shell. Mobile application lifecycle, full-screen presentation, launcher
information architecture, touch geometry, safe compact chrome, and vertical
viewport use are experience semantics and require an independent identity.

**Changed:** Added the pinned `sos.stock.mobile` v4 package, a new Luau source,
and the revision-local `mobile.theme` sidecar. Stock Mobile has its own top bar,
bottom navigation, large touch targets, vertically scrolling Today, Apps,
Agent, and Controls screens, a source-owned launcher for registered SOS
Experiences and compatible applications, and full-screen root lifecycle
effects. It contains no desktop `window_space`, window list, command rail,
floating/tiling policy, hover interaction, or shell overlay. Linux retains
`sos.stock.shell`, `default.luau`, and `stock.theme` unchanged.

The duplicate Java-owned Compat workspace and attention screens were removed.
The fixed Compat chrome now stays hidden while Stock Mobile owns focus and is
shown only over a selected foreign Android application. Its Apps and Attention
buttons return through a bounded `sos://mobile/navigate/{apps,controls}` handoff;
the native host restores Stock Mobile first when another Experience owns the
root, then dispatches the corresponding source-defined navigation action.
Staging also deletes obsolete Linux Stock Shell prebuilts, and artifact
inspection rejects their presence in an Android product image.

The registry now reserves and prevents retirement of both platform Stock IDs
while preserving separate current/previous pointers. Legacy migration accepts
an explicit reserved target, so Linux imports old state under Stock Shell and
Android imports it under Stock Mobile. The Android authority derives its
active/recovery identity from its immutable bootstrap, requires exactly
`sos.stock.mobile` for v4 products, keys state and grants to that ID, returns to
it after dismissing an ordinary root, and accepts appearance writes only from
that exact pinned identity. Existing Android Stock Shell records from rejected
development candidates remain dormant and untouched rather than being
silently rekeyed. The Android host's deterministic authoring seed, candidate
module validator, A33x and Cuttlefish products, init command, agent example,
artifact inspectors, composition audit, and mock gates now use Mobile source,
package, theme, and identity. Android product images no longer package the
Linux default source.

**Evidence / failures:** Focused `revision-supervisor`,
`android-system-authority`, and `sos-experience` suites pass 95 tests with one
explicit performance campaign ignored. The new runtime test renders Today,
Apps, and Agent branches, finds the mobile top and bottom bars and full-screen
Experience launch action, requires the agent composer, and rejects accidental
desktop chrome. `runtime-luau` validates the default, launcher, agent, and
controls scenarios (73/30/31/50 nodes respectively), including the one mobile
text session. The checked-in package now passes the Rust package validator as
the reserved Stock Mobile Shell contract. Both the A33x and AOSP/Cuttlefish
host suites pass after their state/grant fixtures moved to `sos.stock.mobile`;
Bash parsing, Rust formatting, and `git diff --check` pass. A release ARM64
Compat APK build completed in 11.26 seconds and its DEX contains the mobile
navigation and Stock-owner chrome-hiding markers while omitting both deleted
Java screen classes.

The first integrated test compile failed only because the new Rust test
referenced helper functions scoped inside an older test; moving reusable scene
predicates to module scope fixed it. The first repeated A33x host test then
failed because its new static checks used `repo` instead of the existing
`repo_root`; correcting the variable made both host gates pass. Neither failure
changed product behavior.

**Decision / next gate:** Platform Stock identity is part of the v4 wire and
registry model, not a theme or viewport switch. Build the second superseding
Compat OTA with Stock Mobile and the HOME-owned ADB consent repair, inspect its
compiled bootstrap identity and source/theme absence/presence invariants, then
install it once. Physical acceptance must show that the phone fits its panel,
the launcher and navigation are touch-first, ordinary experiences and
compatible applications occupy the full root, and returning Home restores
Stock Mobile. Real cutout/inset behavior and orientation remain hardware gates;
desktop tests do not close them.

## 2026-08-27: Seal the first Stock Mobile Compat candidate

**Goal / build:** Produce the exact Android candidate that combines the
HOME-owned ADB consent repair with the independent `sos.stock.mobile`
experience. Clean source revision
`a7dba1080c6337e10218d60c97febb381feead19` built Compat 1 successfully in
5:58. Its immutable product identity is
`sos.compat1.a7dba1080c63.f384ef7bfb46`. Soong installed `mobile.luau`,
`mobile.package.json`, and `modules/mobile-theme.luau` while explicitly
removing the Linux `default.luau`, `default.package.json`, and
`modules/stock-theme.luau` prebuilts.

**Offline evidence:** `./tools/a33xctl inspect-compat1` passed in 19.48 seconds
with 47,660 KiB peak RSS. It verified the complete package signature, PIT
ceilings, boot-chain AVB data, recovery packaging, the compiled HOME consent
Activity and headless bridge, v4 package/API markers, registry and authority
markers, and the Stock Mobile source contract. The inspector also confirmed
`ro.sos.revision_format=4`, `ro.sos.experience_api=4`, and the bounded
rollback-only v3 reader. This remains an offline candidate result.

The sealed OTA is
`.cache/evidence/android-v4-a7dba10/compat1/lineage-23.0-20260827-UNOFFICIAL-sos_compat_a33x.zip`,
1,067,649,751 bytes, SHA-256
`911e2ec4c5a49e374abbac9c4f58d3c82261ff4a4e92df4e0feb812d23e8b4b8`.
The 9,673-entry deterministic target-files archive is 1,863,732,829 bytes,
SHA-256
`96c2402936c8d428009bfa35e1736b2799fe291f9d9d38c2c5dadc591f0146d4`.
Its zstd integrity passes. It contains all three Mobile inputs and none of the
three Linux Stock Shell inputs. Archive creation took 117.23 seconds with
2,390,888 KiB peak RSS.

The finalized nine-file manifest at
`.cache/evidence/android-v4-a7dba10/manifest.tsv` is 928 bytes, SHA-256
`1d8ff51f75a2705e9254de461afc96c78640f74959f506f916037b6fcc6bd828`.
Independent manifest verification passes.

**Decision / next gate:** This exact OTA is the only candidate authorized for
the next Recovery transfer. First prove that a fresh workstation request
shows the fixed consent surface and that Deny leaves ADB unauthorized. Then
exercise Allow once and Always allow with the known workstation key before
running the Compat composition, full-screen layout, touch, inset, restart,
rollback, memory, thermal, crash, and AVC campaign. No physical claim is made
until those checks run on product identity
`sos.compat1.a7dba1080c63.f384ef7bfb46`.

## 2026-08-27: Reject the first physical Stock Mobile layout and repair its owning boundaries

**Goal / physical evidence:** Install the sealed `a7dba10` Compat candidate,
close the HOME-owned ADB consent gate, and begin the ordered v4 composition
campaign on SM-A336B `RFCT50EGFCN`. The exact 1,067,649,751-byte OTA with
SHA-256 `911e2ec4c5a49e374abbac9c4f58d3c82261ff4a4e92df4e0feb812d23e8b4b8`
was reverified immediately before installation. Its single Recovery transfer
completed with `Total xfer: 1.00x`, exit 0, and 86.94 seconds wall time. The
6,855-byte sideload log at
`.cache/evidence/android-v4-a7dba10-physical/compat1/install/sideload.log` has
SHA-256 `71b9215ccb3d1aee40488f61a55d8a1014c695ba6a01a0d1f43f2350c3a8f99f`.
The device booted exact product
`sos.compat1.a7dba1080c63.f384ef7bfb46` with boot-complete, Compat, package
format 4, and Experience API 4 markers.

The consent surface passed Deny and Allow once with a temporary workstation
key. Deny left the transport unauthorized. Allow once authorized the live
daemon, then restarting that daemon returned the same key to unauthorized,
proving it was not persisted. The accidentally selected Always allow result
was retained as the persistent-path gate: after a full device reboot the
original workstation key reconnected as `device` in 87.81 seconds. The
original key files were restored byte-for-byte and the temporary private and
public keys were removed. The 740-byte focused consent log has SHA-256
`18325397fd768269ad140dcb9bfeff5f0ae7a6a81da85abffefaefed76fa5c98` and
records the HOME renderer plus bridge-accepted Deny and Allow-once decisions.
An attempted `adb root` was rejected by the product setting as intended;
debug-root was not enabled.

**Rejected physical result / diagnosis:** Stock Mobile was visibly distinct
from the Linux shell, but this candidate does not pass phone layout. The
source rendered with the runtime's 1024x768 fallback instead of the real
top-level Android viewport, and the fixed host Theme/Rollback strip overlapped
its top bar. The measured panel is 1080x2400 at density 450 (2.8125), with an
88-physical-pixel top display cutout. The 178,378-byte diagnosis screenshot at
`.cache/evidence/android-v4-a7dba10-physical/compat1/layout-diagnosis/stock.png`
has SHA-256 `9da17a7f65bf49c51ae7b2f27c00e8e3c07ca36af810cc826bdf8d2fb84c8355`.
The first campaign capture also failed closed because the non-root ADB shell
could not read `/data/misc/sos/provider-state.composition.json`; the attempted
`uiautomator dump /dev/tty` returned only a status line. Making authority state
readable or enabling root ADB were rejected.

**Changed / evidence:** Experience API viewport context now includes bounded
logical safe insets. The Android Activity derives live logical width, height,
density, cutout, and gesture insets from its real decor view, sends them over a
fixed JNI boundary, and wakes the host. The graph runtime validates and
rerenders the root transactionally without changing revision or Instance ID;
mounted children retain host-measured bounds and zero physical-display insets.
Stock Mobile consumes those values in its source-owned top bar, content, and
bottom navigation. Its Controls screen now owns touch-sized Theme and Rollback
rows. The fixed host strip appears only over ordinary top-level Experiences,
and the two reserved Stock actions are accepted only from the namespaced Stock
Mobile root.

The authority now exposes a bounded, read-only `AuditSnapshot` response with
the presented Experience plus authority-owned state, appearance, and grant
resources. `a33xctl` obtains it through a temporary authorized-ADB local
forward and removes the forward; it never changes storage permissions.
Accessibility capture uses a shell-owned temporary device file instead of
`/dev/tty`, and stage directories become visible only after the complete
capture succeeds. Protocol, authority, runtime, Experience, and host-harness
tests pass, including new read-only, safe-inset rerender, and failed-capture
retry cases. The real ARM64 `aosp-system` Rust build completed in 20.62 seconds
and the release Java/APK build passed.

**Decision / next gate:** The installed `a7dba10` candidate is accepted for
ADB consent only and rejected for layout/composition. Commit the repair, build
and inspect one superseding exact Compat OTA, then install it through the
already authorized transport. Physical acceptance must prove the corrected
cutout fit, source-owned Stock controls, full-screen ordinary roots, non-root
evidence capture, and every ordered composition/restart/authoring/rollback
stage before Android parity closes.

## 2026-08-27: Seal the viewport-corrected Stock Mobile Compat candidate

**Goal / build:** Produce the one exact replacement for the rejected physical
layout candidate. Clean source revision
`7da25e9c8a6c125f68f6adb6aef105509edbbb2a` built Compat 1 successfully in
247.44 seconds with 2,979,196 KiB peak RSS. Its immutable product identity is
`sos.compat1.7da25e9c8a6c.b93bb9eb8875`. This revision adds the live Android
viewport and safe-inset boundary, transactionally rerenders the root while
preserving its Instance ID, moves the reserved Theme and Rollback actions into
Stock Mobile, and leaves the fixed host controls only over ordinary roots.

**Offline evidence:** `./tools/a33xctl inspect-compat1` passed in 19.64 seconds
with 47,620 KiB peak RSS. It verified the Stock Mobile identity and
phone-native source contract, host-owned system controls, authority markers,
package signature and boot-chain data, package format 4, Experience API 4, and
the rollback-only v3 reader. The inspection log is 29,355 bytes with SHA-256
`4112b815af0f33edac3f73ce800962d652801d49b86baa77ea007f77893c2714`.

The sealed OTA at
`.cache/evidence/android-v4-7da25e9/compat1/lineage-23.0-20260827-UNOFFICIAL-sos_compat_a33x-7da25e9.zip`
is 1,067,674,864 bytes with SHA-256
`2a3c91d3c784cdf04cd3dc79b76520b017d53fb61a7545016b96e0a57ad74945`.
Its complete ZIP integrity check passed in 4.50 seconds. The finalized
five-file evidence manifest at
`.cache/evidence/android-v4-7da25e9/compat1/MANIFEST.sha256` is 465 bytes with
SHA-256
`cdadd44107d647777762b86b818d68c4f751c7207989b4b79a5ea186391cfd53`;
independent manifest verification passes.

**Decision / next gate:** This exact OTA is the only candidate authorized for
the replacement Recovery transfer. Reverify its identity and digest at the
device boundary, install it once, then require exact-product readiness and the
complete ordered non-root composition campaign. Offline evidence does not
close the physical cutout, touch, composition, restart, recovery, or rollback
gates.

## 2026-08-27: Reject the corrected viewport candidate for a Compat chrome race

**Goal / physical evidence:** Install the sealed `7da25e9` candidate and judge
the first real Stock Mobile frame before beginning the ordered campaign. Serial
`RFCT50EGFCN` was the only device and no competing transfer was active. The
1,067,674,864-byte OTA was reverified at the device boundary with SHA-256
`2a3c91d3c784cdf04cd3dc79b76520b017d53fb61a7545016b96e0a57ad74945`.
Automatic Recovery entry took 29.54 seconds; the single 82.52-second transfer
exited 0 with `Total xfer: 1.00x`. Automatic reboot and the complete Compat
readiness predicate took 99.69 seconds. The phone reached exact identity
`sos.compat1.7da25e9c8a6c.b93bb9eb8875`, boot complete, API/package v4,
Compat stage 1, SOS HOME ownership, live authority and HOME processes, and
Enforcing SELinux without manual Recovery input.

The native boundary reported the real viewport as 384x853 logical pixels at
2.813 scale with a 31-logical-pixel safe top, matching the 1080x2400 panel and
88-physical-pixel cutout. Stock now begins below that cutout and fills the
phone. The 187,340-byte first-frame screenshot at
`.cache/evidence/android-v4-7da25e9-physical/compat1/install/first-stock.png`
has SHA-256
`06439806ee2889e670f1f775d5c450cab31ca975f7e9eb858227a174a96b4004`.

**Rejected result / root cause:** The first frame still exposed the fixed
Compat Back, Apps, Attention, and Exit drawer above Stock Mobile, obscuring the
right side of the source-owned interface. This was not persisted Experience
state. The service logged `compat_chrome_visibility=hidden owner=stock-mobile`,
then its later `onStartCommand()` unconditionally scheduled a reveal after 750
ms. The 180-byte focused service log has SHA-256
`a048d83c3b42ced177655bf75cdc2d5fb566bc9f3553e153e5d6c177a5dae7f4`.
The `7da25e9` image is therefore accepted only for the viewport boundary and
rejected as a Stock Mobile composition candidate.

**Changed / verification:** Delayed reveal now checks the current owner, and
service start hides immediately whenever Stock Mobile owns focus. The compiled
artifact marker and source host gate require this owner-aware guard. The real
release APK compiles successfully, and the complete mock Compat/Core staged
campaign passes in 7.92 seconds with 5,012 KiB peak RSS. That gate also exposed
that a failed stage capture could leave its hidden temporary directory because
the EXIT trap outlived a function-local path. Capture cleanup now owns a
process-scope exact temporary path, clears it after atomic rename, and the
failed-capture regression passes. No device state was changed while making
either repair.

**Decision / next gate:** Build and inspect one superseding exact Compat OTA.
Install it once through the authorized automatic Recovery path, require Stock
Mobile to remain unobscured after the reveal interval, then begin the ordered
non-root composition campaign. The Android physical milestone remains open.

## 2026-08-27: Seal the owner-guarded Stock Mobile Compat candidate

**Goal / build:** Produce the exact replacement for the Compat chrome race.
Clean source `af57c0fed7202d384fb588fe3116f405dc7d89b5` built Compat 1 in
237.19 seconds with 2,978,244 KiB peak RSS. Its immutable product identity is
`sos.compat1.af57c0fed720.332e773ac9ad`. The compiled APK contains the
owner-aware delayed-reveal guard, while the product retains the live physical
viewport boundary, independent Stock Mobile package and appearance sidecar,
and non-root authority audit protocol.

**Offline evidence:** `./tools/a33xctl inspect-compat1` passed in 19.70 seconds
with 47,800 KiB peak RSS. It verified the new owner-visibility marker, Stock
Mobile identity and phone-native source contract, host-owned ordinary-root
controls, signed authority reference composition, package and boot-chain
signatures, API/package v4, and the bounded rollback-only v3 reader. The
29,148-byte inspection log has SHA-256
`360317aa4a90f888cf3fa470f05b821ad6a58b18e0829233fb9710c234d6b053`.

The sealed OTA at
`.cache/evidence/android-v4-af57c0f/compat1/lineage-23.0-20260827-UNOFFICIAL-sos_compat_a33x-af57c0f.zip`
is 1,067,697,350 bytes with SHA-256
`0049a6eff5ca1b5810dd3c2e89894d6e5b04a90aa50fa86e694b6aeecb4af402`.
Its complete ZIP integrity check passed in 4.50 seconds. The finalized
seven-file manifest is 627 bytes with SHA-256
`dcc763d2ebab8ab23d8b5f19cc9120a1dc385d28b519c4f9b69e586d9fe95ee5`;
independent verification passes.

**Decision / next gate:** This exact OTA is the only candidate authorized for
the next replacement transfer. Reverify it at the physical boundary, install
it once, prove the Stock owner remains unobscured beyond the delayed-reveal
interval, and only then initialize a new ordered composition campaign.

## 2026-08-27: Accept the Stock frame and reject incomplete recovery evidence

**Goal / physical evidence:** Install the owner-guarded `af57c0f` candidate and
begin the complete ordered campaign. The sealed 1,067,697,350-byte OTA with
SHA-256 `0049a6eff5ca1b5810dd3c2e89894d6e5b04a90aa50fa86e694b6aeecb4af402`
was reverified at the device boundary. Automatic Recovery entry took 29.44
seconds, the one transfer took 81.40 seconds and exited 0 with `Total xfer:
1.00x`, and exact-product readiness took 109.73 seconds without manual input.
The phone reached `sos.compat1.af57c0fed720.332e773ac9ad`, boot-complete v4
Compat, live authority and HOME processes, and Enforcing SELinux.

Stock Mobile passed the repaired frame gate. Two owner-focus callbacks logged
the Compat chrome hidden after its reveal deadline, and neither the screenshot
nor accessibility tree contains the fixed Back/Apps/Attention/Exit drawer. The
175,069-byte screenshot at
`.cache/evidence/android-v4-af57c0f-physical/compat1/install/first-stock.png`
has SHA-256
`47ecbef033a30726b7c107178f7a434306af434e6327e904c9e1adf9344eb299`.
The independent phone-native top and bottom bars fit the full panel and remain
clear of the display cutout.

Dashboard then presented as a three-Instance graph. A real touch on the
host-owned Theme control advanced authority appearance generation from 0 to 1,
rerendered the complete graph with zero failed instances, and did not change
any Experience revision ID. Agenda's `open_first` event durably set its own
`selected` state and Dashboard's separately keyed `opened` state to `Design
review`. Both the deliberate update exception and execution timeout left the
parent ready and preserved those states. Captured evidence is retained under
`.cache/evidence/android-v4-af57c0f-physical/compat1/composition/`, but the
campaign is not a PASS.

**Failures / changed:** The real authority snapshot exposed an audit-fixture
mismatch: `AppearanceResource` is serialized with a flattened profile, while
the shell audit and mock expected `.appearance.profile.generation`. The audit
and fixture now use the actual `.appearance.generation` wire shape, and the
complete mock campaign passes.

The failed Agenda instance recovered on an authority model refresh before its
explicit recovery event ran, but the host logged status transitions only for
action completions. The refresh branch therefore installed a healthy snapshot
without the required failed-to-ready evidence. The host now compares the old
and refreshed snapshots through the same status-transition logger before
installation. Focused `sos-experience` tests pass 20 tests, and the full A33x
mock campaign passes in 7.97 seconds with 5,212 KiB peak RSS.

**Decision / next gate:** Rebuild and inspect one superseding exact Compat OTA.
Repeat the automatic install and start a new campaign root. Require explicit
Agenda recovery markers after both containment failures before continuing to
IME, restart, authoring, and rollback. The `af57c0f` product remains accepted
for Stock layout, Compat chrome ownership, Dashboard composition, appearance,
and failure containment only.

## 2026-08-27: Seal the recovery-observable Compat candidate

**Goal / build:** Produce the exact candidate that makes failed-to-ready child
transitions observable whether recovery comes from an action or an authority
refresh. Clean source `3693138b49afb7103b33e63edbcbd82eb5532908` built
Compat 1 in 242.29 seconds with 2,978,672 KiB peak RSS. Its immutable identity
is `sos.compat1.3693138b49af.cf58ca7e57f2`.

**Offline evidence:** `./tools/a33xctl inspect-compat1` passed in 19.73 seconds
with 47,784 KiB peak RSS, including the Stock Mobile, owner-guarded chrome,
signed authority, v4 graph/API, package signature, boot-chain, and bounded
legacy-reader gates. The sealed OTA at
`.cache/evidence/android-v4-3693138/compat1/lineage-23.0-20260827-UNOFFICIAL-sos_compat_a33x-3693138.zip`
is 1,067,652,445 bytes with SHA-256
`84c5523b948aea5b0462fd4e2b6d1f078dccc381f754cdb978b2c30b79aec47b`.
Its complete ZIP check passed in 4.43 seconds. The seven-file evidence manifest
has SHA-256
`09850f701fb9eb3a279c20e82aa96e6709f184b8f98bfd58483f11cd165f81db`;
independent verification passes.

**Decision / next gate:** Install only this exact OTA. Begin a fresh campaign,
repeat the accepted Stock, Dashboard, appearance, failure, and timeout gates,
and require two explicit recovery markers before the remaining ordered stages.

## 2026-08-27: Accept composition through host recovery; add bounded authority recovery

**Goal / physical evidence:** Install the recovery-observable candidate and
advance the ordered Compat campaign through composition, containment, input,
accessibility, and host recovery. Automatic Recovery entry took 29.54 seconds,
the single sideload took 82.53 seconds and exited 0 with `Total xfer: 1.00x`,
and exact-product readiness took 107.48 seconds. The installed identity is
`sos.compat1.3693138b49af.cf58ca7e57f2`; authority PID 944 and initial HOME
PID 1466 were live under Enforcing SELinux.

Stock Mobile remained unobscured. Dashboard ran as three independent
Instances. A real Theme touch advanced appearance to generation 2 without
changing revisions. Agenda and Dashboard retained separately keyed `Design
review` state; Dashboard retained two acceptance pings. The deliberate Agenda
update exception and execution timeout each logged a contained failed Instance
with `root_ready=true` followed by an explicit recovery of the same namespaced
Instance. A physical tap inside the mounted Agenda field logged namespaced IME
focus, a tap outside logged inactive blur, and Android published all 12 expected
semantic nodes. The accepted IME stage screenshot, authority snapshot, and log
have SHA-256 `e122692449bde35769e425543268d9ac7a0808a326b062caead032496452d170`,
`c0e391a9370d8f5008bc0d921f4f86ad0d8a14b4d92e829b8f3ef8014cd6c171`,
and `437cc7de142d03cd774bad0dbfb1ae7c0d54d96c3efaddd635b3cf5a62e5b582`.
Restarting only HOME changed PID 1466 to 4314, left authority PID 944 intact,
and restored the same three-Instance Dashboard and durable resources.

**Failure / changed:** Android correctly denied both non-root `kill -9 944` and
the broad `ctl.restart=sos_authority` property. The authority-only physical gate
therefore lacked a safe actuator; enabling root ADB would invalidate the gate.
The A33x product now defines one exact boolean
`sys.sos.authority_recovery_probe` property. Only shell on userdebug/eng builds
may set it. Init consumes value 1 by restarting only `sos_authority`, resets the
property to 0, and leaves the experience host untouched. `a33xctl` exposes an
exact-revision `restart-v4-authority` command that verifies debuggability, PID
replacement, host preservation, service recovery, and one-shot reset. Package
inspection requires the trigger and its reset. `bash -n`, the product-graph
gate, and the complete A33x host fixture pass; the latter took 8.00 seconds.

**Decision / remaining risk / next gate:** The current candidate is accepted
through host recovery but cannot close authority recovery because it predates
the bounded trigger. Build and inspect one superseding exact Compat OTA, install
it once, and repeat the ordered campaign. The next candidate must pass the new
authority-only PID gate before authoring and exact rollback. No production user
build receives the shell property permission.

## 2026-08-27: Seal the bounded-recovery Compat candidate

**Goal / build:** Produce the exact userdebug candidate containing the
authority-only physical recovery actuator. Clean source
`0f15bf98f78cd107468994903555c1e72c7fc880` built Compat 1 successfully in
303.73 seconds with 2,978,784 KiB peak RSS. Its immutable identity is
`sos.compat1.0f15bf98f78c.3ec21b65cb07`. SELinux compilation, neverallow,
property-context, compatibility, and APEX policy tests all passed. The compiled
policy contains only the expected shell `property_service set` permission for
`sos_authority_recovery_probe_prop` on this userdebug build.

**Offline evidence:** The strengthened `inspect-compat1` gate passed in 19.68
seconds with 47,768 KiB peak RSS. It verifies the one-shot init trigger, exact
installed boolean property context, compiled userdebug shell permission, Stock
Mobile identity, v4 graph/API, signed authority composition, host-owned system
controls, package signatures, and boot chain. The sealed OTA at
`.cache/evidence/android-v4-0f15bf9/compat1/lineage-23.0-20260827-UNOFFICIAL-sos_compat_a33x-0f15bf9.zip`
is 1,067,709,730 bytes with SHA-256
`146eb025a59ce33140f2829049abca2ce02bd301bbb2cce77bce09bb38d3d733`.
Its complete ZIP test passed in 4.84 seconds. The finalized five-file evidence
manifest is 467 bytes with SHA-256
`50bc82c85044c064dd8b009f619ef1b4b6c19e5fe88b81d8578f821c6e5a308f`;
independent verification passes.

**Decision / next gate:** This exact OTA is the only authorized replacement.
Reverify it at the device boundary, install it once through automatic Recovery,
repeat the complete ordered campaign, and require the authority PID to change
while the recovered HOME PID remains exact. Authoring and rollback remain open
until that campaign passes.

## 2026-08-27: Reject transient authority recovery and make its listeners restartable

**Goal / physical evidence:** Install the bounded-recovery candidate and run
the ordered Compat v4 campaign far enough to judge independent host and
authority recovery. Recovery entry took 29.64 seconds, the single exact OTA
transfer took 81.54 seconds with `Total xfer: 1.00x`, and exact boot readiness
took 107.69 seconds. The installed identity was
`sos.compat1.0f15bf98f78c.3ec21b65cb07`; SELinux was Enforcing. The distinct
Stock Mobile frame remained unobscured. Its 175,754-byte screenshot at
`.cache/evidence/android-v4-0f15bf9-physical/compat1/install/stock-mobile.png`
has SHA-256
`df382f446ba3d4d737993a4e0da1e4f9b042a93f3bdc9d84bc0a72f73a315a79`.

Dashboard again ran as three Instances. A real Theme touch advanced appearance
from generation 2 to 3 without revision churn. The Agenda update exception and
time-budget violation were contained and recovered; parent liveness, separately
keyed state and grants, mounted accessibility, and namespaced IME focus/blur
passed. The first IME attempt used Android's reported virtual-node rectangle
and missed the renderer hit target; the previously measured renderer coordinate
`(500,900)` focused the expected `i-...::agenda-input`. Preserve this as an
accessibility/hit-test alignment risk. HOME-only recovery changed PID 1480 to
5025, kept authority PID 935, and restored Dashboard, state, and appearance.
The accepted stages through `host-restart` are under
`.cache/evidence/android-v4-0f15bf9-physical/compat1/composition/`; the campaign
is not a PASS and has no sealed verdict.

**Failures / diagnosis:** Lineage's hardened userdebug configuration sets
`PRODUCT_NOT_DEBUGGABLE_IN_USERDEBUG := true`, so the initial actuator guard
incorrectly rejected the genuine `ro.build.type=userdebug` product because
`ro.debuggable=0`. The guard now follows the same `userdebug|eng` boundary as
the compiled SELinux macro and still rejects `user`. After that correction,
the old actuator check observed transient authority PID 5366 and reported PASS,
but the process exited within 93 ms. The subsequent authority snapshot rejected
the stage, and init then repeated status-1 exits while HOME PID 5025 survived.
The finalized 5,104,827-byte lifecycle log has SHA-256
`d71a00651a96a5eff573cdd161cbb44eba3409a1a165e833860d465b2167f13a`.
A cold reboot recovered unmodified durable state, authority PID 944, and HOME
PID 1469 in 127.82 seconds; the 121-byte result has SHA-256
`98a0a9d9cdc9eb74549bf54d28de55cacc72312b4e0dcd85948b1962b25991b0`.

An audit request against the recovered daemon exposed many server-side
`TIME-WAIT` sockets for both `127.0.0.1:47777` and `:47778` while the original
listeners remained live. The 10,026-byte socket snapshot has SHA-256
`c2f04cde88a7ee6f0aabd04f85dd102ac757a7753973d61db4afbe0188a15fce`.
The daemon used plain `TcpListener::bind`, so init could not immediately bind
replacement listeners after killing an authority with active provider and
revision clients. A local full-daemon reproduction was rejected as evidence:
the workstation's resident `providerd` already owned port 47777, causing the
probe to kill its test authority during reference installation and produce an
unrelated incomplete-registry error.

**Changed / verification:** Authority loopback listeners now set
`SO_REUSEADDR` before bind and retain close-on-exec. A unit test opens a live
connection, closes the listener, and immediately rebinds the exact address.
`restart-v4-authority` now requires a valid authority audit snapshot after PID
replacement, in addition to exact revision/build type, running service, stable
HOME PID, and one-shot property reset; a transient PID can no longer pass.
All 26 authority unit, binary, wire, and doc tests pass. The complete A33x host
fixture passes in 8.09 seconds with 5,088 KiB peak RSS, including hardened
userdebug acceptance, user rejection, and audit readiness. The Android ARM64
release check passes in 4.37 seconds with 419,116 KiB peak RSS.

**Decision / next gate:** The `0f15bf9` campaign is rejected at authority
recovery despite all earlier accepted checkpoints. Build and inspect one new
exact Compat OTA containing reusable listeners, install it once, and start a
fresh artifact-bound campaign. Require the replacement authority to serve a
valid audit snapshot while HOME remains exact before authoring and v4-to-v4
rollback.

## 2026-08-27: Seal the restartable-authority Compat candidate

**Goal / build:** Produce the exact replacement containing reusable authority
listeners and the audit-ready recovery gate. Clean source
`b89f779d4067f82d9fd4c6ed785578c16ed48111` built Compat 1 successfully in
257.47 seconds with 2,978,416 KiB peak RSS. Its immutable product identity is
`sos.compat1.b89f779d4067.a7a7e01f504a`.

**Offline evidence:** `./tools/a33xctl inspect-compat1` passed in 19.72 seconds
with 48,248 KiB peak RSS. The packaged 1,924,616-byte authority has SHA-256
`e71befb9aab7dd75ec1379d0dd38ea4d2fa9adef1ad1316990d216956cfd2b2f`.
The inspector reverified the userdebug-only recovery property and compiled
permission, Stock Mobile, Experience API and package format v4, signed
reference graph, host-owned controls, SELinux ownership, APK signatures,
VINTF, AVB, and the boot-chain image graph. The exact 1,067,728,950-byte OTA
at
`.cache/evidence/android-v4-b89f779/compat1/lineage-23.0-20260827-UNOFFICIAL-sos_compat_a33x-b89f779.zip`
has SHA-256
`fa9dcd4bde19faf544ce168bd93f2e36f3043b85277b7ceefba1df816a8260fe`.
Its complete ZIP test passed in 4.83 seconds. The finalized eight-file offline
manifest is 712 bytes with SHA-256
`462e2939bbc468fcfdfbcc86adb0421831ba30cf75ec97620014d84a161fe668`;
independent verification passes.

**Decision / next gate:** Install only this digest, start a new physical
campaign root bound to this artifact and revision, and require authority PID
replacement plus a valid post-restart audit snapshot while HOME remains exact.
Only after that gate may the campaign advance to Stock authoring and exact
v4-to-v4 rollback.

## 2026-08-27: Reject the b89 authority restart and instrument its true startup boundary

**Goal / physical evidence:** Install the exact restartable-authority Compat
candidate and judge the complete ordered v4 campaign on the Samsung SM-A336B.
Automatic Recovery entry took 29.54 seconds, the only sideload took 81.53
seconds with `Total xfer: 1.00x`, and exact-product readiness took 109.16
seconds. The installed identity was
`sos.compat1.b89f779d4067.a7a7e01f504a`; initial authority PID 939 and HOME PID
1529 were live under Enforcing SELinux. Stock Mobile remained unobscured, and
the 175,650-byte screenshot has SHA-256
`bf802ad84f0573004037ae225a688b0c64a351524c06254221cb0b102fa3ec93`.

Dashboard again presented three independent Instances. Appearance advanced
from generation 3 to 4 without revision changes. Both deliberate Agenda
failures were contained and explicitly recovered, the two acceptance pings and
separately keyed state survived, namespaced IME focus/blur and all expected
semantics passed, and HOME-only recovery changed PID 1529 to 3713 while
authority PID 939 remained exact. These stages are retained under
`.cache/evidence/android-v4-b89f779-physical/compat1/composition/`, but the
campaign is rejected at `authority-restart`: the bounded command exited 1 after
20.21 seconds because no replacement daemon became audit-ready. HOME PID 3713
survived while init repeatedly observed authority status 1. The finalized
4,547,951-byte lifecycle log has SHA-256
`8aa738f06a79d689915cc9bb41d112919fce0a366ac3560fd1876f1a92d985eb`.

**Corrected diagnosis:** The preceding TIME-WAIT inference was wrong. A tiny
ARM64 probe run as the same unprivileged Android shell identity closed a live
loopback connection and immediately rebound the exact address successfully,
both with only `SO_REUSEADDR` and with `SO_REUSEPORT`. Its 80-byte result at
`.cache/evidence/android-v4-b89f779-physical/compat1/composition/android-kernel-rebind-probe.txt`
has SHA-256
`78e3b83f08d1f5df64754bfe1c8aef73eff4ee426c9a0e6871254be6bfabecd5`.
The reusable-listener change remains harmless and tested, but neither the
2,210-byte socket snapshot nor TIME-WAIT explains this restart failure. The
current init service discards stderr, so the 4.5 MiB system log cannot expose
the process's actual startup error. A cold reboot recovered authority PID 944,
HOME PID 1464, and the durable graph in 127.53 seconds; the 95-byte result has
SHA-256
`70c497db53a1b3ef777ba46c92147813a70f02d87f30b78960ce5ac77fcb6f9e`.

**Changed / verification:** Fatal authority startup failures now go directly
to Android `logd` with tag `sos-authority`. Every fallible startup boundary has
context, including v4 authority open, reference-composition installation,
loopback listener bind, and Core Unix-socket replacement. Package inspection
requires the `liblog.so` dependency, fatal marker, and reference-install
boundary so a stale binary cannot satisfy this diagnostic gate. All 27
authority unit, binary, wire, and documentation tests pass in 0.58 seconds with
366,176 KiB peak RSS. A linked ARM64/API 31 release build passes in 13.97
seconds with 857,044 KiB peak RSS and carries the expected `liblog.so`
dependency.

**Decision / next gate:** Build and inspect one exact instrumented Compat OTA,
then install it once and begin a fresh artifact-bound campaign. At the
authority-only checkpoint, require either an audit-ready replacement or the
new exact `android_system_authority_failed` log. Fix only that observed startup
cause before authoring and v4-to-v4 rollback; do not add `SO_REUSEPORT` or make
another socket-based inference.

## 2026-08-27: Seal the authority-diagnostic Compat candidate

**Goal / build:** Produce the exact replacement that exposes authority startup
failures through Android `logd` without changing the restart actuator or
durable registry. Clean source `dcbe6109b7ef0efcf70407f4a2ec08be2a5abdc4`
built Compat 1 successfully in 239.21 seconds with 2,978,392 KiB peak RSS. Its
immutable product identity is
`sos.compat1.dcbe6109b7ef.88b08470bdbf`.

**Offline evidence:** The strengthened `./tools/a33xctl inspect-compat1` gate
passed in 19.71 seconds with 48,720 KiB peak RSS. In addition to Stock Mobile,
v4 composition, policy, signature, VINTF, PIT, AVB, and boot-chain checks, it
verified that the packaged 1,927,968-byte authority links `liblog.so` and
contains both the fatal marker and reference-install startup boundary. That
binary has SHA-256
`5a99ef96ee47e820c0b7723a19216b946b15f7a8397f5a22bd3bc810d39d3ecd`.
The exact 1,067,694,920-byte OTA at
`.cache/evidence/android-v4-dcbe610/compat1/lineage-23.0-20260827-UNOFFICIAL-sos_compat_a33x-dcbe610.zip`
has SHA-256
`99dbed3ed79d8e0e12f4807f04f87a8bcde624f210ffb7cd2120d62b617729d3`.
Its complete ZIP test passed in 4.53 seconds. The finalized eight-file offline
manifest is 712 bytes with SHA-256
`5f5e62660436f9e1334a298312f0ad90c44fdb18f238bd331470a800af65f978`;
independent verification passes.

**Decision / next gate:** This digest is the only authorized replacement.
Reverify it at the device boundary, install it once through automatic Recovery,
and start a fresh campaign. Require the authority-only restart either to serve
an audit snapshot or to emit its exact contextual fatal cause while HOME stays
live; do not advance to authoring on a transient PID.

## 2026-08-27: Isolate the authority failure to its loopback-listener helper

**Goal / physical evidence:** Install the exact authority-diagnostic candidate
and use its Android log boundary to replace the remaining restart hypothesis
with an observed cause. Device preflight reverified the 1,067,694,920-byte OTA
and SHA-256 `99dbed3ed79d8e0e12f4807f04f87a8bcde624f210ffb7cd2120d62b617729d3`
against live b89 Compat. Automatic Recovery entry took 29.44 seconds, the only
sideload took 82.44 seconds with `Total xfer: 1.00x`, and exact-product
readiness took 125.51 seconds. The installed identity was
`sos.compat1.dcbe6109b7ef.88b08470bdbf`; authority PID 953 and HOME PID 1524
were live under Enforcing SELinux with no relevant crash, ANR, fatal authority
marker, or SOS AVC.

The distinct Stock Mobile shell again fit the complete 1080x2400 panel with no
Compat drawer. Its 181,576-byte screenshot has SHA-256
`2757901190da72aa6433220e5dcac1bd3a5229eddf4e3822b22ca109c413d845`.
Dashboard presented and confirmed a three-Instance graph. A real Theme tap
advanced appearance generation 4 to 5 without revision churn. The Agenda
update exception and time-budget violation were independently contained and
recovered; two parent pings, separately keyed state, mounted IME focus/blur,
and the 12-node Android accessibility tree passed. HOME-only recovery changed
PID 1524 to 3746, preserved authority PID 953, and restored Dashboard at
generation 5 with eight durable parent pings.

**Failure / exact diagnosis boundary:** `restart-v4-authority` correctly exited
1 after 20.25 seconds because no replacement became audit-ready. HOME PID 3746
survived, the one-shot property reset to 0, and init reported the service as
restarting. Every five-second replacement emitted the new direct log marker:
`android_system_authority_failed error=bind provider listener
127.0.0.1:47777 failed: Connection refused (os error 111)`. The finalized
4,617,195-byte lifecycle log has SHA-256
`607b7bdc4c164e508ec967cf081e0bad1ae93404741895547ee0b930398bb6c0`.
This excludes registry installation, durable state open, revision listener,
Core Unix sockets, and audit probing, but the helper still combined socket
creation, `SO_REUSEADDR`, kernel bind, and listen under one outer label. No
`authority-restart` campaign stage was captured and the campaign remains
rejected. A cold reboot recovered the exact product, authority PID 942, HOME
PID 1470, and durable Dashboard in 127.56 seconds; the 142-byte result has
SHA-256
`db733acae267d7bf7a7284e7e65ec61463eee55a8c8ecc3b954878d13fb5746a`.

**Changed / verification:** Each raw listener operation now has a stable error
boundary: socket creation, `SO_REUSEADDR`, bind, and listen. The artifact
inspector requires all four compiled markers. All 28 authority unit, binary,
wire, and documentation tests pass, including a non-local-address test that
proves the bind boundary. The linked ARM64/API 31 release build passes in 13.64
seconds with 854,504 KiB peak RSS and contains every marker. Bash parsing and
the complete A33x host fixture pass; the latter took 8.01 seconds with 5,360
KiB peak RSS.

**Decision / next gate:** Build and inspect one exact raw-step diagnostic
candidate, install it once, and repeat the ordered campaign. At authority
recovery, use the emitted raw step to determine the implementation change; do
not infer which syscall returned errno 111 and do not advance to authoring.

## 2026-08-27: Seal the raw-step authority diagnostic candidate

**Goal / build:** Package the exact authority binary that identifies socket
creation, `SO_REUSEADDR`, bind, and listen as separate fatal boundaries. Clean
source `0f519dd1a318ccd71b71d208c176d9d9dea09ee0` built Compat 1 successfully
in 242.22 seconds with 2,978,888 KiB peak RSS. The immutable product identity
is `sos.compat1.0f519dd1a318.967ed8346550`.

**Offline evidence:** `./tools/a33xctl inspect-compat1` passed in 19.72 seconds
with 47,912 KiB peak RSS. It verified the OTA signature, VINTF, PIT and AVB
limits, boot chain, v4 and Stock Mobile contracts, Android authority markers,
and all four compiled raw socket-step labels. The packaged 1,930,032-byte
authority has SHA-256
`88b9a5a05b9fdf41e4a9dbfb8b7f12f9e05bfb84246a42d05926d52c0dad9e97`.
The exact 1,067,731,695-byte OTA at
`.cache/evidence/android-v4-0f519dd/compat1/lineage-23.0-20260827-UNOFFICIAL-sos_compat_a33x-0f519dd.zip`
has SHA-256
`3891b536043aa7e150bb630f22e848c14af7767aef113c721253b63cb6e08a39`.
Its complete ZIP test passed in 4.52 seconds. The finalized eight-file offline
manifest is 712 bytes with SHA-256
`f2e3eb1d36ec6ba16fdfffc3bdc251c2b76599d2601ce01472b558fc9fb38276`;
independent verification passes.

**Decision / next gate:** Reverify this digest at the device boundary, install
it once through automatic Recovery, and run a fresh artifact-bound campaign.
The authority restart must either become audit-ready or report the exact raw
socket operation returning errno 111. No authoring or rollback claim follows
from this diagnostic artifact alone.
