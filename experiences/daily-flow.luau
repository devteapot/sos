--!strict
-- Stateful primitive gate: native IME input, image, animation, focus, and
-- accessibility semantics remain expressed as bounded data.

local ink = 0x171A21
local paper = 0xF5F0E7
local muted = 0x666B73
local coral = 0xE76F51
local white = 0xFFFFFF

local function text(value, size, color, semantics, id)
    return {
        id = id,
        content = { kind = "text", value = tostring(value), size = size, color = color or ink },
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

    local agenda = {}
    for _, event in ipairs(model.calendar) do
        table.insert(agenda, group("row", { gap = 14, padding = 12 }, {
            text(event.time, 13, coral),
            group("column", { gap = 2, grow = true }, {
                text(event.title, 17, ink), text(event.detail, 12, muted),
            }),
        }))
    end

    return group("column", { scroll_y = true }, {
        group("column", { padding = 22, gap = 18 }, {
            text("A living day", 30, ink, {
                role = "header", label = "A living day",
            }, "living-day-header"),
            group("row", { gap = 14, align = "center" }, {
                {
                    id = "album-art",
                    layout = { width = 76, height = 76 },
                    content = { kind = "image", asset = "album-orbit" },
                    paint = {{ kind = "fill_bounds", color = white, radius = 22 }},
                    animation = { kind = "pulse", duration_ms = 1800, loop = true },
                    semantics = { role = "image", label = "Orbit album artwork" },
                },
                group("column", { gap = 4, grow = true }, {
                    text(model.music.title, 19, ink), text(model.music.artist, 13, muted),
                    group("row", { padding = 10 }, {
                        text(playing and "PAUSE" or "PLAY", 12, white),
                    }, {
                        id = "music-toggle",
                        background = ink,
                        radius = 14,
                        tap_action = "toggle_music",
                        semantics = {
                            role = "button",
                            label = playing and "Pause music" or "Play music",
                            value = playing and "Playing" or "Paused",
                            hint = "Double tap to change playback",
                        },
                    }),
                }),
            }),
            group("column", { gap = 8, padding = 16 }, {
                text("Capture a thought", 18, ink),
                {
                    id = "note-draft",
                    layout = { height = 38, padding = 7 },
                    content = {
                        kind = "text_session", state_key = "draft", value = draft,
                        placeholder = "Write a note…", submit_action = "save_note", autofocus = true,
                    },
                    paint = {{ kind = "fill_bounds", color = 0xF0ECE4, radius = 10 }},
                    semantics = {
                        role = "text_field", label = "Note draft", value = draft,
                        hint = "Edit a note and press enter to save",
                    },
                },
                text(state.last_saved and "Saved without leaving the flow" or "Draft saves while you type", 12, muted, {
                    role = "status", label = "Note status",
                    value = state.last_saved and "Saved" or "Editing",
                }, "note-status"),
            }, { background = white, radius = 20 }),
            text(model.weather.temperature_c .. "° · " .. model.weather.summary, 18, ink, {
                role = "status", label = "Weather",
                value = model.weather.temperature_c .. " degrees, " .. model.weather.summary,
            }, "weather-status"),
            text("TODAY", 11, muted),
            group("column", { gap = 6 }, agenda),
        }),
    }, { id = "daily-flow-root", background = paper })
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
