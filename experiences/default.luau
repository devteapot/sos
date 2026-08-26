--!strict
-- SOS Stock Base. This is an ordinary editable Scene ABI v3 revision. It uses
-- only the provider and interaction contracts available to generated Luau.
-- Credentials, lock, permission, emergency, confirmation, and Recovery remain
-- fixed native surfaces below the revision boundary.

local ink = 0x17211B
local muted = 0x637069
local paper = 0xF3F1E8
local white = 0xFEFDF8
local moss = 0x3E6B52
local mint = 0xDCE8D8
local amber = 0xE9A23B
local alert = 0x8C3A36
local rose = 0xF2DDD7
local sand = 0xE8E4D7
local panel = 0xECE9DC

local assets = {
    stock_mark = {
        kind = "svg",
        data = [[<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><circle cx="24" cy="24" r="22" fill="#3e6b52"/><circle cx="18" cy="20" r="6" fill="#fefdf8"/><circle cx="31" cy="17" r="4" fill="#dce8d8"/><path d="M12 34c7-9 17-10 25-4" fill="none" stroke="#fefdf8" stroke-width="4" stroke-linecap="round"/></svg>]],
    },
}

local workspace_order = {
    { id = "home", label = "HOME" },
    { id = "agenda", label = "AGENDA" },
    { id = "notes", label = "NOTES" },
    { id = "media", label = "MEDIA" },
    { id = "attention", label = "ATTENTION" },
    { id = "system", label = "SYSTEM" },
    { id = "apps", label = "APPS" },
    { id = "agent", label = "AGENT" },
}

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

local function button(label, action, tone, id, active)
    return group("row", { padding = 10, justify = "center", align = "center" }, {
        text(label, 11, active and white or (tone or moss)),
    }, active and moss or white, 12, id or action, action, label)
end

local function card(id, children, layout, background)
    layout = layout or {}
    layout.padding = layout.padding or 16
    layout.gap = layout.gap or 8
    return group("column", layout, children, background or white, 18, id)
end

local function heading(title, subtitle, id)
    local children = {
        text(title, 26, ink, { role = "header", label = title }, id),
    }
    if subtitle and subtitle ~= "" then
        table.insert(children, text(subtitle, 12, muted))
    end
    return group("column", { gap = 4 }, children)
end

local function empty_state(id, title, detail)
    return card(id, {
        text(title, 15, ink, { role = "status", label = title }, id .. "-status"),
        text(detail, 12, muted),
    }, { padding = 18, gap = 4 }, panel)
end

local function has_capability(providers, expected)
    for _, capability in ipairs(providers.capabilities or {}) do
        if capability == expected then return true end
    end
    return false
end

local function providers_available(providers)
    return (providers.abi_version or 0) > 0
end

local function active_workspace(state)
    local requested = state.active_workspace or "home"
    for _, workspace in ipairs(workspace_order) do
        if workspace.id == requested then return requested end
    end
    return "home"
end

local function stock_header(providers)
    local clock = providers.clock or {}
    local mark = {
        id = "stock-mark",
        layout = { width = 34, height = 34 },
        content = { kind = "image", asset = "stock_mark" },
        semantics = { role = "image", label = "SOS Stock Base" },
    }
    return group("row", { justify = "between", align = "center", gap = 12 }, {
        group("row", { align = "center", gap = 10 }, {
            mark,
            group("column", { gap = 1 }, {
                text("SOS", 18, ink, { role = "header", label = "SOS Stock Base" },
                    "stock-title"),
                text("A source-editable system experience", 10, muted),
            }),
        }),
        group("column", { align = "end", gap = 1 }, {
            text(clock.time_label ~= "" and clock.time_label or "--:--", 24, ink,
                { role = "status", label = "Current time", value = clock.time_label or "" },
                "stock-time"),
            text(clock.date_label ~= "" and clock.date_label or "Date unavailable", 10, muted),
        }),
    }, nil, 0, "stock-header")
end

local function workspace_navigation(state)
    local active = active_workspace(state)
    local children = {}
    for _, workspace in ipairs(workspace_order) do
        table.insert(children, button(workspace.label, "navigate_" .. workspace.id, moss,
            "workspace-nav-" .. workspace.id, active == workspace.id))
    end
    return group("row", { wrap = true, gap = 7 }, children, nil, 0, "workspace-navigation")
end

local function power_summary(providers)
    local power = providers.power or {}
    if not providers_available(providers) or power.battery_percent == nil then
        return "Power unavailable"
    end
    local suffix = power.charging == true and " · charging" or ""
    return tostring(power.battery_percent) .. "%" .. suffix
end

local function network_summary(providers)
    if not providers_available(providers) then return "Connectivity unavailable" end
    local connectivity = providers.connectivity or {}
    if not connectivity.connected then return "Offline" end
    local state = connectivity.validated and "Online" or "Connected"
    if connectivity.network_label ~= "" then state = state .. " · " .. connectivity.network_label end
    return state
end

local function audio_summary(providers)
    local audio = providers.audio or {}
    if not providers_available(providers) or audio.volume_percent == nil then
        return "Audio unavailable"
    end
    local suffix = audio.muted == true and " · muted" or ""
    return tostring(audio.volume_percent) .. "%" .. suffix
end

local function status_plane(providers)
    local clock = providers.clock or {}
    local facts = {
        { id = "stock-power", label = "POWER", value = power_summary(providers) },
        { id = "stock-network", label = "NETWORK", value = network_summary(providers) },
        { id = "stock-audio", label = "AUDIO", value = audio_summary(providers) },
    }
    local cards = {}
    for _, fact in ipairs(facts) do
        table.insert(cards, card(fact.id .. "-card", {
            text(fact.label, 10, moss),
            text(fact.value, 13, ink,
                { role = "status", label = fact.label, value = fact.value }, fact.id),
        }, { width = 420, padding = 13, gap = 3 }))
    end
    return group("column", { gap = 14, padding = 20, program = { measure_width = 1 } }, {
        group("row", { justify = "between", align = "end", gap = 14 }, {
            group("column", { gap = 3, grow = true }, {
                text(clock.time_label ~= "" and "Welcome back" or "Welcome", 13, moss),
                text("Your day, your tools, your source.", 27, ink),
            }),
            text(providers_available(providers) and "LIVE" or "DEGRADED", 10,
                providers_available(providers) and moss or amber),
        }),
        group("row", { wrap = true, gap = 9, program = { measure_width = 1 } }, cards),
    }, mint, 22, "status-plane")
end

local function agenda_preview(model)
    local events = model.calendar or {}
    local children = {
        group("row", { justify = "between", align = "center" }, {
            text("NEXT UP", 11, moss),
            text(tostring(#events) .. " events", 10, muted),
        }),
    }
    if #events == 0 then
        table.insert(children, text("No agenda items are available.", 13, muted))
    else
        local item = events[1]
        table.insert(children, text(item.time, 11, amber))
        table.insert(children, text(item.title, 18, ink))
        table.insert(children, text(item.detail, 12, muted))
    end
    table.insert(children, button("OPEN AGENDA", "navigate_agenda", moss, "home-open-agenda"))
    return card("home-agenda", children, { width = 760 })
end

local function notes_preview(model)
    local notes = model.notes or {}
    local children = {
        group("row", { justify = "between", align = "center" }, {
            text("NOTES & DOCUMENTS", 11, moss),
            text(tostring(#notes) .. " items", 10, muted),
        }),
    }
    if #notes == 0 then
        table.insert(children, text("No notes are available yet.", 13, muted))
    else
        table.insert(children, text(notes[1].title, 18, ink))
        table.insert(children, text(notes[1].preview, 12, muted))
    end
    table.insert(children, button("OPEN NOTES", "navigate_notes", moss, "home-open-notes"))
    return card("home-notes", children, { width = 760 })
end

local function current_media(model, providers)
    local media = ((providers.audio or {}).media or {})
    if providers_available(providers) and media.active then
        return media.title ~= "" and media.title or "Active media", media.artist or "", media.playing
    end
    local legacy = model.music or {}
    if legacy.title and legacy.title ~= "" then
        return legacy.title, legacy.artist or "", legacy.playing
    end
    return nil, nil, false
end

local function media_preview(model, providers)
    local title, artist, playing = current_media(model, providers)
    local children = { text("NOW PLAYING", 11, moss) }
    if title == nil then
        table.insert(children, text("No media session is available.", 13, muted))
    else
        table.insert(children, text(title, 18, ink))
        table.insert(children, text(artist or "", 12, muted))
        table.insert(children, text(playing and "Playing" or "Paused", 10, playing and moss or muted))
    end
    table.insert(children, button("OPEN MEDIA", "navigate_media", moss, "home-open-media"))
    return card("home-media", children, { width = 760 })
end

local function attention_preview(providers)
    local attention = providers.attention or {}
    local items = attention.items or {}
    local children = {
        group("row", { justify = "between", align = "center" }, {
            text("ATTENTION", 11, moss),
            text(tostring(attention.urgent_count or 0) .. " urgent", 10,
                (attention.urgent_count or 0) > 0 and alert or muted),
        }),
    }
    if not providers_available(providers) then
        table.insert(children, text("Attention provider unavailable.", 13, muted))
    elseif #items == 0 then
        table.insert(children, text("Nothing needs your attention.", 13, muted))
    else
        table.insert(children, text(items[1].title ~= "" and items[1].title or items[1].kind, 18, ink))
        table.insert(children, text(items[1].detail or "", 12, muted))
    end
    table.insert(children, button("OPEN CENTER", "navigate_attention", moss,
        "home-open-attention"))
    return card("home-attention", children, { width = 760 })
end

local function home_workspace(model, state, providers)
    return group("column", { gap = 16 }, {
        status_plane(providers),
        heading("Home", "A responsive workspace assembled entirely by this Luau revision.",
            "home-heading"),
        group("row", { wrap = true, gap = 14, program = { measure_width = 1 } }, {
            agenda_preview(model), notes_preview(model), media_preview(model, providers),
            attention_preview(providers),
        }, nil, 0, "home-responsive-grid"),
        group("row", { wrap = true, gap = 8 }, {
            button("SYSTEM CONTROLS", "navigate_system", moss, "home-open-system"),
            button("APPLICATIONS", "navigate_apps", moss, "home-open-apps"),
            button("CUSTOMIZE WITH AGENT", "navigate_agent", moss, "home-open-agent"),
        }),
    }, nil, 0, "workspace-home")
end

local function agenda_workspace(model, state)
    local rows = {
        heading("Agenda", "Calendar content stays behind a provider; the layout stays replaceable.",
            "agenda-heading"),
        {
            id = "agenda-composer",
            layout = { height = 46, padding = 10 },
            content = {
                kind = "text_session", state_key = "agenda_draft",
                value = state.agenda_draft or "", placeholder = "09:30 Design review",
                submit_action = "agenda_submit",
            },
            paint = {{ kind = "fill_bounds", color = white, radius = 14 }},
            semantics = {
                role = "text_field", label = "Add an agenda item",
                value = state.agenda_draft or "", hint = "Enter a time and title, then press enter",
            },
        },
        text(state.agenda_error or "Use HH:MM followed by a title.", 10,
            state.agenda_error and alert or muted, nil, "agenda-composer-status"),
    }
    local events = model.calendar or {}
    if #events == 0 then
        table.insert(rows, empty_state("agenda-empty", "No events yet",
            "Your calendar is empty or its provider has no readable items."))
    end
    for index, event in ipairs(events) do
        if index > 32 then break end
        table.insert(rows, card("agenda-event-" .. tostring(index), {
            group("row", { justify = "between", align = "center", gap = 12 }, {
                text(event.time, 11, amber),
                text(index == 1 and "NEXT" or "", 9, moss),
            }),
            text(event.title, 17, ink),
            text(event.detail, 12, muted),
        }))
    end
    return group("column", { gap = 10 }, rows, nil, 0, "workspace-agenda")
end

local function notes_workspace(model, state)
    local note_status = state.note_status or "The first line becomes the note title."
    local rows = {
        heading("Notes & documents", "Quick capture backed by the notes provider.", "notes-heading"),
        {
            id = "note-composer",
            layout = { height = 58, padding = 10 },
            content = {
                kind = "text_session", state_key = "note_draft", value = state.note_draft or "",
                placeholder = "Write a quick note…", submit_action = "note_submit",
            },
            paint = {{ kind = "fill_bounds", color = white, radius = 14 }},
            semantics = {
                role = "text_field", label = "Write a note", value = state.note_draft or "",
                hint = "Write a note and press enter to save it",
            },
        },
        text(note_status, 10, state.note_error and alert or muted,
            { role = "status", label = note_status }, "note-composer-status"),
    }
    local notes = model.notes or {}
    if #notes == 0 then
        table.insert(rows, empty_state("notes-empty", "No notes yet",
            "Capture a thought above, or connect a notes provider."))
    end
    local cards = {}
    for index, note in ipairs(notes) do
        if index > 32 then break end
        table.insert(cards, card("note-" .. tostring(index), {
            text(note.title, 16, ink),
            text(note.preview, 12, muted),
        }, { width = 520 }))
    end
    if #cards > 0 then
        table.insert(rows, group("row", {
            wrap = true, gap = 10, program = { measure_width = 1 },
        }, cards,
            nil, 0, "notes-responsive-grid"))
    end
    return group("column", { gap = 10 }, rows, nil, 0, "workspace-notes")
end

local function media_controls(model, providers)
    local media = ((providers.audio or {}).media or {})
    local canonical = providers_available(providers) and media.active
    local legacy = (model.music or {}).title ~= ""
    local controls = {}
    if canonical then
        if has_capability(providers, "media_previous") then
            table.insert(controls, button("PREVIOUS", "media_previous", moss))
        end
        if has_capability(providers, "media_play_pause") then
            table.insert(controls, button(media.playing and "PAUSE" or "PLAY", "media_play_pause", moss))
        end
        if has_capability(providers, "media_next") then
            table.insert(controls, button("NEXT", "media_next", moss))
        end
    elseif legacy then
        table.insert(controls, button("PREVIOUS", "media_previous", moss))
        table.insert(controls, button((model.music or {}).playing and "PAUSE" or "PLAY",
            "media_play_pause", moss))
        table.insert(controls, button("NEXT", "media_next", moss))
    end
    return controls
end

local function media_workspace(model, providers)
    local title, artist, playing = current_media(model, providers)
    local children = {
        heading("Media", "MPRIS and platform media sessions stay behind one typed contract.",
            "media-heading"),
    }
    if title == nil then
        table.insert(children, empty_state("media-unavailable", "No media session",
            "Start playback in a compatible application to make controls available."))
    else
        table.insert(children, card("media-now-playing", {
            text(playing and "NOW PLAYING" or "PAUSED", 10, playing and moss or muted),
            text(title, 28, ink),
            text(artist or "", 14, muted),
        }, { padding = 24, gap = 5 }, mint))
        local controls = media_controls(model, providers)
        if #controls > 0 then
            table.insert(children, group("row", { wrap = true, gap = 8 }, controls,
                nil, 0, "media-transport-controls"))
        else
            table.insert(children, empty_state("media-controls-unavailable", "Controls unavailable",
                "The active provider exposes metadata but no transport authority."))
        end
    end
    return group("column", { gap = 12 }, children, nil, 0, "workspace-media")
end

local function attention_workspace(providers)
    local attention = providers.attention or {}
    local rows = {
        heading("Attention center", "Notifications, alarms, calls, and background work in one place.",
            "attention-heading"),
    }
    local items = attention.items or {}
    if not providers_available(providers) then
        table.insert(rows, empty_state("attention-unavailable", "Attention provider unavailable",
            "This system has not exposed the attention contract."))
    elseif #items == 0 then
        table.insert(rows, empty_state("attention-empty", "Nothing needs attention",
            "New notifications and urgent activity will appear here."))
    end
    for index, item in ipairs(items) do
        if index > 32 then break end
        local title = item.title ~= "" and item.title or item.kind
        local can_acknowledge = has_capability(providers, "attention_acknowledge")
        local children = {
            group("row", { justify = "between", gap = 10 }, {
                text(item.source, 10, item.urgent and alert or moss),
                text(item.urgent and "URGENT" or string.upper(item.kind), 10,
                    item.urgent and alert or muted),
            }),
            text(title, 16, ink),
            text(item.detail or "", 12, muted),
        }
        if can_acknowledge then
            table.insert(children, button("ACKNOWLEDGE", "attention_ack_" .. tostring(index), moss,
                "attention-acknowledge-" .. tostring(index)))
        else
            table.insert(children, text("Acknowledgement unavailable", 10, muted))
        end
        table.insert(rows, card("attention-" .. tostring(index), children, nil,
            item.urgent and rose or white))
    end
    return group("column", { gap = 10 }, rows, nil, 0, "workspace-attention")
end

local function system_workspace(providers)
    local rows = {
        heading("System controls", "Connectivity, power, audio, and media through provider effects.",
            "system-heading"),
    }
    if not providers_available(providers) then
        table.insert(rows, empty_state("system-providers-unavailable", "System providers unavailable",
            "Stock Base remains navigable while system controls are disabled."))
        return group("column", { gap = 10 }, rows, nil, 0, "workspace-system")
    end

    local power = providers.power or {}
    table.insert(rows, card("system-power", {
        text("POWER", 10, moss),
        text(power_summary(providers), 20, ink),
        text(power.thermal_status and "Thermal state · " .. tostring(power.thermal_status)
            or "Thermal state unavailable", 11, muted),
        text(power.charging_source ~= "" and "Source · " .. power.charging_source
            or "Charging source unavailable", 11, muted),
    }))

    local audio = providers.audio or {}
    local audio_children = {
        text("AUDIO", 10, moss), text(audio_summary(providers), 20, ink),
    }
    local audio_buttons = {}
    if has_capability(providers, "audio_set_volume") then
        table.insert(audio_buttons, button("VOLUME −", "audio_volume_down", moss))
        table.insert(audio_buttons, button("VOLUME +", "audio_volume_up", moss))
    end
    if has_capability(providers, "audio_set_muted") then
        table.insert(audio_buttons, button(audio.muted == true and "UNMUTE" or "MUTE",
            "audio_toggle_mute", moss))
    end
    if #audio_buttons > 0 then
        table.insert(audio_children, group("row", { wrap = true, gap = 7 }, audio_buttons))
    else
        table.insert(audio_children, text("Audio controls unavailable", 11, muted))
    end
    table.insert(rows, card("system-audio", audio_children))

    local connectivity = providers.connectivity or {}
    local network_children = {
        text("CONNECTIVITY", 10, moss), text(network_summary(providers), 20, ink),
    }
    if connectivity.connected and has_capability(providers, "wifi_disconnect") then
        table.insert(network_children, button("DISCONNECT WI-FI", "wifi_disconnect", alert))
    end
    local networks = connectivity.wifi_networks or {}
    if #networks == 0 then
        table.insert(network_children, text(connectivity.wifi_enabled and "No saved networks found"
            or "Wi-Fi is unavailable", 11, muted))
    end
    for index, network in ipairs(networks) do
        if index > 12 then break end
        local can_connect = not network.connected and has_capability(providers, "wifi_connect")
        local suffix = network.connected and "CURRENT" or (network.saved and "SAVED" or "AVAILABLE")
        table.insert(network_children,
            group("row", { padding = 12, justify = "between", align = "center", gap = 8 }, {
                group("column", { gap = 2, grow = true }, {
                    text(network.label, 14, ink),
                    text(string.rep("●", math.max(1, math.min(4, network.signal_level or 1))),
                        9, muted),
                }),
                text(can_connect and "CONNECT" or suffix, 10, can_connect and moss or muted),
            }, panel, 13, "wifi-network-" .. tostring(index),
                can_connect and "wifi_connect_" .. tostring(index) or nil,
                can_connect and "Connect to " .. network.label or nil))
    end
    table.insert(rows, card("system-connectivity", network_children))
    return group("column", { gap = 10 }, rows, nil, 0, "workspace-system")
end

local function apps_workspace(providers)
    local rows = {
        heading("Applications", "Only provider-approved, compatible launch targets appear here.",
            "apps-heading"),
    }
    local applications = ((providers.apps or {}).compatible or {})
    if not providers_available(providers) then
        table.insert(rows, empty_state("apps-unavailable", "Application provider unavailable",
            "Launching is disabled, but the rest of Stock Base remains usable."))
    elseif #applications == 0 then
        table.insert(rows, empty_state("apps-empty", "No compatible applications",
            "Eligible applications will appear when the provider discovers them."))
    end
    local cards = {}
    for index, application in ipairs(applications) do
        if index > 48 then break end
        local can_launch = has_capability(providers, "app_launch")
        table.insert(cards, group("row", {
            width = 360, padding = 15, justify = "between", align = "center",
        }, {
            text(application.label, 15, ink),
            text(can_launch and "OPEN" or "UNAVAILABLE", 10, can_launch and moss or muted),
        }, white, 15, "application-" .. tostring(index),
            can_launch and "app_launch_" .. tostring(index) or nil,
            can_launch and "Open " .. application.label or nil))
    end
    if #cards > 0 then
        table.insert(rows, group("row", {
            wrap = true, gap = 9, program = { measure_width = 1 },
        }, cards,
            nil, 0, "application-responsive-grid"))
    end
    return group("column", { gap = 10 }, rows, nil, 0, "workspace-apps")
end

local function has_agent_configuration_action(agent, action)
    for _, supported in ipairs(agent.configuration_actions or {}) do
        if supported == action then return true end
    end
    return false
end

local function agent_workspace(model, state)
    local agent = model.agent or {}
    local children = {
        heading("Agent", "Conversation is part of the revision, while credentials stay native.",
            "agent-heading"),
        group("row", { justify = "between", align = "center" }, {
            text("CONVERSATION", 11, moss),
            text(agent.activity ~= "" and agent.activity
                or (agent.available and "Ready" or "Unavailable"), 10, muted),
        }),
    }
    local messages = {}
    for index, message in ipairs(agent.messages or {}) do
        if index > 24 then break end
        table.insert(messages, card("agent-message-" .. tostring(index), {
            text(message.role == "user" and "YOU" or "SOS", 10, moss),
            text(message.text, 13, ink),
        }, { padding = 12, gap = 3 }, message.role == "user" and sand or mint))
    end
    if #messages == 0 then
        table.insert(messages, empty_state("agent-empty", "Make Stock Base yours",
            "Ask the agent to replace navigation, data composition, visuals, or the whole source."))
    end
    table.insert(children, group("column", { gap = 7 }, messages, nil, 0, "agent-messages"))
    local draft = state.agent_draft or ""
    table.insert(children, {
        id = "agent-prompt",
        layout = { height = 50, padding = 10 },
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
    local controls = {}
    local configurations = {
        { capability = "configure_openai", label = "OPENAI", action = "agent_configure_openai" },
        { capability = "configure_openrouter", label = "OPENROUTER", action = "agent_configure_openrouter" },
        { capability = "configure_codex", label = "CODEX", action = "agent_configure_codex" },
        { capability = "use_fake", label = "OFFLINE", action = "agent_use_fake" },
        { capability = "clear_credential", label = "CLEAR CREDENTIAL", action = "agent_clear_credential" },
    }
    for _, configuration in ipairs(configurations) do
        if has_agent_configuration_action(agent, configuration.capability) then
            table.insert(controls, button(configuration.label, configuration.action,
                configuration.capability == "clear_credential" and alert or moss))
        end
    end
    if #controls > 0 then
        table.insert(children, group("row", { wrap = true, gap = 7 }, controls,
            nil, 0, "agent-provider-controls"))
    else
        table.insert(children, text("Provider changes are managed before login.", 10, muted))
    end
    if agent.error then table.insert(children, text(agent.error, 11, alert)) end
    return group("column", { gap = 10 }, children, nil, 0, "workspace-agent")
end

local function render_workspace(model, state, providers)
    local active = active_workspace(state)
    if active == "agenda" then return agenda_workspace(model, state) end
    if active == "notes" then return notes_workspace(model, state) end
    if active == "media" then return media_workspace(model, providers) end
    if active == "attention" then return attention_workspace(providers) end
    if active == "system" then return system_workspace(providers) end
    if active == "apps" then return apps_workspace(providers) end
    if active == "agent" then return agent_workspace(model, state) end
    return home_workspace(model, state, providers)
end

local function render(model, state)
    local providers = model.providers or {}
    return group("column", { scroll_y = true }, {
        group("column", {
            padding = 22, gap = 16, max_width = 1876,
            program = { measure_width = 1 },
        }, {
            stock_header(providers),
            workspace_navigation(state),
            render_workspace(model, state, providers),
            group("row", { justify = "between", gap = 12 }, {
                text("STOCK BASE · EDITABLE LUAU REVISION", 9, muted),
                text("SYSTEM PROVIDERS ABI " .. tostring(providers.abi_version or 0), 9, muted),
            }),
        }, nil, 0, "stock-content-frame"),
    }, paper, 0, "stock-experience-root")
end

local function provider_effect(provider, action, payload, state)
    return { state = state, effects = {{ provider = provider, action = action, payload = payload }} }
end

local function update(model, state, event)
    local providers = model.providers or {}
    local audio = providers.audio or {}
    local connectivity = providers.connectivity or {}
    local action = event.action or ""

    local destination = string.match(action, "^navigate_([a-z]+)$")
    if destination then
        for _, workspace in ipairs(workspace_order) do
            if workspace.id == destination then
                state.active_workspace = destination
                return state
            end
        end
    end

    if action == "audio_volume_down" then
        return provider_effect("audio", "adjust_volume", { delta = -10 }, state)
    elseif action == "audio_volume_up" then
        return provider_effect("audio", "adjust_volume", { delta = 10 }, state)
    elseif action == "audio_toggle_mute" then
        return provider_effect("audio", "set_muted", { muted = not (audio.muted == true) }, state)
    elseif action == "media_play_pause" or action == "media_next" or action == "media_previous" then
        local media = audio.media or {}
        if providers_available(providers) and media.active then
            local canonical = action == "media_play_pause" and "play_pause"
                or (action == "media_next" and "next" or "previous")
            return provider_effect("media", canonical, nil, state)
        end
        local legacy = action == "media_play_pause" and "play-pause"
            or (action == "media_next" and "next" or "previous")
        return provider_effect("music", "command", { command = legacy }, state)
    elseif action == "wifi_disconnect" then
        return provider_effect("network", "disconnect", nil, state)
    elseif action == "text_changed" and event.target == "agent-prompt" then
        state.agent_draft = event.value or ""
        return state
    elseif action == "text_changed" and event.target == "agenda-composer" then
        state.agenda_draft = event.value or ""
        state.agenda_error = nil
        return state
    elseif action == "text_changed" and event.target == "note-composer" then
        state.note_draft = event.value or ""
        state.note_error = nil
        state.note_status = nil
        return state
    elseif action == "agenda_submit" then
        local draft = event.value or state.agenda_draft or ""
        local time, title = string.match(draft, "^%s*(%d%d?:%d%d)%s+(.+)%s*$")
        if not time or not title then
            state.agenda_error = "Use HH:MM followed by a title."
            return state
        end
        state.agenda_draft = ""
        state.agenda_error = nil
        state.agenda_sequence = (state.agenda_sequence or 0) + 1
        local stamp = tostring(((providers.clock or {}).unix_time_ms or 0))
        return provider_effect("calendar", "append", {
            name = "stock-event-" .. stamp .. "-" .. tostring(state.agenda_sequence),
            time = time, title = title, detail = "Created from SOS Stock Base",
        }, state)
    elseif action == "note_submit" then
        local draft = event.value or state.note_draft or ""
        draft = string.match(draft, "^%s*(.-)%s*$") or ""
        if draft == "" then
            state.note_error = true
            state.note_status = "Write something before saving."
            return state
        end
        local title = string.match(draft, "^([^\n]+)") or "Quick note"
        if string.len(title) > 64 then title = string.sub(title, 1, 64) end
        state.note_draft = ""
        state.note_error = nil
        state.note_status = "Saved “" .. title .. "”."
        state.note_sequence = (state.note_sequence or 0) + 1
        local stamp = tostring(((providers.clock or {}).unix_time_ms or 0))
        return provider_effect("notes", "write", {
            name = "stock-note-" .. stamp .. "-" .. tostring(state.note_sequence),
            content = "# " .. title .. "\n\n" .. draft,
        }, state)
    elseif action == "agent_submit" and not ((model.agent or {}).busy) then
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
    elseif action == "agent_clear_credential" then
        return provider_effect("agent", "clear_credential", nil, state)
    end

    local network_index = string.match(action, "^wifi_connect_(%d+)$")
    local network = network_index and (connectivity.wifi_networks or {})[tonumber(network_index)] or nil
    if network then
        return provider_effect("network", "connect", { network_id = network.id }, state)
    end
    local app_index = string.match(action, "^app_launch_(%d+)$")
    local application = app_index and (((providers.apps or {}).compatible or {})[tonumber(app_index)]) or nil
    if application then
        return provider_effect("apps", "launch", { app_id = application.id }, state)
    end
    local attention_index = string.match(action, "^attention_ack_(%d+)$")
    local item = attention_index and (((providers.attention or {}).items or {})[tonumber(attention_index)]) or nil
    if item then
        return provider_effect("attention", "acknowledge", { attention_id = item.id }, state)
    end
    return state
end

return {
    api_version = 3,
    state_version = 1,
    assets = assets,
    render = render,
    update = update,
}
