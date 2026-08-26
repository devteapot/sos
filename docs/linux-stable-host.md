# Linux stable-host vertical slice

Date: 2026-08-09

SOS now has both a real Linux presentation host and a minimal trusted Wayland
compositor. The host can remain an ordinary client for desktop development, or
the Debian reference VM can boot directly into a systemd/PAM session where SOS
owns tty1, seat0, DRM/KMS, libinput, surface policy, and presentation evidence.
Both paths preserve the same generated experience, permanent-host lifecycle,
and local provider/state boundary.

## Current process and presentation boundary

```text
revision supervisor
    | newline JSON: boot / prepare / present / confirm / discard / shutdown
    v
permanent sos-experience-host process
    | Luau -> validated Scene ABI v3 -> retained GPUI elements
    v
one GPUI Wayland surface
    v
existing Wayland compositor
```

`linux-run` also owns a sibling provider/state authority. The Linux-only
`sos-linux-session` helper verifies the revision store, bootstraps an empty
authority from the committed pointer, and stages each candidate before asking
the coordinated supervisor to activate it. The same authority socket is passed
to the supervisor and permanent host; either top-level service exiting tears
down the developer session. A non-empty mismatch remains fatal unless the
durable activation journal binds the pointer as its previous revision and the
authority as its candidate; that one crash boundary is handed to the existing
coordinator recovery path instead of being mistaken for corruption.

`experience-host-protocol` owns the transport ABI shared by the supervisor and
host. Host stdout is reserved for newline-delimited protocol events; all GPUI,
runtime, timing, and recovery diagnostics go to stderr. A candidate is compiled
and rendered in a fresh Luau VM while the accepted scene remains active. On
`present`, the worker commits that prepared VM and the same GPUI entity renders
the new scene. In an ordinary Wayland session, a GPUI next-frame callback emits
`presented`. Under `sos-compositor`, the host instead arms the scene handoff and
waits for the compositor to observe its later shell commit in a successful
nested backend submit. A later `confirm` request proves that the event loop is
still responsive before the supervisor advances `current`.

The Linux adapter consumes Scene ABI v3: orthogonal layout/content/paint/
interaction/animation facets, bounded containing-block layout programs, nested
clips/transforms/layers, host-shaped glyph runs, revision images/fonts, and tap,
double-tap, long-press, swipe, drag, drop, and single-pointer events all render
through GPUI. Android and Linux use the same revision asset/font registry and
retained paint/gesture surface. A small shared hook lets Android register each
surface with its raw NDK multi-pointer router without making that router part of
the Linux client.

The host validates manifest format 3, file sizes, SHA-256 values, source/state
binding, schema, and API version 3 again before activation. It separately loads
and re-verifies the candidate revision's content-addressed image/font/shader
sidecars, then supplies only that set to the candidate's fresh VM. A candidate
therefore cannot inherit assets from the boot or accepted revision. Wayland
objects and file descriptors are never exposed to Luau.

When `--service-socket` is configured, interaction results are committed on a
background thread through the versioned provider/state Unix-socket protocol.
The Linux boundary maps only the typed `notes.attach_to_event`, `notes.write`,
`calendar.append`, and `music.command` effects; unknown actions are rejected.
It checks the active revision's grant for each real adapter, isolates caller
cancellation/disconnection, reconciles an ambiguous promotion by stable
transaction ID, and accepts the new scene only when authority returns the exact
expected state.

## Run it in an existing Wayland session

On a Linux desktop or VM logged into a Wayland session:

```sh
./tools/sosctl linux-run --windowed
```

Omit `--windowed` for a fullscreen shell surface. The first run creates an
ignored content-addressed store at `.cache/linux-revisions-v3`, installs
`experiences/default.luau`, and boots it through the real supervisor. The
command stays in the foreground so host and recovery logs remain visible.

From a second terminal:

```sh
./tools/sosctl linux-script experiences/daily-flow.luau
./tools/sosctl linux-status
./tools/sosctl linux-stop
```

`linux-script` accepts an optional second `state.json` argument and otherwise
starts the candidate with `{}`. It stages that immutable state in the authority,
then supplies the stable transaction ID to the coordinated supervisor. Set
`SOS_LINUX_REVISION_ROOT` to use a disposable or isolated revision store. Use
the supervisor's `install --asset ID:KIND:FILE` interface described in
[`revision-supervisor.md`](revision-supervisor.md) when testing v3 sidecars.
The shell helper remains development orchestration. The boot-owned appliance
path below implements the same lifecycle without granting generated code
process authority.

The reproducible Debian 13 guest definition, provisioning command, and nested
acceptance gate are in [`linux-vm.md`](linux-vm.md).
The authenticated Smithay shell, compatibility-surface policy, input fence,
crash recovery, and compositor-owned submit evidence are in
[`linux-compositor.md`](linux-compositor.md).

## Select SOS from the GDM login screen

SOS can be installed as a selectable Wayland session without replacing GNOME
or changing the machine's default systemd target:

```sh
sudo apt-get install \
  libgbm-dev libinput-dev libseat-dev libudev-dev libwayland-dev \
  libxkbcommon-dev libxkbcommon-x11-dev
./tools/install-linux-login-session doctor
./tools/install-linux-login-session install
```

On Fedora, the direct-session module names map to
`mesa-libgbm-devel`, `libinput-devel`, `libseat-devel`, `systemd-devel`,
`wayland-devel`, `libxkbcommon-devel`, and
`libxkbcommon-x11-devel`. The complete first-hardware dependency recipe is in
[`linux-hardware-gate.md`](linux-hardware-gate.md).

Log out after installation. On GDM, select the session menu on the login screen,
choose **SOS**, and authenticate normally. GDM's PAM/logind session remains the
seat owner, but `sos-compositor` replaces GNOME as the session compositor and
the experience host is its only ordinary desktop surface. Press
`Ctrl+Alt+Backspace` to end SOS cleanly and return to GDM. Choose GNOME from the
same session menu on the next login to return to the conventional desktop.

The installer builds the direct compositor, Linux host, authoring broker, and
pinned Node agent in release mode. It installs the session binaries and
launchers below `/usr/local/libexec/sos`, the resident agent below
`/usr/local/libexec/sos-agent`, the reference experiences and API documentation
below `/usr/share/sos` and `/usr/share/doc/sos`, the per-user
`sos-session.target` and `sos-session-shutdown.target`, and adds
`/usr/share/wayland-sessions/sos.desktop`. On first installation it also runs
the agent's device-code authentication flow as the desktop user; set
`SOS_AGENT_MODEL` before invoking the installer to override the `gpt-5.6-sol`
default. The selectable-session credential helper currently requires the
subscription-backed `openai-codex` provider; API-key providers retain their
separate appliance/development configuration. The installer does not stop or
reconfigure GDM, change the default boot target, create service users, or enable
the appliance units. The first SOS login creates the authenticated user's
private revision, authority, recovery, and shell-token state below
`${XDG_STATE_HOME:-$HOME/.local/state}/sos`; each login receives a fresh private
runtime directory below `XDG_RUNTIME_DIR`.

The experience host keeps the authenticated login's real `HOME`,
`XDG_CACHE_HOME`, user D-Bus address, and `XDG_RUNTIME_DIR`, while its compositor
socket remains an absolute path in the private SOS runtime directory. This is
required for user PipeWire, portals, and other session services. Once the
provider and supervisor sockets are ready, the launcher publishes the bounded
graphical environment to the user manager and starts `sos-session.target`.
That target binds the standard `graphical-session.target`; its paired shutdown
target conflicts both targets out on logout, so portal and native-application
services cannot linger after SOS exits. `XDG_CURRENT_DESKTOP=SOS:GNOME` permits
the standard GNOME and GTK portal backends to participate, although a portal
operation that specifically requires a Mutter-only protocol remains unavailable
under the SOS compositor.

For an active SOS login, the launcher holds a logind block inhibitor for
`idle`, `sleep`, and `handle-lid-switch` around the complete
`sos-linux-session run-user` lifetime. The direct compositor has no independent
screensaver, so the stock development behavior is an always-present shell that
keeps its network link available and does not suspend when the lid changes.
The inhibitor is released automatically on SOS logout and does not change the
GNOME session's stored preferences. A future trusted suspend action must first
release this session-owned inhibitor as part of its fixed native ceremony;
generated Luau cannot remove or bypass it.

For a credential- and network-independent first hardware gate, install with
`./tools/install-linux-login-session install --offline`. This configures the
same resident runner with the checked-in deterministic `daily-flow.luau`
candidate and preserves the same broker, validation, activation, and monitored
lifecycle boundaries. It is a hardware isolation mode, not live-model evidence.
Running `sos-agent-login` later replaces that configuration with the normal
subscription-backed mode.

Each install writes `/usr/share/doc/sos/install-metadata.env` and
`install-manifest.tsv` with the source revision, dirty state, toolchain, mode,
and installed artifact sizes and SHA-256 values. The session reads bounded
mode/scale/rotation overrides and absolute-input output associations from the
user's private
`${XDG_STATE_HOME:-$HOME/.local/state}/sos/output.json`; `{}` retains preferred
mode, scale 1.0, rotation 0, and the mirrored output layout. Mirror mode uses
the largest logical canvas that fits every connected output, centers it on each
physical mode, and keeps one workspace when a lid or external output appears.
Set `"layout": "extend"` for a connector-sorted horizontal desktop. On a
multi-output system, every touchscreen, tablet, or absolute mouse must name its
output; ambiguous or unavailable mappings fail closed. Relative pointers stay
inside the mirrored canvas or traverse the complete extended layout.

```json
{
  "layout": "mirror",
  "input_outputs": {
    "PiKVM PiKVM Composite Device": "DP-1",
    "ILIT2901:00 222A:5539": "eDP-1",
    "ILIT2901:00 222A:5539 Stylus": "eDP-1",
    "ILIT2901:00 222A:5539 Mouse": "eDP-1"
  }
}
```

```text
GDM authentication (login user owns the active logind seat)
    -> sos-login-session
        -> sos-linux-session run-user
            -> sos-compositor --backend drm
            -> provider/state authority
            -> revision supervisor
                -> permanent SOS experience host
        -> per-user authoring broker
        -> resident Pi agent
```

The authoring broker and resident agent start automatically after the provider
and supervisor sockets are ready. They are monitored background children of the
login session, use the same private runtime directory, and are stopped on
logout. If either exits unexpectedly, the session fails back to GDM instead of
silently leaving an apparently available composer without an agent. Credentials,
model selection, and message history live below
`${XDG_STATE_HOME:-$HOME/.local/state}/sos/agent`. To reauthenticate or change
the exact model before logging into SOS, run:

```sh
SOS_AGENT_MODEL=gpt-5.6-sol \
  /usr/local/libexec/sos/sos-agent-login
```

The installer completes authentication before offering a successful handoff.
If credentials are later removed, the SOS login refuses to start and its journal
names the login helper rather than presenting a composer backed by no agent.

This path intentionally differs from the boot-owned appliance session's
multi-UID isolation. A process launched from a display-manager session must keep
the authenticated UID that logind authorizes for DRM and input. `run-user`
therefore rejects root, rejects explicit service-account overrides, and requires
the compositor, provider, supervisor, host, authoring broker, and agent to use
the current effective UID and GID. Its per-user directories and shell token are
private, but processes in that login account are not security-isolated from one
another. Use the system session below when the separate service identities are
required.

The selectable-session path has now completed physical GDM login, direct DRM
page flip, provider actions, native application composition, coordinated
activation, and clean logout on a Framework Laptop 12 development-live boot.
The diagnostic campaign did not exercise touchpad motion or touchscreen input,
and its iterative seven-host-launch journal is not a stable-lifecycle pass;
suspend/resume also remains open. Keep SSH and a text console available, then use
`tools/linux-hardware-gate` and the exact PASS contract in
[`linux-hardware-gate.md`](linux-hardware-gate.md). The gate refuses VMs, dirty
or revision-mismatched installs, missing observations, and tampered evidence.
A SOS-baked Fedora Workstation `development-live` remix is a mutable diagnostic
path; it is not an installed product or release-acceptance artifact. See
[`linux-live-image.md`](linux-live-image.md).

After returning to the conventional desktop,
`./tools/install-linux-login-session uninstall` removes the exact installed SOS
session/product paths while preserving per-user state, packages, GDM, and the
default boot target.

## Boot-owned direct session

The packaged session is deliberately one logind session with one Rust lifecycle
owner:

```text
sos-session.target
    -> sos-session.service (PAM login on tty1, active logind session)
        -> sos-linux-session run
            -> sos-compositor --backend drm
            -> sos-provider-state-service
            -> sos-revision-supervisor
                -> permanent sos-experience-host
```

[`sos-session.service`](../packaging/systemd/sos-session.service) creates
private `/run/sos` and `/var/lib/sos` directories, conflicts with the display
manager and tty1 getty, and asks libseat explicitly for its logind backend. The
PAM session is important: merely starting a user process from SSH does not make
it the active seat owner. `pam_systemd` moves the process tree into the active
`session-N.scope`; `sos-linux-session run` therefore remains the explicit owner
that starts, monitors, gracefully stops, and reaps every child. An unexpected
provider, supervisor, or compositor exit fails the lifecycle owner and lets
systemd restart the whole session on durable authority and the committed
revision. A refused stale supervisor socket is removed only after proving that
no process is listening on it.

The lifecycle owner also persists exact executable/PID/start-time records. An
uncatchable death of that owner is recovered by validating this registry before
the replacement session starts; matching survivors are reaped, while already
completed logind/kernel cleanup is recorded explicitly. This is necessary
because the active PAM tree lives in `session-N.scope`, outside the service
cgroup. `PrivatePIDs=yes` was rejected after systemd 257 failed its namespace
exec step for this PAM/tty topology before SOS started. The current boot
campaign kills the lifecycle owner and proves every old-tree PID disappears
before accepting the replacement session.

Startup is presentation-ordered rather than socket-ordered. The compositor
creates an exclusive readiness record only after its recovery view reaches a
DRM page flip. Only then does the lifecycle owner start authority and the
supervisor/host. This keeps the trusted recovery surface present before any
generated shell can connect. Routine unchanged page flips are trace-level;
recovery transitions and armed revision frames remain info-level evidence so a
long-running session does not flood journald.

The shell-token source is a root-owned `0400` file. systemd copies it into the
service's private credential directory with `LoadCredential=`, and the
compositor and host receive only the credential path. The bounded parser rejects
empty, oversized, non-UTF-8, or newline-bearing credentials. The secret is not
present in the lifecycle owner, compositor, supervisor, or host command lines
or environment values. The compositor/session, provider, supervisor, and
generated host run under distinct Unix identities. Per-role credential copies
are owner-readable only, and every role child clears inherited lifecycle
capabilities before exec.

When real Linux providers are configured, `SOS_PROVIDER_GRANTS` names a private
revision-keyed capability manifest. The host loads the candidate's grants and
snapshot before candidate render, switches the live watcher only at commit,
and drops events tagged for another revision. A generated interface therefore
receives typed data/effects, not provider handles, filesystem paths, network
access, or ambient credentials. Development wildcard grants require the
explicit `SOS_PROVIDER_DEVELOPMENT_GRANTS=1` escape hatch. The selectable
session enables that escape hatch automatically only when the baked image
identity says both `image_kind=development-live` and `mutable_runtime=true`;
stable and installed sessions still require exact revision grants.

The first Linux canonical provider slice adapts UPower, NetworkManager, MPRIS,
PipeWire/WirePlumber, and freedesktop desktop entries. D-Bus is preferred for
stateful services. The narrow `wpctl` and `gio launch` adapters pass fixed
argument vectors without a shell, and opaque selections are resolved again at
action time inside the provider boundary. Stock and generated Luau receive the
same `model.providers` value and cannot observe service paths or commands.

`SOS_REVISION_SIGNING_KEY_FILE` makes revision installation emit a detached
HMAC-SHA256 manifest authenticator; `SOS_REVISION_VERIFY_KEY_FILE` makes every
load require and constant-time verify it. Key files are bounded and must not be
group/world accessible. This is suitable for one-owner prototype update and
rollback testing. Permanent-host binaries remain distribution/image-owned and
must use that update system's signing policy; production revisions should move
to asymmetric signatures rather than distribute a shared HMAC key.

The executable link on Debian/Ubuntu requires the XKB development libraries in
addition to the usual Rust/GPUI Linux prerequisites:

```sh
sudo apt-get install libxkbcommon-dev libxkbcommon-x11-dev
```

## Local evidence

The first nested proof ran on ARM64 Ubuntu 24.04 with Weston 13.0.0. Weston ran
with its X11 backend inside Xvfb to supply a real `wl_seat`; its headless backend
has no seat, and the pinned GPUI Wayland client currently unwraps that global at
startup. Rendering fell back to Mesa software paths because the user could not
open the host render node. That is sufficient functional evidence, not a GPU or
latency result.

After rebasing onto Scene ABI v2, the real host booted revision `ff63f61d…` and
emitted a GPUI next-frame event in PID 1527912. It prepared and committed
`daily-flow.luau` as revision `bc81479e…` in the same PID (116 us queue,
1,194 us compile, 646 us render, 1,848 us worker total). It then activated the
richer `android-exit-agent.luau` scene as revision `99ba2162…`, exercising
layers, shaped glyphs, gestures, and a revision SVG in that same process (26 us
queue, 1,936 us compile, 665 us render, 2,608 us total). The deliberately
infinite revision `628cb7a7…` was interrupted and rejected while `99ba2162…`
remained accepted in PID 1527912. Killing that host made the supervisor boot the
exact committed revision in PID 1528477 and report `HostRestarted`.

The v3 integration reran the nested proof in the same environment. The host
booted `f174e726…` in PID 1606742, then prepared and presented sidecar-backed
revision `728f905e…` in that same PID (119 us queue, 874 us compile, 157 us
render, 1,039 us worker total). Its 986-byte source
`tests/fixtures/sidecar-image.luau` (SHA-256 `3ec9aa6d…`) references the checked-in
4,021-byte PNG `mipmap-mdpi/ic_launcher.png` (SHA-256 `11ddafaa…`) through the
supervisor manifest rather than embedding it in Luau. Infinite revision
`632ce86e…` was rejected on its Luau time budget while `728f905e…` and PID
1606742 remained accepted. Killing the host restarted that exact committed
sidecar revision in PID 1607073 with a 1,089 us worker initialization and a new
GPUI frame.

The Linux host unit suite also starts the real provider/state daemon on a local
Unix socket, commits state plus one typed notes attachment, shuts the service
down, restarts it from the durable authority file, and reads the same attachment
back through the socket. The combined workspace suite additionally proves that
each prepared worker candidate receives its own sidecar set and that the Linux
manifest loader carries verified sidecars into the runtime boundary.

The coordinated executable rerun used an isolated Xvfb/Weston seat and store.
It bootstrapped authority revision `f174e726…`, booted the real host in PID
1661845, staged transaction `linux-activate-1-552f0696…`, and activated revision
`552f0696…` in that same PID. The worker reported 34 us queue, 1,162 us compile,
654 us render, and 1,825 us total. After activation the durable authority and
supervisor `current` pointer both named the full candidate revision. `linux-stop`
shut down both top-level services, and the exact disposable read-only store was
made owner-writable and removed. This was still the ARM64 Ubuntu nested setup,
not the Debian VM gate.

The reproducible VM gate then passed in an ARM64 Debian 13.6 guest on kernel
`6.12.100+deb13-arm64`, provisioned from the verified official generic image.
Weston 14.0.2 ran inside Xvfb 21.1.16 with Mesa Vulkan 25.0.7. The real host
booted `f174e726…` and initialized its worker in 3,635 us, then activated
`552f0696…` through `linux-activate-1-552f0696…` in unchanged PID 3874. That
candidate measured 374 us queue, 1,147 us compile, 651 us render, and 1,807 us
worker total. The exact authority revision and supervisor pointer matched after
the GPUI frame and confirmation. This completes the functional client-host VM
gate, not a GPU, compositor-presentation, or latency gate.

The boot-session gate then rebooted that same VM with `sos-session.target` as
its default while seatd was disabled. logind reported active Wayland session 1
on seat0/tty1 with lifecycle PID 770 as its leader. The host booted in PID 883,
activated `552f0696…` in that same PID, and recovered the killed host in PID
1089. Boot, activation, and recovered boot produced DRM-backed commit/submit
pairs 1/3, 43/11, and 50/19. Killing the provider failed the lifecycle owner;
systemd restarted it once as PID 1222, removed the refused stale supervisor
socket, and booted the committed revision in host PID 1312 with a new 1/3
page-flip fence. The verifier then rebooted to `graphical.target`, confirmed GDM and
seatd, and removed its exact units, binaries, credential, and state.

## Completed Linux envelope

The permanent Linux host now has the following platform adapters around the
same Scene ABI and worker:

- compositor/client `wl_touch` transport into the shared multi-pointer router,
  plus a compositor-owned cursor and direct-libinput VM campaign;
- a compositor-restricted `sos-input-method` input-method-v2 client with pinyin
  preedit/candidates/commit, candidate selection, dead acute composition,
  keyboard grab, popup rendering, and cursor rectangles;
- end-to-end Wayland data-device clipboard ownership for copy, cut, and paste;
- Markdown notes, iCalendar, JSON/MPRIS music, and Linux time, connectivity,
  PipeWire audio, battery/AC, DRM-display, and input-device snapshots with
  revision-scoped grants, live generation events, typed writes/commands,
  cancellation, disconnected-client isolation, and explicit unavailable errors;
- capability-scoped video/camera frame surfaces backed by provider-owned atomic
  PNG/JPEG/WebP updates; protected content is represented but deliberately
  remains unavailable because this prototype has no secure scanout path;
- an SOS-owned Unix semantic service for traversal, semantic focus, activation,
  scrolling, editable text/selection/clipboard actions, status changes, and
  automation waits;
- a direct compositor recovery panel showing current/previous revision,
  progress and failure, with restart, rollback, safe-mode, and provider-disable
  controls. The lifecycle owner republishes status after ordinary activation;
- connector/DRM-device rescans, libseat pause/activate handling, live output
  mode/scale/rotation configuration, simultaneous outputs, input hotplug,
  child/host/lifecycle-owner recovery, clean stop, and durable authority;
- executable resource-free WGSL paint through a capped offscreen target and an
  opt-in bounded rootless XWayland compatibility service;
- private revision grant manifests, bounded Luau execution, a restricted
  systemd unit, scoped shell credential delivery, and optional detached
  HMAC-SHA256 revision-manifest verification.

`tools/linux-compositor/verify-nested` proves the IME, clipboard, provider,
accessibility, activation, abort suppression, and host-recovery interactions in
one campaign. `tools/linux-vm/verify-direct-session` uses kernel `uinput`
keyboard, relative-pointer, and two-contact touchscreen devices through direct
libinput, including held inputs across successful and aborted activations.
`tools/linux-vm/verify-boot-session` boots the appliance target, invokes the
recovery rollback channel in both directions, kills/restarts the host, kills a
provider to restart the whole service, and restores GNOME afterward.

## Honest remaining boundaries

- An ordinary `linux-run` still uses GPUI's next-frame callback. Nested
  compositor submit evidence cannot prove that its outer compositor displayed
  the buffer. Direct mode waits for DRM VBlank, but only physical hardware can
  turn that into a panel/touch latency claim.
- Core `wl_touch` does not carry finger pressure; tablet-v2 transports stylus
  pressure. Physical calibration and touch/stylus coexistence remain unverified.
- The VM proves libseat pause/resume, kernel freezer suspend/resume, connector
  removal/reconnect, two simultaneous VirtIO outputs, and live mode/scale/
  rotation. Full platform sleep/wake, physical hotplug, another real DRM device,
  target GPU/panel behavior, memory pressure, latency, thermals, and physical
  touch remain unverified under the user's hardware waiver.
- The prototype service processes have distinct Unix identities, executables,
  owner-managed sockets, peer checks, scoped credentials/capabilities, and
  zero effective child capabilities. Fine-grained MAC policy and production
  secret rotation remain hardening work.
- HMAC manifests provide prototype authenticated rollback/install control, not
  public-key distribution or a production permanent-host update system. A
  production image should use distro/immutable-image signing for the host and
  asymmetric revision signatures.
- XWayland is deliberately opt-in and bounded; it is compatibility, not a
  promise to integrate every legacy application or protected media stack.
- Provider frames are functional image updates, not zero-copy decoded video;
  protected playback, camera capture ownership, and secure scanout require a
  concrete provider/device integration.

The Linux integration prototype envelope is complete at virtual-device scope.
The remaining items above are physical-device evidence or production/optional
compatibility work, not claims inferred from the VM. Physical touch-device
verification is explicitly waived because no target is available.
