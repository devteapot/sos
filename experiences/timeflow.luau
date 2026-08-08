--!strict
-- A source-only alternative: no cards, one continuous temporal spine, and
-- music controls rendered only while playback is active.

local night = 0x101916
local cream = 0xF3F0E4
local sage = 0xA7C7AE
local dim = 0x9EAAA3
local amber = 0xF0A83B

local function text(value, size, color)
    return { content = { kind = "text", value = tostring(value), size = size, color = color or cream } }
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

local function timeline_item(time, title, detail, last)
    return group("row", { gap = 16 }, {
        group("column", { width = 58, align = "center" }, {
            text(time, 13, amber),
            group("overlay", { width = 2, height = last and 22 or 86 }, nil, 0x426052),
        }),
        group("column", { gap = 4, padding = 2, grow = true }, {
            text(title, 20, cream), text(detail, 13, dim),
        }),
    })
end

local function render(model, state)
    local playing = state.playing
    if playing == nil then playing = model.music.playing end

    local children = {
        group("row", { align = "end", justify = "between" }, {
            group("column", { gap = 3 }, {
                text(model.greeting, 25, cream), text(model.date, 13, dim),
            }),
            group("column", { align = "end" }, {
                text(model.weather.temperature_c .. "°", 34, sage), text(model.weather.summary, 11, dim),
            }),
        }),
    }

    if playing then
        table.insert(children, group("row", { gap = 11, padding = 13, align = "center" }, {
            text("Ⅱ", 22, amber),
            group("column", { gap = 1, grow = true }, {
                text(model.music.title, 16, cream), text(model.music.artist, 12, dim),
            }),
            text("Tap to pause", 11, sage),
        }, 0x213129, 14, "music-playing", "toggle_music"))
    end

    table.insert(children, text("YOUR DAY, AS A FLOW", 11, sage))
    for index, event in ipairs(model.calendar) do
        table.insert(children, timeline_item(event.time, event.title, event.detail, index == #model.calendar))
    end
    table.insert(children, text("NOTES ALONG THE WAY", 11, sage))
    for _, note in ipairs(model.notes) do
        table.insert(children, group("column", { gap = 3, padding = 4 }, {
            text(note.title, 17, cream), text(note.preview, 13, dim),
        }))
    end

    return group("column", { scroll_y = true }, {
        group("column", { padding = 22, gap = 18 }, children),
    }, night, 0, "timeflow-root")
end

local function update(_, state, event)
    if event.action == "toggle_music" then state.playing = false end
    return state
end

return { api_version = 2, render = render, update = update }
