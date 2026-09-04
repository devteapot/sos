--!strict
-- SOS Stock Shell. The root keeps only Shell-role authority and composes its
-- replaceable workspace through the Experience API v4 mount boundary.

local theme = require("stock.theme")

local ink = theme.color.ink
local muted = theme.color.muted
local paper = theme.color.paper
local surface = theme.color.white
local accent = theme.color.moss
local accent_subtle = theme.color.mint
local warning = theme.color.amber
local danger = theme.color.alert
local danger_subtle = theme.color.rose
local secondary = theme.color.sand
local panel = theme.color.panel
local outline = theme.color.outline
local space_sm = theme.space.sm
local space_md = theme.space.md
local space_lg = theme.space.lg
local control_radius = theme.radius.control
local card_radius = theme.radius.card
local panel_radius = theme.radius.panel
local label_size = 10
local body_size = 13
local title_size = 24

local function appearance_color(appearance: any, token: string, fallback: number): number
    local value = appearance and appearance.colors and appearance.colors[token] or nil
    if type(value) ~= "string"
        or not string.match(value, "^#[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]$") then
        return fallback
    end
    return tonumber(string.sub(value, 2, 7), 16) or fallback
end

local function appearance_number(
    appearance: any,
    bucket: string,
    token: string,
    fallback: number
): number
    local values = appearance and appearance[bucket] or nil
    local value = values and values[token] or nil
    if type(value) ~= "number" or value < 0 or value > 4096 then return fallback end
    return value
end

local function appearance_type_size(appearance: any, token: string, fallback: number): number
    local role = appearance and appearance.typography and appearance.typography[token] or nil
    local value = role and role.size_milli_points or nil
    if type(value) ~= "number" or value < 1000 or value > 512000 then return fallback end
    return value / 1000
end

local function apply_appearance(appearance: any)
    ink = appearance_color(appearance, "text.primary", theme.color.ink)
    muted = appearance_color(appearance, "text.muted", theme.color.muted)
    paper = appearance_color(appearance, "surface.canvas", theme.color.paper)
    surface = appearance_color(appearance, "surface.primary", theme.color.white)
    accent = appearance_color(appearance, "accent.primary", theme.color.moss)
    accent_subtle = appearance_color(appearance, "accent.subtle", theme.color.mint)
    warning = appearance_color(appearance, "status.warning", theme.color.amber)
    danger = appearance_color(appearance, "status.danger", theme.color.alert)
    danger_subtle = appearance_color(appearance, "status.danger_subtle", theme.color.rose)
    secondary = appearance_color(appearance, "surface.secondary", theme.color.sand)
    panel = appearance_color(appearance, "surface.panel", theme.color.panel)
    outline = appearance_color(appearance, "border.subtle", theme.color.outline)
    space_sm = appearance_number(appearance, "spacing", "space.sm", theme.space.sm)
    space_md = appearance_number(appearance, "spacing", "space.md", theme.space.md)
    space_lg = appearance_number(appearance, "spacing", "space.lg", theme.space.lg)
    control_radius = appearance_number(appearance, "radii", "radius.control", theme.radius.control)
    card_radius = appearance_number(appearance, "radii", "radius.card", theme.radius.card)
    panel_radius = appearance_number(appearance, "radii", "radius.panel", theme.radius.panel)
    label_size = appearance_type_size(appearance, "type.label", 10)
    body_size = appearance_type_size(appearance, "type.body", 13)
    title_size = appearance_type_size(appearance, "type.title", 24)
end

local workspace_order = {
    { id = "home", label = "Home" },
    { id = "agenda", label = "Agenda" },
    { id = "notes", label = "Notes" },
    { id = "media", label = "Media" },
    { id = "attention", label = "Attention" },
    { id = "system", label = "System" },
    { id = "apps", label = "Apps" },
    { id = "agent", label = "Agent" },
}

local validation_scenarios = {
    { name = "launcher", state = { shell_panel = "command" } },
    { name = "agent_panel", state = { shell_panel = "agent" } },
    { name = "agent_hud", state = { agent_overlay_hovered = true } },
}

local function text(value: any, size: number, color: number?, semantics: Semantics?, id: string?): SceneNode
    return {
        id = id,
        content = { kind = "text", value = tostring(value), size = size, color = color or ink },
        semantics = semantics,
    }
end

local function group(
    flow: "overlay" | "column" | "row",
    layout: Layout?,
    children: {SceneNode},
    background: number?,
    radius: number?,
    id: string?,
    tap_action: string?,
    tap_label: string?
): SceneNode
    local resolved_layout: Layout = layout or {}
    resolved_layout.flow = flow
    local paint: {PaintOp}? = nil
    local interaction: Interaction? = nil
    local semantics: Semantics? = nil
    if background ~= nil then
        paint = ({{ kind = "fill_bounds", color = background, radius = radius or 0 }} :: {PaintOp})
    end
    if tap_action ~= nil then
        interaction = ({ tap_action = tap_action } :: Interaction)
        semantics = ({ role = "button", label = tap_label or tap_action } :: Semantics)
    end
    return {
        id = id,
        layout = resolved_layout,
        paint = paint,
        interaction = interaction,
        semantics = semantics,
        children = children,
    }
end

local function button(
    label: string,
    action: string,
    tone: number?,
    id: string?,
    active: boolean?
): SceneNode
    return group("row", { padding = 1, justify = "center", align = "center" }, {
        group("row", { padding = space_sm, justify = "center", align = "center" }, {
            text(label, label_size, active and paper or (tone or ink)),
        }, active and accent or surface, math.max(0, control_radius - 1)),
    }, outline, control_radius, id or action, action, label)
end

local function framed_panel(
    id: string,
    layout: Layout,
    children: {SceneNode},
    background: number?
): SceneNode
    local inner_width = layout.width
    local inner_height = layout.height
    local outer: Layout = {
        width = inner_width, height = inner_height, max_width = layout.max_width,
        max_height = layout.max_height, padding = 1,
    }
    return group("column", outer, {
        group("column", {
            grow = true, padding = space_lg, gap = space_md,
            scroll_y = layout.scroll_y,
        }, children, background or surface, math.max(0, panel_radius - 1), id .. "-body"),
    }, outline, panel_radius, id)
end

local function has_capability(providers: SystemProviders, expected: string): boolean
    for _, capability in ipairs(providers.capabilities) do
        if capability == expected then return true end
    end
    return false
end

local function window_has_capability(window: ShellWindow, expected: string): boolean
    for _, capability in ipairs(window.capabilities) do
        if capability == expected then return true end
    end
    return false
end

local function active_workspace(state: ExperienceState): string
    local requested = state.active_workspace or "home"
    for _, workspace in ipairs(workspace_order) do
        if workspace.id == requested then return requested end
    end
    return "home"
end

local function active_title(model: ExperienceModel, state: ExperienceState): string
    for _, window in ipairs(model.shell.windows) do
        if window.active then return window.title end
    end
    local active = active_workspace(state)
    for _, workspace in ipairs(workspace_order) do
        if workspace.id == active then return workspace.label end
    end
    return "SOS"
end

local function compact_workspace_navigation(state: ExperienceState): SceneNode
    local active = active_workspace(state)
    local children: {SceneNode} = {}
    for index, workspace in ipairs(workspace_order) do
        local selected = workspace.id == active
        table.insert(children, group("row", {
            width = 24, height = 24, justify = "center", align = "center",
        }, {
            text(tostring(index), 9, selected and paper or muted),
        }, selected and accent or paper, control_radius,
            "top-workspace-" .. workspace.id, "navigate_" .. workspace.id,
            "Open " .. workspace.label))
    end
    return group("row", { gap = 2, align = "center" }, children,
        nil, 0, "top-workspace-navigation")
end

local function shell_top_bar(
    model: ExperienceModel,
    state: ExperienceState,
    providers: SystemProviders
): SceneNode
    local right: {SceneNode} = {}
    if (providers.attention.urgent_count or 0) > 0 then
        table.insert(right, group("row", { padding = 5, align = "center" }, {
            text(tostring(providers.attention.urgent_count) .. " URGENT", 9, danger),
        }, nil, 0, "shell-attention", "navigate_attention", "Open attention"))
    end
    table.insert(right, group("row", { padding = 5, align = "center" }, {
        text(providers.connectivity.connected and "ONLINE" or "OFFLINE", 9,
            providers.connectivity.connected and muted or danger),
    }, nil, 0, "shell-network", "navigate_system", "Open system controls"))
    local clock = providers.clock
    table.insert(right, text(clock.time_label ~= "" and clock.time_label or "--:--", body_size, ink,
        { role = "status", label = "Current time", value = clock.time_label or "" }, "shell-time"))

    return group("column", { height = 41, padding = 1, program = { measure_width = 1 } }, {
        group("row", {
            height = 39, padding = 6, gap = 10, align = "center", justify = "between",
            program = { measure_width = 1 },
        }, {
            group("row", { align = "center", gap = space_sm }, {
                group("row", { padding = 5, gap = 6, align = "center" }, {
                    text("/", 14, ink),
                    text("SOS", 11, ink, { role = "header", label = "SOS shell" }, "shell-title"),
                    text("SUPER + SPACE", 8, muted),
                }, paper, control_radius, "launcher-toggle", "toggle_command_center", "Open launcher"),
                compact_workspace_navigation(state),
            }),
            text(active_title(model, state), label_size, muted,
                { role = "status", label = "Active workspace or window",
                    value = active_title(model, state) }, "shell-active-title"),
            group("row", { align = "center", gap = space_sm }, right,
                nil, 0, "shell-status"),
        }, paper, 0),
    }, outline, 0, "shell-top-bar")
end

local function contains(haystack: string, needle: string): boolean
    return needle == "" or string.find(string.lower(haystack), needle, 1, true) ~= nil
end

local function launcher_row(
    id: string,
    title: string,
    detail: string,
    action: string?,
    action_label: string?
): SceneNode
    return group("row", { padding = 1 }, {
        group("row", {
            padding = space_md, gap = space_sm, align = "center", justify = "between", grow = true,
        }, {
            group("column", { gap = 2, grow = true }, {
                text(title, body_size, ink),
                text(detail, 9, muted),
            }),
            text(action and "OPEN" or "UNAVAILABLE", 9, action and ink or muted),
        }, surface, math.max(0, card_radius - 1)),
    }, outline, card_radius, id, action, action_label)
end

local function launcher(model: ExperienceModel, state: ExperienceState, providers: SystemProviders): SceneNode
    local query = string.lower(state.launcher_query or "")
    local rows: {SceneNode} = {
        group("row", { align = "center", justify = "between" }, {
            group("column", { gap = 2 }, {
                text("Launcher", title_size, ink,
                    { role = "header", label = "Application launcher" }, "launcher-title"),
                text("Experiences, Linux applications, and open windows", label_size, muted),
            }),
            button("CLOSE", "close_shell_panel", muted, "launcher-close"),
        }),
        {
            id = "launcher-search",
            layout = { height = 44, padding = space_sm },
            content = {
                kind = "text_session", state_key = "launcher_query",
                value = state.launcher_query or "", placeholder = "Search",
                autofocus = true,
            },
            paint = {{ kind = "fill_bounds", color = surface, radius = control_radius }},
            semantics = {
                role = "text_field", label = "Search launcher",
                value = state.launcher_query or "", hint = "Type to filter launch targets",
            },
        },
        text("WORKSPACES", 9, muted),
    }
    local workspace_buttons: {SceneNode} = {}
    for _, workspace in ipairs(workspace_order) do
        if contains(workspace.label, query) then
            table.insert(workspace_buttons, button(workspace.label, "navigate_" .. workspace.id,
                ink, "launcher-workspace-" .. workspace.id,
                active_workspace(state) == workspace.id))
        end
    end
    table.insert(rows, group("row", { wrap = true, gap = 5 }, workspace_buttons,
        nil, 0, "launcher-workspaces"))

    local target_count = 0
    for index, experience in ipairs(model.shell.experiences or {}) do
        if index > 32 then break end
        if experience.experience_id ~= "sos.stock.workspace" and contains(experience.title, query) then
            target_count = target_count + 1
            table.insert(rows, launcher_row("launcher-experience-" .. tostring(index),
                experience.title, "EXPERIENCE", "experience_present_" .. tostring(index),
                "Open " .. experience.title))
        end
    end
    for index, application in ipairs(providers.apps.compatible) do
        if index > 48 then break end
        if contains(application.label, query) then
            target_count = target_count + 1
            local action = has_capability(providers, "app_launch")
                and "app_launch_" .. tostring(index) or nil
            table.insert(rows, launcher_row("launcher-app-" .. tostring(index),
                application.label, "LINUX APPLICATION", action,
                action and ("Open " .. application.label) or nil))
        end
    end
    if target_count == 0 then
        table.insert(rows, text(query == "" and "No launch targets are available."
            or "No launch target matches your search.", body_size, muted,
            { role = "status", label = "No launcher results" }, "launcher-empty"))
    end

    table.insert(rows, text("WINDOWS", 9, muted))
    if #model.shell.windows == 0 then
        table.insert(rows, text("No application windows are open.", body_size, muted))
    end
    for index, window in ipairs(model.shell.windows) do
        if index > 32 then break end
        if contains(window.title, query) then
            local action = window_has_capability(window, "focus")
                and "shell_window_focus_" .. tostring(index) or nil
            table.insert(rows, launcher_row("launcher-window-" .. tostring(index),
                window.title, string.upper(window.kind), action,
                action and ("Focus " .. window.title) or nil))
        end
    end
    table.insert(rows, group("row", { align = "center", justify = "between", gap = space_sm }, {
        group("row", { gap = 5 }, {
            button("TILE", "window_layout_tiling", ink, "layout-tiling",
                (state.window_layout or "tiling") == "tiling"),
            button("FLOAT", "window_layout_floating", ink, "layout-floating",
                state.window_layout == "floating"),
            button("SCROLL", "window_layout_scrolling", ink, "layout-scrolling",
                state.window_layout == "scrolling"),
        }),
        text("SUPER + SPACE TO TOGGLE", 8, muted),
    }))
    return framed_panel("shell-launcher", { width = 640, height = 620, scroll_y = true }, rows, paper)
end

local function agent_messages(model: ExperienceModel, scope: string, limit: number): SceneNode
    local messages: {SceneNode} = {}
    local start = math.max(1, #model.agent.messages - limit + 1)
    for index = start, #model.agent.messages do
        local message = model.agent.messages[index]
        table.insert(messages, group("column", { padding = space_sm, gap = 3 }, {
            text(message.role == "user" and "YOU" or "SOS", 9, muted),
            text(message.text, body_size, ink),
        }, message.role == "user" and secondary or accent_subtle, control_radius,
            scope .. "-agent-message-" .. tostring(index)))
    end
    if #messages == 0 then
        table.insert(messages, text("Ask SOS to change the system or build an experience.",
            body_size, muted, { role = "status", label = "Agent ready" },
            scope .. "-agent-empty"))
    end
    return group("column", { gap = space_sm }, messages, nil, 0, scope .. "-agent-messages")
end

local function agent_input(model: ExperienceModel, state: ExperienceState, scope: string): SceneNode
    return {
        id = scope .. "-agent-prompt",
        layout = { height = 46, padding = space_sm },
        content = {
            kind = "text_session", state_key = "agent_draft", value = state.agent_draft or "",
            placeholder = model.agent.busy and "SOS is working" or "Ask SOS",
            submit_action = "agent_submit",
        },
        paint = {{ kind = "fill_bounds", color = surface, radius = control_radius }},
        semantics = {
            role = "text_field", label = "Ask SOS", value = state.agent_draft or "",
            hint = "Type a request and press enter",
        },
    }
end

local function agent_panel(model: ExperienceModel, state: ExperienceState): SceneNode
    local rows: {SceneNode} = {
        group("row", { align = "center", justify = "between" }, {
            group("column", { gap = 2 }, {
                text("Agent", title_size, ink,
                    { role = "header", label = "Agent" }, "agent-panel-title"),
                text(model.agent.activity ~= "" and model.agent.activity
                    or (model.agent.available and "Ready" or "Unavailable"), label_size, muted),
            }),
            button("CLOSE", "close_shell_panel", muted, "agent-panel-close"),
        }),
        agent_messages(model, "panel", 24),
        agent_input(model, state, "panel"),
    }
    if model.agent.error then table.insert(rows, text(model.agent.error, body_size, danger)) end
    return framed_panel("shell-agent-panel", { width = 680, height = 650, scroll_y = true }, rows, paper)
end

local function agent_overlay(model: ExperienceModel, state: ExperienceState): SceneNode
    local hovered = state.agent_overlay_hovered == true
    local positioned = type(state.agent_overlay_x) == "number"
        and type(state.agent_overlay_y) == "number"
    local bubble_x: number = positioned and (state.agent_overlay_x :: number) or 0
    local bubble_y: number = positioned and (state.agent_overlay_y :: number) or 0
    local bubble_size = 54
    local hud_width = 560
    local hud_height = 184
    local width = hovered and hud_width or bubble_size
    local height = hovered and (hud_height + bubble_size) or bubble_size
    local above = bubble_y >= hud_height + 24
    local x = hovered and (bubble_x - (width - bubble_size)) or bubble_x
    local y = hovered and (above and (bubble_y - hud_height) or bubble_y) or bubble_y

    local bubble = group("row", {
        width = bubble_size, height = bubble_size, justify = "center", align = "center",
    }, { text(model.agent.busy and "..." or "/", 18, paper) }, accent, panel_radius,
        "agent-overlay-action", "toggle_agent_panel", "Open the SOS agent")
    if bubble.interaction ~= nil then bubble.interaction.surface_drag = true end

    local hud = group("column", {
        width = hud_width, height = hud_height, padding = space_md, gap = space_sm,
    }, {
        group("row", { align = "center", justify = "between" }, {
            text("SOS / HUD", label_size, ink,
                { role = "header", label = "SOS agent HUD" }, "hud-title"),
            text(model.agent.busy and "WORKING" or "READY", 9,
                model.agent.busy and warning or muted),
        }),
        agent_messages(model, "hud", 2),
        agent_input(model, state, "hud"),
    }, paper, panel_radius, "agent-hud")

    local children: {SceneNode}
    if not hovered then
        children = { bubble }
    elseif above then
        if hud.layout ~= nil then hud.layout.position = { x = 0, y = 0 } end
        if bubble.layout ~= nil then
            bubble.layout.position = { x = width - bubble_size, y = hud_height }
        end
        children = { hud, bubble }
    else
        if bubble.layout ~= nil then bubble.layout.position = { x = width - bubble_size, y = 0 } end
        if hud.layout ~= nil then hud.layout.position = { x = 0, y = bubble_size } end
        children = { bubble, hud }
    end

    local placement: ShellOverlayPlacement? = nil
    local anchor: ShellOverlayAnchor? = nil
    if positioned then
        anchor = ({
            x = bubble_x, y = bubble_y, width = bubble_size, height = bubble_size, above = above,
        } :: ShellOverlayAnchor)
    else
        placement = ({ horizontal = "end", vertical = "end", margin = 16 } :: ShellOverlayPlacement)
    end
    return {
        id = "agent-overlay",
        layout = { width = width, height = height, flow = "overlay" },
        content = {
            kind = "shell_overlay", x = x, y = y, width = width, height = height,
            placement = placement, anchor = anchor,
        },
        interaction = { hover_action = "agent_overlay_hover" },
        semantics = { role = "status", label = "SOS agent HUD" },
        children = children,
    }
end

local function workspace_mount(state: ExperienceState): SceneNode
    return {
        id = "stock-workspace-mount",
        layout = { grow = true, clip_bounds = true },
        content = {
            kind = "experience_mount", dependency = "workspace",
            properties = { active_workspace = active_workspace(state) },
        },
    }
end

local function window_space(state: ExperienceState): SceneNode
    return {
        id = "shell-window-space",
        layout = { grow = true, min_width = 320, min_height = 200, clip_bounds = true },
        content = {
            kind = "window_space", layout = state.window_layout or "tiling", gap = 8,
            fallback = "No application windows are open",
        },
        paint = {{ kind = "fill_bounds", color = paper, radius = 0 }},
        semantics = { role = "scroll_area", label = "Application window space" },
        children = { workspace_mount(state) },
    }
end

local function modal_layer(
    model: ExperienceModel,
    state: ExperienceState,
    providers: SystemProviders
): SceneNode?
    local content: SceneNode? = nil
    if state.shell_panel == "command" then
        content = launcher(model, state, providers)
    elseif state.shell_panel == "agent" then
        content = agent_panel(model, state)
    end
    if content == nil then return nil end
    return group("row", {
        padding = 72, align = "start", justify = "center",
        program = { measure_width = 1, measure_height = 1, arrange_x = 0, arrange_y = 0 },
    }, { content }, nil, 0, "shell-modal-layer")
end

local function render(model: ExperienceModel, state: ExperienceState): SceneNode
    apply_appearance(model.appearance)
    local base = group("column", {
        program = { measure_width = 1, measure_height = 1 },
    }, {
        shell_top_bar(model, state, model.providers),
        window_space(state),
    }, paper, 0, "shell-base")
    local children: {SceneNode} = { base }
    local modal = modal_layer(model, state, model.providers)
    if modal ~= nil then table.insert(children, modal) end
    table.insert(children, agent_overlay(model, state))
    return group("overlay", {
        program = { measure_width = 1, measure_height = 1 }, clip_bounds = true,
    }, children, paper, 0, "stock-experience-root")
end

local function provider_effect(
    provider: string,
    action: string,
    payload: JsonValue?,
    state: ExperienceState
): UpdateOutcome
    return { state = state, effects = {{ provider = provider, action = action, payload = payload }} }
end

local function update(model: ExperienceModel, state: ExperienceState, event: any): ExperienceState | UpdateOutcome
    local action = event.action or ""
    if event.dependency == "workspace" and event.event == "navigate" then
        state.active_workspace = event.payload.workspace or state.active_workspace
        return state
    elseif action == "toggle_command_center" or action == "shell_shortcut_launcher" then
        state.shell_panel = state.shell_panel == "command" and nil or "command"
        if state.shell_panel ~= "command" then state.launcher_query = "" end
        return state
    elseif action == "close_shell_panel" then
        state.shell_panel = nil
        state.launcher_query = ""
        return state
    elseif action == "toggle_agent_panel" or action == "shell_overlay_activated" then
        state.shell_panel = state.shell_panel == "agent" and nil or "agent"
        return state
    elseif action == "agent_overlay_hover" then
        state.agent_overlay_hovered = event.focused == true
        return state
    elseif action == "shell_overlay_moved" then
        state.agent_overlay_x = event.x or state.agent_overlay_x
        state.agent_overlay_y = event.y or state.agent_overlay_y
        return state
    elseif action == "window_layout_tiling" then
        state.window_layout = "tiling"
        state.shell_panel = nil
        return state
    elseif action == "window_layout_floating" then
        state.window_layout = "floating"
        state.shell_panel = nil
        return state
    elseif action == "window_layout_scrolling" then
        state.window_layout = "scrolling"
        state.shell_panel = nil
        return state
    elseif action == "text_changed" and event.target == "launcher-search" then
        state.launcher_query = event.value or ""
        return state
    elseif action == "text_changed"
        and (event.target == "panel-agent-prompt" or event.target == "hud-agent-prompt") then
        state.agent_draft = event.value or ""
        return state
    elseif action == "agent_submit" and not model.agent.busy then
        local prompt = event.value or state.agent_draft or ""
        if prompt ~= "" then
            state.agent_draft = ""
            return provider_effect("agent", "prompt", { prompt = prompt }, state)
        end
    end

    local destination = string.match(action, "^navigate_([a-z]+)$")
    if destination then
        for _, workspace in ipairs(workspace_order) do
            if workspace.id == destination then
                state.active_workspace = destination
                state.shell_panel = nil
                return state
            end
        end
    end
    local app_index = tonumber(string.match(action, "^app_launch_(%d+)$"))
    local application = app_index and model.providers.apps.compatible[app_index] or nil
    if application then
        state.shell_panel = nil
        return provider_effect("apps", "launch", { app_id = application.id }, state)
    end
    local experience_index = tonumber(string.match(action, "^experience_present_(%d+)$"))
    local experience = experience_index and model.shell.experiences[experience_index] or nil
    if experience and experience.experience_id ~= "sos.stock.workspace" then
        state.shell_panel = nil
        return provider_effect("shell", "present_experience", {
            experience_id = experience.experience_id,
        }, state)
    end
    local focus_index = tonumber(string.match(action, "^shell_window_focus_(%d+)$"))
    local window = focus_index and model.shell.windows[focus_index] or nil
    if window and window_has_capability(window, "focus") then
        state.shell_panel = nil
        return provider_effect("shell", "focus_window", { window_id = window.id }, state)
    end
    return state
end

return {
    api_version = 4,
    state_version = 1,
    validation_scenarios = validation_scenarios,
    exports = { main = { render = render, update = update } },
}
