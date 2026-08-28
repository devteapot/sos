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
2. Verify host access will survive every expected USB identity and
   re-enumeration. Prefer the durable rule managed by
   `./tools/a33xctl install-host-usb-rules --group GROUP`; a one-device ACL is
   only a diagnostic and must not be treated as lifecycle readiness. Installing
   the rule still requires the user's authority for the root change.
3. Inspect the artifact with the matching `./tools/a33xctl inspect-*` command
   before installation. Do not install an uninspected or mismatched artifact.
4. Check for another device owner and live `adb`, `a33xctl`, sideload, or
   Recovery process. Adopt an operation already in progress when its identity
   is clear; never start a competing operation.
5. Resolve the control path before installation: current ADB authorization,
   supported Recovery entry, automatic reboot behavior, and UI input. For a
   Core userdebug or eng build, determine whether the current build exposes the
   bounded input service. After installing the candidate, require
   `./tools/a33xctl core-input-status --serial SERIAL` before a touch-driven
   campaign. Automation evidence must be labeled synthetic and cannot prove a
   physical touchscreen.
6. Treat the user's requested gate as the complete operating scope. Recovery
   entry, sideload, automatic or necessary reboot, readiness observation, soak,
   evidence capture, and evidence-driven retry proceed without intermediate
   questions. Do not tell the user to select Reboot unless captured device
   state proves it is actually waiting for manual input.

Read only the relevant host-USB, artifact-profile, Recovery, and acceptance
sections of `docs/samsung-sm-a336b.md` for the requested gate.

## Execute one lifecycle

Keep one device owner from preflight through terminal evidence. Treat the
evidence root as the campaign cursor and record any missing live-operation
fields in an ignored cursor beside it. On resume, read the product, revision,
artifact digest, serial, input mode, completed stages, active operation, and
expected next event, then compare them with the live device before issuing
another command.

1. Capture a monotonic start timestamp and initial device/transport state.
2. Enter the required transport and install the inspected artifact. Prefer
   `adb reboot sideload-auto-reboot` only when the currently installed build is
   documented and observed to support that transition. Otherwise capture the
   exact Recovery state before requesting the smallest manual step.
3. Observe inherent Recovery and reboot transitions continuously with a long,
   event-driven command or wait. Do not convert automatic reboot into a user
   handoff. Start one observer for each transition and wait for its phase marker
   or terminal result; do not poll merely to report unchanged state.
4. Evaluate the product-specific readiness predicate below.
5. Run the requested interaction, restart, recovery, provider, or soak checks.
6. Capture raw logs, properties, process state, screenshots, crashes, and AVCs
   needed by every acceptance criterion.
7. If a step fails, identify the earliest evidenced failure. Retry only when
   evidence explains the failure and the next attempt is safe; never blind-loop.
8. After two rejected physical artifacts or two late failures in one campaign,
   apply `$sos-runtime-debug` to every remaining criterion and the packaged
   activation boundary before building or installing another candidate. A new
   late symptom does not reset this budget.
9. Finish at PASS, an evidenced product failure, or a genuinely unknowable
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

## Run composition acceptance

For a requested v4 composition campaign, use
`./tools/a33xctl capture-v4-composition-stage` for ordered checkpoints and
`./tools/a33xctl audit-v4-composition-campaign` for the final verdict. Use
`--input-mode automation` only when `core-input-status` has verified the
debug-only Core boundary. Use `--input-mode physical` only for real device
input. Do not relabel one as the other.

Use `./tools/a33xctl restart-v4-authority` for the bounded authority recovery
criterion instead of hand-written process control. If manual input is genuinely
required, arm the observer first, state why automation cannot satisfy the
criterion, and consolidate the interaction into the shortest safe sequence.

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
