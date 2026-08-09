return {
    api_version = 3,
    state_version = 1,

    render = function(_, state)
        return {
            id = "direct-input-surface",
            layout = {
                flow = "column",
                padding = 48,
                gap = 16,
            },
            paint = {
                { kind = "fill_bounds", color = 0x151B25 },
            },
            interaction = {
                pointer_action = "direct_pointer",
                multi_pointer_action = "direct_transform",
                capture = "surface",
            },
            semantics = {
                role = "status",
                label = "Direct input verification surface",
                value = tostring(state.pointer_count or 0),
            },
            children = {
                {
                    content = {
                        kind = "text",
                        value = "Direct libinput verification",
                        size = 28,
                        color = 0xF1F5FF,
                    },
                },
            },
        }
    end,

    update = function(_, state, event)
        if event.action == "direct_pointer" or event.action == "direct_transform" then
            state.pointer_count = event.pointer_count or 0
            state.phase = event.phase
        end
        return state
    end,
}
