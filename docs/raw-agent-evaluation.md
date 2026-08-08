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
