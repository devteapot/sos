--!strict
-- SOS Stock Base v0. The same System Providers v1 model and effect ABI are
-- available to every generated experience. Credentials, lock, permission,
-- emergency, confirmation, and Recovery UI remain fixed native surfaces.

local ink = 0x17211B
local muted = 0x637069
local paper = 0xF3F1E8
local white = 0xFEFDF8
local moss = 0x3E6B52
local mint = 0xDCE8D8
local amber = 0xE9A23B
local alert = 0x8C3A36

local function text(value, size, color, semantics, id)
    return {
        id = id,
        content = { kind = "text", value = tostring(value), size = size, color = color or ink },
        semantics = semantics,
    }
end

local function group(flow, layout, children, background, radius, id, tap_action, tap_label)
    layout = layout or {}
    layout.flow = flow
    return {
        id = id,
        layout = layout,
        paint = background and {{ kind = "fill_bounds", color = background, radius = radius or 0 }} or nil,
        interaction = tap_action and { tap_action = tap_action } or nil,
        semantics = tap_action and { role = "button", label = tap_label or tap_action } or nil,
        children = children,
    }
end

local function button(label, action, tone, id)
    return group("row", { padding = 10, justify = "center", align = "center" }, {
        text(label, 11, tone or moss),
    }, white, 12, id or action, action, label)
end

local function has_capability(providers, expected)
    for _, capability in ipairs(providers.capabilities or {}) do
        if capability == expected then return true end
    end
    return false
end

local function status_plane(providers)
    local clock = providers.clock or {}
    local power = providers.power or {}
    local connectivity = providers.connectivity or {}
    local audio = providers.audio or {}
    local battery = power.battery_percent == nil and "Battery unavailable"
        or tostring(power.battery_percent) .. "%" .. (power.charging and " · charging" or "")
    local connection = connectivity.connected and
        ((connectivity.validated and "Online" or "Connected") ..
            (connectivity.network_label ~= "" and " · " .. connectivity.network_label or ""))
        or "Offline"
    local volume = audio.volume_percent == nil and "Volume unavailable"
        or "Volume " .. tostring(audio.volume_percent) .. "%" .. (audio.muted and " · muted" or "")
    return group("column", { gap = 14, padding = 20 }, {
        group("row", { justify = "between", align = "center" }, {
            group("column", { gap = 3 }, {
                text(clock.time_label ~= "" and clock.time_label or "--:--", 42, ink,
                    { role = "status", label = "Current time", value = clock.time_label or "" },
                    "stock-time"),
                text(clock.date_label ~= "" and clock.date_label or "Date unavailable", 14, muted),
            }),
            text("SOS", 13, moss),
        }),
        group("column", { gap = 8 }, {
            group("column", { padding = 12, gap = 3 }, {
                text("POWER", 10, moss),
                text(battery, 13, ink,
                    { role = "status", label = "Power status", value = battery }, "stock-power"),
            }, white, 14),
            group("column", { padding = 12, gap = 3 }, {
                text("NETWORK", 10, moss),
                text(connection, 13, ink,
                    { role = "status", label = "Network status", value = connection },
                    "stock-network"),
            }, white, 14),
            group("column", { padding = 12, gap = 3 }, {
                text("AUDIO", 10, moss),
                text(volume, 13, ink,
                    { role = "status", label = "Audio status", value = volume }, "stock-audio"),
            }, white, 14),
        }),
    }, mint, 22, "status-plane")
end

local function quick_controls(providers)
    local audio = providers.audio or {}
    local connectivity = providers.connectivity or {}
    local media = audio.media or {}
    local controls = {
        text("QUICK CONTROLS", 12, moss),
    }
    local audio_buttons = {}
    if has_capability(providers, "audio_set_volume") then
        table.insert(audio_buttons, button("VOLUME −", "audio_volume_down", moss))
        table.insert(audio_buttons, button("VOLUME +", "audio_volume_up", moss))
    end
    if has_capability(providers, "audio_set_muted") then
        table.insert(audio_buttons,
            button(audio.muted and "UNMUTE" or "MUTE", "audio_toggle_mute", moss))
    end
    if #audio_buttons > 0 then
        table.insert(controls, group("row", { gap = 8 }, audio_buttons))
    else
        table.insert(controls, text("Audio controls unavailable", 12, muted))
    end
    if media.active and has_capability(providers, "media_play_pause") then
        table.insert(controls, group("row", { padding = 14, gap = 10, align = "center" }, {
            group("column", { gap = 2, grow = true }, {
                text(media.title ~= "" and media.title or "Active media", 15, ink),
                text(media.artist or "", 12, muted),
            }),
            button("PREVIOUS", "media_previous", moss),
            button(media.playing and "PAUSE" or "PLAY", "media_play_pause", moss),
            button("NEXT", "media_next", moss),
        }, mint, 16, "media-controls"))
    end
    if connectivity.connected and has_capability(providers, "wifi_disconnect") then
        table.insert(controls, button("DISCONNECT WI-FI", "wifi_disconnect", alert))
    end
    for index, network in ipairs(connectivity.wifi_networks or {}) do
        if index > 6 then break end
        local can_connect = not network.connected and has_capability(providers, "wifi_connect")
        local suffix = network.connected and " · connected" or (network.saved and " · saved" or "")
        table.insert(controls, group("row", { padding = 12, justify = "between", align = "center" }, {
            group("column", { gap = 2, grow = true }, {
                text(network.label, 14, ink),
                text(string.rep("●", math.max(1, math.min(4, network.signal_level or 1))) .. suffix,
                    10, muted),
            }),
            text(network.connected and "CURRENT" or "CONNECT", 10, moss),
        }, white, 14, "wifi-network-" .. tostring(index),
            can_connect and "wifi_connect_" .. tostring(index) or nil,
            can_connect and "Connect to " .. network.label or nil))
    end
    return group("column", { gap = 9 }, controls, nil, 0, "quick-controls")
end

local function application_workspace(providers)
    local rows = { text("APPLICATIONS", 12, moss) }
    local applications = ((providers.apps or {}).compatible or {})
    if #applications == 0 then
        table.insert(rows, group("column", { padding = 16, gap = 3 }, {
            text("No compatible applications", 15, ink),
            text("Only exported, eligible launcher applications can appear here.", 12, muted),
        }, white, 16))
    end
    for index, application in ipairs(applications) do
        if index > 24 then break end
        local can_launch = has_capability(providers, "app_launch")
        table.insert(rows, group("row", { padding = 14, justify = "between", align = "center" }, {
            text(application.label, 15, ink), text(can_launch and "OPEN" or "UNAVAILABLE", 10,
                can_launch and moss or muted),
        }, white, 15, "application-" .. tostring(index),
            can_launch and "app_launch_" .. tostring(index) or nil,
            can_launch and "Open " .. application.label or nil))
    end
    return group("column", { gap = 9 }, rows, nil, 0, "application-workspace")
end

local function attention_center(providers)
    local attention = providers.attention or {}
    local rows = {
        group("row", { justify = "between", align = "center" }, {
            text("ATTENTION", 12, moss),
            text(tostring(attention.urgent_count or 0) .. " urgent", 11,
                (attention.urgent_count or 0) > 0 and alert or muted),
        }),
    }
    local items = attention.items or {}
    if #items == 0 then
        table.insert(rows, group("column", { padding = 16 }, {
            text("Nothing needs attention", 14, muted),
        }, white, 16))
    end
    for index, item in ipairs(items) do
        if index > 24 then break end
        local title = item.title ~= "" and item.title or item.kind
        local can_acknowledge = has_capability(providers, "attention_acknowledge")
        table.insert(rows, group("column", { padding = 14, gap = 4 }, {
            group("row", { justify = "between" }, {
                text(item.source, 10, item.urgent and alert or moss),
                text(item.urgent and "URGENT" or string.upper(item.kind), 10,
                    item.urgent and alert or muted),
            }),
            text(title, 15, ink),
            text(item.detail or "", 12, muted),
            can_acknowledge and button("ACKNOWLEDGE", "attention_ack_" .. tostring(index), moss,
                "attention-acknowledge-" .. tostring(index))
                or text("Acknowledgement unavailable", 10, muted),
        }, item.urgent and 0xF2DDD7 or white, 16, "attention-" .. tostring(index)))
    end
    return group("column", { gap = 9 }, rows, nil, 0, "attention-center")
end

local function has_agent_configuration_action(agent, action)
    for _, supported in ipairs(agent.configuration_actions or {}) do
        if supported == action then return true end
    end
    return false
end

local function authoring_surface(model, state)
    local agent = model.agent or {}
    local messages = {}
    for index, message in ipairs(agent.messages or {}) do
        if index > 12 then break end
        table.insert(messages, group("column", { padding = 11, gap = 3 }, {
            text(message.role == "user" and "YOU" or "SOS", 10, moss),
            text(message.text, 13, ink),
        }, message.role == "user" and 0xE8E4D7 or mint, 13,
            "agent-message-" .. tostring(index)))
    end
    if #messages == 0 then
        table.insert(messages,
            text("Describe how this experience should adapt to you.", 12, muted))
    end
    local draft = state.agent_draft or ""
    table.insert(messages, {
        id = "agent-prompt",
        layout = { height = 44, padding = 9 },
        content = {
            kind = "text_session", state_key = "agent_draft", value = draft,
            placeholder = agent.busy and "SOS is working…" or "Change this experience…",
            submit_action = "agent_submit",
        },
        paint = {{ kind = "fill_bounds", color = white, radius = 14 }},
        semantics = {
            role = "text_field", label = "Ask SOS to change this experience", value = draft,
            hint = "Type a request and press enter",
        },
    })
    local children = {
        group("row", { justify = "between", align = "center" }, {
            text("MAKE IT YOURS", 12, moss),
            text(agent.activity or (agent.available and "Ready" or "Unavailable"), 11, muted),
        }),
    }
    local controls = {}
    if has_agent_configuration_action(agent, "configure_openai") then
        table.insert(controls, button("OPENAI", "agent_configure_openai", moss))
    end
    if has_agent_configuration_action(agent, "configure_openrouter") then
        table.insert(controls, button("OPENROUTER", "agent_configure_openrouter", moss))
    end
    if has_agent_configuration_action(agent, "configure_codex") then
        table.insert(controls, button("CODEX", "agent_configure_codex", moss))
    end
    if has_agent_configuration_action(agent, "use_fake") then
        table.insert(controls, button("OFFLINE", "agent_use_fake", moss))
    end
    if #controls > 0 then
        table.insert(children, group("row", { gap = 6 }, controls))
    else
        table.insert(children, text("Provider changes are managed before login.", 10, muted))
    end
    table.insert(children, group("column", { gap = 7 }, messages))
    table.insert(children, agent.error and text(agent.error, 11, alert) or text("", 1, muted))
    return group("column", { padding = 16, gap = 9 }, children,
        0xECE9DC, 20, "authoring-surface")
end

local function render(model, state)
    local providers = model.providers or {}
    local sections = {
        status_plane(providers),
        quick_controls(providers),
        application_workspace(providers),
        attention_center(providers),
        authoring_surface(model, state),
        text("SYSTEM PROVIDERS ABI " .. tostring(providers.abi_version or 0), 10, muted),
    }
    return group("column", { scroll_y = true }, {
        group("column", { padding = 22, gap = 22 }, sections),
    }, paper, 0, "stock-experience-root")
end

local function update(model, state, event)
    local providers = model.providers or {}
    local audio = providers.audio or {}
    local connectivity = providers.connectivity or {}
    if event.action == "audio_volume_down" then
        return { state = state, effects = {{
            provider = "audio", action = "adjust_volume", payload = { delta = -10 },
        }} }
    elseif event.action == "audio_volume_up" then
        return { state = state, effects = {{
            provider = "audio", action = "adjust_volume", payload = { delta = 10 },
        }} }
    elseif event.action == "audio_toggle_mute" then
        return { state = state, effects = {{
            provider = "audio", action = "set_muted", payload = { muted = not audio.muted },
        }} }
    elseif event.action == "media_play_pause" then
        return { state = state, effects = {{ provider = "media", action = "play_pause" }} }
    elseif event.action == "media_next" then
        return { state = state, effects = {{ provider = "media", action = "next" }} }
    elseif event.action == "media_previous" then
        return { state = state, effects = {{ provider = "media", action = "previous" }} }
    elseif event.action == "wifi_disconnect" then
        return { state = state, effects = {{ provider = "network", action = "disconnect" }} }
    elseif event.action == "text_changed" and event.target == "agent-prompt" then
        state.agent_draft = event.value
    elseif event.action == "agent_submit" and not ((model.agent or {}).busy) then
        local prompt = event.value or state.agent_draft or ""
        if prompt ~= "" then
            state.agent_draft = ""
            return { state = state, effects = {{
                provider = "agent", action = "prompt", payload = { prompt = prompt },
            }} }
        end
    elseif event.action == "agent_configure_openai" then
        return { state = state, effects = {{ provider = "agent", action = "configure_openai" }} }
    elseif event.action == "agent_configure_openrouter" then
        return { state = state, effects = {{ provider = "agent", action = "configure_openrouter" }} }
    elseif event.action == "agent_configure_codex" then
        return { state = state, effects = {{ provider = "agent", action = "configure_codex" }} }
    elseif event.action == "agent_use_fake" then
        return { state = state, effects = {{ provider = "agent", action = "use_fake" }} }
    end
    local network_index = string.match(event.action or "", "^wifi_connect_(%d+)$")
    local network = network_index and (connectivity.wifi_networks or {})[tonumber(network_index)] or nil
    if network then
        return { state = state, effects = {{
            provider = "network", action = "connect", payload = { network_id = network.id },
        }} }
    end
    local app_index = string.match(event.action or "", "^app_launch_(%d+)$")
    local application = app_index and (((providers.apps or {}).compatible or {})[tonumber(app_index)]) or nil
    if application then
        return { state = state, effects = {{
            provider = "apps", action = "launch", payload = { app_id = application.id },
        }} }
    end
    local attention_index = string.match(event.action or "", "^attention_ack_(%d+)$")
    local item = attention_index and (((providers.attention or {}).items or {})[tonumber(attention_index)]) or nil
    if item then
        return { state = state, effects = {{
            provider = "attention", action = "acknowledge",
            payload = { attention_id = item.id },
        }} }
    end
    return state
end

return { api_version = 3, render = render, update = update }
