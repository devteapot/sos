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
change, commands and measurements, failures, decision, and next gate—not the
agent orchestration details. Changing this local workflow policy does not
itself require a `docs/progress.md` entry.

Do not mark a hardware or latency gate complete from desktop-only evidence.
Keep generated artifacts out of Git, but record an evidence artifact's path,
revision, byte size, and SHA-256. Use `docs/runtime-evaluation.md` for runtime
selection, `docs/experiment.md` for the original GPUI Mobile hardware gate,
and focused milestone reports where needed; `docs/progress.md` stays the
concise chronological index.

## Agent workflow v2

The parent coordinates short phases and never runs long CLI or device work.
Custom agents in `.codex/agents/` have distinct ownership:

- `implementor` (Sol/high) is the only writer. Use it for architecture,
  ambiguous diagnosis, high-risk review, and complex or related code changes.
  Keep a coherent implementation milestone in one Sol task instead of serial
  tiny tasks. After that milestone completes, a new root cause discovered by a
  hardware run gets a fresh Sol implementor; explicitly finish the prior
  writer first and never overlap writers.
- `gate` (Terra/medium) owns acceptance criteria, runner briefs, evidence
  judgment, and coordination for every device or hardware gate. It does not
  edit files or touch the device.
- `runner` (Luna/medium) is the only device owner and executes one complete,
  bounded host/device transaction, including authorized automatic transitions,
  boot observation, and soak. It does not design or edit.

For an implementation, give Sol files, invariants, acceptance conditions, and
required evidence. The brief must explicitly say whether `docs/progress.md` is
required. Safe read-only exploration or unrelated host verification may run
concurrently when independent, but there is never more than one writer or one
device owner. Never run concurrent `adb` or `a33xctl` commands. Named custom
roles must be spawned with `agent_type` and `fork_turns: "none"`; do not retry
the invalid full-history plus named-role form.

For a device/hardware gate, spawn one Terra gate with the acceptance criteria
and authorization envelope. Terra activates one Luna runner, which remains the
sole device owner for the complete lifecycle of the same artifact and
authorization envelope. At later state-change boundaries Terra follows up or
reactivates that same runner instead of spawning observers. A replacement is
allowed only when the artifact/envelope changes or the runner is genuinely
unusable, and only after ownership is explicitly released. The parent and
Terra each use one long, event-driven agent wait per actual phase; no minute
polling, status-only follow-ups, or repeated "still running" updates. Agents
communicate only on a state change, failure, approval boundary, or completion.

Every device transaction brief must name the serial, exact operation, terminal
conditions, evidence paths, and any artifact path plus expected revision,
size, and SHA-256. User authorization of that exact envelope covers routine
entry into the required Recovery/sideload transport, one sideload attempt, its
inherent automatic reboot, readiness observation, and the specified soak; do
not pause for authorization between those inherent transitions. A different
artifact or serial, wipe, slot change, bootloader/Download mode, extra manual
reboot, or second sideload attempt always requires new explicit authorization.
An unresolved earlier sideload/recovery process is a stop-and-escalate
condition. Report elapsed time only from command/tool output or captured
monotonic timestamps, never from wait/yield/timeout values.

Readiness predicates are product-specific. Core 1/no-Zygote must never use
`sys.boot_completed` or `dev.bootcomplete`: require the exact expected revision,
the `SOS Core Experience` surface, running supervisor and experience child,
running authority and platform adapter, the current native lifecycle marker,
and no relevant crash or enforcing SOS AVC. Android/Compat instead requires
the exact revision, `sys.boot_completed=1`, its expected HOME/surface and
required Android plus SOS processes, with the same crash/AVC scrutiny.

Evidence comes only from command/tool output and captured files, never
model-written summaries or timeout/yield inference. Finalize and close every
evidence file before generating a deterministic manifest. Each manifest lists
path, byte size, and SHA-256, excludes itself and temporary outputs, is written
atomically, and must be independently verified before PASS. Terra owns the
host-side criterion and evidence audit; the Sol parent reviews only ambiguous
or high-risk failures, avoiding a serialized duplicate audit.

## Efficiency targets

- coordination/model overhead: under 15% of task cost;
- waits: one event-driven wait per phase;
- device-runner sessions per artifact/authorization envelope: 1;
- avoidable authorization pauses: zero;
- product-readiness timeout classification errors: zero;
- evidence-manifest verification failures at PASS: zero;
- record measured wall time and model-weighted cost per gate;
- device ownership violations and inferred durations: zero;
- false manual reboot requests: zero.
