--!strict

local function renderDashboard(model: ExperienceModel, state: ExperienceState, _properties: JsonValue, _context: ExperienceContext): SceneNode
    local opened = state.opened or "Nothing opened"
    local background = model.appearance.colors.background or "#111827ff"
    local _ = background
    return {
        id = "dashboard",
        layout = { flow = "column", gap = 16, padding = 20 },
        paint = {{ kind = "fill_bounds", color = 0x111827ff }},
        children = {
            {
                id = "heading",
                content = { kind = "text", value = "Dashboard", size = 28, color = 0xf9fafbff },
                semantics = { role = "header", label = "Dashboard" },
            },
            { id = "opened", content = { kind = "text", value = opened, size = 14, color = 0x9ca3afff } },
            {
                id = "agenda-slot",
                layout = { width = 440, height = 180, clip_bounds = true },
                content = {
                    kind = "experience_mount",
                    dependency = "agenda",
                    properties = { title = "Today" },
                    container_appearance = { radii = { surface = 14 } },
                },
            },
            {
                id = "media-slot",
                layout = { width = 440, height = 180, clip_bounds = true },
                content = { kind = "experience_mount", dependency = "media", properties = {} },
            },
        },
    }
end

local function updateDashboard(_model: ExperienceModel, state: ExperienceState, event: any): UpdateOutcome
    if event.dependency == "agenda" and event.event == "open" then
        state.opened = event.payload.item
    elseif event.dependency == "media" and event.event == "playback_changed" then
        state.media_playing = event.payload.playing
    elseif event.action == "acceptance_ping" then
        state.acceptance_pings = (state.acceptance_pings or 0) + 1
    end
    return { state = state, effects = {} }
end

return {
    api_version = 4,
    state_version = 1,
    exports = { main = { render = renderDashboard, update = updateDashboard } },
}
