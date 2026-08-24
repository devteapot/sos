--!strict
-- A source-only alternative: no cards, one continuous temporal spine, and
-- music controls rendered only while playback is active.

local night = 0x101916
local cream = 0xF3F0E4
local sage = 0xA7C7AE
local dim = 0x9EAAA3
local amber = 0xF0A83B

local function text(value, size, color, semantics, id)
    return {
        id = id,
        content = { kind = "text", value = tostring(value), size = size, color = color or cream },
        semantics = semantics,
    }
end

local function group(flow, layout, children, background, radius, id, tap_action)
    layout = layout or {}
    layout.flow = flow
    return {
        id = id,
        layout = layout,
        paint = background and {{ kind = "fill_bounds", color = background, radius = radius or 0 }} or nil,
        interaction = tap_action and { tap_action = tap_action } or nil,
        children = children,
    }
end

local function timeline_item(time, title, detail, last)
    return group("row", { gap = 16 }, {
        group("column", { width = 58, align = "center" }, {
            text(time, 13, amber),
            group("overlay", { width = 2, height = last and 22 or 86 }, nil, 0x426052),
        }),
        group("column", { gap = 4, padding = 2, grow = true }, {
            text(title, 20, cream), text(detail, 13, dim),
        }),
    })
end

local function network_panel(model)
    local network = model.network or {}
    local status = "Wi-Fi off"
    if network.connected then
        status = (network.validated and "Online · " or "Connected · ")
            .. (network.connected_ssid or "Wi-Fi")
    elseif network.wifi_enabled then
        status = network.activity ~= "" and network.activity or "Choose a network"
    end
    local rows = {
        group("row", { justify = "between", align = "center" }, {
            group("column", { gap = 3, grow = true }, {
                text("NETWORK", 11, sage),
                text(status, 12, network.validated and sage or dim, {
                    role = "status", label = "Network status", value = status,
                }, "network-status"),
            }),
            group("row", { padding = 9 }, { text("REFRESH", 10, sage) },
                0x213129, 11, "network-refresh", "wifi_refresh"),
        }),
    }
    if network.error then table.insert(rows, text(network.error, 12, 0xE07A6A)) end
    for index, item in ipairs(network.networks or {}) do
        if index > 6 then break end
        local suffix = item.saved and " · saved" or ""
        local security = item.security == "open" and "Open" or
            (item.security == "enterprise" and "Enterprise" or "Secured")
        local signal = string.rep("●", math.max(1, math.min(4, item.signal_level or 1)))
        table.insert(rows, group("row", { padding = 10, justify = "between", align = "center" }, {
            group("column", { gap = 2, grow = true }, {
                text(item.ssid, 13, cream), text(security .. suffix, 10, dim),
            }),
            text(signal, 10, sage),
        }, 0x213129, 12, "network-" .. tostring(index), "wifi_connect_" .. tostring(index)))
    end
    if network.connected then
        table.insert(rows, group("row", { padding = 9, justify = "center" }, {
            text("Disconnect", 12, 0xE07A6A),
        }, 0x213129, 12, "network-disconnect", "wifi_disconnect"))
    end
    return group("column", { gap = 8, padding = 14 }, rows, 0x18251F, 16, "network-panel")
end

local function has_agent_configuration_action(agent, action)
    for _, supported in ipairs(agent.configuration_actions or {}) do
        if supported == action then return true end
    end
    return false
end

local function agent_panel(model, state)
    local agent = model.agent or {}
    local children = {
        group("row", { justify = "between" }, {
            text("SHAPE THIS FLOW", 11, sage),
            text(agent.activity or (agent.available and "Ready" or "Agent unavailable"), 11, dim, {
                role = "status", label = "SOS agent status",
                value = agent.activity or (agent.available and "Ready" or "Agent unavailable"),
            }, "agent-status"),
        }),
    }
    local primary_controls = {}
    if has_agent_configuration_action(agent, "configure_openai") then
        table.insert(primary_controls,
            group("row", { padding = 8 }, { text("OPENAI", 10, sage) },
                0x213129, 10, "agent-configure", "agent_configure_openai"))
    end
    if has_agent_configuration_action(agent, "configure_openrouter") then
        table.insert(primary_controls,
            group("row", { padding = 8 }, { text("OPENROUTER", 10, sage) },
                0x213129, 10, "agent-configure-openrouter", "agent_configure_openrouter"))
    end
    if has_agent_configuration_action(agent, "configure_codex") then
        table.insert(primary_controls,
            group("row", { padding = 8 }, { text("CODEX SUB", 10, sage) },
                0x213129, 10, "agent-configure-codex", "agent_configure_codex"))
    end
    local secondary_controls = {}
    if has_agent_configuration_action(agent, "use_fake") then
        table.insert(secondary_controls,
            group("row", { padding = 8 }, { text("FAKE", 10, sage) },
                0x213129, 10, "agent-use-fake", "agent_use_fake"))
    end
    if has_agent_configuration_action(agent, "clear_credential") then
        table.insert(secondary_controls,
            group("row", { padding = 8 }, { text("REMOVE KEY", 10, 0xE07A6A) },
                0x213129, 10, "agent-clear-key", "agent_clear_credential"))
    end
    if #primary_controls > 0 then
        table.insert(children, group("row", { gap = 6 }, primary_controls))
    end
    if #secondary_controls > 0 then
        table.insert(children, group("row", { gap = 6 }, secondary_controls))
    end
    if #primary_controls == 0 and #secondary_controls == 0 then
        table.insert(children, text("Provider changes are managed before login.", 10, dim))
    end
    for index, message in ipairs(agent.messages or {}) do
        table.insert(children, group("column", { gap = 3, padding = 11 }, {
            text(message.role == "user" and "YOU" or "SOS", 10, amber),
            text(message.text, 13, cream, {
                role = "status", label = (message.role == "user" and "YOU" or "SOS") .. " message",
                value = message.text,
            }, "agent-message-" .. tostring(index)),
        }, 0x213129, 12))
    end
    if #(agent.messages or {}) == 0 then
        table.insert(children, text("Tell SOS how this experience should change.", 13, dim))
    end
    if agent.error then table.insert(children, text(agent.error, 12, 0xE07A6A)) end
    local draft = state.agent_draft or ""
    table.insert(children, {
        id = "agent-prompt",
        layout = { height = 44, padding = 9 },
        content = {
            kind = "text_session", state_key = "agent_draft", value = draft,
            placeholder = agent.busy and "SOS is working…" or "Change this flow…",
            submit_action = "agent_submit",
        },
        paint = {{ kind = "fill_bounds", color = 0x213129, radius = 14 }},
        semantics = {
            role = "text_field", label = "Ask SOS to change this experience", value = draft,
            hint = "Type a request and press enter",
        },
    })
    return group("column", { gap = 9, padding = 14 }, children, 0x18251F, 16, "agent-conversation")
end

local function render(model, state)
    local playing = state.playing
    if playing == nil then playing = model.music.playing end

    local children = {
        group("row", { align = "end", justify = "between" }, {
            group("column", { gap = 3 }, {
                text(model.greeting, 25, cream), text(model.date, 13, dim),
            }),
            group("column", { align = "end" }, {
                text(model.weather.temperature_c .. "°", 34, sage), text(model.weather.summary, 11, dim),
            }),
        }),
    }

    if playing then
        table.insert(children, group("row", { gap = 11, padding = 13, align = "center" }, {
            text("Ⅱ", 22, amber),
            group("column", { gap = 1, grow = true }, {
                text(model.music.title, 16, cream), text(model.music.artist, 12, dim),
            }),
            text("Tap to pause", 11, sage),
        }, 0x213129, 14, "music-playing", "toggle_music"))
    end

    table.insert(children, text("YOUR DAY, AS A FLOW", 11, sage))
    for index, event in ipairs(model.calendar) do
        table.insert(children, timeline_item(event.time, event.title, event.detail, index == #model.calendar))
    end
    table.insert(children, text("NOTES ALONG THE WAY", 11, sage))
    for _, note in ipairs(model.notes) do
        table.insert(children, group("column", { gap = 3, padding = 4 }, {
            text(note.title, 17, cream), text(note.preview, 13, dim),
        }))
    end
    table.insert(children, network_panel(model))
    table.insert(children, agent_panel(model, state))

    return group("column", { scroll_y = true }, {
        group("column", { padding = 22, gap = 18 }, children),
    }, night, 0, "timeflow-root")
end

local function update(model, state, event)
    if event.action == "toggle_music" then state.playing = false end
    if event.action == "text_changed" and event.target == "agent-prompt" then
        state.agent_draft = event.value
    elseif event.action == "agent_submit" and not (model.agent and model.agent.busy) then
        local prompt = event.value or state.agent_draft or ""
        if prompt ~= "" then
            state.agent_draft = ""
            return {
                state = state,
                effects = {{ provider = "agent", action = "prompt", payload = { prompt = prompt } }},
            }
        end
    elseif event.action == "agent_configure_openai" then
        return { state = state, effects = {{ provider = "agent", action = "configure_openai" }} }
    elseif event.action == "agent_configure_openrouter" then
        return { state = state, effects = {{ provider = "agent", action = "configure_openrouter" }} }
    elseif event.action == "agent_configure_codex" then
        return { state = state, effects = {{ provider = "agent", action = "configure_codex" }} }
    elseif event.action == "agent_use_fake" then
        return { state = state, effects = {{ provider = "agent", action = "use_fake" }} }
    elseif event.action == "agent_clear_credential" then
        return { state = state, effects = {{ provider = "agent", action = "clear_credential" }} }
    elseif event.action == "wifi_refresh" then
        return { state = state, effects = {{ provider = "network", action = "refresh" }} }
    elseif event.action == "wifi_disconnect" then
        return { state = state, effects = {{ provider = "network", action = "disconnect" }} }
    else
        local index = string.match(event.action or "", "^wifi_connect_(%d+)$")
        local selected = index and model.network and model.network.networks[tonumber(index)] or nil
        if selected then
            return { state = state, effects = {{
                provider = "network", action = "connect",
                payload = { ssid = selected.ssid, security = selected.security },
            }} }
        end
    end
    return state
end

return { api_version = 3, render = render, update = update }
