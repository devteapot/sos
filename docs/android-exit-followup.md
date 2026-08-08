# Android-exit follow-up: raw generation, latency, recovery, and state

Date: 2026-08-08

> Historical evidence: the process/surface mechanism in this report has been
> superseded by stable-host Luau activation. The measurements remain useful;
> the current contract is [`revision-supervisor.md`](revision-supervisor.md).

This follow-up implements the next work on gates B–D from
[`android-exit-gate.md`](android-exit-gate.md). It does not claim that SOS is
ready to leave the Android application laboratory.

## Raw single-shot baseline

The frozen six-case protocol and results live in
[`raw-agent-evaluation.md`](raw-agent-evaluation.md). Luna medium compiled three
of six untouched outputs and scored 17/40. Three failures were the same
unsupported accessibility role; the drag case independently produced a
reachable `notes.attach_to_event` effect but missed the phone-safe hit bound.
This is a lower bound for an agent with generic repository context, not a
forecast for a tailored authoring system.

## Source-to-visible attribution

Telemetry now divides an accepted swap into worker compile/render, worker-to-UI
commit handshake, commit-to-GPUI render entry, GPUI tree construction, and the
next-frame callback. These are monotonic host timestamps; the last signal is
still GPUI's callback rather than an Android compositor fence.

On the Samsung SM-A336B, a confirming 1,000-swap run reported:

```text
accepted=1000 rejected=0 duration_ms=44266
visible_p50_us=37657 visible_p95_us=79390 visible_p99_us=80304 max=82127
worker_p95_us=8086 worker_to_commit_p95_us=51600
commit_to_render_p95_us=875 gpui_tree_build_p95_us=272
frame_callback_p95_us=26124
rss_start_kb=295484 rss_end_kb=299232 rss_peak_kb=299240 delta=3748
```

Percentiles are not additive, but attribution makes the engineering decision
clear: Luau and GPUI tree creation are not the dominant tail. The two-message
prepare/authorize/commit handshake crosses the GPUI event loop twice and owns
the largest segment; the frame callback is second. Visible p95 is now below the
100 ms gate, so this is an optimization opportunity rather than a blocker.

## Disposable process and surface probe

The APK now contains an isolated `:candidate` Android process with its own
hardware-accelerated surface and task. The permanent GPUI Activity launches it;
the candidate reports its first draw and has a candidate-local uncaught
exception boundary that removes its task before killing only that process.
`./tools/sosctl candidate-probe` verifies PID and resumed-surface invariants.

Two physical-device probes kept the accepted PID at `16889`:

- PID `16995` threw before first frame, died, and returned to the accepted
  surface without presenting the candidate.
- PID `17048` logged `candidate_first_frame`, then threw and died; the accepted
  surface returned with active source hash `81e606cc…` and state hash
  `e7a5c6ea…` unchanged.

The recovered screenshot `candidate-probe-recovered.png` is 186,126 bytes with
SHA-256 `5ee950df4be98385f0356e652684093864ea9df77cb1885efb3b5f0bbbc7cdaa`.
A final exact-source rebuild repeated the after-first-frame probe with accepted
PID `17754` and candidate PID `17861`; its 117,470-byte
`candidate-probe-final-recovered.png` has SHA-256
`b7f122d124c7ae327a4d1ff8d242bb05695dba940a0e35001aa95d302da93aef`.

Three rejected approaches were informative. Direct `Process.killProcess` in a
same-task candidate caused Android to restart it repeatedly. An uncaught
exception in the same task returned to the launcher. A separate task without a
candidate-local handler exposed Samsung's application-error dialog. The final
combination avoids all three for caught candidate exceptions.

This remains **partial** recovery evidence. The candidate surface is Java, not
a second GPUI/Luau experience; the accepted surface is not interactive while
the candidate is foreground; and fatal native signals can bypass the Java
handler. A fixed privileged supervisor/compositor remains necessary.

## External versioned state service

The TCP provider daemon now also owns a durable `StateEnvelope` containing an
optimistic revision, schema version, and JSON state. Its transaction protocol
supports load, stage, promote, abort, and one-shot faults before/after stage and
promotion. The Luau runtime exposes a 20 ms, 1 MiB-bounded explicit
`state_version`/`migrate(from_version, state)` contract and rejects schema
changes without migration.

Desktop protocol evidence staged schema 2 at revision 0, promoted revision 1,
injected `BeforePromote`, verified revision 1 remained current, restarted
`providerd`, and reloaded the identical envelope from
`artifacts/state-probe/state.json`. On the phone, the APK booted from remote
revision 1/schema 2. A real drag produced externally promoted revisions through
43. Injected `BeforePromote` caused `experience_state_rejected` without
accepting that transition. Injected `AfterPromote` returned an error after the
write; the client reloaded and reconciled the exact revision/state, reporting
revision 50 as promoted rather than duplicating or discarding it.

This gate is also **partial**. The gate APK retains an offline linked fake
provider fallback and a local state mirror. Migration is unit-tested but not
yet tied atomically to candidate source/surface promotion. Provider side effects
and state promotion do not share a two-phase transaction. Finally, continuous
drag currently commits every intermediate coordinate, which is correct but
needlessly expensive and should be coalesced.

## Artifact and decision

The final tested dirty APK `sos-experience-8ec147677706-dirty.apk` is
37,256,773 bytes
with SHA-256
`aacdc9fc8545ee008c8a542c1f3ff7f2928a688d13071204796e3d2031e0aa44`.

Gate B now has a reproducible baseline and a clear improvement path. The
latency portion of gate E passes its 100 ms target in the confirming run. Gates
C and D advanced materially but remain partial for the reasons above. Continue
on Android until a real GPUI candidate process can be promoted by a fixed
supervisor and state migration/effects participate in the same revision
transaction.
