--!strict
-- Agent-produced structural revision: the same stable input and state keys are
-- reimagined as an orbital timeline rather than a stack of cards.

local night = 0x10131A
local cream = 0xF5EFE4
local dim = 0x9DA5B4
local violet = 0xA58BFA
local gold = 0xF5BB55

local function text(value, size, color, semantics, id)
    return {
        id = id,
        content = { kind = "text", value = tostring(value), size = size, color = color or cream },
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
        animation = options.animation,
        semantics = options.semantics,
        children = children,
    }
end

local function render(model, state)
    local draft = state.draft
    if draft == nil then draft = "Caffè ☕️ – 明日のデザイン" end
    local playing = state.playing
    if playing == nil then playing = model.music.playing end

    local timeline = {}
    for index, event in ipairs(model.calendar) do
        table.insert(timeline, group("row", { gap = 13, align = "center" }, {
            group("overlay", { width = index * 7 + 8, height = 2 }, nil, {
                background = 0x443B66,
            }),
            text(event.time, 12, gold),
            group("column", { gap = 2, grow = true }, {
                text(event.title, 18, cream), text(event.detail, 12, dim),
            }),
        }))
    end

    return group("column", { scroll_y = true }, {
        group("column", { padding = 22, gap = 20 }, {
            group("row", { justify = "between", align = "center" }, {
                group("column", { gap = 3 }, {
                    text("ORBIT / TODAY", 11, violet),
                    text(model.greeting, 28, cream, { role = "header", label = "Today orbit" }, "today-orbit-header"),
                }),
                text(model.weather.temperature_c .. "°", 35, gold, {
                    role = "status", label = "Weather",
                    value = model.weather.temperature_c .. " degrees, " .. model.weather.summary,
                }, "weather-status"),
            }),
            group("row", { gap = 16, align = "center" }, {
                {
                    id = "album-art",
                    layout = { width = 92, height = 92 },
                    content = { kind = "image", asset = "album-orbit" },
                    paint = {{ kind = "fill_bounds", color = 0x30284B, radius = 46 }},
                    animation = { kind = "pulse", duration_ms = 1200, loop = true },
                    semantics = { role = "image", label = "Pulsing orbit album artwork" },
                },
                group("column", { grow = true, gap = 5 }, {
                    text(model.music.title, 20, cream), text(model.music.artist, 13, dim),
                    group("row", { padding = 10, justify = "center" }, {
                        text(playing and "Ⅱ" or "▶", 17, violet),
                    }, {
                        id = "music-toggle",
                        background = 0x30284B,
                        radius = 18,
                        tap_action = "toggle_music",
                        semantics = {
                            role = "button", label = playing and "Pause music" or "Play music",
                            value = playing and "Playing" or "Paused",
                        },
                    }),
                }),
            }),
            group("column", { gap = 9 }, timeline),
            group("column", { gap = 8, padding = 2 }, {
                text("NOTE IN THE CURRENT ORBIT", 11, violet),
                {
                    id = "note-draft",
                    layout = { height = 42, padding = 8 },
                    content = {
                        kind = "text_session", state_key = "draft", value = draft,
                        placeholder = "Signal a thought…", submit_action = "save_note", autofocus = true,
                    },
                    paint = {{ kind = "fill_bounds", color = 0x252A36, radius = 18 }},
                    semantics = {
                        role = "text_field", label = "Note draft", value = draft,
                        hint = "Edit a note and press enter to save",
                    },
                },
                text(state.last_saved and "Signal archived" or "The signal persists across revisions", 12, dim, {
                    role = "status", label = "Note status",
                    value = state.last_saved and "Saved" or "Editing",
                }, "note-status"),
            }),
        }),
    }, { id = "daily-orbit-root", background = night })
end

local function update(model, state, event)
    if event.action == "text_changed" and event.target == "note-draft" then
        state.draft = event.value
    elseif event.action == "save_note" then
        state.draft = event.value or state.draft
        state.last_saved = true
    elseif event.action == "toggle_music" then
        if state.playing == nil then state.playing = not model.music.playing
        else state.playing = not state.playing end
    elseif event.action == "focus_changed" then
        state.editor_focused = event.focused
    end
    return state
end

return { api_version = 2, render = render, update = update }
