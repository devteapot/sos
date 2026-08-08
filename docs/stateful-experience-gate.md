# Stateful generated-experience gate

## Result

The gate is **confirmed with two explicit platform constraints** on a physical
Samsung SM-A336B (Android API 35, ARM64). An external coding agent produced a
nontrivial Luau rewrite of the synthetic daily experience; the installed GPUI
host validated it on-device, displayed it in the same process, retained the
editable Unicode note and music state, and rolled back to the prior source
without rolling back user state.

The rewrite path does not compile Rust or replace the APK:

```text
request → Luau source patch → local validation → device candidate
        → worker compile/evaluate → bounded IR → first GPUI frame
        → accept or rollback
```

The two constraints are important:

- the pinned GPUI Mobile port delivers Android keyboard characters through a
  simple global callback, not through GPUI's full marked-text input protocol;
  Unicode content, cursor editing, submit, focus, and persistence work, but a
  complete non-Latin composition lifecycle is not yet proven;
- GPUI Mobile does not expose per-element Android accessibility nodes. The
  prototype publishes the six bounded semantics as one Android-visible window
  description. TalkBack can discover the screen summary, but cannot yet
  navigate each painted GPUI control independently.

## Implemented vertical slice

### 1. Bounded native primitives

The runtime-neutral IR now carries keyed `text_input` and allowlisted `image`
nodes, native `pulse`/`fade_in` animation declarations, and bounded
accessibility roles, labels, values, and hints. Validation caps source, tree,
text, dimensions, animation duration, and semantic strings. Unknown image
assets and native node types are rejected.

The GPUI host owns the actual image decode, animation, focus, shaping, cursor,
selection, clipboard, hit testing, and paint. Luau receives no GPUI entity,
Android object, filesystem, network, or raw pointer.

### 2. Stable keyed state through generated rewrites

[`daily-flow.luau`](../experiences/daily-flow.luau) is a light chronological
screen and [`daily-flow-agent.luau`](../experiences/daily-flow-agent.luau) is a
structurally different dark orbital composition. Both use stable
`id = "note-draft"` and `state_key = "draft"` boundaries while freely changing
the surrounding hierarchy.

Native input entities are cached by stable ID. Rust immediately shadows and
persists text mutations, then coalesces typed events for the Luau worker. A
candidate first renders and validates in a fresh VM; the host swaps only after
commit and restores the keyed focus/callback boundary. Stale action results
cannot overwrite newer native input because the Rust-owned shadow is merged
back before persistence.

### 3. Non-blocking worker lifecycle

Cold startup now opens a small native GPUI loading tree immediately and waits
for worker readiness asynchronously. Compilation, module evaluation, render,
updates, and VM destruction remain on the dedicated `sos-luau-runtime` thread.
The worker can be destroyed and recreated from `sos://worker-restart` while the
accepted GPUI tree remains visible.

### 4. Reproducible agent and stress controls

```sh
./tools/sosctl validate experiences/daily-flow-agent.luau
./tools/sosctl agent-apply experiences/daily-flow-agent.luau
./tools/sosctl rollback
./tools/sosctl worker-restart
./tools/sosctl stress 10000
```

`agent-apply` performs local compile/evaluate/IR validation, transfers only
source, waits for the on-device accepted/rejected result, captures a screenshot,
and reports the accepted source SHA-256. `stress` alternates the two structural
experiences and waits for a GPUI post-render callback after every candidate.

## Physical-device evidence — 2026-08-08

Target: Samsung SM-A336B, Android API 35, `arm64-v8a`.

### Startup and worker recreation

One cold launch logged the native window before worker readiness:

```text
9562.760 runtime_worker_spawned ui_thread=ThreadId(2)
9562.783 SOS experience window is live
9562.784 runtime_worker_ready ui_thread=ThreadId(2)
          worker_thread=ThreadId(11) initialize_us=7431
```

An explicit worker restart changed the owning thread from `ThreadId(11)` to
`ThreadId(12)` in 1.681 ms. PID `6448`, active-source hash, persistent JSON,
visible tree, open keyboard, and subsequent typing were retained.

### Agent rewrite, focus, and rollback

The agent-authored source validated as 5,130 bytes, 37 nodes, one input, one
image, one animation, and six semantics. Its final source-only rewrite was
visible in the same PID in 15.858 ms:

```text
source_to_visible_us=15858 queue_us=121 compile_us=4481
render_us=1322 worker_total_us=5805
```

Before and after the light-to-dark rewrite, the JSON state was byte-for-byte
equivalent and contained the existing UTF-8 note, `playing=true`, focus, and
saved status. Android reported `mInputShown=true`; another keyboard tap changed
the note after the swap. Rollback restored source SHA-256
`539ae81da1396cb676270f7f06af1868e46b0da0b0b8bbc97190e705abb1f338`
in 15.302 ms while retaining the same state.

The Android keyboard also exercised Enter: `save_note` set
`last_saved=true`. A touch on the music control changed `playing=false` to
`playing=true` and updated the published semantics from “Play music, Paused” to
“Pause music, Playing.” A swipe exposed all three calendar events. Two album-art
crops 450 ms apart differed in 1,082 pixels, confirming that the native pulse
animation advanced rather than being a static script value.

### 10,000 swaps while typing

The final-code run injected five keyboard taps while light and dark trees were
alternating. The keyboard remained open and every character reached the keyed
draft. The last coalesced `text_changed` action completed after the stress gate.

| Measurement | Value |
| --- | ---: |
| Accepted / rejected | 10,000 / 0 |
| Total duration | 172.646 s |
| Source-to-visible p50 | 16.997 ms |
| Source-to-visible p95 | 18.092 ms |
| Source-to-visible p99 | 23.062 ms |
| Source-to-visible maximum | 35.533 ms |
| Worker-total p95 | 3.816 ms |
| RSS start | 289,268 KB |
| RSS peak | 294,432 KB |
| RSS end | 281,088 KB |
| Positive end-to-start RSS delta | 0 KB |
| RSS samples | 41 |

RSS rose and fell repeatedly between samples rather than increasing with each
revision. This 173-second run rejects an obvious per-swap leak; it is not a
substitute for an hours-long soak or heap attribution.

### Rejection, lifecycle, and accessibility

An infinite render was interrupted in 20.351 ms and a memory bomb returned a
recoverable error in 13.252 ms. Both left PID, accepted source hash, state, and
visible tree unchanged.

Twenty home/resume cycles started with the keyboard open, reported zero launch
failures, retained PID `6448`, and retained the exact JSON state. The field
accepted another key afterward. Android automatically reopened the visible
keyboard on only 1/20 resumes; tapping the still-focused field reopened it on
every other cycle.

`uiautomator dump` exposed one window description containing all six semantic
records, including the heading, album image, playback button, Unicode note
field, saved/editing status, and weather status.

Host verification passed:

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The workspace has 15 passing unit tests. Android build, install, launch, source
rewrite, rollback, worker restart, hostile candidates, and the stress/lifecycle
matrix were all run against the physical phone; no hardware or latency claim is
inferred from desktop tests.

The clean revision artifact is
`artifacts/sos-experience-8687339234c5.apk`, built from implementation commit
`8687339234c5`, 37,036,955 bytes, SHA-256
`c57255a3f3f42c320726cf4361c1ffffed131b3295f5e24333764fb69e31a372`.
It is 2,358,504 bytes larger than the prior worker-gate APK; the delta includes
the embedded image/input bridge and Android/JNI support. Generated APKs and
screenshots remain outside Git.

## Failures and fixes

- The first native input implementation only registered GPUI's
  `EntityInputHandler`. The Android keyboard appeared, but text never changed:
  GPUI Mobile uses a separate global character callback. A narrow callback-to-
  keyed-entity bridge fixed real keyboard input while retaining GPUI shaping and
  selection.
- The first structural swap kept the cursor and keyboard visible but lost the
  active callback, so the next key did nothing. Candidate commit now snapshots
  the active stable ID and reinstalls that boundary when rebuilding the tree.
- The first long soak revealed that the final coalesced input event could remain
  queued after stress ended. The render loop now drains pending input as soon as
  stress/candidate activity clears; the full 10,000-swap run was repeated on
  that final code.
- Autofocus during the earliest cold frame is not sufficient to make Android
  display the IME reliably, and resume normally hides it. The native field
  remains focused and a tap restores the keyboard; automatic IME restoration is
  deferred rather than simulated.

## Decision and next gate

The proposed success condition is confirmed for a trusted prototype:

> An agent invented a nontrivial custom GPUI experience, the phone interpreted
> and displayed it without an APK rebuild, user state and keyboard editing
> survived the structural revision, and source rollback did not roll back data.

Continue with Luau + GPUI, but do not mistake the current bounded IR for the
end-state UI model. This gate proves a fast stateful composition path; it does
not yet prove the [north-star claim](vision.md) that an agent can invent a
component implementation outside a predefined catalog.

The next gate is generative depth: implement the original “bent time axis” and
drag-a-note interaction using agent-authored geometry, hit testing, state, and a
typed provider action. In parallel, turn the manual `agent-apply` scaffolding
into a request→patch→inspect→self-correct→accept/rollback loop. Once that works,
prove whole native revision process/surface promotion behind a minimal recovery
supervisor and move providers/state across IPC. Those are the Android-exit
criteria. Production isolation, signed delivery, real personal data, polished
per-element accessibility, and a full composition-aware IME remain later gates
while the experiment uses synthetic data.
