# Stock experiences

Date: 2026-08-27

SOS has platform-specific pinned Stock experiences. They share the Experience
API, System Providers ABI, semantic appearance snapshot, and Shell role, but
they do not share an Experience ID, durable state, revision history, source,
layout, or product semantics.

Stock Shell is the Linux default and desktop integration target. It is the
reserved, pinned `sos.stock.shell` Experience API v4 package whose source lives in
[`experiences/default.luau`](../experiences/default.luau), not a fixed Rust UI
or catalog of native widgets. The supervisor resolves and activates its graph
through the same content-addressed registry path used by other v4 experiences.

Privilege is narrow and structural. One Shell-role source node may declare
each of `window_space` and `shell_overlay`. The Linux host
reports only bounded geometry and a closed policy over its authenticated
compositor connection. The compositor—not Luau—continues to own surface
identity, mapping, focus, activation, stacking, input, movement, and lifecycle.
No Wayland object, socket, PID, desktop-file path, or arbitrary geometry
authority crosses into the revision. Android renders these Linux integration
primitives as explicit unavailable surfaces.

## Stock Mobile product surface

Android boots the separate pinned `sos.stock.mobile` package from
[`experiences/mobile.luau`](../experiences/mobile.luau) and
[`experiences/mobile.package.json`](../experiences/mobile.package.json). It is
not a responsive branch or adapted revision of Stock Shell. It defines a
phone-native top bar, large touch targets, a bottom navigation model, a
source-owned full-screen application launcher, vertically scrolling content,
and a mobile agent surface. Registry experiences and compatible Android
applications open as independent full-screen roots; there is no desktop
window region, floating/tiled policy, command rail, hover UI, or window list.

Stock Mobile owns its own history, state, appearance-write grant, authoring
target, recovery pointer, and `mobile.theme` sidecar. Android's immutable
bootstrap and agent example package only this mobile source. Linux continues
to package Stock Shell and `stock.theme`. Shared semantic appearance values
can keep both products in one design language without sharing style code or
information architecture.

The Shell role here is an authority role: it may present another registered
Experience and write reviewed system appearance. It does not imply desktop
shell chrome. Dismissing an Android top-level Experience returns to Stock
Mobile, while Linux returns to Stock Shell.

## Stock Shell product surface

The initial stock revision is a complete source-defined shell with:

- one permanent top status bar, including provider-backed connectivity, audio,
  attention and clock controls plus up to four bounded native-application
  status contributions;
- one compositor-backed absolute application region whose initial policy is
  bounded floating placement and whose command-center controls can select the
  deterministic balanced-recursive tiling or scrolling policy;
- one reserving shell rail. Its collapsed form owns the command-center toggle
  and workspace shortcuts; opening command or agent content reduces the
  application region;
- one compositor-owned agent overlay above shell and application surfaces. Its
  bubble can be dragged anywhere inside the logical output, expands an inline
  source-defined composer centered over the action and above or below it on
  hover, clamps the composer without moving the action at an output edge, and
  opens the full agent rail only from a stationary action click;
- a command center for workspaces, application launch, window policy, and the
  current bounded `model.shell.windows` list. Each row renders only the
  compositor-advertised focus/close controls for its opaque window ID; and
- one source-defined workspace inside the shell graph, plus registry-discovered
  independently supervised v4 Experiences that Stock can present into the
  compositor's application window space. The current Stock workspace exposes:

- Home, with workspace navigation, provider status, agenda, notes, media,
  attention, system-control, application, and agent entry points;
- Agenda, including calendar-provider event creation;
- Notes, including notes-provider capture and accessible transaction status;
- Media, with canonical media actions and the legacy music fallback while that
  compatibility model remains supported;
- Attention, with bounded notification summaries and acknowledgement;
- System, with connectivity, battery, audio, and power/session facts plus only
  the actions advertised by the provider capability set;
- Apps, with bounded compatible-application rows and opaque launch selections;
- Agent, with the resident conversation, composer, activity/error states, and
  only platform-advertised configuration ceremonies.

Every workspace and contribution slot renders an explicit empty or unavailable
state when its provider, capability, or resource is absent. Provider object paths, desktop
files, process arguments, credentials, and platform handles never enter the
source. Stock emits the same typed provider-effect envelopes as any generated
revision.

## Customization boundary

"Fully customizable" means replacing the Luau source, state schema and assets,
not selecting a theme or rearranging a fixed widget catalog. A replacement may
remove every stock workspace, change navigation and information architecture,
or introduce a different visual system. It remains constrained by the Scene
ABI, the provider grants, the sandboxed runtime, and native trusted ceremonies.

Existing desktop applications are not rewritten or wrapped in CLI calls. The
Linux applications provider discovers eligible freedesktop entries and
launches their normal Wayland/XWayland processes through a strict `gio launch`
argument vector. Those surfaces are compositor clients placed within the
declared window space. An independently presented ordinary-role v4 Experience
runs in its own host process, authenticates as `NativeApplication` without
receiving shell control, and opens a GPUI/XDG toplevel. The compositor tiles,
focuses, clips, unmaps, and reflows it beside ordinary applications instead of
embedding it in Stock.
Stock observes those independent toplevels through the typed shell model. The
model distinguishes native and compatibility windows, marks current focus, and
contains no application ID, process identity, executable, or protocol handle.
Its actions are closed `shell.focus_window` and `shell.close_window` effects;
the compositor re-resolves an opaque ID and rejects stale selections.

Top-level launch and embedded composition are deliberately distinct. A launch
is a registry lifecycle request from the Shell role and preserves the target's
independent Experience ID, state, grants, graph activation, and recovery. An
`experience_mount` is host-owned composition inside one graph and is bounded
by the declared dependency contract described in
[`experience-composition.md`](experience-composition.md). The old
`application_surface` node remains decodable only for retained API v3 rollback
revisions; checked-in v4 Stock does not emit it.

SOS-native applications may additionally publish a bounded `status_widgets`
contribution through the trusted applications provider: an ID, visible
label/value and optional opaque compatible-application selection. Stock
renders that data in its own style. The optional tap reuses the existing typed
`apps.launch` authority; arbitrary callbacks or app-supplied Luau do not enter
the bar. The current Linux provider publishes an empty contribution set until
the native-app registration broker exists.

The mark is currently an inline immutable SVG declaration and therefore enters
the same revision asset set and validation limits as agent content. Sidecar
images, fonts, and shaders can be added through the normal revision manifest;
there is no stock-only asset path.

[`experiences/modules/stock-theme.luau`](../experiences/modules/stock-theme.luau)
demonstrates the typed token shape used by the bootstrap. Larger installed
revisions can submit it as the namespaced `stock.theme` Luau sidecar and load it
through the sandboxed revision-local `require`; the immutable cross-platform
bootstrap keeps an in-file copy so Android can still start without sidecars.
The intended multi-experience contract keeps style modules revision-local and
publishes global accessibility preferences and semantic appearance tokens as
authority-owned data. A mounted child may accept a bounded container override,
but a parent never injects style code or repaints the child scene.

## Stock Shell responsive contract

The root and shell body measure against the complete logical output. The top
bar remains fixed, the application region grows, and opening the 390-pixel
command panel or 430-pixel agent panel reserves that width. The agent overlay
does not reserve shell space. Its 64-pixel action is the persisted anchor; the
expanded composer centers on it when possible and clamps independently at an
edge. The overlay collapses while it is actively moving and expands again after
release, avoiding hover/layout feedback during the gesture. Rows containing
navigation, cards, controls, notes and applications use retained flex wrapping;
the application region enforces a 320-by-240 minimum and publishes bounds only
once it is at least 160-by-120. This provides one clamshell/tablet source
without branching on a device name or calling back into Luau during layout.

Before the user has moved it, the overlay uses declarative end/end placement
with an 18-pixel logical margin, resolved against the current output rather
than hard-coded 1,920-by-1,080 coordinates. A compositor-reported persisted
anchor takes precedence after movement. The shell model also supplies the
logical canvas, output rectangles, scale, primary flag, and mirrored state, so
future source revisions can make output-aware layout decisions without seeing
connector or backend identities.

Stock declares nine hidden validation states covering every workspace, the
command panel, and the agent overlay. Together with the default state, local
and resident-agent validation reports ten scenarios with scene statistics and
path-specific failures before activation.

The compositor-backed shell is physically accepted on the Framework's native
1,920-by-1,200 panel and fitted as a complete undistorted canvas on the
1,920-by-1,080 PiKVM output. PiKVM clicks opened both reserving rails,
selected tiling, and launched Calculator and Calendar through the applications
provider. Closing Calendar unmapped it and immediately expanded Calculator;
opening the agent pane did not restore the closed client. The source-native
application surface was registered independently as `NativeApplication`.
Stationary bubble input opened the agent rail exactly once, while a drag moved
the overlay to the logical edge and persisted its new anchor without opening
the rail. Hover exposed a working inline composer, centered in ordinary space
and clamped while its action remained at the edge. Its field accepted keyboard
focus without opening the agent rail. In Floating mode, both the stock native
chrome and a GNOME Calculator `xdg_toplevel.move` gesture changed and retained
their bounded positions. A physical portrait/tablet gate remains open.
Independent per-output shell surfaces remain separate work.

## Stock trust and recovery

Content addressing proves the bytes activated and makes ordinary revision
mutation detectable; it is not provenance by itself. Android installs the
bootstrap source from AVB/OTA-protected `/system_ext`, pins its revision ID
independently of the mutable current pointer, and transactionally restores it
when a generated current revision fails. A failure of pinned stock escalates to
fixed native Recovery.

The Linux revision store can optionally create and require a detached
HMAC-SHA-256 over `manifest.json`, but the selectable development-live session
does not provision those keys or pin a system-owned stock revision. Its
read-only `/usr/share` source plus content-addressed user revision is suitable
for rapid development evidence, not a signed recovery claim. A Linux release
still needs an immutable system-owned stock pointer and asymmetric signature
verification rooted in release keys; user-owned or on-device HMAC material is
not that trust boundary.
