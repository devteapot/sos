--!strict
-- Stateful primitive gate: native IME input, image, animation, focus, and
-- accessibility semantics remain expressed as bounded data.

local ink = 0x171A21
local paper = 0xF5F0E7
local muted = 0x666B73
local coral = 0xE76F51
local white = 0xFFFFFF

local function text(value, size, color, semantics, id)
    return {
        id = id,
        content = { kind = "text", value = tostring(value), size = size, color = color or ink },
        semantics = semantics,
    }
end

local function group(flow, layout, children, options)
    options = options or {}
    layout = layout or {}
    layout.flow = flow
    return {
        id = options.id,
        layout = layout,
        paint = options.background and {{
            kind = "fill_bounds", color = options.background, radius = options.radius or 0,
        }} or nil,
        interaction = options.tap_action and { tap_action = options.tap_action } or nil,
        animation = options.animation,
        semantics = options.semantics,
        children = children,
    }
end

local function agent_panel(model, state)
    local agent = model.agent or {}
    local messages = {}
    for index, message in ipairs(agent.messages or {}) do
        table.insert(messages, group("column", { gap = 3, padding = 11 }, {
            text(message.role == "user" and "YOU" or "SOS", 10, coral),
            text(message.text, 13, ink, {
                role = "status", label = (message.role == "user" and "YOU" or "SOS") .. " message",
                value = message.text,
            }, "agent-message-" .. tostring(index)),
        }, { background = message.role == "user" and 0xF4E6DD or 0xE8EEE8, radius = 13 }))
    end
    if #messages == 0 then
        table.insert(messages, text("Describe a different rhythm, layout, or behavior.", 13, muted))
    end
    if agent.error then table.insert(messages, text(agent.error, 12, 0x9A3D36)) end
    local draft = state.agent_draft or ""
    table.insert(messages, {
        id = "agent-prompt",
        layout = { height = 44, padding = 9 },
        content = {
            kind = "text_session", state_key = "agent_draft", value = draft,
            placeholder = agent.busy and "SOS is working…" or "Change this experience…",
            submit_action = "agent_submit",
        },
        paint = {{ kind = "fill_bounds", color = 0xF0ECE4, radius = 12 }},
        semantics = {
            role = "text_field", label = "Ask SOS to change this experience", value = draft,
            hint = "Type a request and press enter",
        },
    })
    return group("column", { gap = 9, padding = 16 }, {
        group("row", { justify = "between" }, {
            text("MAKE IT YOURS", 12, coral),
            text(agent.activity or (agent.available and "Ready" or "Agent unavailable"), 11, muted, {
                role = "status", label = "SOS agent status",
                value = agent.activity or (agent.available and "Ready" or "Agent unavailable"),
            }, "agent-status"),
        }),
        group("row", { gap = 6 }, {
            group("row", { padding = 8 }, { text("OPENAI", 10, coral) }, {
                id = "agent-configure", background = 0xF0ECE4, radius = 10,
                tap_action = "agent_configure_openai",
            }),
            group("row", { padding = 8 }, { text("FAKE", 10, coral) }, {
                id = "agent-use-fake", background = 0xF0ECE4, radius = 10,
                tap_action = "agent_use_fake",
            }),
            group("row", { padding = 8 }, { text("REMOVE KEY", 10, 0x9A3D36) }, {
                id = "agent-clear-key", background = 0xF0ECE4, radius = 10,
                tap_action = "agent_clear_credential",
            }),
        }),
        group("column", { gap = 7 }, messages),
    }, { id = "agent-conversation", background = white, radius = 20 })
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
                text("NETWORK", 11, coral),
                text(status, 12, network.validated and coral or muted, {
                    role = "status", label = "Network status", value = status,
                }, "network-status"),
            }),
            group("row", { padding = 8 }, { text("REFRESH", 10, coral) }, {
                id = "network-refresh", background = 0xF0ECE4, radius = 10,
                tap_action = "wifi_refresh",
            }),
        }),
    }
    if network.error then table.insert(rows, text(network.error, 12, 0x9A3D36)) end
    for index, item in ipairs(network.networks or {}) do
        if index > 6 then break end
        local suffix = item.saved and " · saved" or ""
        local security = item.security == "open" and "Open" or
            (item.security == "enterprise" and "Enterprise" or "Secured")
        local signal = string.rep("●", math.max(1, math.min(4, item.signal_level or 1)))
        table.insert(rows, group("row", { padding = 10, justify = "between", align = "center" }, {
            group("column", { gap = 2, grow = true }, {
                text(item.ssid, 13, ink), text(security .. suffix, 10, muted),
            }),
            text(signal, 10, coral),
        }, {
            id = "network-" .. tostring(index), background = 0xF0ECE4, radius = 12,
            tap_action = "wifi_connect_" .. tostring(index),
        }))
    end
    if network.connected then
        table.insert(rows, group("row", { padding = 8, justify = "center" }, {
            text("Disconnect", 12, 0x9A3D36),
        }, {
            id = "network-disconnect", background = 0xF0ECE4, radius = 10,
            tap_action = "wifi_disconnect",
        }))
    end
    return group("column", { gap = 8, padding = 14 }, rows, {
        id = "network-panel", background = white, radius = 20,
    })
end

local function render(model, state)
    local draft = state.draft
    if draft == nil then draft = "Caffè ☕️ – 明日のデザイン" end
    local playing = state.playing
    if playing == nil then playing = model.music.playing end

    local agenda = {}
    for _, event in ipairs(model.calendar) do
        table.insert(agenda, group("row", { gap = 14, padding = 12 }, {
            text(event.time, 13, coral),
            group("column", { gap = 2, grow = true }, {
                text(event.title, 17, ink), text(event.detail, 12, muted),
            }),
        }))
    end

    local provider_surfaces = {}
    for _, surface in ipairs(model.surfaces or {}) do
        table.insert(provider_surfaces, group("column", { gap = 6, width = 220 }, {
            {
                id = "provider-surface-" .. surface.id,
                layout = { width = 220, height = 124 },
                content = { kind = "provider_surface", surface = surface.id },
                semantics = {
                    role = "image",
                    label = surface.kind .. " surface " .. surface.id,
                    value = surface.status,
                },
            },
            text(surface.kind .. " · " .. surface.status, 11, muted),
        }))
    end
    local system = model.system or {}
    local audio = system.audio_volume_percent
    local system_summary = tostring(#(system.online_interfaces or {})) .. " network · "
        .. tostring(#(system.connected_displays or {})) .. " display · "
        .. (audio and (tostring(audio) .. "% audio") or "audio unavailable")

    return group("column", { scroll_y = true }, {
        group("column", { padding = 22, gap = 18 }, {
            text("A living day", 30, ink, {
                role = "header", label = "A living day",
            }, "living-day-header"),
            group("row", { gap = 14, align = "center" }, {
                {
                    id = "album-art",
                    layout = { width = 76, height = 76 },
                    content = { kind = "image", asset = "album-orbit" },
                    paint = {{ kind = "fill_bounds", color = white, radius = 22 }},
                    animation = { kind = "pulse", duration_ms = 1800, loop = true },
                    semantics = { role = "image", label = "Orbit album artwork" },
                },
                group("column", { gap = 4, grow = true }, {
                    text(model.music.title, 19, ink), text(model.music.artist, 13, muted),
                    group("row", { padding = 10 }, {
                        text(playing and "PAUSE" or "PLAY", 12, white),
                    }, {
                        id = "music-toggle",
                        background = ink,
                        radius = 14,
                        tap_action = "toggle_music",
                        semantics = {
                            role = "button",
                            label = playing and "Pause music" or "Play music",
                            value = playing and "Playing" or "Paused",
                            hint = "Double tap to change playback",
                        },
                    }),
                }),
            }),
            group("row", { gap = 12 }, provider_surfaces),
            group("column", { gap = 8, padding = 16 }, {
                text("Capture a thought", 18, ink),
                {
                    id = "note-draft",
                    layout = { height = 38, padding = 7 },
                    content = {
                        kind = "text_session", state_key = "draft", value = draft,
                        placeholder = "Write a note…", submit_action = "save_note", autofocus = true,
                    },
                    paint = {{ kind = "fill_bounds", color = 0xF0ECE4, radius = 10 }},
                    semantics = {
                        role = "text_field", label = "Note draft", value = draft,
                        hint = "Edit a note and press enter to save",
                    },
                },
                text(state.last_saved and "Saved without leaving the flow" or "Draft saves while you type", 12, muted, {
                    role = "status", label = "Note status",
                    value = state.last_saved and "Saved" or "Editing",
                }, "note-status"),
            }, { background = white, radius = 20 }),
            text(model.weather.temperature_c .. "° · " .. model.weather.summary, 18, ink, {
                role = "status", label = "Weather",
                value = model.weather.temperature_c .. " degrees, " .. model.weather.summary,
            }, "weather-status"),
            text(system_summary, 12, muted, {
                role = "status", label = "Linux system state", value = system_summary,
            }, "system-status"),
            text("TODAY", 11, muted),
            group("column", { gap = 6 }, agenda),
            network_panel(model),
            agent_panel(model, state),
        }),
    }, { id = "daily-flow-root", background = paper })
end

local function update(model, state, event)
    if event.action == "text_changed" and event.target == "note-draft" then
        state.draft = event.value
    elseif event.action == "text_changed" and event.target == "agent-prompt" then
        state.agent_draft = event.value
    elseif event.action == "save_note" then
        state.draft = event.value or state.draft
        state.last_saved = true
    elseif event.action == "toggle_music" then
        if state.playing == nil then state.playing = not model.music.playing
        else state.playing = not state.playing end
    elseif event.action == "focus_changed" then
        state.editor_focused = event.focused
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
