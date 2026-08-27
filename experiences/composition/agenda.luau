--!strict

local function summary(model: ExperienceModel, state: ExperienceState, properties: JsonValue, context: ExperienceContext): SceneNode
    if state.acceptance_failure == true then
        error("reference Agenda acceptance failure")
    end
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
            {
                id = "agenda-update-fail",
                content = { kind = "text", value = "FAIL CHILD", size = 12, color = 0xfca5a5ff },
                interaction = { tap_action = "acceptance_update_fail" },
                semantics = { role = "button", label = "Trigger child update failure" },
            },
            {
                id = "agenda-timeout",
                content = { kind = "text", value = "TIMEOUT CHILD", size = 12, color = 0xfca5a5ff },
                interaction = { tap_action = "acceptance_timeout" },
                semantics = { role = "button", label = "Trigger child timeout" },
            },
            {
                id = "agenda-input",
                layout = { height = 42, padding = 8 },
                content = {
                    kind = "text_session", state_key = "draft", value = state.draft or "",
                    placeholder = "Type inside mounted Agenda",
                },
                paint = {{ kind = "fill_bounds", color = 0x374151ff, radius = 10 }},
                semantics = {
                    role = "text_field", label = "Agenda child text input",
                    value = state.draft or "",
                    hint = "Exercises mounted Experience input and IME ownership",
                },
            },
        },
    }
end

local function updateSummary(_model: ExperienceModel, state: ExperienceState, event: any): UpdateOutcome
    if event.action == "acceptance_update_fail" then
        error("reference Agenda update failure")
    elseif event.action == "acceptance_timeout" then
        while true do end
    elseif event.action == "open_first" then
        state.selected = "Design review"
        return {
            state = state,
            effects = {},
            events = {{ event = "open", payload = { item = "Design review" } }},
        }
    elseif event.action == "text_changed" then
        state.draft = event.value or ""
    elseif event.action == "acceptance_fail" then
        state.acceptance_failure = true
    elseif event.action == "acceptance_recover" then
        state.acceptance_failure = false
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
