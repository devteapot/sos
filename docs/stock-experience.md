# Stock experience

Date: 2026-08-26

Stock Base is the product's substantial default experience and the integration
target for the System Providers ABI. It is not a privileged host view or a
catalog of native widgets. The shipped implementation is the ordinary Scene
ABI v3 module in [`experiences/default.luau`](../experiences/default.luau), and
the permanent host compiles, validates, installs, and activates it through the
same content-addressed revision path used by agent-authored experiences.

## Product surface

The initial stock revision contains eight source-defined workspaces:

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

Every workspace renders an explicit empty or unavailable state when its
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

The mark is currently an inline immutable SVG declaration and therefore enters
the same revision asset set and validation limits as agent content. Sidecar
images, fonts, and shaders can be added through the normal revision manifest;
there is no stock-only asset path.

## Responsive contract

Rows that contain navigation, cards, controls, notes, applications, or agent
actions opt into retained flex wrapping. The readable content frame is bounded
to 1,876 logical pixels while measuring relative to its parent, so the current
Framework development compositor does not stretch one experience across its
side-by-side PiKVM and internal-panel output space. Fixed card widths provide
natural clamshell and tablet reflow without branching on a device name or
calling back into Luau during host layout.

This closes the retained-layout implementation and physical 1,920-by-1,080
clamshell gate. It does not close a physical portrait/tablet gate. The current
direct compositor still exposes multiple outputs as one global scene rather
than one workspace per output; per-output surfaces or a single-output nested
portrait campaign remain separate host acceptance work.

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
