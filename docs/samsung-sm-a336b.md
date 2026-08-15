# Samsung SM-A336B physical-image viability

## Decision

The connected Galaxy A33 5G has passed the stock rollback, bootloader-unlock,
LineageOS bring-up, and **physical ARM64 SOS runtime gates**. It is running the
corrected Android 16 / LineageOS 23 SOS a33x product over exact FYH2 vendor
firmware with an unlocked/orange boot chain. SOS is the permanent HOME, its
authority is on-device, and a live Luau revision has survived independent
authority and application restarts. A custom boot has irreversibly changed
the Knox warranty bit to `1`.

- The owner explicitly accepted factory reset and device-loss risk on
  2026-08-15 and waived the user-data backup gate because this is a dedicated
  development phone. Exact FYH2 and GYI8 packages and their PITs are retained
  outside Git as the stock restore path.
- The FYH2 rollback, destructive unlock, matched bootstrap, repaired recovery,
  full baseline sideload, and Android 16 first boot all passed on this unit.
- The 2024 official TWRP build remains rejected. A pinned Android 16 /
  LineageOS 23 recovery now builds reproducibly from the contemporary a33x,
  s5e8825, kernel, and vendor graph.
- The recovery USB/watchdog defect was reproduced, repaired, reflashed, and
  proven by physical ADB sideload. The separately named SOS a33x product now
  passes build, OTA-signature, PIT, full AVB, ARM64, HOME-manifest, recovery,
  component-identity, compiled-SELinux, physical sideload, live-revision,
  restart-recovery, and hardware-service smoke gates.
- Actual audio, fingerprint enrollment, Wi-Fi association, calls/data
  transfer, suspend/resume, thermal behavior, and longer soak testing remain
  open; the service smoke results do not close those functional gates.

## Connected-device evidence

The non-destructive 2026-08-15 probe used the already-authorized ADB connection. It
did not reboot, unlock, wipe, root, enter Download Mode, or write a partition.
Developer Options was opened for UI inspection and the phone was returned to
HOME. Hardware identifiers are deliberately omitted.

```sh
adb devices -l
adb shell getprop
adb shell am start -W -a android.settings.APPLICATION_DEVELOPMENT_SETTINGS
adb shell uiautomator dump /data/local/tmp/sos-a33x-development.xml
adb exec-out cat /data/local/tmp/sos-a33x-development.xml
research_dir="$(mktemp -d /tmp/sos-a33x-research.XXXXXX)"
adb pull /system/priv-app/SecSettings/SecSettings.apk \
  "$research_dir/SecSettings-A336BXXUEGYI8.apk"
apkanalyzer dex code \
  --class com.android.settings.development.OemUnlockPreferenceController \
  "$research_dir/SecSettings-A336BXXUEGYI8.apk"
```

The connected device reported:

| Property | Observation |
| --- | --- |
| Product | `SM-A336B`, codename `a33x`, board `s5e8825` / `universal8825` |
| Runtime | Android 16 / API 36, One UI 8.0, 2025-09-01 security patch |
| Stock build | `A336BXXUEGYI8`, EUX |
| Architecture | `arm64-v8a` primary ABI; Treble enabled |
| Partitions | Dynamic `super`, no slot suffix, separate `boot`, `vendor_boot`, `recovery`, `dtbo`, `vbmeta`, and `vbmeta_system` |
| Rollback state | `ro.boot.rp=14` (Samsung binary revision `E`) |
| Lock state | `ro.boot.flash.locked=1`, `ro.boot.vbmeta.device_state=locked`, verified boot `green`, `ro.boot.other.locked=1` |
| Samsung state | Knox Guard `Completed`, warranty bit `0`, unlock count `0` |
| Unlock capability | `ro.oem_unlock_supported` absent; OEM unlocking row absent from the top of Developer Options |

The missing row is not only a UI observation. This firmware's
`OemUnlockPreferenceController` constructs an `OemLockManager` only when
`ro.oem_unlock_supported` equals `1`; its `isAvailable()` also returns false
when `ro.boot.other.locked` equals `1`. Both conditions reject this phone's
current state. Android's platform contract likewise says an unlock-capable
product sets `ro.oem_unlock_supported=1` and requires the user to enable OEM
unlocking before a bootloader unlock. See the AOSP
[locking and unlocking contract](https://source.android.com/docs/core/architecture/bootloader/locking_unlocking).

The ignored pulled artifact was
`SecSettings-A336BXXUEGYI8.apk`, 114,799,707 bytes, SHA-256
`b16a9bbd64d740f7482afe1fcd0c44b7ef8340f01bdb1950f73cf125fedd6a6c`.
It and the temporary source clones were removed after inspection and are not
retained in Git.

## Available device, kernel, and vendor basis

There is now substantially more than a recovery-only tree. A shallow audit of
the community `lineage-23.0` set found the following coherent source graph:

| Role | Evaluated source revision |
| --- | --- |
| SM-A336B device tree | [`exynos1280/android_device_samsung_a33x`](https://github.com/exynos1280/android_device_samsung_a33x/tree/lineage-23.0) at `a85c2a9652c93880a1c1474a098a72368d416e21` |
| Exynos 1280 common tree | [`exynos1280/android_device_samsung_s5e8825-common`](https://github.com/exynos1280/android_device_samsung_s5e8825-common/tree/lineage-23.0) at `33dd9c99978647a44aa22089db4830f95bb91fb8` |
| Kernel | [`exynos1280/android_kernel_samsung_s5e8825`](https://github.com/exynos1280/android_kernel_samsung_s5e8825/tree/lineage-23.0) at `0f885d194baaed657ad05bc4ff0d8d5cd4a2f4e5` |
| SM-A336B blobs | [`exynos1280/proprietary_vendor_samsung_a33x`](https://github.com/exynos1280/proprietary_vendor_samsung_a33x/tree/lineage-23.0) at `a7efdd5712ece827ad3632fd38c93dd267f58b51` |
| Common blobs | [`exynos1280/proprietary_vendor_samsung_s5e8825-common`](https://github.com/exynos1280/proprietary_vendor_samsung_s5e8825-common/tree/lineage-23.0) at `4a2275bfabd9fcce764bcf773a7d1e236ff67346` |
| Dependency manifest | [`exynos1280/local_manifests`](https://github.com/exynos1280/local_manifests/blob/lineage-23.0/s5e8825.xml) at `23b84e5f5dc0b63f0e52edf4e02ef931afd6013b` |

The board configuration is ARM64, builds an `Image` from
`kernel/samsung/s5e8825` with `s5e8825-unified_defconfig`, defines the observed
dynamic/A-only layout, enables AVB, and supplies device init, VINTF, SELinux,
audio, NFC, radio, camera, fingerprint, graphics, and recovery integration.
The kernel is 5.10.239 and includes the SM-A336B DTS, panels, cameras, and
touch firmware. The audited vendor checkouts contained 318 device files and
162 common files; the corresponding extraction lists contain 529 lines.

A separate community builder published an
[unofficial LineageOS 23.2 a33x release](https://github.com/fynrae/lineage_releases_a33x/releases/tag/2026-08-08)
on 2026-08-08 and source branches with subsequent UDFPS fixes. Its flash ZIP is
1,260,835,762 bytes with published SHA-256
`54f38de8d898ba6ea6712fd69ce4853b99f8f9b336505baed6ee05368ff843b1`.
This is useful evidence that the source family can produce a contemporary
Android 16 package; it is not SOS evidence, an official LineageOS build, or a
substitute for reproducing the build and booting it on sacrificial hardware.

Two limitations matter for SOS:

1. These are community LineageOS 23.x / Android 16 trees, not a drop-in device
   directory for the current Android 17 `android-latest-release` checkout.
   The lowest-risk first port is an independently reproducible Android 16
   baseline with the SOS product layered onto it. An Android 17 port is a later
   compatibility gate.
2. The proprietary trees have no distribution license recorded by GitHub.
   SOS must retain extraction provenance and decide whether blobs are locally
   extracted rather than redistributed.

### Reproducible LineageOS 23 recovery baseline

The audited graph is encoded in `aosp/manifests/a33x-lineage-23.0.xml` with
exact Git revisions. It is checked out separately at
`/home/carlid/dev/lineage-a33x`; the existing Android 17 Cuttlefish checkout is
unchanged. `tools/a33xctl` runs the build in the dedicated Ubuntu 24.04 image
`localhost/sos-lineage-build:ubuntu-24.04`, image ID
`4e351528281b6b7085676140451e0f2cc531764963668a8f0f3016f2f82596dc`.

The first `repo sync` completed successfully with a clean worktree and wrote
`.repo/sos-a33x-resolved-manifest.xml`, 280,889 bytes, containing 1,150
projects. Its SHA-256 is
`91594f3ddcbeee8b87196d017cfedd8b5bff5b66622c6363b0228efa56d8d573`.
Direct `git rev-parse HEAD` checks matched the five critical pins listed above.
The first sync spent most of its time materializing the filtered Clang, Rust,
and SDK prebuilts; this was active transfer/checkout work rather than a hang.

`./tools/a33xctl build-recovery` selected Android 16 /
`lineage_a33x-userdebug`, compiled 11,704 Ninja actions including the
5.10.239 s5e8825 kernel and A33 EU DTBO revisions `r00` through `r04`, and
completed in 10 minutes 19 seconds. The resulting ignored artifact is:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `out/target/product/a33x/recovery.img` | 100,663,296 | `9bbf7983feb5dbb0854dc34448690c18c037273821c0eb45a210ac50218b48e9` |

The image exactly fills the 100,663,296-byte recovery partition recorded by
the live PIT. `unpack_bootimg` reports Android boot header version 2, 4,096-byte
pages, kernel address `0x10008000`, ramdisk address `0x10000000`, a
31,455,744-byte ARM64 kernel, 32,405,550-byte ramdisk, 999,528-byte recovery
DTBO, and 232,284-byte DTB. `avbtool info_image` verifies its recovery hash
descriptor and SHA256_RSA4096 footer; the signed content is 65,105,920 bytes,
and the complete image is padded to the partition boundary.

For comparison, exact FYH2 `recovery.img.lz4` was extracted from the verified
AP archive and decoded outside Git. The compressed file is 56,973,312 bytes,
SHA-256 `cb7910d8ee1727ea6f2ba91ebf0f2daf818990ba3c57c6498e646905c574a442`;
the decoded 100,663,296-byte stock image is SHA-256
`49b0745a746aaa45ccba806479d6e9c2cc7f74f756d03e2390bdc6ffb3f78712`.
It uses the same header version, page size, load addresses, and near-identical
DTB/DTBO sizes. Its signed content is 74,007,328 bytes.

The decisive offline rejection is key binding. FYH2 `vbmeta.img` contains a
chain descriptor for `recovery` at rollback-index location 1 using public-key
SHA-1 `557dab1a3e7a1b571d6d864f8414d0e39468f835`; FYH2 recovery uses that same
key. The source-built recovery uses public-key SHA-1
`2597c218aae470a130f61162feaae70afd97f011`. Flashing recovery alone would
therefore create an internally inconsistent AVB chain. The device tree's OTA
extension deliberately replaces `dtbo.img`, `vbmeta.img`, and
`vendor_boot.img` as part of a full ROM install. AOSP's non-A/B releasetools
explicitly omit recovery from the generated top-level vbmeta descriptors and
give recovery its own AVB footer; that differs from Samsung's stock recovery
chain and makes final-image inspection and an atomic bootstrap sequence
mandatory. The rejected shortcut is a generic disabled-verification vbmeta:
it would bypass rather than prove the matched boot chain and could leave stock
Android unbootable. Build and inspect the complete package first.

The pinned vendor radio set is labeled `A336BXXSEFYH2`. Streaming SHA-1 checks
after decoding the corresponding members directly from the verified FYH2 BL
and CP archives proved byte identity for all ten staged SM-A336B firmware
files: `fld`, `sboot`, `ldfw`, `tzsw`, `tzar`, `harx`, `keystorage`, `uh`,
`modem`, and `modem_debug`. The full-ROM path therefore does not cross a
bootloader or modem firmware generation relative to the live handset.

| Decoded FYH2 / pinned vendor file | SHA-1 |
| --- | --- |
| `fld.bin` | `1136f30e38c254adf91b576665b7555251f55f34` |
| `sboot.bin` | `ef11913175c6b9f80a47148a25a385b9e80366e0` |
| `ldfw.img` | `af59ba8a8c19b9127d32ef726a3168b47bbf9cda` |
| `tzsw.img` | `5d400680e29c23a28072da4ff087601a292e988a` |
| `tzar.img` | `3c3658addbbebba3599658bbc93d16d835e4553e` |
| `harx.bin` | `535423b47c4156c4802cdc2e4a901bd2a52f0fbd` |
| `keystorage.bin` | `b443046c9b76563f3399ded8391b9913bbef82f6` |
| `uh.bin` | `b560c96e92b38233fa8d1aad4feb055a02545b9e` |
| `modem.bin` | `6d7d58bfd38daa53677550eb2179e9def72d3391` |
| `modem_debug.bin` | `203a495a068be98b5e53c4b5e2d60b5e050176b6` |

The first complete-ROM build exposed one source-graph defect at action
123,295 of 147,270. Android's prebuilt-ELF checker proved that the FYH2
`libexynoscamera3.so` imports `createScenarioOperator`, while the pinned common
tree's `libepicoperator` shim exported `createOperator`. The build correctly
stopped instead of packaging a camera library with an unresolved runtime
symbol. The maintainers fixed that exact typo later in commit
`cb4ca128b0867d9cc92f22501430d0775018f5f1`. SOS carries the isolated one-line
backport at
`aosp/patches/a33x-lineage-23.0/0001-s5e8825-fix-epicoperator-symbol.patch`
(SHA-256
`a8dea6c8c01f3c8572f952b8455560c690e700626df0332bd391abc04b175c61`),
and `tools/a33xctl` applies it idempotently after sync and before builds.
Setting `allow_undefined_symbols` was rejected because it would only hide the
checker failure; the backport provides the symbol the blob actually imports.

The cached resume then exposed a second independent generated-metadata typo.
FYH2 `libsec-ril.so` has a dynamic dependency on
`libprotobuf-cpp-full-21.7.so`, while its pinned `Android.bp` declared generic
`libprotobuf-cpp-full`. The vendor tree already supplies and packages the
correct 21.7 compatibility library. The corresponding upstream correction is
from commit `cf2678a02cedac743ddd00502fc390731a337301`; its isolated backport is
`aosp/patches/a33x-lineage-23.0/0002-s5e8825-match-libsec-ril-protobuf-soname.patch`
(SHA-256
`18517d33328c5ffdf101bf8e8b1f25d5383d96ee50cfde5463a6d036da543795`).
Bypassing `check_elf_files` was rejected because it would leave the declared
loader dependency wrong.

A third cached-build stop came from intentionally skipped Git-LFS smudging,
not from a device source incompatibility. The ARM64 WebView APK remained its
134-byte LFS pointer, and `manifest_check` correctly refused it as an invalid
ZIP. `tools/a33xctl hydrate-lfs` now explicitly pulls and verifies the pinned
object after sync and before builds. The hydrated ignored APK is 265,525,351
bytes, SHA-256
`68fa550b7a76e39f0382308d93b235c0623d032c0aa6c4a56fc02eedfdbe6342`;
`unzip -tq` passes. Only the ARM64 WebView object is hydrated for this product.

### Reproduced full package and authoritative bootstrap set

After the two audited source backports and explicit LFS hydration, the final
cached build completed in 8m05s and VINTF reported `COMPATIBLE`. The resulting
ignored package is:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `lineage-23.0-20260815-UNOFFICIAL-a33x.zip` | 1,226,299,848 | `765e4a9045bcece5fba8777f041d9f86d5d8569870ca63a183483086c3451e20` |

`unzip -tq` and AOSP's whole-file OTA signature checker pass. The updater
asserts `a33x`, rebuilds the dynamic partitions, writes boot, DTBO, vbmeta, and
vendor boot, and leaves recovery untouched. Its SM-A336B firmware branch is
conditional on the live bootloader differing from `A336BXXSEFYH2`; this phone
already reports that exact build, so the byte-matched firmware is packaged but
will not be rewritten during sideload.

The ZIP's images, not stale top-level product copies, are authoritative. They
were extracted outside Git to
`/home/carlid/sos-samsung-work/lineage-a33x/lineage-23.0-20260815-UNOFFICIAL-a33x-bootstrap`
and proved byte-identical to target-files:

| Bootstrap image | Bytes | SHA-256 |
| --- | ---: | --- |
| `boot.img` | 67,108,864 | `d27ea3e21a8643631f744616b1b98f5fe949f5f43914a599c2758720cc191d9a` |
| `dtbo.img` | 8,388,608 | `bb8b37acf0f6122228a203d9890ee2bcb45cf6860172f6fd5f86a0b032c99ba4` |
| `recovery.img` | 100,663,296 | `fe53c96b609dfd4c3a4121551bd8b965990d43784783ffbc54d2f52d82b50800` |
| `vbmeta.img` | 8,192 | `6061dd683af3d33a3ad01a2d4fc05e2e51a8f0d63aa4bc05d8bb7f1eb5e966b8` |
| `vendor_boot.img` | 33,554,432 | `053d9b6cab655ebdd89cb6895aae3667df7c44b33e020132e14ed03c77b2b82d` |

Each image fits its exact live-PIT ceiling and verifies with `avbtool`.
Recovery carries its own SHA256_RSA4096 footer. Top vbmeta uses algorithm
`NONE` with flags `0`, intentionally omits recovery, and successfully verifies
the package's boot, DTBO, vendor boot, and six dynamic-partition
hash/hashtree descriptors. The offline flash gate therefore passes. Physical
boot, recovery, ROM, and hardware behavior remain unproven until the first
matched flash and sideload complete on this handset.

The first custom bootstrap write subsequently used one fresh samloader
session, explicit `BOOT`, `VENDOR_BOOT`, `DTBO`, `RECOVERY`, and final
`VBMETA` mappings, and `--no-reboot`. It used no PIT, repartition, skipped size
check, or stock archive. All five uploads succeeded and samloader exited 0.
The immediate Side + Volume Down, then Side + Volume Up transition booted the
new Lineage Recovery. The physical bootstrap and custom-recovery boot gates
therefore pass. Data formatting, full ZIP sideload, Lineage system boot, Knox
read-back, and hardware checks remain pending.

The initial recovery was bootable and could format data, but it exposed no USB
device after selecting `Apply from ADB` and eventually green-screened and
rebooted back into recovery. Host ADB restarts, cable reconnects, connector
orientation, and a direct alternate USB port did not produce even an `lsusb`
enumeration. Inspection of the actual recovery ramdisk found the source
defect: generic recovery init imports `/init.recovery.${ro.hardware}.rc`, but
the image omitted `init.recovery.s5e8825.rc`. The existing device file that
sets configfs, controller `13200000.dwc3`, and starts the watchdog was only
installed under vendor.

SOS now carries
`0003-s5e8825-package-recovery-init.patch` (SHA-256
`51557dbd9e58d1788b505cf87d267ea260c362f9311eef2d9c62d743e906b2d4`) and
applies it through `tools/a33xctl`. The incremental build completed in 2m34s.
Its corrected recovery is 100,663,296 bytes, SHA-256
`d751d08b12c80861a5e0e7800e7df5eb189a94f3d7fda31fdfdb36dce04c7a6c`;
AVB verification passed, and unpacking proved the imported file is present and
byte-identical to source. A fresh no-reboot samloader session read the live PIT
and successfully wrote only `RECOVERY`, with no supplied PIT, repartition, or
size-check bypass.

The repaired recovery enumerated as Google recovery gadget `18d1:d001` and
changed from expected main-menu `unauthorized` state to ADB `sideload` with
model `SM_A336B` and device `a33x`. The exact verified Lineage ZIP then
completed `adb sideload` with `Total xfer: 1.00x`; recovery returned normally,
and the handset subsequently booted LineageOS. Format-data, sideload, and first
custom-system boot therefore pass. The installer did not write recovery, so
the repaired image remains installed. Property/Knox read-back and the full
hardware smoke matrix remain the next gate; the full ROM package must also be
rebuilt before reuse because its embedded recovery predates this repair.

### Booted baseline and inspected SOS install package

Normal ADB read-back completed the baseline gate. The handset reports Android
16 / API 36, `lineage-23.0-20260815-UNOFFICIAL-a33x`, kernel
`5.10.239-android12-9`, FYH2 bootloader, rollback level 14, unlocked/orange
verified boot, Knox warranty bit `1`, SELinux `Enforcing`, and encrypted FBE.
The system security patch is 2026-02-01 and vendor remains 2025-10-01.

CameraService enumerated five cameras and both primary previews ran without a
HAL death; the front preview was visibly correct. Fingerprint HAL sensor 0,
NFC, Bluetooth, and the FYH2 modem all enumerated, and the Salt SIM registered
on LTE. These are service/smoke results only: fingerprint enrollment, Wi-Fi
association, actual audio, calls/data, suspend/resume, thermal, and soak still
require physical tests.

The new `lineage_sos_a33x` product inherits this baseline and adds the
platform-signed privileged ARM64 SOS HOME, ARM64 authority, bootstrap, init
service, properties, overlay, and enforcing system_ext policy. Launcher3 stays
packaged because SystemUI uses it for Recents. The first build exposed an
Android-17-only `priv_app_domain()` macro; the LineageOS 23 port now uses its
native `app_domain` model and narrow explicit socket grants. No permissive or
broad policy bypass was accepted.

The initial build completed 12,924 actions in 3m51s with VINTF `COMPATIBLE`
and all policy tests passing. Its ignored OTA was:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `lineage-23.0-20260815-UNOFFICIAL-sos_a33x.zip` | 1,247,998,781 | `0fb4d1139475b4f53b64f555db34851ab4a55251f578db5efd804984f781cf2a` |

`./tools/a33xctl inspect-sos` verified its whole-file signature, ZIP integrity,
exact SOS target selection, package/target-files image identity, live-PIT
ceilings, complete AVB graph, and the repaired recovery init inside the OTA.
It also proved that the installed 41,536,658-byte platform-signed APK remains
ARM64-only and HOME-enabled, that the packaged authority and bootstrap exactly
match their audited inputs, and that compiled seapp/file contexts assign the
dedicated SOS domains and data labels. The offline SOS flash gate passes. The
next gate is a no-wipe sideload from the repaired recovery followed by
on-device HOME, activation, SELinux-domain, restart, and hardware checks.

That exact first SOS OTA installed with `Total xfer: 1.00x` and booted Android
without formatting data. The authority ran in its dedicated enforcing domain,
HOME resolved to SOS, the APK remained ARM64, and no ADB reverse existed.
However, the physical HOME process repeatedly crashed before attaching to
ActivityManager. The sole SOS denial was `service_manager find` on
`activity_service` from `sos_shell_app`.

LineageOS 23's `app_domain()` is only a base isolation macro; its full
privileged framework contract is carried by the canonical `priv_app` domain,
and the Android 17 `priv_app_all` inheritance attribute is absent. The fix maps
only this named, platform-signed privileged package to canonical enforcing
`priv_app` and adds only the two SOS TCP endpoints. Permissive policy, copied
framework allow lists, and wildcard service access were rejected.

The corrected cached rebuild completed 292 actions in 3m48s, passing all
neverallow, compatibility, context, APEX-policy, and VINTF tests. Its current
ignored OTA supersedes the table above:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `lineage-23.0-20260815-UNOFFICIAL-sos_a33x.zip` | 1,247,480,012 | `6476f3a80556708491992b8a88b305d353e03d4a3390346f5062746d9b3f61ce` |

Full inspection again passed the package signature, ZIP, PIT, AVB, repaired
recovery, ARM64/HOME, authority/bootstrap identity, and exact compiled
package-to-`priv_app` mapping. Only this corrected hash is approved; it is the
artifact installed and exercised below.

### Corrected SOS physical runtime and revision persistence

The corrected OTA above installed without formatting data; Recovery reported
`Total xfer: 1.00x`. After reboot, Android reported
`lineage-23.0-20260815-UNOFFICIAL-sos_a33x`, SELinux `Enforcing`, encrypted
FBE, FYH2, unlocked/orange AVB, and warranty bit `1`. HOME resolved with
priority 1000 to `dev.sos.experience/.SosHomeActivity`. The on-device
authority ran in `u:r:sos_authority:s0`, while the named platform-signed
package correctly inherited Lineage's enforcing `priv_app` domain. There was
no ADB reverse mapping. The app opened a 1080x2400 GPUI window on Mali-G68
Vulkan and produced regular healthy heartbeats.

The initial authority query returned bootstrap revision
`b0d20599c81f62db31cfffd4883289e64a12ee9ada6f20a1c92ef518277e9be4`
at state revision 0. The test then installed the 5,678-byte
`experiences/timeflow.luau`, SHA-256
`4983de6756ef4b21ba6a0eddaed9f2a01f4363b0ab18d0292f55987f49f7ceb9`.
The application—not a manual authority command—presented the candidate via
`sos://reload`; validation measured 7,204 us compile, 5,493 us render, and
12,716 us worker-total time. Activation returned revision
`32fa86a739260e3b13a7bf7f4bc9639708a7d9517d852c6bfe71acb13a552f59`,
state revision 1, with the exact candidate source hash. The application PID
did not change during activation.

The usual `run-as` staging helper could not traverse canonical
`privapp_data_file`. Product SELinux expansion solely for test tooling was
rejected. Rooted debugging was temporarily enabled to place the candidate
with the package's UID, mode, and MCS label; it was switched off immediately
after the restart tests. Final UI read-back showed the toggle unchecked,
`adb root` was rejected by the system setting, and ADB again ran as UID 2000
in the shell domain.

Killing authority PID 938 caused init to start PID 2946 while application PID
2061 and revision 1 remained stable. Killing application PID 2061 then
started PID 3000 while authority PID 2946 and revision 1 remained stable. SOS
returned as focused HOME, initialized its runtime worker in 9,104 us, and kept
rendering the Timeflow experience. The final enforcing-state log scan found
no SOS AVC, fatal exception, or ANR.

The ignored physical screenshots under
`/home/carlid/sos-samsung-work/lineage-a33x/evidence-20260815` are:

| Evidence | Bytes | SHA-256 |
| --- | ---: | --- |
| `sos-home-corrected.png` | 171,681 | `67ed68a8f883bb9c3fedfe27759b65b29c3c9d7e99c63f050baadfd733cc4364` |
| `sos-timeflow-activated.png` | 178,465 | `ca07d68fabaf65412598dd4c5592d68416c2796a0119548ce2621eda0bef985d` |
| `sos-timeflow-after-restarts.png` | 173,926 | `3bb3189d0602011ac94347dbdb8239cfcd24ec6765333940a22fcfb4261c7042` |

Post-test service read-back still showed five camera devices, fingerprint
sensor 0 with zero HAL deaths, NFC on, Bluetooth on, and the Salt SIM
registered for voice and data on LTE. This closes the SOS physical runtime
and service-preservation smoke gates. It does not replace actual audio,
fingerprint enrollment, Wi-Fi association, calls/data transfer,
suspend/resume, thermal, or soak tests, which remain open.

### Wi-Fi association and microphone capture baseline

The next physical pass kept the corrected SOS image, enforcing SELinux, and
non-root ADB unchanged. The owner entered a Wi-Fi credential only on the
handset. Read-only checks then found IPv4 and the default route on `wlan0`, an
Android `VALIDATED` Wi-Fi network, successful DNS resolution, a TCP connection
to `api.openai.com:443`, and a completed TLS request returning the expected
unauthenticated HTTP `401`. No SSID, BSSID, credential, or assigned address was
retained in the evidence. Wi-Fi association and data transfer therefore pass;
this does not yet provide an SOS-authored network setup surface.

Lineage Recorder captured the owner's speech for 41.720 s. The ignored raw
artifact is
`evidence-20260815/sos-microphone-baseline.wav`, 7,359,452 bytes, SHA-256
`344f363a4d4c9c6a945ad2b8af3d10e098d52b08c486ec953a9385f4517a2218`.
It is 44.1-kHz, signed 16-bit stereo PCM. Read-only sample analysis found
sustained non-silent signal in 231 of 418 100-ms windows, channel RMS levels
of -40.00 and -20.76 dBFS, and only 14 clipped samples. This passes actual
microphone capture, but not playback, speaker, earpiece, Bluetooth audio,
call audio, or a Luau-authored recording path.

The SOS-native Wi-Fi follow-up is deliberately split at a trust boundary.
Luau renders redacted Wi-Fi state and can request only refresh, a selected
scan-row connection, or disconnect. Rust revalidates the selected SSID and
security class against the latest Android snapshot. The platform-signed Java
helper alone calls `WifiManager` and presents password/confirmation dialogs.
The password has save and autofill disabled, is cleared on dismissal, and
never enters JNI, Luau state, the revision authority, or logs. Local ARM64
Rust checking, both Luau validators, and the complete Java/Gradle APK build
pass. Physical scan, permission, association, and policy behavior are the next
OTA gate and remain unclaimed here.

### Resident agent staged while the phone is remote

The combined follow-up adds one resident Android experience agent without
changing the authority boundary. Its deterministic fake and OpenAI provider
both produce a complete bounded Luau proposal, which must still compile,
render, validate, stage, visibly present, and commit through the same on-device
revision transaction proven above. The fake alternates Daily Flow and
Timeflow, and both now retain the SOS network and agent surfaces.

Credential setup is a trusted Android password dialog, not a Luau field. The
API key is AES-GCM encrypted with a non-exportable Android Keystore alias; only
ciphertext and IV are app-private, Android backup is disabled, and the release
APK is non-debuggable. The key does not cross JNI or appear in the experience
model, source, state, agent conversation, or logs. The live request sends only
the user's prompt and the complete active Luau source to the Responses API and
forces one strict proposal function. This is explicitly a sole-device
prototype: because OpenAI advises against long-lived keys in client apps and
this handset is bootloader-unlocked, use a dedicated low-spend revocable key.
A production fleet should use a controlled relay and short-lived credentials.
Codex consumer OAuth is not treated as an undocumented embedded-app API.

Local ARM64 compilation, 31 focused portable tests, all three exact Luau
validators, the Java release build, and the new non-debuggable/no-backup
artifact gates pass. The ignored final APK is 37,764,812 bytes, SHA-256
`5d2f0539bae49c4bdbe0081cf339cd481dfce69d017e77b467634557260ac661`.
This work also found `android:debuggable=true` in the never-installed
Wi-Fi-only OTA. Because the installed core APK came through the same earlier
Gradle path, it is conservatively treated as debuggable until replaced; no
OpenAI credential was configured in either image. The combined release OTA is
therefore a required security correction; the Wi-Fi-only OTA hash is rejected
and must not be sideloaded.
This is not physical-device evidence. The handset is parked safely at Lineage
Recovery's main menu; ADB is intentionally unauthorized there until a person
selects the sideload menu. No OTA or wipe occurred. A combined inspected OTA
is the next remote artifact gate; a physical sideload selection, deterministic
agent activation, Keystore/live request, Wi-Fi UI, and runtime security scans
remain pending.

The lock risk is understood by the device-tree maintainers. The
[`A336BXXSEGYJ3` blob update](https://github.com/exynos1280/android_device_samsung_a33x/commit/a85c2a9652c93880a1c1474a098a72368d416e21)
explicitly declined to update bootloader blobs because the newer `sboot.bin`
adds an auto-lock property, and it pinned matching TEEgris firmware from
`A336BXXSEFYH2`. That directly agrees with the connected One UI 8 observation.

## Recovery and irreversible-risk assessment

The [official TWRP device page](https://twrp.me/samsung/samsunggalaxya33.html)
lists `a33x` as current and correctly documents its dynamic
partitions, A-only system-as-root layout, recovery-partition rooting, AVB
requirements, fastbootd boundary, and Odin installation method. However, the
[available download](https://dl.twrp.me/a33x/) is TWRP 3.7.1_12-0 from
2024-02-18. Its
[device configuration](https://github.com/TeamWin/android_device_samsung_a33x/blob/android-12.1/BoardConfig.mk)
uses a prebuilt kernel, targets Android 12, disables FBE/metadata decryption,
allows missing dependencies, and has not changed since January 2024. It must
not be treated as a recovery image proven compatible with either the proposed
Android 15 rollback or this Android 16 firmware. A direct audit of the
published TeamWin tree measured its `prebuilt/Image` at 31,461,888 bytes,
SHA-256
`593ad8f97564fe067ca5dec37417e7eeac6b0b80f342c6407e4fa280c6fe606e`.
Its embedded version string is Linux
`5.10.66-Gabriel260BR-TWRP-ga0103aac9499`, built 2023-01-01 with the Android 12
toolchain. The official page's device-specific changelog is empty, so it does
not state a compatible Samsung firmware baseline. TWRP can still be useful
later, but this published image is not the project's trusted rescue path and
cannot decrypt `/data`.

Unlocking is intentionally destructive. Android requires a factory reset when
changing the lock state. Samsung documents the Knox Warranty Bit as a one-time
programmable e-fuse: loading a non-Knox kernel can change it from `0` to `1`,
disable Knox-backed services, and cannot be reversed without mainboard
replacement. Samsung also documents rollback fuses that prevent loading a
bootloader older than the fused revision. See Samsung's
[Knox FAQ](https://docs.samsungknox.com/admin/knox-platform-for-enterprise/faq/)
and
[hardware-backed security description](https://docs.samsungknox.com/admin/fundamentals/whitepaper/samsung-knox-mobile-security/system-security/hw-backed-security/).

Samsung's update history identifies `A336BXXSEFYH2` as the 2025-08-27 Android
15 / One UI 7 build immediately preceding the Android 16 / One UI 8
`A336BXXUEGYI8` build. The connected phone reports EUX and rollback level 14;
third-party Samsung-firmware metadata also identifies both packages as binary
revision `E`. An EUX full-file `FYH2` package is listed at 7.86 GB with MD5
`45d5e89f77e1dfa8ee45d62b9c376e93`, but it has not been downloaded or
independently verified here. Samsung's own FYH2 release note warns in one
locale that its security-policy update prevents downgrading to older software;
it does not promise that a later One UI 8 installation can return to FYH2.

The shared revision makes a Samsung-signed stock rollback plausible, not
proven. The rollback bit is a necessary anti-rollback check, but a complete
flash can still be rejected by other Samsung verified-boot/version-binding
checks or fail for host, cable, archive, CSC, storage, or power reasons. No
exact stock package is present locally, two GitHub "firmware preservation"
repositories found during the audit are empty, and no validated
Odin/Heimdall/Thor tool is installed. Firmware metadata alone cannot establish
signature, completeness, downgrade acceptance, or recoverability.

## Authorized rollback preparation

The owner authorized the destructive sole-device experiment on 2026-08-15 and
confirmed that no user-data backup is required. Preparation used native Fedora
44 on bare metal with 572 GiB free, a direct Samsung USB connection, and a
100% battery. No VM, USB passthrough, hub, or extension cable was involved.

The pinned flashing/downloading tool is
[`topjohnwu/samloader-rs`](https://github.com/topjohnwu/samloader-rs/releases/tag/2.0.0)
2.0.0, source tag `714d22edad16038f852cfd15903e387d1cd76d9b`:

- release archive `samloader-v2.0.0-linux-x86_64.tar.xz`, 2,328,736 bytes,
  SHA-256
  `7c6514028f20d5ea0eb57d6f872eee41b3a52336eabac6379b15a01a06ed7a79`;
- extracted static PIE `samloader`, 8,470,752 bytes, SHA-256
  `8a12712a530aa404df50df4fef0b16b7e0081b5362a3a34c752472d79c61f288`.

`samloader check-update --model SM-A336B --region EUX --all` returned both
exact four-part versions from Samsung FUS. `samloader download` then streamed
and decrypted both packages directly from Samsung:

| Package | Bytes | SHA-256 |
| --- | ---: | --- |
| `SM-A336B_EUX_A336BXXSEFYH2.zip` | 8,436,163,243 | `71a9a3433400cd0002541020395b5680f8651c4b3bf47f0e7d895e94d7f959d6` |
| `SM-A336B_EUX_A336BXXUEGYI8.zip` | 8,086,643,743 | `237d6567800569a7120474761643fd3571b1cfbb93a3d841e932495b65300bc3` |

`unzip -t` passed every member in both packages. `samloader verify-md5`
independently accepted every embedded Odin archive. The extracted artifacts,
kept outside Git under `/home/carlid/sos-samsung-work/firmware`, are:

| Build / archive | Bytes | SHA-256 |
| --- | ---: | --- |
| FYH2 `AP_...meta_OS15.tar.md5` | 8,931,051,642 | `e464fb0ca809a70c907cc1d24bc805a729a30485ecc11b77ddcabaf98a17acdf` |
| FYH2 `BL_...tar.md5` | 7,854,192 | `03fde31217af39bdc21d284b93efde41d3f4b424513f68a186b4791fb97b14a3` |
| FYH2 `CP_...tar.md5` | 41,338,989 | `146e9458a4bb5947e8abdb90d293df5397f482b1e49c8e63163ba94062efd7cf` |
| FYH2 `CSC_OXM_...tar.md5` | 751,186,023 | `ff5713c85c1a7bd48fdb7da0ab4da4927211a901fb89a0815ccf11cfa3fd6e9c` |
| FYH2 `HOME_CSC_OXM_...tar.md5` | 751,145,068 | `92e3fad632b0d88166938e358ae286f7e73b46119c7280304e1bdc047679d551` |
| GYI8 `AP_...meta_OS16.tar.md5` | 9,176,709,243 | `b91c211840f79d0d4fd6f25b47b3b8d95b6ce73ffdef989e17389815f7042258` |
| GYI8 `BL_...tar.md5` | 7,977,073 | `8f22613b7254b42059333ebdb8433cb563cc8bffff8b3e5645d1f1ebc9e26fe9` |
| GYI8 `CP_...tar.md5` | 41,359,470 | `62505ccb311f7118d0d5dcc07bab759be6d7dfb50e02b8ed3142d287efbb4b9e` |
| GYI8 `CSC_OXM_...tar.md5` | 419,317,864 | `50087bd7eabe6772836b0e760c9218914ad70938485dbcb04d625e0b63d5d5bc` |
| GYI8 `HOME_CSC_OXM_...tar.md5` | 419,297,389 | `ae2eb59101cb87a5d82daf181cdcdf881d666326feae4333443f888ae7e3243a` |

The FYH2 and GYI8 `A33X_EUR_OPEN.pit` files have different raw SHA-256 values,
`a8e1b5dfa0468bf88278957c29b35dc2016a945019a9face30ed8136f0aa7d01`
and `b940d64afbc05448942fbea517e9ed4868bad1f876355a01d1861010f2d5b872`,
but `samloader print-pit` produces identical 48-entry parsed layouts. No
repartition flag will be used.

The phone was then rebooted with `adb reboot download`. It enumerated normally
as Samsung Download Mode USB `04e8:685d`, and `samloader detect --verbose`
returned `Device detected`. Fedora initially created the node as `root:root`
mode `0664`, so `samloader dump-pit` stopped before protocol setup with
`Access denied (insufficient permissions)`. After the owner granted an
ephemeral per-user ACL, the Odin 5 handshake passed and the live PIT dump
completed. The ignored `SM-A336B-live-before-FYH2.pit` is 8,192 bytes,
SHA-256
`238552c2c4857cb7cf4a5e2c8033b324478bf5201ff14552f33f93cd15c2c53a`;
its parsed header and all 48 entries exactly match the FYH2 package PIT.

The PIT command ended its Odin session but the handset remained in Download
Mode. Two subsequent flash invocations revalidated all four FYH2 archives but
timed out during the initial `ODIN` handshake, before session setup, file
transfer, or a partition write. Resetting the Linux USB port with
`usbreset 001/006` succeeded but did not reset the device-side Odin session;
the second handshake failed identically. The flashing implementation requires
a real device reboot between Odin sessions. The next gate is a persistent
`uaccess` udev rule for future Download Mode enumerations, followed by a
physical Side + Volume Down reboot, a fresh `adb reboot download`, and a single
new flash session. The phone remains in Download Mode with stock GYI8 intact
at this pause.

That reset path worked. GYI8 returned intact, `adb reboot download` created a
fresh `04e8:685d` enumeration with the persistent `uaccess` rule applied, and
one pinned-samloader Odin 5 session flashed the verified FYH2 `BL`, `AP`, `CP`,
and wiping `CSC_OXM` archives. The invocation did not supply a PIT, enable
repartition, clear EFS, or use `HOME_CSC`. Its own session PIT read succeeded,
followed by successful responses for every selected archive member through
the final `PRISM` and `OPTICS` uploads. `samloader` ended the session, issued
its reboot request, released the interface, and exited 0 at 2026-08-15 03:51
CEST. No anti-rollback, signature, size, transport, or device-write error was
reported.

The handset nevertheless remained on the same Download Mode USB identity for
at least 38 seconds after that automatic reboot request. It therefore requires
one physical Side + Volume Down restart. The destructive transfer gate passes;
the FYH2 boot gate does not pass until Android boots and reports the expected
build and security state. Do not flash TWRP or unlock the bootloader before
that read-back.

After confirming the host flasher had exited and no process held the USB node,
the owner used Side + Volume Down to restart the handset. It left Download
Mode, completed first boot, and reached the Android welcome/setup screen. The
host then enumerated a new normal-mode Samsung `04e8:6860` MTP device at
2026-08-15 03:58 CEST. The stock boot and normal-USB portions of the rollback
gate therefore pass. The exact installed build and boot-security properties
still require ADB read-back after setup; OEM-unlock availability is likewise
not yet established.

Setup and ADB read-back subsequently established the exact installed state:

| Property | FYH2 read-back |
| --- | --- |
| Build | `A336BXXSEFYH2`; Android 15 / API 35; 2025-08-01 patch |
| Fingerprint | `samsung/a33xnseea/a33x:15/AP3A.240905.015.A2/A336BXXSEFYH2:user/release-keys` |
| Bootloader | `A336BXXSEFYH2`; rollback level `14` |
| Lock / AVB | `flash.locked=1`; `vbmeta.device_state=locked`; verified boot `green` |
| Knox | warranty bit `0` |
| OEM-unlock UI | Visible and enabled; switch unchecked; summary `Allow the bootloader to be unlocked` |

The UI result was confirmed with a temporary `uiautomator` dump and the device
file was removed immediately. The `ro.oem_unlock_supported` property remains
unset on FYH2 despite the functional Samsung OEM-unlock preference, so the UI
and eventual Download Mode state—not that generic property alone—are the
relevant evidence on this build. The rollback gate is complete. Bootloader
unlock remains a separate, not-yet-attempted destructive operation.

Before entering the unlock UI, the phone was at 100% on USB power and
`automatic_system_updates` was set to `0` to prevent unattended installation
of a newer stock build. The owner enabled the `OEM unlocking` preference and
accepted its warning. A fresh temporary UI dump reported the switch
`checked=true`, while ADB still reported `flash.locked=1` and warranty bit
`0`. This is the expected authorization-only state; no unlock, wipe, or custom
flash had occurred at that checkpoint.

The owner then powered off, held both volume keys while reconnecting USB,
selected `Device unlock mode` by long-pressing Volume Up, and confirmed the
device's `Yes (may void warranty)` prompt. The handset left Download Mode and,
after about 48 seconds with no USB enumeration, returned as Samsung normal-mode
MTP `04e8:6860` at 2026-08-15 04:15 CEST. This proves the unlock-triggered wipe
booted Android, but the unlock result is not final until ADB read-back confirms
the boot lock, AVB, and Knox properties after setup.

Post-reset ADB and UI verification completed the unlock gate:

| Property | Post-unlock result |
| --- | --- |
| Stock baseline | FYH2 fingerprint and `ro.bootloader=A336BXXSEFYH2` unchanged |
| Boot lock | `ro.boot.flash.locked=0`; `ro.boot.vbmeta.device_state=unlocked` |
| AVB | verified boot `orange` |
| Rollback | `ro.boot.rp=14` |
| Knox | warranty bit `0`; `ro.boot.kg=0x1` |
| OEM-unlock UI | Checked and disabled; summary `Bootloader already unlocked` |

The unlock-triggered wipe reset the Android update preference, so
`automatic_system_updates=0` was applied again. No custom binary has been
flashed, which explains why the Knox warranty bit remains untripped at this
stage. The signed-stock rollback and bootloader unlock are now proven on this
unit. A matched recovery/custom-ROM boot and stock restoration remain separate
gates; the old official TWRP build is not promoted by this result.

## Historical gate checklist before the first Samsung system flash

This is retained as the decision record that governed the first custom flash.
The owner classified the sole phone as expendable and explicitly waived
backup; the project then satisfied the stock-restore, unlock, contemporary
recovery, baseline build, and first-boot gates above. The remaining items are
still useful recovery constraints:

1. Back up all user data, account-recovery material, passkeys and two-factor
   credentials somewhere independent of the phone. The owner explicitly
   waived this item for the dedicated development device; factory reset is
   still expected.
2. Acquire exact Samsung-signed EUX four-file packages for both the installed
   `GYI8` build and intended `FYH2` baseline. Record filenames, sizes, SHA-256
   hashes, CSC, and binary revision outside Git; verify both archives before
   putting the phone in Download Mode.
3. Select and pin the flashing host/tool, cable, USB port, and recovery host.
   First prove non-writing Download Mode detection. Do not start the downgrade
   without a verified current-build restore bundle on a second storage device.
4. If accepting the destructive experiment, perform only the complete
   Samsung-signed stock-to-stock rollback and factory boot first. A visible
   OEM-unlock control and a matching `FYH2` fingerprint are the next gate; do
   not combine rollback, unlock, TWRP, and SOS into one operation.
5. After unlock and before system experiments,
   capture EFS and other device-unique partitions through a known-compatible
   recovery. Never restore another phone's EFS.
6. Accept the factory reset and irreversible Knox consequences explicitly,
   then confirm unlock state as `flash.locked=0` / verified boot `orange` before
   flashing custom images.
7. Do not use the 2024 official TWRP image as the sole recovery plan. Reproduce
   a pinned community Android 16 `a33x` baseline from source, build a recovery
   from its matching kernel/device tree, and pass boot, display/touch, ADB,
   Wi-Fi, suspend/resume, thermal, and stock-restore gates before adding SOS.
8. Only then create a separate ARM64 SOS product: package the ARM64 APK and
   authority, port the SOS `system_ext` init/SELinux policy, provision signing,
   and adapt verification away from Cuttlefish assumptions.

The SOS authority itself is not the present blocker. From SOS revision
`b6da0369c29092b85720c4c20a8a56707afc2942`, this command passed:

```sh
cargo ndk -t arm64-v8a -P 31 build \
  -p android-system-authority \
  --bin sos-android-system-authority \
  --release --locked
```

The ignored `target/aarch64-linux-android/release/sos-android-system-authority`
is a stripped ARM64 Android 31 PIE, 1,216,976 bytes, SHA-256
`d4e25d1e5be06b5c4b7cf2fd159f7a9f593788f37626e0b49b278faeaa2b8158`.
