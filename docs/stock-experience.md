# Stock experiences

Date: 2026-08-28

SOS has separate Linux and Android Stock products. They share Experience API
v4, the System Providers ABI, and authority-owned semantic appearance. They do
not share an Experience ID, revision history, source, state, or layout.

## Stock Shell composition

Linux Stock is a two-package graph:

- [`experiences/default.luau`](../experiences/default.luau) is the compact
  Shell-role root, `sos.stock.shell`. It owns only global shell concerns: the
  top bar, application launcher, compositor window space, layout selection,
  and agent HUD.
- [`experiences/stock-workspace.luau`](../experiences/stock-workspace.luau) is
  the ordinary-role `sos.stock.workspace` package. It owns Home, Agenda, Notes,
  Media, Attention, System, Apps, and Agent workspace content and its typed
  provider effects.

The root mounts the workspace through `experience_mount`. Its locked package
binding names the exact child revision and contract digest. The boundary grants
one optional `active_workspace` property and one `navigate` event. Root and
child keep separate Luau VMs, state, package identities, and provider grants.
Changing the workspace does not require merging its implementation into the
Shell source.

The login session installs the child before the root, resolves the complete
graph, and reviews provider grants for the exact workspace revision pinned by
the trusted root. Trust does not extend to unrelated or merely transitive
children. The supervisor rejects a missing child, stale digest, mismatched
revision, undeclared mount, or boundary field outside the grant.

At login, a newer packaged root replaces the active graph only when the current
root still equals the previously trusted Stock revision. An agent-customized
current root is not overwritten by a package update.

Privilege remains structural. Only the Shell-role root can declare
`window_space` and `shell_overlay`. The compositor owns surface identity,
mapping, focus, stacking, placement, input, movement, and lifecycle. Luau sees
only bounded shell observations and opaque selections, never Wayland objects,
PIDs, executable paths, or sockets.

## Stock Shell product surface

The opinionated default uses a black and white, high-contrast visual system:

- a 41-pixel top bar with the SOS launcher affordance, numbered workspace
  selectors, active title, urgent attention count, connectivity, and clock;
- balanced tiling as the initial window policy with an 8-pixel gap, plus
  explicit floating and scrolling alternatives;
- a centered, source-defined launcher for workspaces, registered experiences,
  compatible Linux applications, current windows, and layout policy;
- a compositor-owned global `Super+Space` chord. The compositor consumes the
  key, emits a closed `launcher` shortcut only to the authenticated Shell, and
  the Shell opens or closes the launcher even when an application has focus;
- no reserving side rail. Launcher and full agent surfaces are centered modal
  layers above the window space; and
- a 54-pixel draggable agent action that expands on hover into a compact
  560-by-184 HUD with recent messages, activity, and a composer. A stationary
  action opens the full agent surface. The compositor persists and clamps the
  anchor.

The root filters its internal `sos.stock.workspace` component from the public
experience launcher. Other registered v4 experiences still open as independent
toplevels with their own state, grants, graph, and recovery history. Compatible
freedesktop applications launch through the typed applications provider and
enter the same compositor-managed window space.

The mounted workspace renders explicit absent or unavailable states when a
provider, resource, or capability is missing. It retains the existing provider
features:

- Agenda reads events and can append an event;
- Notes reads and writes notes with accessible transaction status;
- Media uses canonical media controls with the legacy music fallback;
- Attention displays and acknowledges bounded items;
- System displays connectivity, battery, audio, and power facts and emits only
  advertised controls;
- Apps launches bounded compatible applications; and
- Agent exposes the resident conversation, composer, activity, errors, and
  platform-advertised credential ceremonies.

## Appearance and fonts

The default authority profile supplies semantic dark and light palettes,
high-contrast preference, spacing, small radii, and Geist typography. Dark
Stock uses a black canvas, near-black surfaces, white primary type, muted gray
secondary type, and thin `#2a2a2a` borders. The Shell and workspace resolve the
same propagated appearance snapshot independently. Native Linux host gaps,
text-field borders, and status surfaces also resolve these tokens, so fixed
host paint does not expose the previous green and cream palette.

[`experiences/modules/stock-theme.luau`](../experiences/modules/stock-theme.luau)
is a revision-local monochrome fallback. Global appearance remains
authority-owned data. Parents cannot inject style code or repaint a child.

Geist 1.7.2 is pinned to Vercel's official release archive and verified by
SHA-256 before installation. [`tools/install-geist-fonts`](../tools/install-geist-fonts)
can install the same four variable TTFs for the current user, the host system,
or an image destination root. Fontconfig maps `system-ui` and `sans-serif` to
Geist and `monospace` to Geist Mono. The normal Linux session installer stages
the fonts, OFL license, and alias file into future live images. GPUI Linux uses
Geist as its default text family.

## Customization boundary

The default is opinionated, not monolithic. A customization can replace the
workspace package while retaining the top bar, launcher, tiling region, and
agent HUD, provided the root dependency is updated to the new exact revision,
contract digest, and reviewed boundary. It can instead replace the Shell root
and keep the workspace contract, or replace the full graph.

This remains source customization, not a fixed widget catalog. A replacement
may remove workspaces, change information architecture, draw custom geometry,
or use a different visual system. It remains bounded by the Scene ABI, package
contracts, provider grants, runtime sandbox, and trusted native ceremonies.

Top-level launch and embedded composition remain distinct. Launch crosses the
registry and graph lifecycle boundary. A mount stays inside one resolved graph
and crosses only its declared property and event contract. See
[`experience-composition.md`](experience-composition.md).

Agent-authored Shell candidates must retain exactly one mount for every locked
dependency. The shipped faux-authoring fixture demonstrates changing Shell
chrome without absorbing or silently removing the workspace.

## Responsive and validation contract

The root measures against the logical output. The top bar stays fixed and the
window space consumes the remainder. Modal surfaces overlay the base without
resizing application windows. The default agent action uses end/end placement
with a 16-pixel margin until the user moves it; afterward the compositor's
persisted anchor wins. The HUD opens above the anchor when space allows and
below it otherwise.

The Shell declares default, launcher, full-agent, and expanded-HUD validation
scenarios. The workspace declares Home as its default plus one scenario for
each of the other seven workspaces. Local validation checks all twelve scenes
before activation.

Current evidence is desktop and host-only: both Luau packages validate, the
locked graph resolves, the compositor shortcut and appearance protocols pass,
the login and image packaging tests pass, and faux agent authoring activates a
replacement while retaining the mount. This redesign has not completed a
physical Linux visual, keyboard, hover, drag, font, or multi-window acceptance
gate. Earlier physical results applied to the previous reserving-rail shell and
do not establish this design.

## Stock Mobile

Android continues to boot the separate `sos.stock.mobile` package from
[`experiences/mobile.luau`](../experiences/mobile.luau). It remains phone-native
with a top bar, large touch targets, bottom navigation, full-screen launcher,
scrolling content, and mobile agent surface. It has no desktop window region,
hover HUD, or global launcher chord. Android now rebuilds the complete Stock
semantic palette when toggling light and dark appearance, while preserving
accessibility scale, contrast, and reduced-motion preferences.

## Trust and recovery

Content addressing proves activated bytes but not provenance. The selectable
Linux development session trusts its packaged root and explicitly named
workspace revision for rapid iteration. A release still needs an immutable
system-owned Stock pointer and asymmetric signature verification rooted in
release keys. User-owned state or on-device HMAC material is not that release
trust boundary. Fixed native Recovery remains the fallback when pinned Stock
cannot start.
