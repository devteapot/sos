--!strict

local function renderMedia(model: ExperienceModel, state: ExperienceState, _properties: JsonValue, context: ExperienceContext): SceneNode
    local playing = state.playing == true
    local label = if playing then "Pause" else "Play"
    return {
        id = "media-summary",
        layout = { flow = "column", gap = 8, padding = 12, width = context.viewport.width, height = context.viewport.height },
        paint = {{ kind = "fill_bounds", color = 0x241c32ff, radius = 14 }},
        children = {
            { id = "media-title", content = { kind = "text", value = model.music.title, size = 18, color = 0xfff7edff } },
            {
                id = "media-toggle",
                content = { kind = "text", value = label, size = 14, color = 0xf0abfcff },
                interaction = { tap_action = "toggle" },
                semantics = { role = "button", label = label .. " media" },
            },
        },
    }
end

local function updateMedia(_model: ExperienceModel, state: ExperienceState, event: any): UpdateOutcome
    if event.action == "toggle" then
        state.playing = state.playing ~= true
        return {
            state = state,
            effects = {{ provider = "music", action = "toggle", payload = {} }},
            events = {{ event = "playback_changed", payload = { playing = state.playing } }},
        }
    end
    return { state = state, effects = {} }
end

return {
    api_version = 4,
    state_version = 1,
    exports = {
        main = { render = renderMedia, update = updateMedia },
        summary = { render = renderMedia, update = updateMedia },
    },
}
