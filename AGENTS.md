# SOS working agreement

## Progress documentation is part of the work

Every material experiment or architectural change must update
`docs/progress.md` in the same commit. Record the dated goal, changed code or
environment, evidence (including commands and measurements), failures and
rejected approaches, decision, remaining risks, and next gate. The Sol writer
normally makes this update with the related code; do not create a separate
documentation agent unless the documentation is itself substantial.

`docs/progress.md` is a product and experiment evidence ledger. Do not add
entries solely about local development workflow administration, including
Codex/T3Code orchestration, agent prompts, roles or model routing, token or
cost optimization, `.codex` configuration, AGENTS-only policy changes, or
similar local tooling. If a workflow change accompanies material product,
build or device experimentation, record only the product or environment
change, commands and measurements, failures, decision, and next gate. Omit the
agent orchestration details. Changing this local workflow policy does not
itself require a `docs/progress.md` entry.

Do not mark a hardware or latency gate complete from desktop-only evidence.
Keep generated artifacts out of Git, but record an evidence artifact's path,
revision, byte size, and SHA-256. Use `docs/runtime-evaluation.md` for runtime
selection, `docs/experiment.md` for the original GPUI Mobile hardware gate,
and focused milestone reports where needed; `docs/progress.md` stays the
concise chronological index.

## Execution invariants

Use the focused workflows in `.agents/skills/` when their descriptions match
the task. Skills own procedural detail; this file retains only invariants that
apply across workflows. Let the harness decide whether independent work
benefits from delegation. Do not require a fixed agent hierarchy, model,
reasoning level, or agent-per-phase sequence.

Keep one coherent owner for related diagnosis and implementation instead of
starting a new thread for every symptom. There is never more than one writer or
one device owner. Safe read-only exploration and unrelated host verification
may run concurrently when genuinely independent. Never run concurrent `adb` or
`a33xctl` commands.

For a long external-state campaign, keep an ignored durable cursor with the
exact revision, artifact identity, target, transport, phase, live operation,
evidence root, and expected next event. After a continuation or context
compaction, compare that cursor with current external state before mutating
anything. Do not reconstruct device state from conversational memory.

Long commands and device transitions use event-driven waits sized for the
operation. Do not minute-poll agents or tools, send status-only follow-ups, or
replay large context merely to observe unchanged state. Communicate when state
meaningfully changes, a terminal failure needs judgment, or the task completes.

After two failed end-to-end attempts or two late failures that require a new
artifact or deployment in one campaign, stop broad reruns. A different
downstream symptom does not reset that budget. Build focused reproductions and
prove the packaged target state before one fresh downstream attempt.

The user's requested task defines the operating scope. Complete routine
intermediate steps, automatic transitions, evidence collection, and safe
evidence-driven retries without asking for confirmation at every boundary. Do
not silently expand the task to a different device or destructive operation.

Ask the user to interact with a target only when that physical action is the
required evidence or relevant control discovery proves that safe automation is
unavailable. Arm the observer first, state the exact manual boundary, and group
dependent actions into the shortest safe interaction window.

Before an external gate, distinguish source checks from the artifact and target
under test. Verify packaged or deployed bytes, configuration, grants,
credentials, migrations, and activation predicates at the earliest layer that
can prove them. A green source test does not establish target parity.

Evidence comes from command output and captured files, never model-written
facts or timeout/yield inference. Report elapsed time from tool output or
captured monotonic timestamps. Finalize evidence files before hashing or
manifest generation.

## Efficiency targets

- coordination/model overhead: under 15% of task cost;
- waits: one event-driven wait per actual phase;
- device owners per gate lifecycle: 1;
- avoidable user interruptions: zero;
- product-readiness timeout classification errors: zero;
- evidence-manifest verification failures at PASS: zero;
- record measured wall time and model-weighted cost per gate;
- device ownership violations and inferred durations: zero;
- false manual reboot requests: zero.
