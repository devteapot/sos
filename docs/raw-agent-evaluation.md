# Raw single-shot agent evaluation

Date: 2026-08-08

This suite measures the unoptimized baseline of the external authoring loop. It
is not an estimate of the eventual product: the agent receives no tailored
system prompt, skill, examples selected for the request, validation feedback,
visual feedback, or repair turn. The baseline is useful because later prompt,
tooling, retrieval, model, and fine-tuning work can be compared against a fixed
starting point.

## Frozen protocol

- Model: `gpt-5.6-luna`, medium reasoning, through headless Codex.
- Context: the repository, `docs/experience-api.md`, and checked-in experiences.
- Attempt: one source-writing turn with no tests, validation, screenshots, or
  repair.
- Cases: the six prompts and machine-readable expectations in
  `evals/raw-single-shot/cases.json`.
- Output: one complete Luau module per case under the ignored
  `artifacts/evals/<suite-id>/` directory.
- Grading: deterministic compilation/render validation plus requested text,
  conditional music, low-level path/quad counts, phone-safe hit bounds, and a
  simulated reachable `notes.attach_to_event` effect where applicable.

Run the suite from a clean tracked worktree:

```sh
./tools/eval-single-shot
```

The runner stops if the agent edits a tracked file. It records elapsed time,
token usage, source SHA-256, individual checks, and an aggregate summary. Raw
source and transcripts stay out of Git; the immutable suite definition and the
result summary/hashes belong in the progress record.

## Interpretation

A compilation pass is not a task pass. Geometry cases are deliberately scored
for minimal structural evidence rather than visual quality, so passing the
grader is necessary but not sufficient. Selected outputs still require a
physical-device interaction/visual audit. Conversely, a poor baseline does not
reject agent-authored experiences: it identifies the distance that better
context, a purpose-built skill, examples, structured generation, stronger
models, and later repair loops need to close.

Fine-tuning is a future experiment, not a current assumption. It should follow
collection of accepted/rejected revisions and deterministic grader signals, and
only after confirming that a suitable supported base model is available.

## Raw v2 result

Suite `20260808-luna-medium-raw-v2` completed all six cases in 454.403 seconds.
Three sources compiled, and the aggregate deterministic score was 17/40. The
run consumed 659,981 input tokens (522,752 cached) and 22,630 output tokens.
The 7,564-byte ignored `summary.json` has SHA-256
`efbcdb243908268d7bd40e313e620a96c8f91414ee87aaedda809ed2bda6b2d8`.

- `next_focus`: 5/5.
- `cardless_river`, `weather_orbit`, and `quiet_until_lunch`: rejected because
  each invented the unsupported accessibility role `text`.
- `drag_attach`: 6/8; it generated a reachable provider effect, but no quad
  commands and its note hit region exceeded the phone-safe initial band.
- `tension_map`: 6/7; visually nontrivial paths and safe hit regions, but no
  quad commands.

The three compilable sources were also presented on the SM-A336B. The ignored
screenshots are `next_focus-phone.png` (148,486 bytes, SHA-256
`b80a5265fd8fc3aef3d81d7b0ce84908cffd079fbeea7a96a571e30e0838919a`),
`drag_attach-phone.png` (123,320 bytes,
`9b573e76c918d7df3d0f4b6fe26e6ed4cc560e9ad4da4a5cb5eaafdb408c3209`),
and `tension_map-phone.png` (193,702 bytes,
`cc8187d6c6caffaa08b03445a4244feb9d7ca7659bffc32b13404242775e1562`).
They confirm invented compositions, not polished product quality. Existing
durable state made the first two show a previous attachment, which is useful
evidence that state schemas and migration must be revision-aware.

This baseline supports the user's expectation: the failures are largely
contract/context errors, while the hard drag case already generated the typed
effect. The next comparison should add a small schema-aware authoring skill and
curated examples while leaving these prompts and graders unchanged.
