# SOS working agreement

## Progress documentation is part of the work

Every material experiment or architectural change must update
`docs/progress.md` in the same commit. An entry should record:

- the date and concrete hypothesis or goal;
- the code, device, or environment that changed;
- commands and measurements that constitute evidence;
- failures and fixes, including approaches that were rejected;
- the resulting decision, remaining risks, and next gate.

Do not mark a hardware or latency gate complete from a desktop-only test. Keep
raw generated artifacts out of Git, but record the artifact filename, revision,
size, and SHA-256 when one is part of the evidence.

Use `docs/runtime-evaluation.md` for runtime-selection reasoning,
`docs/experiment.md` for the original GPUI Mobile hardware gate, and focused
documents for detailed milestone reports. `docs/progress.md` is the concise
chronological index linking those deeper records.

## Multi-agent (required)

The parent thread is the orchestrator. It must not run long CLI itself.

Custom Codex agents live in `.codex/agents/`. Spawn them by `name`:
`implementor` (gpt-5.6-sol, high) for repo patches, `runner` (gpt-5.6-luna,
medium) for host tests and allowlisted device recipes.

For every implementation task:

1. Spawn `implementor` with a bounded brief (files, invariant, done-when).
2. Wait for its postcard. Do not ingest diffs.
3. Spawn `runner` with the implementor's Verify recipe, or a named
   `a33xctl inspect-*`.
4. Review outcomes (postcard + screenshot path). If `escalate` is screenshot
   or sepolicy, the parent may `view_image` or reason; it still must not poll
   `adb`.
5. `close_agent` when done.

Never spawn two writers. Never spawn two runners. Never give `runner`
reboot/sideload unless the user asked. Parent model and effort are chosen
per thread at start; do not assume a project default.
