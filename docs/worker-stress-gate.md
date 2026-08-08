# Luau worker and 1,000-swap gate

## Result

The gate is **confirmed on the Samsung SM-A336B**. Luau compilation,
evaluation, update handling, and runtime destruction now happen on one dedicated
worker thread. GPUI receives only owned Rust model, event, state, and `UiNode`
values. A frame-paced run completed 1,000 transactional swaps in the same
process without a rejection, crash, or positive end-to-start RSS delta.

This confirms that interpreted experience mutation can stay comfortably within
a conversational latency budget on this phone. It does not establish a
production security boundary, compositor-level photon latency, or long-duration
leak freedom.

## Runtime boundary

```text
GPUI thread                              sos-luau-runtime thread
-----------                              -------------------------
source + model + state ── prepare ─────→ compile, evaluate, decode
                         ← prepared ───── candidate UiNode + timings
persist accepted source ── commit ─────→ atomically replace active VM
                         ← committed ─── active UiNode
render new tree
GPUI post-render callback ──────────────→ record source-to-visible upper bound
```

The worker owns the non-`Send` `mlua::Lua` value for its entire lifetime.
Messages cross unbounded `async-channel` queues as owned Rust data. Candidate
replacement is a two-phase prepare/commit protocol: the old VM stays active
until the candidate has compiled, rendered, decoded, validated, and—during a
normal source load—its accepted source has been persisted. Actions use the same
worker and return a new JSON-like state plus a validated tree.

Startup still waits for the worker's initial tree before opening the experience.
No Luau executes on the GPUI thread, but eliminating that cold-start wait is a
separate lifecycle optimization.

## Telemetry definition

For each candidate the app records:

- queue time, from GPUI submission until the worker begins;
- compile time, including text-only Luau module loading;
- render time, including Luau evaluation, bounded table decoding, and IR
  validation;
- worker total; and
- source-to-visible, from GPUI submission until GPUI's `on_next_frame`
  callback, which GPUI documents as running directly after the current frame is
  rendered.

The last value includes queueing, worker work, message delivery, GPUI tree
construction, and a frame boundary. It begins after the app has read the
candidate; it excludes workstation-to-device file transfer and deep-link
delivery. It is the best in-process visible-frame signal currently exposed by
GPUI, but it is not an Android compositor-present timestamp or a camera-based
photon measurement.

## Physical-device evidence — 2026-08-08

Target: Samsung SM-A336B, Android API 35, `arm64-v8a`.

The production Android log proved thread separation:

```text
runtime_worker_ready ui_thread=ThreadId(2) worker_thread=ThreadId(11) initialize_us=6642
```

A normal `experiences/timeflow.luau` load in the installed app measured:

```text
source_to_visible_us=29349 queue_us=253 compile_us=1202 render_us=779 worker_total_us=1983
```

The reproducible stress command alternates the accepted experience with the
embedded alternative. It submits the next candidate only after the prior
candidate's GPUI post-render callback and always restores the original source
on the final iteration:

```sh
./tools/sosctl stress 1000
```

Result:

| Measurement | Value |
| --- | ---: |
| Accepted / rejected | 1,000 / 0 |
| Total duration | 18.150 s |
| Source-to-visible p50 | 17.073 ms |
| Source-to-visible p95 | 20.618 ms |
| Source-to-visible p99 | 20.815 ms |
| Source-to-visible maximum | 29.803 ms |
| Worker-total p95 | 2.936 ms |
| RSS start | 274,016 KB |
| RSS peak | 274,080 KB |
| RSS end | 259,180 KB |
| Reported positive RSS delta | 0 KB |

The RSS result shows no monotonic growth in this run; process RSS is noisy and
one 18-second sample is not proof that no leak exists.

After the stress run, a hostile infinite-loop candidate was interrupted after
its render budget. PID `30200` stayed unchanged and the SHA-256 of
`experience.active.luau` remained
`7193e43a278a5c4c430baa1488f7a5865ef35c53b120d3828e88fdc989885ba6`:

```text
script_rejected ... queue_us=163 compile_us=521 render_us=20298 worker_total_us=20820
```

Twenty subsequent home/resume cycles also retained PID `30200` and logged no
panic or fatal error.

Host regression evidence:

```sh
cargo test -p runtime-luau
```

All seven tests passed. The new worker test verifies a distinct owning thread,
prepare/commit replacement, hostile-candidate rejection, and continued actions
through the previously committed runtime.

The device APK produced while this gate was under development is
`artifacts/sos-experience-074d8c5738d5-dirty.apk`, 34,678,451 bytes, SHA-256
`77265416cda78e12ec3b65ca6d9e912fbdb0d65bdb5a02fb8440916933b9b249`.
A clean revision artifact is recorded in the progress ledger after publication.

## Failures and fixes

- The first stress deep link used an unescaped `&`. Android's remote shell
  treated the remainder as a separate command and reported `/system/bin/sh: -n:
  inaccessible or not found`. Escaping the separator in `sosctl` fixed the
  transport; a 10-swap shakedown then passed before the 1,000-swap run.
- Internal VM timings alone were insufficient. Stress is now sequentially paced
  by a GPUI post-render callback, preventing the harness from measuring only
  queue throughput.

## Decision and next gate

Continue with Luau + typed IR + GPUI. Compilation/evaluation is no longer a
frame-blocking architectural objection, and observed frame-confirmed latency is
well below the earlier 100 ms prototype threshold.

The next gate should add native text input, focus/IME, image, animation, and
accessibility primitives through concrete generated experiences, while adding
longer soak tests and per-run RSS sampling. Before any untrusted or remote
source is admitted, the sandbox still needs a security review and a clear
process-isolation decision.
