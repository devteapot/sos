# SOS

SOS is a research prototype for an agent-generated native mobile experience.
The project starts with a kill-or-confirm spike for the community GPUI Mobile
Android port. No agent, provider protocol, or operating-system architecture is
part of Milestone 0.

**Status:** Milestone 0 is confirmed on a physical Samsung SM-A336B. See the
recorded hardware checks and limitations in
[`docs/experiment.md`](docs/experiment.md).

## Milestone 0

The upstream GPUI Mobile source is fetched at a pinned commit into `.cache/` and
is never patched. The wrapper builds an optimized ARM64 Rust library, packages
it in a debug-signed APK, installs it on a connected phone, launches it, and
follows the process logs:

```sh
./tools/sosctl run
```

Useful individual commands:

```sh
./tools/sosctl doctor
./tools/sosctl sync
./tools/sosctl build
./tools/sosctl install
./tools/sosctl launch
./tools/sosctl logs
```

Use `./tools/sosctl run --no-follow` when a non-blocking command is needed in
automation. Build products are placed in `artifacts/` and are intentionally not
tracked.

The experiment contract and current device results are recorded in
[`docs/experiment.md`](docs/experiment.md).
