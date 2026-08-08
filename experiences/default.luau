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
    return { type = "text", text = tostring(value), style = { text_size = size, color = color or ink } }
end

local function event_row(event)
    return {
        type = "row", style = { gap = 16, padding = 14, background = white, radius = 18 }, children = {
            text(event.time, 14, moss),
            { type = "column", style = { gap = 3, grow = true }, children = {
                text(event.title, 17, ink),
                text(event.detail, 13, muted),
            } },
        }
    }
end

local function render(model, state)
    local playing = state.playing
    if playing == nil then playing = model.music.playing end

    local agenda = {}
    for _, event in ipairs(model.calendar) do table.insert(agenda, event_row(event)) end

    local notes = {}
    for _, note in ipairs(model.notes) do
        table.insert(notes, {
            type = "column", style = { gap = 4, padding = 14, background = 0xECE9DC, radius = 16 }, children = {
                text(note.title, 16, ink), text(note.preview, 13, muted),
            }
        })
    end

    local sections = {
        { type = "column", style = { gap = 4 }, children = {
            text(model.greeting, 27, ink), text(model.date, 14, muted),
        } },
        { type = "row", style = { padding = 18, gap = 14, background = mint, radius = 22, align = "center" }, children = {
            text(tostring(model.weather.temperature_c) .. "°", 42, moss),
            { type = "column", style = { gap = 3 }, children = {
                text(model.weather.summary, 16, ink),
                text("High " .. model.weather.high_c .. "°  ·  Low " .. model.weather.low_c .. "°", 13, muted),
            } },
        } },
        text("TODAY", 12, moss),
        { type = "column", style = { gap = 9 }, children = agenda },
        text("NOTES", 12, moss),
        { type = "column", style = { gap = 9 }, children = notes },
    }

    if playing then
        table.insert(sections, {
            type = "row", action = "toggle_music", style = {
                padding = 16, gap = 12, background = ink, radius = 22, align = "center",
            }, children = {
                { type = "box", style = { width = 42, height = 42, background = sun, radius = 21 } },
                { type = "column", style = { gap = 2, grow = true }, children = {
                    text(model.music.title, 16, white), text(model.music.artist, 13, 0xB8C1BA),
                } },
                text("PAUSE", 11, white),
            }
        })
    else
        table.insert(sections, {
            type = "row", action = "toggle_music", style = {
                padding = 13, background = 0xE5E2D6, radius = 18, justify = "center",
            }, children = { text("Resume music", 14, ink) }
        })
    end

    return {
        type = "scroll", id = "experience-root", style = { background = paper }, children = {
            { type = "column", style = { padding = 22, gap = 18 }, children = sections }
        }
    }
end

local function update(_, state, event)
    if event.action == "toggle_music" then
        if state.playing == nil then state.playing = false else state.playing = not state.playing end
    end
    return state
end

return { render = render, update = update }
