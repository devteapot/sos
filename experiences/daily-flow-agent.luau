--!strict
-- Agent-produced structural revision: the same stable input and state keys are
-- reimagined as an orbital timeline rather than a stack of cards.

local night = 0x10131A
local cream = 0xF5EFE4
local dim = 0x9DA5B4
local violet = 0xA58BFA
local gold = 0xF5BB55

local function text(value, size, color, semantics)
    return {
        type = "text", text = tostring(value),
        style = { text_size = size, color = color or cream },
        accessibility = semantics,
    }
end

local function render(model, state)
    local draft = state.draft
    if draft == nil then draft = "Caffè ☕️ – 明日のデザイン" end
    local playing = state.playing
    if playing == nil then playing = model.music.playing end

    local timeline = {}
    for index, event in ipairs(model.calendar) do
        table.insert(timeline, {
            type = "row", style = { gap = 13, align = "center" }, children = {
                { type = "box", style = { width = index * 7 + 8, height = 2, background = 0x443B66 } },
                text(event.time, 12, gold),
                { type = "column", style = { gap = 2, grow = true }, children = {
                    text(event.title, 18, cream), text(event.detail, 12, dim),
                } },
            },
        })
    end

    return {
        type = "scroll", id = "daily-orbit-root", style = { background = night }, children = {
            { type = "column", style = { padding = 22, gap = 20 }, children = {
                { type = "row", style = { justify = "between", align = "center" }, children = {
                    { type = "column", style = { gap = 3 }, children = {
                        text("ORBIT / TODAY", 11, violet),
                        text(model.greeting, 28, cream, { role = "header", label = "Today orbit" }),
                    } },
                    text(model.weather.temperature_c .. "°", 35, gold, {
                        role = "status", label = "Weather",
                        value = model.weather.temperature_c .. " degrees, " .. model.weather.summary,
                    }),
                } },
                { type = "row", style = { gap = 16, align = "center" }, children = {
                    { type = "image", id = "album-art", asset = "album-orbit",
                        style = { width = 92, height = 92, radius = 46 },
                        animation = { kind = "pulse", duration_ms = 1200, loop = true },
                        accessibility = { role = "image", label = "Pulsing orbit album artwork" },
                    },
                    { type = "column", style = { grow = true, gap = 5 }, children = {
                        text(model.music.title, 20, cream), text(model.music.artist, 13, dim),
                        { type = "row", id = "music-toggle", action = "toggle_music",
                            style = { padding = 10, radius = 18, background = 0x30284B, justify = "center" },
                            accessibility = {
                                role = "button", label = playing and "Pause music" or "Play music",
                                value = playing and "Playing" or "Paused",
                            },
                            children = { text(playing and "Ⅱ" or "▶", 17, violet) },
                        },
                    } },
                } },
                { type = "column", style = { gap = 9 }, children = timeline },
                { type = "column", style = { gap = 8, padding = 2 }, children = {
                    text("NOTE IN THE CURRENT ORBIT", 11, violet),
                    { type = "text_input", id = "note-draft", state_key = "draft",
                        value = draft, placeholder = "Signal a thought…", submit_action = "save_note",
                        autofocus = true,
                        style = { height = 42, padding = 8, radius = 18, background = 0x252A36, color = cream },
                        accessibility = {
                            role = "text_field", label = "Note draft", value = draft,
                            hint = "Edit a note and press enter to save",
                        },
                    },
                    text(state.last_saved and "Signal archived" or "The signal persists across revisions", 12, dim, {
                        role = "status", label = "Note status",
                        value = state.last_saved and "Saved" or "Editing",
                    }),
                } },
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
