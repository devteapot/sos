--!strict

local function renderRemix(model: ExperienceModel, state: ExperienceState, _properties: JsonValue, _context: ExperienceContext): SceneNode
    local selected = state.selected or "Design review"
    local playing = state.playing == true
    return {
        id = "agenda-media-remix",
        layout = { flow = "row", gap = 18, padding = 20 },
        children = {
            {
                id = "agenda",
                layout = { flow = "column", gap = 8, grow = true },
                children = {
                    { id = "agenda-heading", content = { kind = "text", value = "Agenda", size = 24, color = 0xffffffff } },
                    { id = "agenda-selected", content = { kind = "text", value = selected, size = 16, color = 0xd1d5dbff } },
                },
            },
            {
                id = "media",
                layout = { flow = "column", gap = 8, grow = true },
                children = {
                    { id = "media-heading", content = { kind = "text", value = model.music.title, size = 24, color = 0xffffffff } },
                    {
                        id = "media-toggle",
                        content = { kind = "text", value = if playing then "Pause" else "Play", size = 16, color = 0xf0abfcff },
                        interaction = { tap_action = "toggle" },
                    },
                },
            },
        },
    }
end

local function updateRemix(_model: ExperienceModel, state: ExperienceState, event: SceneEvent): UpdateOutcome
    if event.action == "toggle" then
        state.playing = state.playing ~= true
    end
    return { state = state, effects = {} }
end

return {
    api_version = 4,
    state_version = 1,
    exports = { main = { render = renderRemix, update = updateRemix } },
}
