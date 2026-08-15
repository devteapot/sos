--!strict
-- The source is interpreted on-device. Rust owns data, validation, rendering,
-- persistence and rollback; this module owns composition and local UI behavior.

local ink = 0x17211B
local muted = 0x637069
local paper = 0xF3F1E8
local white = 0xFEFDF8
local moss = 0x3E6B52
local mint = 0xDCE8D8
local sun = 0xE9A23B

local function text(value, size, color, semantics, id)
    return {
        id = id,
        content = { kind = "text", value = tostring(value), size = size, color = color or ink },
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

local function event_row(event)
    return group("row", { gap = 16, padding = 14 }, {
        text(event.time, 14, moss),
        group("column", { gap = 3, grow = true }, {
            text(event.title, 17, ink),
            text(event.detail, 13, muted),
        }),
    }, white, 18)
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
                text("NETWORK", 12, moss),
                text(status, 13, network.validated and moss or muted, {
                    role = "status", label = "Network status", value = status,
                }, "network-status"),
            }),
            group("row", { padding = 10 }, { text("REFRESH", 10, moss) },
                mint, 12, "network-refresh", "wifi_refresh"),
        }),
    }
    if network.error then table.insert(rows, text(network.error, 12, 0x8C3A36)) end
    for index, item in ipairs(network.networks or {}) do
        if index > 6 then break end
        local suffix = item.saved and " · saved" or ""
        local security = item.security == "open" and "Open" or
            (item.security == "enterprise" and "Enterprise" or "Secured")
        local signal = string.rep("●", math.max(1, math.min(4, item.signal_level or 1)))
        table.insert(rows, group("row", { padding = 11, justify = "between", align = "center" }, {
            group("column", { gap = 2, grow = true }, {
                text(item.ssid, 14, ink), text(security .. suffix, 10, muted),
            }),
            text(signal, 10, moss),
        }, white, 13, "network-" .. tostring(index), "wifi_connect_" .. tostring(index)))
    end
    if network.connected then
        table.insert(rows, group("row", { padding = 10, justify = "center" }, {
            text("Disconnect", 12, 0x8C3A36),
        }, 0xE8E4D7, 12, "network-disconnect", "wifi_disconnect"))
    end
    return group("column", { gap = 8, padding = 16 }, rows, 0xECE9DC, 20, "network-panel")
end

local function agent_panel(model, state)
    local agent = model.agent or {}
    local messages = {}
    for index, message in ipairs(agent.messages or {}) do
        local speaker = message.role == "user" and "YOU" or "SOS"
        table.insert(messages, group("column", { gap = 3, padding = 12 }, {
            text(speaker, 10, moss), text(message.text, 14, ink, {
                role = "status", label = speaker .. " message", value = message.text,
            }, "agent-message-" .. tostring(index)),
        }, message.role == "user" and 0xE8E4D7 or mint, 14))
    end
    if #messages == 0 then
        table.insert(messages, text("Describe what you want this experience to become.", 13, muted))
    end
    if agent.error then table.insert(messages, text(agent.error, 12, 0x8C3A36)) end
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
    return group("column", { gap = 9, padding = 16 }, {
        group("row", { justify = "between", align = "center" }, {
            text("MAKE IT YOURS", 12, moss),
            text(agent.activity or (agent.available and "Ready" or "Agent unavailable"), 11, muted, {
                role = "status", label = "SOS agent status",
                value = agent.activity or (agent.available and "Ready" or "Agent unavailable"),
            }, "agent-status"),
        }),
        group("column", { gap = 7 }, messages),
    }, 0xECE9DC, 20, "agent-conversation")
end

local function render(model, state)
    local playing = state.playing
    if playing == nil then playing = model.music.playing end

    local agenda = {}
    for _, event in ipairs(model.calendar) do table.insert(agenda, event_row(event)) end

    local notes = {}
    for _, note in ipairs(model.notes) do
        table.insert(notes, group("column", { gap = 4, padding = 14 }, {
            text(note.title, 16, ink), text(note.preview, 13, muted),
        }, 0xECE9DC, 16))
    end

    local sections = {
        group("column", { gap = 4 }, {
            text(model.greeting, 27, ink), text(model.date, 14, muted),
        }),
        group("row", { padding = 18, gap = 14, align = "center" }, {
            text(tostring(model.weather.temperature_c) .. "°", 42, moss),
            group("column", { gap = 3 }, {
                text(model.weather.summary, 16, ink),
                text("High " .. model.weather.high_c .. "°  ·  Low " .. model.weather.low_c .. "°", 13, muted),
            }),
        }, mint, 22),
        text("TODAY", 12, moss),
        group("column", { gap = 9 }, agenda),
        text("NOTES", 12, moss),
        group("column", { gap = 9 }, notes),
        network_panel(model),
        agent_panel(model, state),
    }

    if playing then
        table.insert(sections, group("row", { padding = 16, gap = 12, align = "center" }, {
            group("overlay", { width = 42, height = 42 }, nil, sun, 21),
            group("column", { gap = 2, grow = true }, {
                text(model.music.title, 16, white), text(model.music.artist, 13, 0xB8C1BA),
            }),
            text("PAUSE", 11, white),
        }, ink, 22, "music-playing", "toggle_music"))
    else
        table.insert(sections, group("row", { padding = 13, justify = "center" }, {
            text("Resume music", 14, ink),
        }, 0xE5E2D6, 18, "music-paused", "toggle_music"))
    end

    return group("column", { scroll_y = true }, {
        group("column", { padding = 22, gap = 18 }, sections),
    }, paper, 0, "experience-root")
end

local function update(model, state, event)
    if event.action == "toggle_music" then
        if state.playing == nil then state.playing = false else state.playing = not state.playing end
    elseif event.action == "text_changed" and event.target == "agent-prompt" then
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
