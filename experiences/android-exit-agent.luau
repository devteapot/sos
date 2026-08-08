--!strict
-- A hand-authored spatial day: the spine bends toward appointments as travel
-- time accumulates. One node combines low-level paint and the source-region
-- drag loop without becoming a catalog-specific canvas widget.

local ink = 0x10151D
local paper = 0x171E29
local mist = 0xDCE7E8
local quiet = 0x8B9AA5
local cyan = 0x7FE1D2
local coral = 0xFF9A75
local violet = 0xA995FF
local rail = 0x34495A

local CANVAS_WIDTH = 360
local CANVAS_HEIGHT = 570
local SPINE_X = 176
local EVENT_W = 142
local EVENT_H = 58
local NOTE_W = 142
local NOTE_H = 62

local assets = {
    ["thread-mark"] = {
        kind = "svg",
        data = [[<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96">
          <rect width="96" height="96" rx="24" fill="#20313a"/>
          <path d="M18 66C34 20 61 18 78 54" fill="none" stroke="#7fe1d2" stroke-width="9" stroke-linecap="round"/>
          <circle cx="25" cy="64" r="8" fill="#ff9a75"/><circle cx="74" cy="55" r="8" fill="#a995ff"/>
        </svg>]],
    },
}

local function text(value, size, color, semantics, id)
    return {
        id = id,
        content = { kind = "text", value = tostring(value), size = size, color = color or mist },
        semantics = semantics,
    }
end

local function group(flow, layout, children, options)
    options = options or {}
    layout = layout or {}
    layout.flow = flow
    return {
        id = options.id,
        layout = layout,
        paint = options.background and {{
            kind = "fill_bounds", color = options.background, radius = options.radius or 0,
        }} or nil,
        interaction = options.tap_action and { tap_action = options.tap_action } or nil,
        semantics = options.semantics,
        children = children,
    }
end

local function clamp(value, low, high)
    return math.max(low, math.min(high, value))
end

-- Travel is intentionally part of the generated geometry, rather than a
-- decoration. Later appointments pull the axis farther sideways.
local function travel_for(index)
    return (index - 1) * 34
end

local function event_geometry(index)
    local progress = (index - 1) / 2
    local y = 110 + progress * 190
    local bend = travel_for(index)
    local x = SPINE_X + math.sin(progress * math.pi) * bend
    return { x = x - EVENT_W / 2, y = y, center_x = x, center_y = y + EVENT_H / 2 }
end

local function note_geometry(state)
    local first = state.note_x
    if first == nil then first = 22 end
    local top = state.note_y
    -- Keep the entire drag source inside the canvas hit bounds on the phone.
    if top == nil then top = 330 end
    return {
        x = clamp(first, 8, CANVAS_WIDTH - NOTE_W - 8),
        y = clamp(top, 8, CANVAS_HEIGHT - NOTE_H - 8),
    }
end

local function inside(x, y, rect)
    return x >= rect.x and x <= rect.x + rect.width
        and y >= rect.y and y <= rect.y + rect.height
end

local function target_rect(model)
    for index, calendar_event in ipairs(model.calendar) do
        if calendar_event.title == "Design review" then
            local geometry = event_geometry(index)
            return { x = geometry.x - 10, y = geometry.y - 10,
                width = EVENT_W + 20, height = EVENT_H + 20 }
        end
    end
    return { x = 0, y = 0, width = 0, height = 0 }
end

local function draw_spine(model, state)
    local commands = {
        { kind = "path", color = rail, width = 3, closed = false, points = {
            { x = SPINE_X, y = 28 },
            { x = SPINE_X - 22, y = 94 },
        } },
    }

    local previous = { x = SPINE_X - 22, y = 94 }
    for index, calendar_event in ipairs(model.calendar) do
        local geometry = event_geometry(index)
        local travel = travel_for(index)
        table.insert(commands, {
            kind = "path", color = index == 1 and coral or violet, width = 3,
            closed = false, points = {
                previous,
                { x = geometry.center_x - travel * 0.35, y = geometry.center_y - 28 },
                { x = geometry.center_x, y = geometry.center_y },
            },
        })
        table.insert(commands, {
            kind = "quad", x = geometry.x, y = geometry.y,
            width = EVENT_W, height = EVENT_H, radius = 18,
            color = index == 1 and 0x3E2D35 or 0x273342,
        })
        table.insert(commands, {
            kind = "glyphs", x = geometry.x + 14, y = geometry.y + 15,
            size = 12, line_height = 16, max_width = EVENT_W - 28,
            runs = {
                { text = calendar_event.time .. "  ", color = index == 1 and coral or violet, weight = 650 },
                { text = calendar_event.title, color = mist, weight = 500 },
            },
        })
        previous = { x = geometry.center_x, y = geometry.center_y }
    end

    local note = note_geometry(state)
    table.insert(commands, {
        kind = "path", color = state.dragging and cyan or 0x536975,
        width = 2, closed = false, points = {
            { x = note.x + NOTE_W / 2, y = note.y + NOTE_H / 2 },
            { x = SPINE_X - 20, y = 405 },
            { x = SPINE_X, y = 350 },
        },
    })
    table.insert(commands, {
        kind = "quad", x = note.x, y = note.y, width = NOTE_W, height = NOTE_H,
        radius = 20, color = state.attached and 0x24564F or 0x2B3B4B,
    })

    local regions = {
        {
            id = "note-1", x = note.x - 6, y = note.y - 6,
            width = NOTE_W + 12, height = NOTE_H + 12,
            press_action = "note_press", drag_action = "note_drag",
            drop_action = "note_drop",
        },
    }
    for index, calendar_event in ipairs(model.calendar) do
        local geometry = event_geometry(index)
        table.insert(regions, {
            id = "event-" .. tostring(index), x = geometry.x - 10,
            y = geometry.y - 10, width = EVENT_W + 20, height = EVENT_H + 20,
            tap_action = "event_tap", double_tap_action = "event_zoom",
            long_press_action = "event_pin", swipe_action = "event_swipe",
        })
    end

    return commands, regions
end

local function render(model, state)
    local playing = state.playing
    if playing == nil then playing = model.music.playing end
    local commands, regions = draw_spine(model, state)
    local design = target_rect(model)
    local attached_label = state.attached and "Attached to Design review" or "Drag the first note onto Design review"
    if state.last_gesture ~= nil then attached_label = state.last_gesture end

    table.insert(commands, 1, { kind = "fill_bounds", color = paper, radius = 0 })
    table.insert(commands, {
        kind = "layer",
        clip = { x = 12, y = 12, width = CANVAS_WIDTH - 24, height = 66 },
        transform = {
            translate_x = 5, translate_y = 1, scale_x = 0.98, scale_y = 0.98,
            rotation_degrees = -1.5,
        },
        opacity = 0.92,
        paint = {
            { kind = "quad", x = 16, y = 18, width = 160, height = 44, radius = 18, color = 0x22343F },
            { kind = "glyphs", x = 30, y = 31, size = 12, line_height = 16, max_width = 130, runs = {
                { text = "LIVE SCENE ABI", color = cyan, weight = 700 },
            } },
        },
    })
    local flow = {
        id = "bent-time-field",
        layout = {
            width = CANVAS_WIDTH, height = CANVAS_HEIGHT,
            min_width = 320, max_width = CANVAS_WIDTH,
            aspect_ratio = CANVAS_WIDTH / CANVAS_HEIGHT, clip_bounds = true,
        },
        paint = commands,
        interaction = { hit_regions = regions },
        semantics = {
            role = "image", label = "Bent time flow with travel-shaped event path",
        },
        children = {
            {
                id = "layout-badge",
                layout = {
                    position = { x = 306, y = 20 }, width = 38, height = 22,
                    padding = 4, clip_bounds = true,
                },
                paint = {{ kind = "fill_bounds", color = 0x334B58, radius = 8 }},
                content = { kind = "text", value = "ABI", size = 9, color = cyan },
            },
            {
                id = "revision-asset-badge",
                layout = {
                    position = { x = 306, y = 50 }, width = 38, height = 38,
                    min_width = 30, max_width = 44, aspect_ratio = 1, clip_bounds = true,
                    program = { measure_width = 0.1, arrange_x = 0.85 },
                },
                content = { kind = "image", asset = "thread-mark" },
                interaction = {
                    pointer_action = "asset_pointer",
                    multi_pointer_action = "asset_transform",
                    capture = "surface",
                },
                semantics = { role = "image", label = "Revision asset gesture target" },
            },
        },
    }

    local children = {
        group("column", { padding = 22, gap = 5 }, {
            text("WHAT IS NEXT", 11, cyan),
            text(model.calendar[1] and model.calendar[1].title or "Open time", 31, mist, {
                role = "header", label = "What is next",
            }, "next-header"),
            text(model.calendar[1] and model.calendar[1].detail or "No appointment ahead", 13, quiet),
        }),
        flow,
        group("column", { padding = 22, gap = 5 }, {
            text("INTERFACE THOUGHT", 10, violet),
            text(model.notes[1] and model.notes[1].title or "First note", 17, mist),
            text(model.notes[1] and model.notes[1].preview or "", 12, quiet),
            text(attached_label, 12, state.attached and cyan or coral, {
                role = "status", label = "Note attachment", value = attached_label,
            }, "attachment-status"),
        }),
    }

    if playing then
        table.insert(children, group("row", { padding = 15, gap = 12, align = "center" }, {
                {
                    id = "playing-art",
                    layout = { width = 42, height = 42 },
                    content = { kind = "image", asset = "thread-mark" },
                    paint = {{ kind = "fill_bounds", color = 0x263D42, radius = 21 }},
                },
                group("column", { gap = 2, grow = true }, {
                    text(model.music.title, 15, mist), text(model.music.artist, 12, quiet),
                }),
                text("Ⅱ", 20, cyan),
            }, {
                id = "music-playing",
                tap_action = "toggle_music",
                background = 0x263D42,
                radius = 24,
                semantics = { role = "button", label = "Pause music", value = "Playing" },
            }))
    end

    table.insert(children, group("column", { padding = 16, gap = 7 }, {
        text("COMPOSING NOTE", 10, violet),
        {
            id = "note-draft",
            layout = { padding = 12, min_height = 48 },
            content = {
                kind = "text_session", state_key = "draft",
                value = state.draft or "", placeholder = "Write with your keyboard…",
                submit_action = "save_draft",
            },
            paint = {{ kind = "fill_bounds", color = 0x263D42, radius = 14 }},
            semantics = {
                role = "text_field", label = "Composing note",
                hint = "Supports selection and marked text",
            },
        },
    }))

    return group("column", {
        scroll_y = true,
        program = { measure_width = 1, measure_height = 1 },
    }, children, {
        id = "android-exit-spatial-root", background = ink,
    })
end

local function update(model, state, event)
    if event.action == "toggle_music" then
        if state.playing == nil then state.playing = false else state.playing = not state.playing end
    elseif event.action == "note_press" then
        state.dragging = true
        state.drop_valid = false
    elseif event.action == "note_drag" and state.dragging then
        state.note_x = (event.x or 0) - NOTE_W / 2
        state.note_y = (event.y or 0) - NOTE_H / 2
        local destination = target_rect(model)
        state.drop_valid = inside(event.x or 0, event.y or 0, destination)
    elseif event.action == "note_drop" and state.dragging then
        local destination = target_rect(model)
        local valid = inside(event.x or 0, event.y or 0, destination)
        state.dragging = false
        state.drop_valid = valid
        if valid and not state.attached then
            state.attached = true
            state.attached_note_id = "note-1"
            state.attached_event_title = "Design review"
            return {
                state = state,
                effects = {{
                    provider = "notes", action = "attach_to_event",
                    payload = { note_id = "note-1", event_title = "Design review" },
                }},
            }
        end
    elseif event.action == "event_tap" then
        state.last_gesture = "Selected " .. tostring(event.target)
    elseif event.action == "event_zoom" then
        state.last_gesture = "Double tap on " .. tostring(event.target)
    elseif event.action == "event_pin" then
        state.last_gesture = "Long press pinned " .. tostring(event.target)
    elseif event.action == "event_swipe" then
        local direction = (event.delta_x or 0) >= 0 and "right" or "left"
        state.last_gesture = "Swiped " .. direction .. " on " .. tostring(event.target)
    elseif event.action == "asset_pointer" then
        state.last_gesture = string.format(
            "Pointer %s · id %s · %d active",
            event.phase or "move", tostring(event.pointer_id or "?"), event.pointer_count or 0)
    elseif event.action == "asset_transform" then
        state.last_gesture = string.format(
            "Transform %.2fx · %.0f°",
            event.scale or 1, event.rotation_degrees or 0)
    elseif event.action == "save_draft" then
        state.last_gesture = "Composing note saved"
    end
    return state
end

return { api_version = 3, assets = assets, render = render, update = update }
