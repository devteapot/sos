---
name: sos-runtime-debug
description: Diagnose and resolve SOS runtime failures across transport, credentials, process lifecycle, IPC, networking, model providers, revision validation, rendering, and commit. Use for boot loops, provider failures, resident-agent failures, credential/login problems, missing services, failed generated experiences, rollback, or repeated end-to-end gate failures on Linux, Android, or Core.
---

# SOS runtime debugging

Keep one coherent diagnosis and implementation owner across related symptoms.
Do not reset context or start a fresh worker whenever a downstream error reveals
the next layer of the same failure.

## Find the earliest broken layer

Trace the path in dependency order and stop at the first layer without valid
evidence:

1. Device or host transport and process reachability.
2. Credential presence, format, ownership, expiry, and provider selection.
3. Service launch, supervision, restart behavior, and stale process/socket state.
4. Local IPC request/response framing and timeouts.
5. DNS, routing, TLS, and remote endpoint reachability.
6. Provider request model, response status, rate limits, and schema.
7. Agent output and source extraction.
8. Revision validation, compile, render, staging, commit, and rollback.
9. Visible UI and interaction behavior.

Do not infer an upstream success from a downstream symptom. Capture the exact
command, exit status, timestamp, earliest error, and relevant raw log before
forming a fix hypothesis. Never expose credential contents in tool output,
patches, evidence, or documentation.

## Work host-first

Prefer the narrowest deterministic reproduction before a full hardware or VM
campaign. Inspect existing logs and tests, then run focused package tests,
shell syntax checks, contract checks, fake-provider paths, or
`./tools/sosctl linux-agent-test` as applicable. Change only the smallest layer
supported by evidence and verify that layer before returning downstream.

Use delegation only for independent read-only investigation or unrelated host
checks that can return a concise result. Let the harness select whether that is
worthwhile. Keep the serial diagnosis, patch, and retest loop with one owner;
do not build a serial agent relay around it.

## Apply the circuit breaker

After two failed full end-to-end attempts for the same objective:

1. Stop rerunning the complete campaign.
2. Compare the attempts and identify the earliest common failed layer.
3. Build a focused reproduction for that layer.
4. Fix and pass the focused reproduction plus nearby regressions.
5. Run one downstream end-to-end attempt with fresh, bounded evidence.

Do not spend retries proving the same failure and do not treat a timeout as a
measured duration or product diagnosis.

## Close the loop

Record the causal chain, rejected hypotheses, changed code/environment, exact
verification, remaining risks, and next gate. Update `docs/progress.md` with
the product or environment result for every material runtime experiment or
architectural change; omit orchestration details.
