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
