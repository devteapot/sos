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

1. Device or host transport, durable device-node permissions, and reachability.
2. Credential and grant presence, format, ownership, expiry, and selection.
3. Source build and cross-target compilation.
4. Packaged artifact contents, build-variant predicates, signatures, and policy.
5. Deployment or installation manifest, remote hashes, configuration, and
   migrations.
6. Service launch, supervision, restart behavior, and stale process/socket state.
7. Local IPC request/response framing and timeouts.
8. DNS, routing, TLS, and remote endpoint reachability.
9. Provider request model, response status, rate limits, and schema.
10. Agent output and source extraction.
11. Revision validation, render, staging, commit, and rollback.
12. Visible UI and interaction behavior.

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

A green source test does not close a packaged or deployed failure. Before the
next hardware artifact or physical login, inspect the actual package and prove
its init or service activation predicate, deployed bundle and configuration,
gate allowlist, required credentials and grants, and relevant SELinux or file
permissions. Add the focused check at the layer that previously allowed the
bad state through.

## Apply the circuit breaker

After two failed full end-to-end attempts, or two late failures that require a
new artifact or deployment in one campaign:

1. Stop rerunning the complete campaign.
2. Compare every attempt and identify the earliest failed layer in each. A new
   downstream symptom does not reset the campaign budget.
3. Enumerate the remaining acceptance criteria and build focused reproductions
   for the failed layers plus a packaged-state closure check.
4. Fix and pass those reproductions and nearby regressions.
5. Produce one coherent candidate and run one downstream attempt with fresh,
   bounded evidence.

Do not spend retries proving the same failure and do not treat a timeout as a
measured duration or product diagnosis.

For a long or compacted campaign, resume from the gate's evidence metadata or
an ignored cursor containing the exact revision, artifact digest, target,
transport, phase, live operation, evidence root, and expected next event.
Compare it with current external state before mutating anything. Do not rebuild
this state from conversational memory.

## Close the loop

Record the causal chain, rejected hypotheses, changed code/environment, exact
verification, remaining risks, and next gate. Update `docs/progress.md` with
the product or environment result for every material runtime experiment or
architectural change; omit orchestration details.
