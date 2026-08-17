# SOS working agreement

## Progress documentation is part of the work

Every material experiment or architectural change must update
`docs/progress.md` in the same commit. Record the dated goal, changed code or
environment, evidence (including commands and measurements), failures and
rejected approaches, decision, remaining risks, and next gate. The Sol writer
normally makes this update with the related code; do not create a separate
documentation agent unless the documentation is itself substantial.

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
  Keep a coherent implementation in one Sol task instead of serial tiny tasks.
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
device owner. Never run concurrent `adb` or `a33xctl` commands.

For a device/hardware gate, spawn one Terra gate with the acceptance criteria
and authorization envelope. Terra issues one bounded transaction to one Luna
runner and judges the returned evidence. The parent and Terra each use one
long, event-driven agent wait per phase; no minute polling, status-only
follow-ups, or repeated "still running" updates. Agents communicate only on a
state change, failure, approval boundary, or completion.

Every device transaction brief must name the serial, exact operation, terminal
conditions, evidence paths, and any artifact path plus expected revision,
size, and SHA-256. Reboot or sideload is forbidden unless the brief explicitly
authorizes that exact operation for that serial and artifact. A transaction
may attempt sideload at most once; an unresolved earlier sideload/recovery
process is a stop-and-escalate condition. The runner follows inherent
auto-reboot transitions and performs the specified soak without asking for a
false manual reboot. Any extra manual reboot requires separate explicit
authorization. Report elapsed time only from command/tool output or captured
monotonic timestamps, never from wait/yield/timeout values.

## Efficiency targets

- coordination/model overhead: under 15% of task cost;
- waits: one event-driven wait per phase;
- device ownership violations and inferred durations: zero;
- false manual reboot requests: zero.
