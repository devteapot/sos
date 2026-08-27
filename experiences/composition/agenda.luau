--!strict

local function summary(model: ExperienceModel, state: ExperienceState, properties: JsonValue, context: ExperienceContext): SceneNode
    local title = properties.title or "Agenda"
    title = title .. " · appearance " .. tostring(model.appearance.generation)
    local selected = state.selected or "None"
    return {
        id = "agenda-summary",
        layout = { flow = "column", gap = 8, padding = 12, width = context.viewport.width, height = context.viewport.height },
        paint = {{ kind = "fill_bounds", color = 0x1f2937ff, radius = 14 }},
        children = {
            {
                id = "agenda-title",
                content = { kind = "text", value = title, size = 18, color = 0xf8fafcff },
                semantics = { role = "header", label = title },
            },
            {
                id = "agenda-open",
                content = { kind = "text", value = "Selected: " .. selected, size = 14, color = 0xcbd5e1ff },
                interaction = { tap_action = "open_first" },
                semantics = { role = "button", label = "Open first agenda item" },
            },
        },
    }
end

local function updateSummary(_model: ExperienceModel, state: ExperienceState, event: any): UpdateOutcome
    if event.action == "open_first" then
        state.selected = "Design review"
        return {
            state = state,
            effects = {},
            events = {{ event = "open", payload = { item = "Design review" } }},
        }
    end
    return { state = state, effects = {} }
end

return {
    api_version = 4,
    state_version = 1,
    exports = {
        main = { render = summary, update = updateSummary },
        summary = { render = summary, update = updateSummary },
    },
}
