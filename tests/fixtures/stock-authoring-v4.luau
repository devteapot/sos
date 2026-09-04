--!strict

local function render(_model: ExperienceModel, state: ExperienceState): SceneNode
    local children: {SceneNode} = {
        {
            id = "heading",
            content = { kind = "text", value = "A calmer Stock shell", size = 28, color = 0xEDEDED },
            semantics = { role = "header", label = "A calmer Stock workspace" },
        },
        {
            id = "stock-workspace-mount",
            layout = { grow = true },
            content = {
                kind = "experience_mount",
                dependency = "workspace",
                properties = { active_workspace = state.active_workspace or "home" },
            },
        },
    }
    if state.active_workspace == "agent" or state.shell_panel == "agent" then
        local draft = type(state.agent_draft) == "string" and state.agent_draft or ""
        local composer: SceneNode = {
            id = "agent-composer",
            content = {
                kind = "text_session",
                state_key = "agent_draft",
                value = draft,
                placeholder = "Change this experience...",
                submit_action = "agent_submit",
            },
            semantics = { role = "text_field", label = "Ask SOS to change this experience" },
        }
        table.insert(children, composer)
    end
    return {
        id = "calm-stock",
        layout = { flow = "column", gap = 16, padding = 24 },
        paint = {{ kind = "fill_bounds", color = 0x000000 }},
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
