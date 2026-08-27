--!strict
-- SOS Stock Mobile. This is a phone-native Experience API v4 package, not a
-- responsive mode of the Linux Stock Shell. It owns mobile navigation,
-- launcher presentation, touch geometry, and phone information architecture.
-- Credentials, lock, permission, confirmation, and Recovery remain fixed
-- trusted surfaces outside this revision.

local theme = require("mobile.theme")

local canvas = theme.color.canvas
local surface = theme.color.surface
local elevated = theme.color.elevated
local ink = theme.color.text
local muted = theme.color.muted
local accent = theme.color.accent
local accent_soft = theme.color.accent_soft
local warning = theme.color.warning
local danger = theme.color.danger
local outline = theme.color.outline

local function appearance_color(appearance: any, token: string, fallback: number): number
    local value = appearance and appearance.colors and appearance.colors[token] or nil
    if type(value) ~= "string" or not string.match(value,
        "^#[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]$") then
        return fallback
    end
    return tonumber(string.sub(value, 2, 7), 16) or fallback
end

local function apply_appearance(appearance: any)
    canvas = appearance_color(appearance, "surface.canvas", theme.color.canvas)
    surface = appearance_color(appearance, "surface.primary", theme.color.surface)
    elevated = appearance_color(appearance, "surface.secondary", theme.color.elevated)
    ink = appearance_color(appearance, "text.primary", theme.color.text)
    muted = appearance_color(appearance, "text.muted", theme.color.muted)
    accent = appearance_color(appearance, "accent.primary", theme.color.accent)
    accent_soft = appearance_color(appearance, "accent.subtle", theme.color.accent_soft)
    warning = appearance_color(appearance, "status.warning", theme.color.warning)
    danger = appearance_color(appearance, "status.danger", theme.color.danger)
    outline = appearance_color(appearance, "surface.panel", theme.color.outline)
end

local validation_scenarios = {
    { name = "launcher", state = { screen = "apps" } },
    { name = "agent", state = { screen = "agent" } },
    { name = "controls", state = { screen = "controls" } },
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
    action: string?,
    label: string?
): SceneNode
    local resolved: Layout = layout or {}
    resolved.flow = flow
    local paint: {PaintOp}? = nil
    if background ~= nil then
        paint = ({{ kind = "fill_bounds", color = background, radius = radius or 0 }} :: {PaintOp})
    end
    local interaction: Interaction? = nil
    local semantics: Semantics? = nil
    if action ~= nil then
        interaction = ({ tap_action = action } :: Interaction)
        semantics = ({ role = "button", label = label or action } :: Semantics)
    end
    return {
        id = id,
        layout = resolved,
        paint = paint,
        interaction = interaction,
        semantics = semantics,
        children = children,
    }
end

local function action_row(
    id: string,
    title: string,
    detail: string,
    action: string?,
    trailing: string?
): SceneNode
    return group("row", {
        min_height = theme.touch.prominent,
        padding = theme.space.lg,
        gap = theme.space.md,
        align = "center",
        justify = "between",
    }, {
        group("column", { gap = theme.space.xs, grow = true }, {
            text(title, 18, ink),
            text(detail, 13, muted),
        }),
        text(trailing or (action and "OPEN" or ""), 11, action and accent or muted),
    }, surface, theme.radius.control, id, action, action and ("Open " .. title) or nil)
end

local function pill(label: string, value: string, action: string?, id: string): SceneNode
    return group("column", {
        width = 224,
        min_height = theme.touch.standard,
        padding = theme.space.md,
        gap = theme.space.xs,
        justify = "center",
    }, {
        text(label, 10, muted),
        text(value, 15, ink),
    }, elevated, theme.radius.control, id, action, action and (label .. ": " .. value) or nil)
end

local function has_capability(providers: SystemProviders, expected: string): boolean
    for _, capability in ipairs(providers.capabilities) do
        if capability == expected then return true end
    end
    return false
end

local function providers_available(providers: SystemProviders): boolean
    return (providers.abi_version or 0) > 0
end

local function power_summary(providers: SystemProviders): string
    local power = providers.power
    if power.battery_percent == nil then return "--%" end
    return tostring(power.battery_percent) .. "%" .. (power.charging == true and " +" or "")
end

local function network_summary(providers: SystemProviders): string
    if not providers.connectivity.connected then return "OFFLINE" end
    if providers.connectivity.network_label ~= "" then
        return providers.connectivity.network_label
    end
    return providers.connectivity.validated and "ONLINE" or "CONNECTED"
end

local function audio_summary(providers: SystemProviders): string
    local audio = providers.audio
    if audio.volume_percent == nil then return "--%" end
    return audio.muted == true and "MUTED" or (tostring(audio.volume_percent) .. "%")
end

local function screen_name(state: ExperienceState): string
    local requested = state.screen or "home"
    if requested == "apps" or requested == "agent" or requested == "controls" then
        return requested
    end
    return "home"
end

local function mobile_top_bar(state: ExperienceState, providers: SystemProviders): SceneNode
    local screen = screen_name(state)
    local titles = {
        home = "Today",
        apps = "Apps",
        agent = "Agent",
        controls = "Controls",
    }
    local time = providers.clock.time_label ~= "" and providers.clock.time_label or "--:--"
    return group("row", {
        height = 86,
        padding = 18,
        gap = 12,
        align = "end",
        justify = "between",
    }, {
        group("column", { gap = 1 }, {
            text("SOS MOBILE", 10, accent),
            text(titles[screen] or "Today", 26, ink,
                { role = "header", label = titles[screen] or "Today" }, "mobile-screen-title"),
        }),
        group("row", { gap = 10, align = "center" }, {
            text(power_summary(providers), 11, muted),
            text(time, 18, ink, { role = "status", label = "Current time", value = time },
                "mobile-time"),
            group("row", {
                min_width = theme.touch.compact,
                min_height = theme.touch.compact,
                padding = theme.space.sm,
                justify = "center",
                align = "center",
            }, { text("•••", 18, accent) }, elevated, theme.radius.control,
                "mobile-controls-open", "navigate_controls", "Open system controls"),
        }),
    }, canvas, 0, "mobile-top-bar")
end

local function section_heading(title: string, detail: string, id: string): SceneNode
    return group("column", { gap = theme.space.xs }, {
        text(title, 24, ink, { role = "header", label = title }, id),
        text(detail, 14, muted),
    })
end

local function media_card(model: ExperienceModel, providers: SystemProviders): SceneNode
    local media = providers.audio.media
    local title = media.active and (media.title ~= "" and media.title or "Active media")
        or (model.music.title or "")
    local artist = media.active and (media.artist or "") or (model.music.artist or "")
    local playing = media.active and media.playing or model.music.playing
    if title == "" then
        return action_row("mobile-media-empty", "Nothing playing",
            "Media controls appear when a session is active.", nil, "IDLE")
    end
    return group("column", { padding = 18, gap = 10 }, {
        group("row", { justify = "between", align = "center" }, {
            text("NOW PLAYING", 10, accent),
            text(playing and "PLAYING" or "PAUSED", 10, playing and accent or muted),
        }),
        text(title, 20, ink),
        text(artist, 13, muted),
        group("row", { gap = 10 }, {
            group("row", {
                min_height = theme.touch.standard,
                grow = true,
                justify = "center",
                align = "center",
            }, { text(playing and "PAUSE" or "PLAY", 13, ink) }, accent_soft,
                theme.radius.control, "mobile-media-toggle", "media_play_pause",
                playing and "Pause media" or "Play media"),
            group("row", {
                min_height = theme.touch.standard,
                grow = true,
                justify = "center",
                align = "center",
            }, { text("NEXT", 13, ink) }, elevated, theme.radius.control,
                "mobile-media-next", "media_next", "Next media item"),
        }),
    }, surface, theme.radius.card, "mobile-media-card")
end

local function home_screen(model: ExperienceModel, providers: SystemProviders): SceneNode
    local clock = providers.clock
    local rows: {SceneNode} = {
        section_heading(clock.date_label ~= "" and clock.date_label or "Your day",
            "One glance, then get moving.", "mobile-home-heading"),
        group("row", { wrap = true, gap = 10 }, {
            pill("NETWORK", network_summary(providers), "navigate_controls", "mobile-network-pill"),
            pill("VOLUME", audio_summary(providers), "navigate_controls", "mobile-audio-pill"),
            pill("ATTENTION", tostring(providers.attention.urgent_count or 0) .. " urgent",
                "navigate_controls", "mobile-attention-pill"),
        }, nil, 0, "mobile-status-row"),
        media_card(model, providers),
        text("NEXT", 11, accent),
    }
    if #model.calendar == 0 then
        table.insert(rows, action_row("mobile-agenda-empty", "No upcoming items",
            "Your calendar is clear or unavailable.", nil, "CLEAR"))
    else
        for index, event in ipairs(model.calendar) do
            if index > 3 then break end
            table.insert(rows, action_row("mobile-agenda-" .. tostring(index), event.title,
                event.time .. (event.detail ~= "" and (" · " .. event.detail) or ""), nil,
                index == 1 and "NEXT" or ""))
        end
    end
    table.insert(rows, text("RECENT NOTES", 11, accent))
    if #model.notes == 0 then
        table.insert(rows, action_row("mobile-notes-empty", "No notes yet",
            "Notes from approved providers appear here.", nil, "EMPTY"))
    else
        for index, note in ipairs(model.notes) do
            if index > 2 then break end
            table.insert(rows, action_row("mobile-note-" .. tostring(index), note.title,
                note.preview, nil, "NOTE"))
        end
    end
    return group("column", { gap = 12 }, rows, nil, 0, "mobile-home")
end

local function apps_screen(model: ExperienceModel, providers: SystemProviders): SceneNode
    local rows: {SceneNode} = {
        section_heading("All apps", "Each experience opens as its own full-screen root.",
            "mobile-apps-heading"),
    }
    for index, experience in ipairs(model.shell.experiences or {}) do
        if index > 32 then break end
        table.insert(rows, action_row("mobile-experience-" .. tostring(index), experience.title,
            "SOS experience · independent state and grants",
            "experience_present_" .. tostring(index), "EXPERIENCE"))
    end
    local applications = providers.apps.compatible
    for index, application in ipairs(applications) do
        if index > 48 then break end
        local can_launch = has_capability(providers, "app_launch")
        table.insert(rows, action_row("mobile-application-" .. tostring(index), application.label,
            "Compatible application · opens full screen",
            can_launch and ("app_launch_" .. tostring(index)) or nil,
            can_launch and "APP" or "UNAVAILABLE"))
    end
    if #(model.shell.experiences or {}) == 0 and #applications == 0 then
        table.insert(rows, action_row("mobile-apps-empty", "No apps available",
            providers_available(providers)
                and "Installed experiences and compatible apps will appear here."
                or "The application provider is unavailable.", nil, "EMPTY"))
    end
    return group("column", { gap = 10 }, rows, nil, 0, "mobile-app-launcher")
end

local function has_agent_configuration_action(agent: AgentConversation, action: string): boolean
    for _, supported in ipairs(agent.configuration_actions) do
        if supported == action then return true end
    end
    return false
end

local function agent_screen(model: ExperienceModel, state: ExperienceState): SceneNode
    local agent = model.agent
    local rows: {SceneNode} = {
        section_heading("Build on the phone", "Change Stock Mobile or create another experience.",
            "mobile-agent-heading"),
    }
    if #agent.messages == 0 then
        table.insert(rows, action_row("mobile-agent-empty", "Make Stock Mobile yours",
            "Ask for a new interaction, visual system, launcher, or complete experience.",
            nil, agent.available and "READY" or "OFFLINE"))
    else
        for index, message in ipairs(agent.messages) do
            if index > 24 then break end
            table.insert(rows, group("column", { padding = 14, gap = 4 }, {
                text(message.role == "user" and "YOU" or "SOS", 10, accent),
                text(message.text, 15, ink),
            }, message.role == "user" and accent_soft or surface, theme.radius.control,
                "mobile-agent-message-" .. tostring(index)))
        end
    end
    local draft = state.agent_draft or ""
    table.insert(rows, ({
        id = "mobile-agent-prompt",
        layout = { min_height = 58, padding = 14 },
        content = {
            kind = "text_session",
            state_key = "agent_draft",
            value = draft,
            placeholder = agent.busy and "SOS is working…" or "Describe what you want…",
            submit_action = "agent_submit",
        },
        paint = {{ kind = "fill_bounds", color = elevated, radius = theme.radius.control }},
        semantics = {
            role = "text_field",
            label = "Ask SOS to change Stock Mobile",
            value = draft,
            hint = "Type a request and press enter",
        },
    } :: SceneNode))
    local configurations = {
        { capability = "configure_openai", label = "OpenAI", action = "agent_configure_openai" },
        { capability = "configure_openrouter", label = "OpenRouter", action = "agent_configure_openrouter" },
        { capability = "configure_codex", label = "Codex", action = "agent_configure_codex" },
        { capability = "use_fake", label = "Offline", action = "agent_use_fake" },
    }
    local controls: {SceneNode} = {}
    for _, configuration in ipairs(configurations) do
        if has_agent_configuration_action(agent, configuration.capability) then
            table.insert(controls, group("row", {
                min_height = theme.touch.standard,
                padding = theme.space.md,
                justify = "center",
                align = "center",
            }, { text(configuration.label, 13, ink) }, elevated, theme.radius.control,
                "mobile-" .. configuration.action, configuration.action, configuration.label))
        end
    end
    if #controls > 0 then
        table.insert(rows, group("row", { wrap = true, gap = 10 }, controls,
            nil, 0, "mobile-agent-provider-controls"))
    end
    if agent.error then table.insert(rows, text(agent.error, 13, danger)) end
    return group("column", { gap = 10 }, rows, nil, 0, "mobile-agent")
end

local function controls_screen(providers: SystemProviders): SceneNode
    local rows: {SceneNode} = {
        section_heading("System controls", "Large targets for quick phone actions.",
            "mobile-controls-heading"),
        action_row("mobile-control-network", "Connectivity", network_summary(providers),
            has_capability(providers, "wifi_disconnect") and providers.connectivity.connected
                and "wifi_disconnect" or nil,
            providers.connectivity.connected and "DISCONNECT" or "STATUS"),
        action_row("mobile-control-volume-down", "Volume down", audio_summary(providers),
            has_capability(providers, "audio_set_volume") and "audio_down" or nil, "−10"),
        action_row("mobile-control-volume-up", "Volume up", audio_summary(providers),
            has_capability(providers, "audio_set_volume") and "audio_up" or nil, "+10"),
        action_row("mobile-control-mute", providers.audio.muted == true and "Unmute" or "Mute",
            "Media audio", has_capability(providers, "audio_set_muted") and "audio_toggle_mute"
                or nil, providers.audio.muted == true and "OFF" or "ON"),
    }
    if #providers.attention.items == 0 then
        table.insert(rows, action_row("mobile-attention-empty", "No pending attention",
            "Nothing needs a response.", nil, "CLEAR"))
    else
        for index, item in ipairs(providers.attention.items) do
            if index > 8 then break end
            table.insert(rows, action_row("mobile-attention-" .. tostring(index),
                item.title ~= "" and item.title or item.kind, item.detail or "",
                "attention_ack_" .. tostring(index), "DONE"))
        end
    end
    return group("column", { gap = 10 }, rows, nil, 0, "mobile-controls")
end

local function mobile_bottom_bar(state: ExperienceState): SceneNode
    local active = screen_name(state)
    local items = {
        { screen = "home", label = "TODAY" },
        { screen = "apps", label = "APPS" },
        { screen = "agent", label = "AGENT" },
    }
    local children: {SceneNode} = {}
    for _, item in ipairs(items) do
        local selected = active == item.screen or (active == "controls" and item.screen == "home")
        table.insert(children, group("column", {
            min_height = 66,
            grow = true,
            gap = 4,
            justify = "center",
            align = "center",
        }, {
            text(selected and "●" or "○", 15, selected and accent or muted),
            text(item.label, 10, selected and ink or muted),
        }, selected and accent_soft or surface, theme.radius.control,
            "mobile-nav-" .. item.screen, "navigate_" .. item.screen,
            "Open " .. string.lower(item.label)))
    end
    return group("row", { height = 84, padding = 10, gap = 8, align = "center" }, children,
        surface, 0, "mobile-bottom-navigation")
end

local function render(model: ExperienceModel, state: ExperienceState, _context: any): SceneNode
    apply_appearance(model.appearance)
    local providers = model.providers
    local active = screen_name(state)
    local content: SceneNode
    if active == "apps" then
        content = apps_screen(model, providers)
    elseif active == "agent" then
        content = agent_screen(model, state)
    elseif active == "controls" then
        content = controls_screen(providers)
    else
        content = home_screen(model, providers)
    end
    return group("column", { program = { measure_width = 1, measure_height = 1 } }, {
        mobile_top_bar(state, providers),
        group("column", {
            scroll_y = true,
            grow = true,
            padding = 18,
            gap = 14,
        }, { content }, canvas, 0, "mobile-content-scroll"),
        mobile_bottom_bar(state),
    }, canvas, 0, "stock-mobile-root")
end

local function provider_effect(
    provider: string,
    action: string,
    payload: JsonValue?,
    state: ExperienceState
): UpdateOutcome
    return { state = state, effects = {{ provider = provider, action = action, payload = payload }} }
end

local function update(
    model: ExperienceModel,
    state: ExperienceState,
    event: SceneEvent
): ExperienceState | UpdateOutcome
    local action = event.action or ""
    local providers = model.providers
    if action == "navigate_home" then state.screen = "home" return state end
    if action == "navigate_apps" then state.screen = "apps" return state end
    if action == "navigate_agent" then state.screen = "agent" return state end
    if action == "navigate_controls" then state.screen = "controls" return state end
    if action == "audio_down" then
        return provider_effect("audio", "adjust_volume", { delta = -10 }, state)
    elseif action == "audio_up" then
        return provider_effect("audio", "adjust_volume", { delta = 10 }, state)
    elseif action == "audio_toggle_mute" then
        return provider_effect("audio", "set_muted",
            { muted = not (providers.audio.muted == true) }, state)
    elseif action == "media_play_pause" then
        return provider_effect("media", "play_pause", nil, state)
    elseif action == "media_next" then
        return provider_effect("media", "next", nil, state)
    elseif action == "wifi_disconnect" then
        return provider_effect("network", "disconnect", nil, state)
    elseif action == "agent_submit" and not model.agent.busy then
        local prompt = event.value or state.agent_draft or ""
        if prompt ~= "" then
            state.agent_draft = ""
            return provider_effect("agent", "prompt", { prompt = prompt }, state)
        end
    elseif action == "agent_configure_openai" then
        return provider_effect("agent", "configure_openai", nil, state)
    elseif action == "agent_configure_openrouter" then
        return provider_effect("agent", "configure_openrouter", nil, state)
    elseif action == "agent_configure_codex" then
        return provider_effect("agent", "configure_codex", nil, state)
    elseif action == "agent_use_fake" then
        return provider_effect("agent", "use_fake", nil, state)
    end
    local app_index = tonumber(string.match(action, "^app_launch_(%d+)$"))
    local application = app_index and providers.apps.compatible[app_index] or nil
    if application then
        return provider_effect("apps", "launch", { app_id = application.id }, state)
    end
    local experience_index = tonumber(string.match(action, "^experience_present_(%d+)$"))
    local experience = experience_index and model.shell.experiences[experience_index] or nil
    if experience then
        return provider_effect("shell", "present_experience", {
            experience_id = experience.experience_id,
        }, state)
    end
    local attention_index = tonumber(string.match(action, "^attention_ack_(%d+)$"))
    local item = attention_index and providers.attention.items[attention_index] or nil
    if item then
        return provider_effect("attention", "acknowledge", { attention_id = item.id }, state)
    end
    return state
end

return {
    api_version = 4,
    state_version = 1,
    validation_scenarios = validation_scenarios,
    exports = {
        main = { render = render, update = update },
    },
}
