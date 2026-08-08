# Generated experience API (prototype)

This is the contract available to an agent-authored Luau revision. It is a
temporary low-level execution API for the Android research gate, not the final
SOS component catalog.

## Module contract

An experience returns a module with:

```luau
return {
    render = function(model, state): UiNode ... end,
    update = function(model, state, event): state | UpdateEnvelope ... end, -- optional
}
```

`model` contains synthetic `greeting`, `date`, `weather`, `calendar`, `notes`,
and `music` values. `state` is JSON-like durable experience state. Mutate and
return it, or return an effect envelope:

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

The only provider action currently allowed is
`notes.attach_to_event(note_id, event_title)`. Provider failure rejects the
whole UI state transition.

## Nodes

Every node is a table with `type`; interactive/native nodes require a stable
unique `id`. Containers use `children`. A normal node may have `action`,
`style`, `animation`, and `accessibility`.

Accessibility is optional. When present it requires `label` and a `role` from
exactly `button`, `image`, `text_field`, `header`, or `status`; arbitrary roles
such as `text` are rejected. Plain text nodes usually need no accessibility
table because their content is already included in the native summary.

Available types are `box`, `column`, `row`, `scroll`, `text`, `text_input`,
`image`, `canvas`, and `spacer`. Text uses `text`. The sole prototype image
asset is `album-orbit`. See the checked-in experiences for conventional node
examples.

Styles accept numeric RGB `background`/`color`, dimensions `padding`, `gap`,
`radius`, `text_size`, `width`, `height`, boolean `grow`, `align` of
`start|center|end`, and `justify` of `start|center|end|between`.

## Low-level canvas

`canvas` is the escape hatch used to invent geometry rather than select a
predefined component:

```luau
{
    type = "canvas", id = "temporal-field",
    style = { width = 360, height = 520 },
    commands = {
        { kind = "path", color = 0x7DA6FF, width = 4, closed = false,
          points = {{x=24,y=20}, {x=80,y=170}, {x=42,y=420}} },
        { kind = "quad", x = 32, y = 330, width = 130, height = 54,
          radius = 14, color = 0x25314A },
    },
    hit_regions = {{
        id = "note-1", x = 32, y = 330, width = 130, height = 54,
        press_action = "note_press",
        drag_action = "note_drag",
        drop_action = "note_drop",
    }},
}
```

Canvas coordinates are local logical pixels. Paths are filled when `width` is
omitted and stroked otherwise. A canvas event contains `action`, `target`, `x`,
and `y`. The revision owns its geometry, hit regions, drag state, and drop
semantics. The host only paints bounded commands and routes coordinates.

For the SM-A336B audit, use an explicit canvas width and height and keep the
complete initial draggable region at local `y <= 400`. Content below that may
be visually present but off the initially reachable interaction area. This is
a current viewport constraint, not a permanent SOS layout rule.

Limits are enforced before presentation: 2,048 tree nodes, depth 32, 4,096
canvas commands, 8,192 path points, 256 hit regions, 16 effects, 16 KiB effect
payloads, a 16 MiB VM cap, and per-call time budgets.

## Agent test request

The Android-exit demonstration uses this single-shot request:

> Center the experience on what is next and show music only while playing.
> Remove cards and invent a spatial flow in which time bends toward events
> with travel. Let me drag the first note onto the Design review appointment;
> calculate the geometry and hit regions in Luau, show attached state, and emit
> the typed `notes.attach_to_event` provider action on a valid drop.

The agent must implement this in source; “bent time flow” and drag/drop are not
host components.
