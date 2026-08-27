--!strict

local function render(_model: ExperienceModel, state: ExperienceState): SceneNode
    local children: {SceneNode} = {
        {
            id = "heading",
            content = { kind = "text", value = "A calmer Stock workspace", size = 28, color = 0x17211B },
            semantics = { role = "header", label = "A calmer Stock workspace" },
        },
    }
    if state.active_workspace == "agent" or state.shell_panel == "agent" then
        table.insert(children, {
            id = "agent-composer",
            content = {
                kind = "text_session",
                state_key = "agent_draft",
                value = state.agent_draft or "",
                placeholder = "Change this experience…",
                submit_action = "agent_submit",
            },
            semantics = { role = "text_field", label = "Ask SOS to change this experience" },
        })
    end
    return {
        id = "calm-stock",
        layout = { flow = "column", gap = 16, padding = 24 },
        paint = {{ kind = "fill_bounds", color = 0xF3F1E8 }},
        children = children,
    }
end

local function update(_model: ExperienceModel, state: ExperienceState, event: SceneEvent): ExperienceState
    if event.action == "agent_submit" then
        state.agent_draft = ""
    end
    return state
end

return {
    api_version = 4,
    state_version = 1,
    validation_scenarios = {
        { name = "agent", state = { active_workspace = "agent" } },
    },
    exports = {
        main = { render = render, update = update },
    },
}
