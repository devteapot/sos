# Luau ↔ host Scene ABI (prototype)

This is the contract available to an agent-authored Luau revision. Luau is the
long-lived experience language; Rust/GPUI is the permanent execution substrate.
An experience revision contains source, state migrations, and eventually typed
assets, but never a native executable.

The prototype deliberately broke the original `UiNode` catalog. API version 3
uses orthogonal scene facets so an agent can combine layout, content, paint,
interaction, animation, and semantics on the same retained node. There is no
compatibility decoder for the catalog ABI or Scene ABI v2. Host upgrades remain
separate system updates while the contract is intentionally fluid.

## Module contract

Every module declares the exact scene API it emits:

```luau
return {
    api_version = 3,
    state_version = 1, -- optional; defaults to 1
    assets = { -- optional immutable, revision-scoped assets
        mark = { kind = "svg", data = "<svg ...>...</svg>" },
    },
    render = function(model, state): SceneNode ... end,
    update = function(model, state, event): state | UpdateEnvelope ... end, -- optional
    migrate = function(from_version, state): state ... end, -- required for a state-version change
}
```

`model` contains `greeting`, `date`, `weather`, `calendar`, `notes`, `music`,
`system`, and `surfaces` values. Android and unconfigured development sessions
use the deterministic fixture provider. A configured Linux host replaces the
resource domains with file/iCalendar/MPRIS-backed data and capability-scoped
system/media snapshots, then pushes live changes into the accepted VM without
installing a revision. `system` includes time/timezone, online interfaces,
battery/AC, audio volume/mute, connected DRM displays, and input devices.
`state` is JSON-like durable experience state.
`render` returns the root scene node. `update` may mutate and return state, or
return a typed effect envelope:

```luau
return {
    state = state,
    effects = {{
        provider = "notes",
        action = "attach_to_event",
        payload = { note_id = "note-1", event_title = "Design review" },
    }},
}
```

The effect allowlist is:

- `notes.attach_to_event(note_id, event_title)` for the durable prototype
  authority;
- `notes.write(name, content)` for an atomically replaced Markdown note;
- `calendar.append(name, time, title, detail)` for an iCalendar event;
- `music.command(command)` where command is `play-pause`, `next`, or
  `previous` through MPRIS/playerctl.

Linux loads a private capability manifest for the candidate revision before it
is allowed to render. A provider effect is validated and staged with the state
promotion, executed only through the trusted adapter, and rejected on missing
grant, cancellation, invalid path/payload, or temporary provider failure.
Luau never receives provider credentials or a provider object.

## Scene nodes

A node has no `type`. It is a table made from independent facets:

```luau
{
    id = "invented-control", -- required for interaction, animation, and text sessions
    layout = { ... },
    content = { ... },
    paint = { ... },
    interaction = { ... },
    animation = { ... },
    semantics = { ... },
    children = { ... },
}
```

This removes distinctions such as “box versus row versus canvas.” A node can,
for example, arrange children in a row, draw generated paths behind them,
define custom hit regions, expose button semantics, and animate as one object.
Source-local Luau helpers may build conventions such as stacks or buttons;
those helpers are not permanent host components.

### Layout

```luau
layout = {
    flow = "overlay" | "column" | "row", -- defaults to overlay
    scroll_y = true,
    padding = 16,
    gap = 8,
    width = 320,
    height = 180,
    min_width = 240,
    min_height = 120,
    max_width = 480,
    max_height = 320,
    aspect_ratio = 1.777,
    position = { x = 24, y = 40 }, -- retained absolute placement
    program = { -- fractions of the containing block, executed by the host
        measure_width = 0.75,
        measure_height = 0.4,
        arrange_x = 0.125,
        arrange_y = 0.1,
    },
    clip_bounds = true,
    grow = true,
    align = "start" | "center" | "end",
    justify = "start" | "center" | "end" | "between",
}
```

Layout is host-owned and runs in GPUI. Luau supplies retained constraints and
placements; it is not called once per primitive during a frame. `position`
removes a node from its parent's flow while preserving a host-owned retained
element. `program` is a bounded responsive measure/arrange program: finite
fractions in `[-4, 4]` are retained and evaluated by GPUI/Taffy against the
current containing block. It composes with min/max and aspect constraints
without a high-frequency Luau callback.

### Content

Each node currently carries at most one content payload:

```luau
content = { kind = "text", value = "Now", size = 18, color = 0xFFFFFF }

content = { kind = "image", asset = "mark" } -- declared in module assets

content = {
    kind = "text_session",
    state_key = "draft",
    value = state.draft or "",
    placeholder = "Write a note…",
    submit_action = "save_note",
    autofocus = true,
}

content = { kind = "provider_surface", surface = "camera-preview" }
```

`text_session` is a host-owned editing session and requires a stable node ID.
`album-orbit` remains a built-in test asset. A module may also declare bounded
inline SVG assets. The runtime validates them, rejects scripts,
external references, doctypes/entities, and foreign objects, hashes their
bytes, and exposes only a content-addressed host path after candidate commit.
Supervisor manifest format 3 additionally packages `svg`, `png`, `jpeg`,
`webp`, `font`, and validated WGSL `shader` sidecars with stable IDs and
individual byte-length/SHA-256 identities. The runtime re-verifies them and
admits them to the same candidate asset set; images enter the host asset source
and fonts enter GPUI's text system. A glyph run selects a loaded font with
`font_family`. A `shader` paint resolves a declared shader ID, executes its
resource-free `vs_main`/`fs_main` entry points into a host-capped (maximum
1024 by 1024) RGBA target, and composites that target into the scene. Naga
validation rejects bindings, compute entry points, malformed modules, and
missing entry points at install and activation. Old revision assets are removed
from the active registry. Arbitrary paths and URLs remain rejected.

`provider_surface` resolves only a surface declared in the current provider
snapshot. On Linux, a `ready` video/camera surface maps a provider-owned,
signature-checked PNG/JPEG/WebP frame through a content-addressed host asset;
an atomic provider update changes that path and rerenders without revision
activation. Read grants are separate for video and camera. A protected surface
also requires its explicit grant but reports `protected_unavailable` and never
maps bytes because the prototype does not claim a secure scanout path. The
Android host renders an explicit unavailable placeholder for this Linux
integration primitive.

### Paint and interaction

Paint and hit testing are facets of any node, not a `canvas` escape-hatch type:

```luau
{
    id = "temporal-field",
    layout = { width = 360, height = 520 },
    paint = {
        { kind = "fill_bounds", color = 0x171E29, radius = 20 },
        { kind = "path", color = 0x7DA6FF, width = 4, closed = false,
          points = {{x=24,y=20}, {x=80,y=170}, {x=42,y=420}} },
        { kind = "quad", x = 32, y = 330, width = 130, height = 54,
          radius = 14, color = 0x25314A },
        { kind = "glyphs", x = 46, y = 344, size = 14, line_height = 18,
          max_width = 100, runs = {
              { text = "13:00 ", color = 0xA995FF, weight = 700 },
              { text = "Lunch", color = 0xFFFFFF, weight = 400 },
          } },
        { kind = "shader", asset = "aurora", x = 24, y = 220,
          width = 256, height = 96 },
        { kind = "layer", opacity = 0.8,
          clip = { x = 8, y = 8, width = 180, height = 70 },
          transform = { translate_x = 4, translate_y = 2,
              scale_x = 0.98, scale_y = 0.98, rotation_degrees = -2 },
          paint = {
              { kind = "quad", x = 12, y = 12, width = 150, height = 44,
                radius = 16, color = 0x22343F },
          } },
    },
    interaction = {
        tap_action = "select_flow", -- optional whole-node action
        pointer_action = "pointer_sample",
        multi_pointer_action = "transform_gesture",
        capture = "none" | "pointer" | "surface",
        hit_regions = {{
            id = "note-1", x = 32, y = 330, width = 130, height = 54,
            press_action = "note_press",
            drag_action = "note_drag",
            drop_action = "note_drop",
            tap_action = "note_open",
            double_tap_action = "note_zoom",
            long_press_action = "note_pin",
            swipe_action = "note_archive",
        }},
    },
}
```

Coordinates are node-local logical pixels. Paths are filled when `width` is
omitted and stroked otherwise. Layers recursively compose bounded paint with a
rectangular clip, affine transform, and opacity; GPUI shapes glyph runs in the
host rather than in Luau. Compatibility gesture events carry `action`,
`target`, coordinates, deltas, velocities, and `phase = "start" | "update" |
"end"`. `pointer_action` exposes the Android pointer stream before GPUI maps it
to mouse/scroll: `phase = "down" | "move" | "up" | "cancel"` plus
`pointer_id`, `pointer_count`, and pressure. `multi_pointer_action` adds a
host-derived centroid, scale, and rotation for the first two captured pointers.
`pointer` capture follows one pointer outside the node; `surface` also assigns
subsequent pointers to that surface. The revision owns geometry and gesture
meaning while the host owns bounded routing and capture lifetime.

For the SM-A336B audit, keep a low-level paint node's complete initial draggable
region at local `y <= 400`. This is a measured viewport constraint, not a
permanent layout rule.

### Animation and semantics

```luau
animation = { kind = "pulse" | "fade_in", duration_ms = 1200, loop = true }

semantics = {
    role = "button" | "image" | "text_field" | "header" | "status" | "scroll_area",
    label = "Pause music",
    value = "Playing", -- optional
    hint = "Double tap to change playback", -- optional
}
```

Every semantic node requires a stable `id`. The host flattens these facets into
a platform-neutral semantic tree containing role, label, value, hint, bounds,
hierarchy, editability, and actions. Android adapts that tree to real virtual
`AccessibilityNodeInfo` descendants with accessibility focus, click, editable
focus, set-text, UTF-16 selection, copy/cut/paste, and forward/backward scroll
actions. A `scroll_y` node automatically publishes its offset, range, viewport,
and moving descendant bounds to TalkBack.

Linux publishes the same tree through the SOS-owned mode-0600 Unix semantic
service selected by `SOS_ACCESSIBILITY_SOCKET`. Its bounded newline-JSON API
supports snapshots/waits, hierarchy traversal and semantic focus, activation,
scrolling, editable value and UTF-16 selection, and copy/cut/paste. It is also
the automation-facing semantic API and reconnects after host crash recovery.

`text_session` uses a host-owned Android `InputConnection`. Commit,
set-composing-text/region, finish-composition, deletion, selection, printable
key events, and editor submission carry the complete text, UTF-16 selection,
and marked range into the keyed GPUI editor. JNI updates wake a host frame, and
the containing scroll area receives the IME inset so the focused field can be
revealed. Linux keeps this semantic/editing contract and supplies its own
Wayland input-method-v2 client and semantic service instead of Android's
TalkBack and input-method services.

## Validation and authority boundary

Before presentation the host enforces, among other checks:

- exact module `api_version = 3`;
- 2,048 scene nodes, depth 32, and 256 children per node;
- 4,096 recursively counted paint operations, depth 16, 8,192 path points,
  256 glyph runs, and 256 hit regions per node;
- bounded text, coordinates, dimensions, animation durations, state, effects,
  and effect payloads;
- unique IDs and stable IDs for interactive, animated, semantic, and
  text-session nodes;
- a 16 MiB VM limit and fixed render/update time budgets.
- at most 64 revision assets, 4 MiB each and 16 MiB total, with checks repeated
  by the supervisor and runtime.

Luau receives model/state values and emits scene/effect values. It never
receives a GPUI context, raw pointer, filesystem, network socket, provider
object, or platform handle. Those exclusions are the authority split and stay
even as scene expressiveness grows.

## Next integration work

Version 3 includes bounded clips/transforms/layers, host-shaped glyph runs and
revision fonts, retained responsive layout programs, raw multi-pointer routing
with capture policy, accessible scrolling/selection, complete Android marked
text transport, and supervisor-packaged sidecars. Further depth should focus on
richer path/clip primitives, declarative animation timelines, more than the
current two-pointer transform recognizer, platform conformance across IME and
accessibility implementations, and richer time-varying shader inputs beyond
the current deterministic, resource-free shader target.

The key execution rule is retained: Luau builds or updates bounded structures;
Rust/GPUI performs frame-critical layout, paint, animation, text, and input.
Avoid a high-frequency cross-language callback for every draw operation.

## Agent test request

The Android-exit demonstration uses this single-shot request:

> Center the experience on what is next and show music only while playing.
> Remove cards and invent a spatial flow in which time bends toward events
> with travel. Let me drag the first note onto the Design review appointment;
> calculate the geometry and hit regions in Luau, show attached state, and emit
> the typed `notes.attach_to_event` provider action on a valid drop.

The agent implements this in source. “Bent time flow” and drag/drop are not host
components.
