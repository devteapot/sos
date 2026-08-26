---
name: sos-linux-acceptance
description: Run and judge an explicit SOS Linux stable-host acceptance campaign. Use when asked to gate, accept, soak, or make a milestone verdict for the Wayland/GPUI host, resident Pi agent, GDM session, lifecycle recovery, or Debian direct-DRM VM. Do not use for ordinary development iteration, focused live-overlay debugging, routine deployment, Android, or Samsung device gates.
---

# SOS Linux acceptance

Treat Linux acceptance as a serial external-state campaign with one coherent
owner, not as one agent or gate per phase. Isolate failures locally, then run
one clean final campaign after every required component is green.

Classify the request before starting. For ordinary development iteration, use
focused checks plus `$sos-runtime-debug` and `$sos-pikvm-workflow` as needed;
do not create a full evidence campaign or issue an acceptance verdict.

## Define the campaign

Resolve the exact revision/worktree, requested criteria, environment, evidence
directory, and which of these surfaces are in scope:

- focused Rust/TypeScript and shell checks;
- `./tools/sosctl linux-agent-test` or a requested live-provider agent path;
- `./tools/linux-compositor/verify-nested`;
- `./tools/install-linux-login-session install` and installed-session behavior;
- lifecycle-owner crash/restart and stale-process recovery;
- `./tools/linux-vm/verify-direct-session` in the disposable Debian VM.

Read only the relevant sections of `docs/linux-stable-host.md`,
`docs/linux-compositor.md`, `docs/linux-vm.md`, and `docs/sos-agent.md`. Do not
rerun historical phases that are outside the requested acceptance criteria.

## Run phase-local checks

1. Confirm prerequisites and capture the source revision and environment before
   mutating installed state or starting the VM.
2. Run cheap checks for the changed components first: focused `cargo test` or
   `cargo check`, service/package tests, `bash -n`, and `git diff --check`.
3. Run each in-scope external phase once. Use long event-driven waits and
   preserve its raw output and monotonic duration.
4. On failure, stop at the earliest failed criterion. Reproduce and fix that
   component with the same implementation owner, then rerun only the focused
   phase. Do not create a new agent, rebuild unrelated context, or restart the
   entire campaign after every failure.
5. Apply the `$sos-runtime-debug` circuit breaker after two failed end-to-end
   attempts for the same objective.
6. Once every component passes locally, run one clean ordered campaign from
   pristine preconditions to prove the combined boundary.

Let the harness decide whether independent host checks merit parallel workers.
Keep installed-session, compositor, lifecycle, live-provider, and VM state
changes serial. Never use an agent merely to supervise another agent or relay
unchanged status.

## Judge evidence

Require command output and captured files for every criterion. A SKIP is open,
not successful, unless the criterion explicitly permits it. Do not claim
physical GPU, latency, seat, DRM, reboot, or recovery behavior from unit tests,
nested software rendering, or a different environment.

Finalize logs before hashing. Record each evidence path, byte size, SHA-256,
measured wall time, exact source revision, criterion result, remaining risk, and
next gate. Independently audit the complete evidence set once; do not serialize
duplicate model reviews of an unambiguous PASS.

Update `docs/progress.md` for a material Linux experiment or architectural
change, recording only product/environment evidence and decisions.
