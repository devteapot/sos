# Stock experience

Date: 2026-08-26

Stock Shell is the product's substantial default experience and the
integration target for the System Providers ABI. It is a privileged but
replaceable Scene ABI v3 module in
[`experiences/default.luau`](../experiences/default.luau), not a fixed Rust UI
or catalog of native widgets. The permanent host compiles, validates, installs,
and activates it through the same content-addressed revision path used by
agent-authored experiences.

Privilege is narrow and structural. One source node may declare each of
`window_space`, `shell_overlay`, and `application_surface`. The Linux host
reports only bounded geometry and a closed policy over its authenticated
compositor connection. The compositor—not Luau—continues to own surface
identity, mapping, focus, activation, stacking, input, movement, and lifecycle.
No Wayland object, socket, PID, desktop-file path, or arbitrary geometry
authority crosses into the revision. Android renders these Linux integration
primitives as explicit unavailable surfaces.

## Product surface

The initial stock revision is a complete source-defined shell with:

- one permanent top status bar, including provider-backed connectivity, audio,
  attention and clock controls plus up to four bounded native-application
  status contributions;
- one compositor-backed absolute application region whose initial policy is
  bounded floating placement and whose command-center controls can select the
  deterministic tiling or scrolling policy;
- one reserving shell rail. Its collapsed form owns the command-center toggle
  and workspace shortcuts; opening command or agent content reduces the
  application region;
- one compositor-owned agent overlay above shell and application surfaces. Its
  bubble can be dragged anywhere inside the logical output, expands an inline
  source-defined composer above or below its anchor on hover, and opens the
  full agent rail on a stationary click;
- a command center for workspaces, application launch and window policy; and
- one source-defined native application surface, managed in the same window
  space as compatibility clients, whose current revision exposes eight
  workspaces:

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

Every workspace and contribution slot renders an explicit empty or unavailable state when its
provider, capability, or resource is absent. Provider object paths, desktop
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
declared window space. Source-native SOS content uses `application_surface` to
open a separate GPUI/XDG toplevel that the compositor classifies as
`NativeApplication`, so it tiles, focuses, clips, unmaps, and reflows beside
ordinary applications instead of being embedded in the shell window.

This is the first composition boundary, not the final application supervisor:
the stock shell and its one active native application surface still come from
the same revision and host process. Independent application revisions,
namespaced state, lifecycle supervision, and an application registration
broker are the next layer. SOS-native applications may additionally publish a
bounded `status_widgets` contribution through the trusted applications
provider: an ID, visible label/value and optional opaque compatible-application
selection. The optional tap reuses the existing typed `apps.launch` authority;
arbitrary callbacks or app-supplied Luau do not enter the bar. The current
Linux provider publishes an empty contribution set until the native-app
registration broker exists.

The mark is currently an inline immutable SVG declaration and therefore enters
the same revision asset set and validation limits as agent content. Sidecar
images, fonts, and shaders can be added through the normal revision manifest;
there is no stock-only asset path.

## Responsive contract

The root and shell body measure against the complete logical output. The top
bar remains fixed, the application region grows, and opening the 390-pixel
command panel or 430-pixel agent panel reserves that width. The agent overlay
does not reserve shell space and clamps its compositor-owned surface to the
logical output. Rows containing
navigation, cards, controls, notes and applications use retained flex wrapping;
the application region enforces a 320-by-240 minimum and publishes bounds only
once it is at least 160-by-120. This provides one clamshell/tablet source
without branching on a device name or calling back into Luau during layout.

The compositor-backed shell is physically accepted at 1,920 by 1,080 on the
Framework development target. PiKVM clicks opened both reserving rails,
selected tiling, and launched Calculator and Calendar through the applications
provider. Closing Calendar unmapped it and immediately expanded Calculator;
opening the agent pane did not restore the closed client. The source-native
application surface was registered independently as `NativeApplication`.
Stationary bubble input opened the agent rail exactly once, while a drag moved
the overlay across the output and persisted its new anchor without opening the
rail. Hover exposed a working inline composer above the moved anchor. A
physical portrait/tablet gate remains open. The direct compositor mirrors one
logical canvas across the Framework panel and PiKVM by default; independent
per-output shell surfaces remain separate work.

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
