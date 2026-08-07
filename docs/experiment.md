# Milestone 0: GPUI Mobile hardware gate

## Thesis

Before building an agent or provider system, prove that unmodified GPUI Mobile
can support the basic interaction loop of a phone experience on real ARM64
Android hardware.

The upstream dependency is pinned to GPUI Mobile commit
`1d3ec2a1d14a63b74d1f4269340441d4eeada27a`. That revision pins GPUI and
`gpui_wgpu` to Zed commit `5688167d224b5eca54875d49afb8bfd73a07915a`.

- [GPUI Mobile](https://github.com/itsbalamurali/gpui-mobile)
- [GPUI Mobile dependency pins](https://github.com/itsbalamurali/gpui-mobile/blob/1d3ec2a1d14a63b74d1f4269340441d4eeada27a/Cargo.toml)
- [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)

## Gate

Milestone 0 is confirmed only when all of the following work on a physical
ARM64 phone:

| Check | Pass condition |
| --- | --- |
| Build | A clean checkout produces an installable APK from one command. |
| Renderer | The app selects Vulkan and renders the complete example UI. |
| Touch | Taps invoke GPUI handlers and visibly mutate state. |
| Scrolling | A long GPUI list scrolls and settles without a crash or hang. |
| Keyboard | Tapping a GPUI text field opens the IME and entered text is rendered. |
| Animation | A custom animated element visibly advances across frames. |
| Lifecycle | Ten home/resume cycles preserve the process and return a valid surface. |
| Repeatability | `./tools/sosctl run` builds, installs, launches, and follows logs. |

If keyboard input, lifecycle recovery, or frame production cannot be made
reliable without taking ownership of a large GPUI Mobile fork, stop and compare
the cost with the Impeller fallback before starting Milestone 1.

## First run: 2026-08-08

Host and target:

- macOS ARM64 host
- Rust `1.94.0`
- Android NDK `29.0.14206865`
- `cargo-ndk 4.1.2`
- Samsung SM-A336B, Android API 35, 1080x2400, Mali-G68
- Upstream example rendered through the Vulkan adapter

Observed results:

| Check | Result | Evidence |
| --- | --- | --- |
| Build/install/launch | Pass | Unmodified upstream debug build installed; optimized-native/debug-signed APK is 26 MB and launches cold. |
| Renderer | Pass | Logs select `Mali-G68 (Vulkan)` and the home screen renders correctly. |
| Touch | Pass | Injected taps changed the upstream counter and created animation particles. |
| Scrolling | Pass | A long swipe moved the Material Form from personal information to later sections with momentum. |
| Animation | Pass | Screenshots 0.8 seconds apart differ after a particle burst and continuous render logs are present. |
| Lifecycle | Pass | Ten home/resume cycles retained PID `24045`; pause, surface termination/recreation, and resume completed without a fatal log. |
| Keyboard/text | Pass | Tapping Full Name invoked the GPUI handler, opened the Samsung IME (`mInputShown=true`), focused the field, and rendered appended text (`Jane DoeCodex42`). |
| Repeatability | Pass | `./tools/sosctl run --no-follow` rebuilt the pinned checkout, installed with `adb install -r`, and cold-launched the generated APK in 1.44 seconds. `./tools/sosctl logs` then followed the live process. |

The current conclusion is **confirm**. The unmodified pinned GPUI Mobile example
passes the Milestone 0 gate on this device. This is evidence to proceed with the
small synthetic Milestone 1 experience, not a claim that the port is
production-ready.

The reproducible APK is `artifacts/gpui-mobile-m0-1d3ec2a1d14a.apk` (26 MB),
with SHA-256
`4e470128b8e710c488063184fd6105ac60ae63356b6ce67acb6cef8c30c3736f`.

## Keyboard reproduction

1. Run `./tools/sosctl run` and stop log following with `Ctrl-C` after launch.
2. Open **Material Form** from the Home screen, or cold-launch the `gpui://form`
   deep link.
3. Tap **Full Name** with a finger.
4. Confirm the software keyboard appears.
5. Replace or append to `Jane Doe` and confirm the GPUI field renders the text.
6. Background and resume the app once while the field is focused.

## Explicit non-goals

- No agent integration.
- No synthetic or real providers.
- No AOSP fork or privileged shell.
- No on-device compiler.
- No dynamic native-code loading or surface hot-swap.
- No changes to GPUI Mobile during the initial gate.
