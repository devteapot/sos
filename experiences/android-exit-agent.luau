--!strict
-- A hand-authored spatial day: the spine bends toward appointments as travel
-- time accumulates. Canvas owns the geometry and the source-region drag loop.

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

local function text(value, size, color, accessibility)
    return {
        type = "text",
        text = tostring(value),
        style = { text_size = size, color = color or mist },
        accessibility = accessibility,
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

    local flow = {
        type = "canvas", id = "bent-time-field",
        style = { width = CANVAS_WIDTH, height = CANVAS_HEIGHT, background = paper },
        commands = commands, hit_regions = regions,
        accessibility = {
            role = "image", label = "Bent time flow with travel-shaped event path",
        },
    }

    local children = {
        { type = "column", style = { padding = 22, gap = 5 }, children = {
            text("WHAT IS NEXT", 11, cyan),
            text(model.calendar[1] and model.calendar[1].title or "Open time", 31, mist, {
                role = "header", label = "What is next",
            }),
            text(model.calendar[1] and model.calendar[1].detail or "No appointment ahead", 13, quiet),
        } },
        flow,
        { type = "column", style = { padding = 22, gap = 5 }, children = {
            text("INTERFACE THOUGHT", 10, violet),
            text(model.notes[1] and model.notes[1].title or "First note", 17, mist),
            text(model.notes[1] and model.notes[1].preview or "", 12, quiet),
            text(attached_label, 12, state.attached and cyan or coral, {
                role = "status", label = "Note attachment", value = attached_label,
            }),
        } },
    }

    if playing then
        table.insert(children, {
            type = "row", id = "music-playing", action = "toggle_music",
            style = { padding = 15, gap = 12, background = 0x263D42, radius = 24, align = "center" },
            accessibility = { role = "button", label = "Pause music", value = "Playing" },
            children = {
                { type = "image", id = "playing-art", asset = "album-orbit",
                    style = { width = 42, height = 42, radius = 21 } },
                { type = "column", style = { gap = 2, grow = true }, children = {
                    text(model.music.title, 15, mist), text(model.music.artist, 12, quiet),
                } },
                text("Ⅱ", 20, cyan),
            },
        })
    end

    return {
        type = "scroll", id = "android-exit-spatial-root", style = { background = ink },
        children = children,
    }
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
    end
    return state
end

return { render = render, update = update }
