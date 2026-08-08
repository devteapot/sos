# Stable-host Android device gate

Date: 2026-08-08

## Result

The stable-host APK gate is **confirmed at prototype scope** on a physical
Samsung SM-A336B (Android API 35, ARM64). Luau revisions were prepared and
presented by one permanent Rust/GPUI process, a hostile revision was rejected
without changing source or durable state, source rollback stayed in the same
PID, typed state and provider effects committed through the external authority,
and both the authority and Android host recovered the committed revision after
restart.

This is evidence for the Android in-process implementation of the stable-host
lifecycle. It is not yet evidence for the Linux supervisor protocol running as
an AOSP service, nor for a compositor presentation fence.

## Environment and artifact

- Device: Samsung SM-A336B, Android API 35, `arm64-v8a`.
- Build host: Linux `aarch64`, Rust 1.95.0, Ubuntu Clang 18.1.3, Android NDK
  29.0.14206865.
- Source revision: dirty `c710597f0296`; the dirt is the stable-host
  implementation described in [`progress.md`](progress.md).
- APK: ignored artifact `artifacts/sos-experience-c710597f0296-dirty.apk`,
  36,649,277 bytes, SHA-256
  `5a425e1f442c5670f2e90117c9cbdcec12ae75a3bb127d1a6f545d1659eb322d`.
- Final attached-state screenshot: ignored artifact
  `artifacts/sos-stable-host-final.png`, 127,899 bytes, SHA-256
  `d7ca4aaa17e83e5c0b8b1fc81a40f1e87c24bdf3a37740845f58209918f84f18`.
- Recovered-host screenshot: ignored artifact
  `artifacts/sos-stable-host-recovered.png`, 118,591 bytes, SHA-256
  `4e73cd2b8d75af747b7c2cb5bb39918ad2f67417aae83560e5523fcb6e6deaa0`.

The final temporary authority file was 500 bytes with SHA-256
`1625bfed3e19673156aa32e2153c5036df199861b50b3032750a6d386eb58cc6`.
It held revision 46, source SHA-256
`a3bde37f3583b727616786a86d09b6e843d8f16c671e0bc3eabc7e198c0bb53d`,
the Unicode draft, attached-note state, and the provider receipt. Generated
APKs, screenshots, backups, and authority files remain outside Git.

## Activation and rejection evidence

The first cold launch opened the real Mali-G68 Vulkan GPUI window, connected to
the external state/provider daemon through `adb reverse tcp:47777 tcp:47777`,
and made the restored Luau source visible in PID 28073. Its initial frame-bound
activation took 121.186 ms, including 6.702 ms of worker time.

A structurally different 5,130-byte, 37-node Luau experience then activated in
the same PID:

```text
source_to_visible_us=102632 queue_us=214 compile_us=4737
render_us=10919 worker_total_us=15694
```

The resulting screen and open keyboard were visually inspected from the device
screenshot. No `:candidate` process existed, and `dumpsys package` exposed only
`dev.gpui.mobile.GpuiActivity` for the experience.

An infinite-render candidate was submitted without host-side validation. The
worker interrupted it after 20.313 ms of render time and logged
`script_rejected`. PID 28073, the accepted source hash, and the complete
authority-file hash remained unchanged. Source rollback then restored hash
`597c42a1ae99002ff9ba5cf5f2e6cf0876f53832a7d5b5d9ad07040aebf881ec`
in the same PID in 47.138 ms while retaining the Unicode draft.

An explicit Luau-worker restart changed its owning thread from `ThreadId(11)`
to `ThreadId(12)` in 7.153 ms without changing the Android PID, source hash, or
authority hash.

## Platform and provider regression evidence

The dark daily-flow revision republished six semantics. `uiautomator` exposed
the expected coarse window description containing the heading, weather status,
album image, playback button, Unicode note field, and editing status. This
confirms the existing summary bridge, not per-element TalkBack navigation.

Android reported `mInputShown=true`. An injected `z` reached the keyed field,
logged `native_text_changed`, and durably committed authority revision 6 with:

```json
{"draft":"Caffè ☕️ – 明日のデザインz"}
```

The corrected bent-time-axis experience activated in PID 28377 in 40.184 ms.
A real 1.2-second device swipe dragged the first note onto Design review. The
host committed the drag states and one final typed effect; authority revision
46 contained `attached=true`, `drop_valid=true`, the draft, and receipt
`notes:note-1->Design review`. The separate daemon logged exactly one:

```text
provider_effect_promoted revision=46 provider=notes
action=attach_to_event note_id=note-1 event_title=Design review
```

## Latency and soak

A 1,000-swap smoke soak passed first:

| Measurement | Value |
| --- | ---: |
| Accepted / rejected | 1,000 / 0 |
| Duration | 38.549 s |
| Visible p50 / p95 / max | 23.988 / 80.191 / 82.777 ms |
| Worker p95 | 5.302 ms |
| RSS start / peak / end | 283,704 / 283,704 / 275,076 KB |

The full frame-paced run then completed:

| Measurement | Value |
| --- | ---: |
| Accepted / rejected | 10,000 / 0 |
| Duration | 428.631 s |
| Visible p50 | 40.197 ms |
| Visible p95 | 92.708 ms |
| Visible p99 | 93.846 ms |
| Visible maximum | 97.376 ms |
| Worker p95 | 5.352 ms |
| Worker-to-commit p95 | 59.699 ms |
| Commit-to-render p95 | 0.890 ms |
| GPUI tree-build p95 | 0.313 ms |
| Frame-callback p95 | 30.411 ms |
| RSS start / peak / end | 296,476 / 306,292 / 306,268 KB |
| Positive end-to-start RSS delta | 9,792 KB |
| RSS samples | 41 |

The stress harness repeatedly exercises fresh-VM prepare, in-process scene
commit, GPUI tree build, and post-render acknowledgement. It deliberately does
not write the source pointer or external authority on every iteration; the
regular activation, rollback, action, and restart cases above cover that durable
transaction path.

The run stayed below the 100 ms visible target even at its maximum and retained
one Android PID. The 9,792 KB positive RSS delta is bounded evidence against a
per-swap leak, not proof of long-term memory stability; a longer thermal/leak
soak should track it.

## Restart recovery

After rollback, a forced Android process restart changed PID 28073 to 28377 and
loaded authoritative revision 4 with the exact source and state hashes, without
replaying activation. After the provider effect and 10,000-swap run, the
external authority daemon was stopped and reopened against the same file. A
second Android cold restart changed PID 28377 to 31099 and loaded revision 46,
including the attachment receipt and Unicode draft, with no extra authority
write. Finally, the APK built by the repaired standard harness installed with
`adb install -r`; PID 31416 again booted revision 46 and the same hashes.

## Build and installation failures

The failures were environmental and were turned into a reproducible harness
path:

- The SDK/JDK variables were initially unset. Explicit system SDK, NDK 29, and
  Java 17 paths made `./tools/sosctl doctor` pass.
- `cargo-ndk` tried to execute Google's x86-64 Linux Clang on the ARM64 host and
  failed because the foreign glibc loader was absent. Native Clang/Clang++,
  `llvm-ar-18`, the NDK sysroot, and `tools/android-clang-linker` produced the
  ARM64 library.
- Gradle tried to install Build Tools 36 into read-only `/usr/lib/android-sdk`,
  then selected Maven's x86-64 `aapt2`. `tools/sosctl` now creates an ignored,
  writable SDK overlay on Linux ARM64 and overrides `aapt2` with the native
  system binary. The repaired `./tools/sosctl m1-build` completed successfully.
- The existing phone APK had a different development signature. Before the
  required uninstall, all five private source/state files were streamed through
  `run-as` into an ignored local backup and verified by SHA-256. They were
  restored byte-for-byte before first launch. Subsequent installation of the
  final harness artifact used `adb install -r` and preserved data.

## Decision and remaining gates

The new phone evidence closes the APK regression gate for stable-host Luau
activation. Native experience promotion is not needed for this path: accepted,
rejected, rollback, worker-restart, IME, semantics, typed-effect, soak, and
durable-restart cases all worked with one GPUI experience process.

The next system gate is narrower and deeper: implement the stable-host protocol
in the AOSP-owned GPUI shell, replace the callback assertion with an actual
compositor-present fence, and quiesce input across the authority-commit/scene-
switch interval. The APK still uses a workstation TCP daemon through `adb
reverse`; the preexisting local state was backed up, but this run began with a
fresh external authority and therefore did not prove automatic migration from
legacy local state. Full marked-text IME, per-element Android accessibility,
real-data isolation, signed manifests/journals, crash injection at every
on-device commit interstice, and longer thermal/leak testing remain open.
