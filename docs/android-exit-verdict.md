# Android laboratory exit verdict

Date: 2026-08-08

> Historical evidence: this gate used disposable native candidate Activities.
> SOS subsequently removed native executables and per-revision processes from
> the experience contract; see [`revision-supervisor.md`](revision-supervisor.md).

This report closes the current APK research gate. It does **not** claim that
SOS is already an operating system, production-safe, or independent of Android.
It answers the narrower question: has the application laboratory de-risked the
novel experience/revision thesis enough to begin the privileged AOSP and system
services work described in [`vision.md`](vision.md)?

## Verdict

**Yes: begin the privileged AOSP/system-services phase.** Keep the APK harness
as a regression target, but stop making agent prompt quality the main line of
work. The remaining unknowns are now predominantly low-level ownership of boot,
surfaces, input, services, recovery, and hardware integration—the work the next
phase is intended to expose.

This is permission to start phase 2, not permission to delete the Android
compatibility substrate. The current fixed supervisor still lives in the
accepted Android process, the provider daemon still reaches the phone through
`adb reverse`, and Android still owns task/surface composition. Those are
explicit inputs to the next architecture, not hidden completions.

## What was implemented

- The Java drawing probe was replaced with the complete GPUI/Luau host in an
  isolated `:candidate` process and separate Android task/surface.
- The accepted process validates and migrates a candidate on its dedicated
  Luau worker while the current surface remains usable.
- Source, schema, migrated state, source SHA-256, and provider effects are
  staged together. The candidate promotes the envelope at its first GPUI frame;
  syntax, validation, timeout, stale-state, and native-process failures leave
  the accepted authority intact.
- A Java-side watchdog in the accepted process watches the exact candidate PID,
  restores the accepted Activity after a native `SIGABRT`, terminates cached
  candidate processes before replacement, and suppresses false recovery during
  intentional replacement.
- Reconciliation accepts later same-source state revisions, so interactions in
  a promoted candidate are not mistaken for stale promotion. It reloads the
  authoritative state before recovering the accepted worker.
- Back-to-back requests remain pending during first-revision reconciliation.
  Candidate cleanup is hash-conditional, preventing an older commit from
  deleting or installing a newer source.
- The gate APK is built with `gate-strict`; its Android dependency tree contains
  no `providers-fake` crate. Synthetic resources and state come from the
  external typed service.
- Provider effects are validated and staged with state, then executed exactly
  once only after promotion. Tests include an ambiguous `AfterPromote` fault.

## Physical evidence

All device evidence below is from the Samsung SM-A336B, API 35, ARM64.

### Native candidate and recovery

A normal isolated candidate logged worker validation, opened a real GPUI/wgpu
surface, promoted revision/source/state at first frame, and accepted an
interaction. Killing its native PID with `SIGABRT` retained the accepted PID,
restored its surface, and reconciled the newer same-source state. A marked
candidate killed before first frame left the previously accepted source hash
authoritative.

The final reproducible probes passed both phases with accepted PID `31437`:

- crash-before candidate PID `31533` emitted no candidate frame;
- crash-after candidate PID `31656` reported a GPUI frame, then aborted;
- both logged `accepted_surface_restored` without changing the accepted PID.

An earlier exact post-promotion recovery revealed and fixed a real bug: after
the candidate interacted, state had advanced past `expected + 1`; equality
incorrectly classified it as unpromoted. Reconciliation now requires a later
revision with the exact source hash and reloads that envelope.

Cold isolated-process launch-to-first-frame was 1.03–1.31 seconds in measured
runs. This is not the normal Luau mutation latency; it includes a fresh VM,
GPUI, wgpu/Vulkan, font, Activity, and process startup. The old surface remains
usable while it starts. This cost is acceptable for the safety candidate path
but should be revisited once a privileged compositor can prewarm or stage
surfaces.

Back-to-back successful revisions finally used candidate PIDs `32676` and
`393`. The supervisor logged termination of cached PID `32676`, suppressed its
old watchdog recovery, started a fresh process, and promoted source
`597c42a1…`. The provider envelope and `experience.active.luau` had that same
hash after the second first frame.

### Curated single-shot generation

One deliberately small comparison added the checked-in context
[`drag-attach-guide.md`](../evals/curated-single-shot/drag-attach-guide.md) and
kept `gpt-5.6-luna` at medium reasoning. There was one generation attempt and
one outer grade, with no repair.

The untouched 8,834-byte source scored **8/8**, versus the raw drag baseline's
6/8. It compiled, rendered the required content, generated four paths and four
quads, kept four hit regions phone-safe, and emitted a reachable typed effect.
It used 71,595 input tokens (57,344 cached) and 3,822 output tokens. Source
SHA-256 is
`597c42a1ae99002ff9ba5cf5f2e6cf0876f53832a7d5b5d9ad07040aebf881ec`.

The agent disobeyed the requested artifact directory and wrote under `evals/`;
the harness moved that unchanged source into ignored artifacts before grading.
This is a tooling/adherence defect, not a source repair, and another reason not
to overinterpret one perfect task score.

After resetting only synthetic experience state, a real device swipe generated
`note_press`, 33 drag/update requests, and `note_drop`. Revision 102 promoted
with `effects=1`; the external service logged exactly:

```text
provider_effect_promoted revision=102 provider=notes
action=attach_to_event note_id=note-1 event_title=Design review
```

The before/after phone screenshots are 102,430 and 107,303 bytes with SHA-256
`ce4b1122…` and `91fb6839…`. The presented candidate screenshot is 105,432
bytes with SHA-256 `4d1b6e4b…`. Raw source, transcript, grade, and screenshots
remain ignored artifacts.

This confirms that curated contract context removes the specific raw-suite
failures. It does not justify further prompt optimization now; broader agent
quality, repair, skills, retrieval, and possible fine-tuning remain a later
product track.

### Sustained swaps and thermal sample

The final 10,000-swap physical run alternated complete Luau revisions and
waited for a GPUI next-frame callback after every commit:

```text
accepted=10000 rejected=0 duration_ms=185146
visible_p50_us=18679 visible_p95_us=20703 visible_p99_us=21452 max=40092
worker_p95_us=3213 worker_to_commit_p95_us=8809
commit_to_render_p95_us=175 gpui_tree_build_p95_us=199
frame_callback_p95_us=9711
rss_start_kb=281356 rss_end_kb=286876 rss_peak_kb=286884 delta=5520
```

Android `dumpsys meminfo` moved from 297,405 to 309,977 KB total RSS
(+12,572 KB), while the app's `/proc` sample moved +5,520 KB. Battery
temperature was 36.3 °C before and 36.2 °C after while USB powered. This
three-minute run is materially deeper than the earlier 1,000-swap test and
shows no immediate leak/thermal slope; it is not an hours-long soak or proof of
steady-state memory.

## Five-gate status

| Gate | Verdict | Reason |
| --- | --- | --- |
| A. Generative depth | **Pass for phase transition** | An untouched agent source invented low-level spatial geometry and completed the canonical real drag/effect on hardware. The IR remains intentionally bounded, not the final native ceiling. |
| B. Single-shot loop | **Pass for phase transition** | One curated Luna attempt compiled, scored 8/8, promoted, rendered, and interacted without source repair. Output-path adherence failed and broader reliability is deferred. |
| C. Revision recovery | **Pass at APK scope** | Real GPUI candidate PIDs promote on first frame; pre/post-frame native aborts restore the accepted PID/surface; cached and back-to-back revisions were exercised. The supervisor is not yet independent of Android or the accepted process. |
| D. Provider/state independence | **Pass at prototype scope** | Strict Android build has no linked fake provider; state/source/schema/effects share promotion authority; migration and promotion faults are tested. Transport still terminates on the Mac and local files remain non-authoritative caches. |
| E. Device viability | **Pass for phase transition** | Touch, drag, scroll, text/IME, animation and lifecycle evidence remain; 10,000 swaps had 0 rejects, 20.7 ms p95, modest RSS growth, and flat short-run temperature. Multi-hour soak remains future work. |

## What moves to the next phase

The next gate was a **privileged shell/supervisor spike**, not more Luau prompt
work:

1. Boot or switch into a minimal SOS home/shell process on an AOSP-capable test
   target while retaining Android apps as compatibility surfaces.
2. Move revision authority and crash recovery into a supervisor that is outside
   every generated/accepted experience process.
3. Own surface staging/promotion and input focus explicitly instead of relying
   on Activity tasks and `adb` task restoration.
4. Move the typed provider/state service onto the device as a durable system
   service; retain the current transaction and fault suite.
5. Define signed immutable revision directories and an atomic current pointer,
   then make the GPUI/Luau APK harness consume the same protocol.

Keep these as parallel regression requirements: the canonical drag/effect,
pre/post-frame crash recovery, back-to-back fresh candidate PIDs, strict
provider dependency check, and 10,000 frame-paced swaps.

### 2026-08-15 AOSP follow-through

AOSP-0, AOSP-1, and AOSP-2 now pass in x86-64 Cuttlefish. Unchanged Android 17
boots first; the SOS product then resolves the platform-signed x86-64 GPUI APK
as HOME while retaining Quickstep only for Recents. An init-supervised native
service in its own enforcing SELinux domain now owns the typed provider/state
service, immutable revision directories, activation journal, and atomic current
pointer on the device. The product uses device loopback and its verifier rejects
any ADB reverse mapping.

The verifier activated a presented revision without replacing GPUI, killed and
recovered the authority without changing HOME or the revision, and killed and
recovered HOME without replacing the authority or the revision. This completes
items 1, 2, and 4 above at Cuttlefish spike scope and completes the immutable-
directory, atomic-pointer, and shared-protocol portions of item 5. A production
signing/verification key boundary remains open. Item 3 also remains open:
Android ActivityManager, SurfaceFlinger, and the input stack still own task
recovery, surface presentation, and focus. Implementation and exact evidence
are in [`aosp-cuttlefish.md`](aosp-cuttlefish.md) and
[`progress.md`](progress.md).

## Remaining risks, explicitly not hidden

- In the AOSP product, authority survives the accepted Java process, but HOME
  recovery is still performed by Android ActivityManager rather than an SOS
  surface supervisor.
- Android owns boot, task lifecycle, composition, IME, accessibility, and input
  arbitration.
- Cold GPUI candidate startup is about 1.1–1.3 seconds on this device.
- The AOSP product provider/state authority is on-device with no reverse
  mapping. The separate physical-device `m1-run` harness remains
  workstation-hosted and uses `adb reverse` as laboratory transport.
- Drag updates commit every intermediate coordinate rather than coalescing.
- The soak is minutes, not hours, and native heap attribution is incomplete.
- The Luau IR cannot express arbitrary new shaders/renderer primitives; the
  unrestricted native experience path remains to be built in the next phase.
- Security is suitable only for synthetic data and trusted prototype source.

The final tested dirty APK `sos-experience-bbfaa87c303f-dirty.apk` is
37,297,301 bytes with SHA-256
`38130723201745591a9e954122f5133148336c987ac1604fcde4708f96c13fbd`.
