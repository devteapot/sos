--!strict
-- The source is interpreted on-device. Rust owns data, validation, rendering,
-- persistence and rollback; this module owns composition and local UI behavior.

local ink = 0x17211B
local muted = 0x637069
local paper = 0xF3F1E8
local white = 0xFEFDF8
local moss = 0x3E6B52
local mint = 0xDCE8D8
local sun = 0xE9A23B

local function text(value, size, color)
    return { content = { kind = "text", value = tostring(value), size = size, color = color or ink } }
end

local function group(flow, layout, children, background, radius, id, tap_action)
    layout = layout or {}
    layout.flow = flow
    return {
        id = id,
        layout = layout,
        paint = background and {{ kind = "fill_bounds", color = background, radius = radius or 0 }} or nil,
        interaction = tap_action and { tap_action = tap_action } or nil,
        children = children,
    }
end

local function event_row(event)
    return group("row", { gap = 16, padding = 14 }, {
        text(event.time, 14, moss),
        group("column", { gap = 3, grow = true }, {
            text(event.title, 17, ink),
            text(event.detail, 13, muted),
        }),
    }, white, 18)
end

local function render(model, state)
    local playing = state.playing
    if playing == nil then playing = model.music.playing end

    local agenda = {}
    for _, event in ipairs(model.calendar) do table.insert(agenda, event_row(event)) end

    local notes = {}
    for _, note in ipairs(model.notes) do
        table.insert(notes, group("column", { gap = 4, padding = 14 }, {
            text(note.title, 16, ink), text(note.preview, 13, muted),
        }, 0xECE9DC, 16))
    end

    local sections = {
        group("column", { gap = 4 }, {
            text(model.greeting, 27, ink), text(model.date, 14, muted),
        }),
        group("row", { padding = 18, gap = 14, align = "center" }, {
            text(tostring(model.weather.temperature_c) .. "°", 42, moss),
            group("column", { gap = 3 }, {
                text(model.weather.summary, 16, ink),
                text("High " .. model.weather.high_c .. "°  ·  Low " .. model.weather.low_c .. "°", 13, muted),
            }),
        }, mint, 22),
        text("TODAY", 12, moss),
        group("column", { gap = 9 }, agenda),
        text("NOTES", 12, moss),
        group("column", { gap = 9 }, notes),
    }

    if playing then
        table.insert(sections, group("row", { padding = 16, gap = 12, align = "center" }, {
            group("overlay", { width = 42, height = 42 }, nil, sun, 21),
            group("column", { gap = 2, grow = true }, {
                text(model.music.title, 16, white), text(model.music.artist, 13, 0xB8C1BA),
            }),
            text("PAUSE", 11, white),
        }, ink, 22, "music-playing", "toggle_music"))
    else
        table.insert(sections, group("row", { padding = 13, justify = "center" }, {
            text("Resume music", 14, ink),
        }, 0xE5E2D6, 18, "music-paused", "toggle_music"))
    end

    return group("column", { scroll_y = true }, {
        group("column", { padding = 22, gap = 18 }, sections),
    }, paper, 0, "experience-root")
end

local function update(_, state, event)
    if event.action == "toggle_music" then
        if state.playing == nil then state.playing = false else state.playing = not state.playing end
    end
    return state
end

return { api_version = 2, render = render, update = update }
