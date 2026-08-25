---
name: sos-pikvm-workflow
description: Safely operate an SOS Linux hardware target through PiKVM virtual media, video, OCR, HID, ATX, and target SSH. Use when asked to discover or inspect a PiKVM, upload or replace an ISO, attach read-only boot media, enter firmware or a boot menu, control a locked or login console, verify a live boot, protect an installed disk, or collect PiKVM-backed physical evidence. Do not use for Samsung/adb gates, QEMU/KVM guests, or ordinary SSH-only Linux acceptance.
---

# SOS PiKVM workflow

Treat PiKVM and its target as one serial external-state transaction. Keep one
console owner, observe every transition, and distinguish PiKVM state from
target state. PiKVM virtual-media upload is not flashing the target.

## Establish the control envelope

1. Resolve the PiKVM endpoint, target machine identity, requested operation,
   exact artifact, acceptance criteria, evidence directory, and installed disks
   that must be preserved.
2. Announce console ownership. If the user interacts while a transition is in
   progress, stop the pending input sequence, capture fresh state, and adopt
   the user's observed state before continuing. Never issue concurrent PiKVM,
   target SSH, power, HID, or virtual-media mutations.
3. Keep credentials out of commands, agent messages, screenshots, OCR output,
   shell history, Git, metadata, and evidence. Use an existing credential store
   or a mode-`0600` task-private curl configuration populated through hidden
   input; never put a literal secret in `-u`, a URL, a header, or an environment
   assignment shown in a tool call. Delete temporary credential material and
   unset task-specific variables at the terminal boundary.
4. Read PiKVM video, HID, virtual-media, and ATX state before changing anything.
   Classify each surface as `verified`, `unverified`, or `unavailable` for this
   campaign. Do not infer target power from an ATX LED, `acts`, a blank frame,
   or a successful HTTP response.
5. Capture a console frame and inspect the image itself. OCR is supporting
   evidence only; restrict its region and never OCR a field that may contain a
   credential.

Never mask a required probe or mutation with `|| true`. Preserve its exit
status and raw response. A transport-success response proves only that PiKVM
accepted the API call, not that the target changed state.

## Protect storage and firmware

- Never launch an installer, select an installation target, mount an installed
  disk read-write, run a block writer, change boot order, wipe, repartition, or
  change firmware settings unless the user explicitly put that action in scope.
- If the task requires a physical USB write, hand that operation to
  `$sos-linux-acceptance` and require one exact, unambiguous removable target.
  PiKVM MSD storage is a different device and must not be described as that USB.
- Prefer a one-time firmware boot selection. Require the visible PiKVM CD-ROM
  label before selecting it. Never choose an entry by remembered position.
- If an installer appears unexpectedly, reset HID, close it with one observed
  action when safe, capture the resulting frame, and audit mounts and writer
  processes. If its state is uncertain, stop rather than sending more keys.

## Prepare read-only virtual media

1. Verify the local artifact's revision, classification, byte size, SHA-256,
   and embedded media check before upload. Reject a dirty, mismatched, or
   promotion-ineligible artifact when the requested gate requires otherwise.
2. Capture the existing MSD state. Disconnect the virtual drive before upload;
   do not delete or replace an image whose identity is uncertain.
3. Upload once with one long, event-driven command and capture measured wall
   time. Do not poll merely to report unchanged progress.
4. Require PiKVM to report the stored image complete with the expected byte
   size. Verify its SHA-256 through authorized PiKVM SSH before attaching it;
   if full remote hashing is unavailable, leave artifact identity open.
5. Select the verified image with CD-ROM enabled and writes disabled, connect
   it, then read the MSD state back. Require the exact image, byte size,
   `complete=true`, `connected=true`, `cdrom=true`, `rw=false`, and
   `writable=false` where the API exposes those fields.

Do not claim that read-only virtual media protects the internal disk from an
installer. It proves only that the emulated media itself is not writable.

## Drive an observed console state machine

For each semantic action, capture and inspect the frame, state the expected
next state, send the smallest input, wait for that predicate, then capture and
inspect the result. Do not chain speculative Tab/Enter sequences across an
unknown focus state.

### Calibrate HID

1. Reset HID before a critical sequence so no modifier remains pressed.
2. In a harmless visible field or console, enter a non-secret sentinel and
   verify every character. Calibrate the exact helper, keyboard layout, and
   boot; success from an earlier boot does not carry forward.
3. If bulk print or shortcut input loses, duplicates, or redirects input, mark
   that helper unavailable for the rest of the boot. Use explicit key-down and
   key-up events, followed by HID reset, or stop.
4. Never enter a secret until the non-secret calibration and focus are visibly
   correct. Never use secret text as the transport test.

### Reboot and enter firmware

- Use target SSH for a graceful reboot when its identity is proven. Use a
  visibly confirmed OS power action second.
- Use ATX only after one calibration has correlated its telemetry and action
  with an observed target transition. On the first mismatch, mark ATX
  unavailable for the entire campaign; do not retry it later under another API
  spelling.
- Do not use Magic SysRq, forced-reset shortcuts, long power presses, or other
  emergency reboot paths without explicit user authorization for that risk.
- Start one bounded firmware-key window only after a confirmed reboot. Send no
  keys other than the intended boot-menu key, stop as soon as the menu is
  visible, and capture the result. If one reboot misses the menu, do not
  escalate by spamming more keys or repeating reboots; record the console-wiring
  limitation and request local selection or a configuration fix.

### Apply circuit breakers

- After one HID calibration mismatch, retire that helper for the boot.
- After one ATX mismatch, retire ATX for the campaign.
- After two failed transitions from the same observed state, stop the complete
  flow and diagnose the earliest layer with `$sos-runtime-debug`.
- When target state becomes unknowable, stop state-changing input. Continue
  only read-only capture and clearly bounded discovery.

## Verify the boot in dependency order

Do not start with a broad subnet scan. Derive the target address from the live
console, a known target MAC/DHCP record, or a narrowly scoped neighbor check.
Then verify:

1. PiKVM visibly shows the expected live environment or login surface.
2. Network link, address, route, and DNS are valid on the target.
3. SSH is enabled, active, listening, and reachable when required.
4. Image identity and source revision match the uploaded artifact exactly.
5. Root uses the expected live overlay rather than an installed root.
6. Every protected internal disk and partition is unmounted, and no installer
   or block writer is active.
7. The requested SOS session, interaction, restart, soak, or latency predicates
   pass on this physical boot.

Stop at the earliest failed layer. Do not infer network success from a visible
desktop, SSH configuration from a baked symlink, or a physical boot from host
tests. A manual boot-menu selection keeps unattended boot open even when the
image and runtime checks pass.

## Finalize evidence

Preserve pre/post API state, inspected screenshots, safe OCR, upload timing,
local and PiKVM-side artifact identity, the final MSD record, target SSH audit,
and measured gate duration. Finalize every file before generating and
independently verifying the manifest:

```sh
./tools/a33xctl evidence-manifest-generate --root EVIDENCE_DIR --output MANIFEST
./tools/a33xctl evidence-manifest-verify --root EVIDENCE_DIR --manifest MANIFEST
```

Report criterion-by-criterion results, manual interventions, retired control
surfaces, artifact identity, evidence paths, measured wall time, remaining
risks, and the next gate. Update `docs/progress.md` for a material physical
experiment, recording product/environment facts without credentials or agent
workflow details. Maintaining this skill alone does not require a progress
entry.
