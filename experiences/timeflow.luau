--!strict
-- A source-only alternative: no cards, one continuous temporal spine, and
-- music controls rendered only while playback is active.

local night = 0x101916
local cream = 0xF3F0E4
local sage = 0xA7C7AE
local dim = 0x9EAAA3
local amber = 0xF0A83B

local function text(value, size, color)
    return { type = "text", text = tostring(value), style = { text_size = size, color = color or cream } }
end

local function timeline_item(time, title, detail, last)
    return {
        type = "row", style = { gap = 16 }, children = {
            { type = "column", style = { width = 58, align = "center" }, children = {
                text(time, 13, amber),
                { type = "box", style = { width = 2, height = last and 22 or 86, background = 0x426052 } },
            } },
            { type = "column", style = { gap = 4, padding = 2, grow = true }, children = {
                text(title, 20, cream), text(detail, 13, dim),
            } },
        }
    }
end

local function render(model, state)
    local playing = state.playing
    if playing == nil then playing = model.music.playing end

    local children = {
        { type = "row", style = { align = "end", justify = "between" }, children = {
            { type = "column", style = { gap = 3 }, children = {
                text(model.greeting, 25, cream), text(model.date, 13, dim),
            } },
            { type = "column", style = { align = "end" }, children = {
                text(model.weather.temperature_c .. "°", 34, sage), text(model.weather.summary, 11, dim),
            } },
        } },
    }

    if playing then
        table.insert(children, {
            type = "row", action = "toggle_music", style = {
                gap = 11, padding = 13, align = "center", background = 0x213129, radius = 14,
            }, children = {
                text("Ⅱ", 22, amber),
                { type = "column", style = { gap = 1, grow = true }, children = {
                    text(model.music.title, 16, cream), text(model.music.artist, 12, dim),
                } },
                text("Tap to pause", 11, sage),
            }
        })
    end

    table.insert(children, text("YOUR DAY, AS A FLOW", 11, sage))
    for index, event in ipairs(model.calendar) do
        table.insert(children, timeline_item(event.time, event.title, event.detail, index == #model.calendar))
    end
    table.insert(children, text("NOTES ALONG THE WAY", 11, sage))
    for _, note in ipairs(model.notes) do
        table.insert(children, {
            type = "column", style = { gap = 3, padding = 4 }, children = {
                text(note.title, 17, cream), text(note.preview, 13, dim),
            }
        })
    end

    return {
        type = "scroll", id = "timeflow-root", style = { background = night }, children = {
            { type = "column", style = { padding = 22, gap = 18 }, children = children }
        }
    }
end

local function update(_, state, event)
    if event.action == "toggle_music" then state.playing = false end
    return state
end

return { render = render, update = update }
