---
name: sos-device-gate
description: Run and judge complete SOS physical-device acceptance gates on the Samsung SM-A336B with adb and tools/a33xctl. Use when asked to build, inspect, install, sideload, boot, verify, soak, collect evidence for, or diagnose a Core or Compat hardware artifact. Do not use for desktop-only unit tests or Linux acceptance.
---

# SOS device gate

Run one complete device lifecycle without serial multi-agent relays or
confirmation pauses. Preserve one device owner, observe automatic transitions,
and accept hardware behavior only from finalized evidence.

## Establish the gate

1. Resolve the target serial, product, acceptance criteria, evidence directory,
   and artifact path. Derive the expected revision, byte size, and SHA-256 from
   the inspected artifact when they are not supplied.
2. Inspect the artifact with the matching `./tools/a33xctl inspect-*` command
   before installation. Do not install an uninspected or mismatched artifact.
3. Check for another device owner and live `adb`, `a33xctl`, sideload, or
   Recovery process. Adopt an operation already in progress when its identity
   is clear; never start a competing operation.
4. Treat the user's requested gate as the complete operating scope. Recovery
   entry, sideload, automatic or necessary reboot, readiness observation, soak,
   evidence capture, and evidence-driven retry proceed without intermediate
   questions. Do not tell the user to select Reboot unless captured device
   state proves it is actually waiting for manual input.

## Execute one lifecycle

Keep one process or agent as the device owner from preflight through terminal
evidence. Let the harness decide whether delegation helps; if it delegates the
transaction, use one bounded device worker rather than an agent hierarchy.

1. Capture a monotonic start timestamp and initial device/transport state.
2. Enter the required transport and install the inspected artifact.
3. Observe inherent Recovery and reboot transitions continuously with a long,
   event-driven command or wait. Do not convert automatic reboot into a user
   handoff and do not poll another agent for unchanged status.
4. Evaluate the product-specific readiness predicate below.
5. Run the requested interaction, restart, recovery, provider, or soak checks.
6. Capture raw logs, properties, process state, screenshots, crashes, and AVCs
   needed by every acceptance criterion.
7. If a step fails, identify the earliest evidenced failure. Retry only when
   evidence explains the failure and the next attempt is safe; never blind-loop.
8. Finish at PASS, an evidenced product failure, or a genuinely unknowable
   device state. A design or code failure returns to one coherent implementation
   owner rather than creating a new worker for each symptom.

Do not wipe, change slots, use bootloader/Download mode, or operate on a
different serial unless the user's requested task requires it.

## Select readiness correctly

- Core 1/no-Zygote must not use `sys.boot_completed` or `dev.bootcomplete`.
  Require the exact revision, `SOS Core Experience` surface, live supervisor and
  experience child, authority and platform adapter, the current native
  lifecycle marker, and no relevant crash or enforcing SOS AVC. Prefer
  `./tools/a33xctl inspect-core1-readiness --serial SERIAL
  --expected-revision REVISION` where it covers the gate.
- Android/Compat requires the exact revision, `sys.boot_completed=1`, its
  expected HOME/surface, required Android and SOS processes, and the same
  crash/AVC scrutiny.

Never mark a device or latency criterion complete from host-only evidence.

## Finalize evidence

Close every evidence file before creating its manifest. Generate and then
independently verify the deterministic manifest:

```sh
./tools/a33xctl evidence-manifest-generate --root EVIDENCE_DIR --output MANIFEST
./tools/a33xctl evidence-manifest-verify --root EVIDENCE_DIR --manifest MANIFEST
```

The manifest must exclude itself and temporary outputs and list every finalized
file's path, byte size, and SHA-256. Report measured wall time, artifact
identity, criterion-by-criterion results, evidence paths, risks, and the next
product gate. Update `docs/progress.md` for a material device experiment.
