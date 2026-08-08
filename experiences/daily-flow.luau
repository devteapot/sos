--!strict
-- Stateful primitive gate: native IME input, image, animation, focus, and
-- accessibility semantics remain expressed as bounded data.

local ink = 0x171A21
local paper = 0xF5F0E7
local muted = 0x666B73
local coral = 0xE76F51
local white = 0xFFFFFF

local function text(value, size, color, semantics)
    return {
        type = "text",
        text = tostring(value),
        style = { text_size = size, color = color or ink },
        accessibility = semantics,
    }
end

local function render(model, state)
    local draft = state.draft
    if draft == nil then draft = "Caffè ☕️ – 明日のデザイン" end
    local playing = state.playing
    if playing == nil then playing = model.music.playing end

    local agenda = {}
    for _, event in ipairs(model.calendar) do
        table.insert(agenda, {
            type = "row", style = { gap = 14, padding = 12 }, children = {
                text(event.time, 13, coral),
                { type = "column", style = { gap = 2, grow = true }, children = {
                    text(event.title, 17, ink), text(event.detail, 12, muted),
                } },
            },
        })
    end

    return {
        type = "scroll", id = "daily-flow-root", style = { background = paper }, children = {
            { type = "column", style = { padding = 22, gap = 18 }, children = {
                text("A living day", 30, ink, {
                    role = "header", label = "A living day",
                }),
                { type = "row", style = { gap = 14, align = "center" }, children = {
                    { type = "image", id = "album-art", asset = "album-orbit",
                        style = { width = 76, height = 76, radius = 22 },
                        animation = { kind = "pulse", duration_ms = 1800, loop = true },
                        accessibility = { role = "image", label = "Orbit album artwork" },
                    },
                    { type = "column", style = { gap = 4, grow = true }, children = {
                        text(model.music.title, 19, ink), text(model.music.artist, 13, muted),
                        { type = "row", id = "music-toggle", action = "toggle_music",
                            style = { padding = 10, radius = 14, background = ink },
                            accessibility = {
                                role = "button",
                                label = playing and "Pause music" or "Play music",
                                value = playing and "Playing" or "Paused",
                                hint = "Double tap to change playback",
                            },
                            children = { text(playing and "PAUSE" or "PLAY", 12, white) },
                        },
                    } },
                } },
                { type = "column", style = { gap = 8, padding = 16, radius = 20, background = white }, children = {
                    text("Capture a thought", 18, ink),
                    { type = "text_input", id = "note-draft", state_key = "draft",
                        value = draft, placeholder = "Write a note…", submit_action = "save_note",
                        autofocus = true,
                        style = { height = 38, padding = 7, radius = 10, background = 0xF0ECE4 },
                        accessibility = {
                            role = "text_field", label = "Note draft", value = draft,
                            hint = "Edit a note and press enter to save",
                        },
                    },
                    text(state.last_saved and "Saved without leaving the flow" or "Draft saves while you type", 12, muted, {
                        role = "status", label = "Note status",
                        value = state.last_saved and "Saved" or "Editing",
                    }),
                } },
                text(model.weather.temperature_c .. "° · " .. model.weather.summary, 18, ink, {
                    role = "status", label = "Weather",
                    value = model.weather.temperature_c .. " degrees, " .. model.weather.summary,
                }),
                text("TODAY", 11, muted),
                { type = "column", style = { gap = 6 }, children = agenda },
            } },
        },
    }
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

return { render = render, update = update }
