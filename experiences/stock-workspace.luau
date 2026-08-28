--!strict
-- SOS Stock Workspace. This ordinary Experience API v4 package is mounted by
-- the privileged Stock Shell and can be replaced without replacing shell
-- authority, compositor integration, the launcher, or the agent HUD.
-- Credentials, lock, permission, emergency, confirmation, and Recovery remain
-- fixed native surfaces below the revision boundary.

-- Visual implementation remains revision-local. The authority supplies only
-- typed semantic appearance values, which are resolved onto these fallbacks.
local theme = require("stock.theme")

local ink = theme.color.ink
local muted = theme.color.muted
local paper = theme.color.paper
local white = theme.color.white
local moss = theme.color.moss
local mint = theme.color.mint
local amber = theme.color.amber
local alert = theme.color.alert
local rose = theme.color.rose
local sand = theme.color.sand
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
    if type(value) ~= "string" or not string.match(value, "^#[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]$") then
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
    white = appearance_color(appearance, "surface.primary", theme.color.white)
    moss = appearance_color(appearance, "accent.primary", theme.color.moss)
    mint = appearance_color(appearance, "accent.subtle", theme.color.mint)
    amber = appearance_color(appearance, "status.warning", theme.color.amber)
    alert = appearance_color(appearance, "status.danger", theme.color.alert)
    rose = appearance_color(appearance, "status.danger_subtle", theme.color.rose)
    sand = appearance_color(appearance, "surface.secondary", theme.color.sand)
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
    { id = "home", label = "HOME" },
    { id = "agenda", label = "AGENDA" },
    { id = "notes", label = "NOTES" },
    { id = "media", label = "MEDIA" },
    { id = "attention", label = "ATTENTION" },
    { id = "system", label = "SYSTEM" },
    { id = "apps", label = "APPS" },
    { id = "agent", label = "AGENT" },
}

-- Candidate validation renders these hidden branches in addition to the
-- default state. Keep this list aligned with state-driven navigation and
-- overlays so a generated revision fails before activation, not on first use.
local validation_scenarios = {
    { name = "workspace_agenda", state = { active_workspace = "agenda" } },
    { name = "workspace_notes", state = { active_workspace = "notes" } },
    { name = "workspace_media", state = { active_workspace = "media" } },
    { name = "workspace_attention", state = { active_workspace = "attention" } },
    { name = "workspace_system", state = { active_workspace = "system" } },
    { name = "workspace_apps", state = { active_workspace = "apps" } },
    { name = "workspace_agent", state = { active_workspace = "agent" } },
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
    local node_semantics: Semantics? = nil
    if background ~= nil then
        paint = ({{ kind = "fill_bounds", color = background, radius = radius or 0 }} :: {PaintOp})
    end
    if tap_action ~= nil then
        interaction = ({ tap_action = tap_action } :: Interaction)
        node_semantics = ({ role = "button", label = tap_label or tap_action } :: Semantics)
    end
    return {
        id = id,
        layout = resolved_layout,
        paint = paint,
        interaction = interaction,
        semantics = node_semantics,
        children = children,
    }
end

local function button(label: string, action: string, tone: number?, id: string?, active: boolean?): SceneNode
    return group("row", { padding = 1, justify = "center", align = "center" }, {
        group("row", {
            padding = space_sm, justify = "center", align = "center", grow = true,
        }, {
            text(label, label_size, active and paper or (tone or ink)),
        }, active and moss or white, math.max(0, control_radius - 1)),
    }, outline, control_radius, id or action, action, label)
end

local function card(id: string, children: {SceneNode}, layout: Layout?, background: number?): SceneNode
    local resolved_layout: Layout = layout or {}
    local inner_padding = resolved_layout.padding or space_lg
    local inner_gap = resolved_layout.gap or space_sm
    resolved_layout.padding = 1
    resolved_layout.gap = nil
    return group("column", resolved_layout, {
        group("column", { padding = inner_padding, gap = inner_gap, grow = true }, children,
            background or white, math.max(0, card_radius - 1)),
    }, outline, card_radius, id)
end

local function heading(title: string, subtitle: string?, id: string?): SceneNode
    local children: {SceneNode} = {
        text(title, title_size, ink, { role = "header", label = title }, id),
    }
    if subtitle and subtitle ~= "" then
        table.insert(children, text(subtitle, body_size - 1, muted))
    end
    return group("column", { gap = 4 }, children)
end

local function empty_state(id: string, title: string, detail: string): SceneNode
    return card(id, {
        text(title, 15, ink, { role = "status", label = title }, id .. "-status"),
        text(detail, 12, muted),
    }, { padding = space_lg, gap = 4 }, panel)
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

local function active_workspace(state: ExperienceState): string
    local requested = state.active_workspace or "home"
    for _, workspace in ipairs(workspace_order) do
        if workspace.id == requested then return requested end
    end
    return "home"
end

local function workspace_navigation(state: ExperienceState): SceneNode
    local active = active_workspace(state)
    local children: {SceneNode} = {}
    for _, workspace in ipairs(workspace_order) do
        table.insert(children, button(workspace.label, "navigate_" .. workspace.id, moss,
            "workspace-nav-" .. workspace.id, active == workspace.id))
    end
    return group("row", { wrap = true, gap = 7 }, children, nil, 0, "workspace-navigation")
end

local function power_summary(providers: SystemProviders): string
    local power = providers.power
    if not providers_available(providers) or power.battery_percent == nil then
        return "Power unavailable"
    end
    local suffix = power.charging == true and " / charging" or ""
    return tostring(power.battery_percent) .. "%" .. suffix
end

local function network_summary(providers: SystemProviders): string
    if not providers_available(providers) then return "Connectivity unavailable" end
    local connectivity = providers.connectivity
    if not connectivity.connected then return "Offline" end
    local state = connectivity.validated and "Online" or "Connected"
    if connectivity.network_label ~= "" then state = state .. " / " .. connectivity.network_label end
    return state
end

local function audio_summary(providers: SystemProviders): string
    local audio = providers.audio
    if not providers_available(providers) or audio.volume_percent == nil then
        return "Audio unavailable"
    end
    local suffix = audio.muted == true and " / muted" or ""
    return tostring(audio.volume_percent) .. "%" .. suffix
end

local function status_plane(providers: SystemProviders): SceneNode
    local clock = providers.clock
    local facts = {
        { id = "stock-power", label = "POWER", value = power_summary(providers) },
        { id = "stock-network", label = "NETWORK", value = network_summary(providers) },
        { id = "stock-audio", label = "AUDIO", value = audio_summary(providers) },
    }
    local cards: {SceneNode} = {}
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

local function agenda_preview(model: ExperienceModel): SceneNode
    local events = model.calendar
    local children: {SceneNode} = {
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

local function notes_preview(model: ExperienceModel): SceneNode
    local notes = model.notes
    local children: {SceneNode} = {
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

local function current_media(
    model: ExperienceModel,
    providers: SystemProviders
): (string?, string?, boolean)
    local media = providers.audio.media
    if providers_available(providers) and media.active then
        return media.title ~= "" and media.title or "Active media", media.artist or "", media.playing
    end
    local legacy = model.music
    if legacy.title and legacy.title ~= "" then
        return legacy.title, legacy.artist or "", legacy.playing
    end
    return nil, nil, false
end

local function media_preview(model: ExperienceModel, providers: SystemProviders): SceneNode
    local title, artist, playing = current_media(model, providers)
    local children: {SceneNode} = { text("NOW PLAYING", 11, moss) }
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

local function attention_preview(providers: SystemProviders): SceneNode
    local attention = providers.attention
    local items = attention.items
    local children: {SceneNode} = {
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

local function home_workspace(model: ExperienceModel, state: ExperienceState, providers: SystemProviders): SceneNode
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

local function agenda_workspace(model: ExperienceModel, state: ExperienceState): SceneNode
    local rows: {SceneNode} = {
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
    local events = model.calendar
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

local function notes_workspace(model: ExperienceModel, state: ExperienceState): SceneNode
    local note_status = state.note_status or "The first line becomes the note title."
    local rows: {SceneNode} = {
        heading("Notes & documents", "Quick capture backed by the notes provider.", "notes-heading"),
        {
            id = "note-composer",
            layout = { height = 58, padding = 10 },
            content = {
                kind = "text_session", state_key = "note_draft", value = state.note_draft or "",
                placeholder = "Write a quick note...", submit_action = "note_submit",
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
    local notes = model.notes
    if #notes == 0 then
        table.insert(rows, empty_state("notes-empty", "No notes yet",
            "Capture a thought above, or connect a notes provider."))
    end
    local cards: {SceneNode} = {}
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

local function media_controls(model: ExperienceModel, providers: SystemProviders): {SceneNode}
    local media = providers.audio.media
    local canonical = providers_available(providers) and media.active
    local legacy = model.music.title ~= ""
    local controls: {SceneNode} = {}
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
        table.insert(controls, button(model.music.playing and "PAUSE" or "PLAY",
            "media_play_pause", moss))
        table.insert(controls, button("NEXT", "media_next", moss))
    end
    return controls
end

local function media_workspace(model: ExperienceModel, providers: SystemProviders): SceneNode
    local title, artist, playing = current_media(model, providers)
    local children: {SceneNode} = {
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

local function attention_workspace(providers: SystemProviders): SceneNode
    local attention = providers.attention
    local rows: {SceneNode} = {
        heading("Attention center", "Notifications, alarms, calls, and background work in one place.",
            "attention-heading"),
    }
    local items = attention.items
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
        local children: {SceneNode} = {
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

local function system_workspace(providers: SystemProviders): SceneNode
    local rows: {SceneNode} = {
        heading("System controls", "Connectivity, power, audio, and media through provider effects.",
            "system-heading"),
    }
    if not providers_available(providers) then
        table.insert(rows, empty_state("system-providers-unavailable", "System providers unavailable",
            "Stock Base remains navigable while system controls are disabled."))
        return group("column", { gap = 10 }, rows, nil, 0, "workspace-system")
    end

    local power = providers.power
    table.insert(rows, card("system-power", {
        text("POWER", 10, moss),
        text(power_summary(providers), 20, ink),
        text(power.thermal_status and "Thermal state / " .. tostring(power.thermal_status)
            or "Thermal state unavailable", 11, muted),
        text(power.charging_source ~= "" and "Source / " .. power.charging_source
            or "Charging source unavailable", 11, muted),
    }))

    local audio = providers.audio
    local audio_children: {SceneNode} = {
        text("AUDIO", 10, moss), text(audio_summary(providers), 20, ink),
    }
    local audio_buttons: {SceneNode} = {}
    if has_capability(providers, "audio_set_volume") then
        table.insert(audio_buttons, button("VOLUME -", "audio_volume_down", moss))
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

    local connectivity = providers.connectivity
    local network_children: {SceneNode} = {
        text("CONNECTIVITY", 10, moss), text(network_summary(providers), 20, ink),
    }
    if connectivity.connected and has_capability(providers, "wifi_disconnect") then
        table.insert(network_children, button("DISCONNECT WI-FI", "wifi_disconnect", alert))
    end
    local networks = connectivity.wifi_networks
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
                    text(string.rep("*", math.max(1, math.min(4, network.signal_level or 1))),
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

local function apps_workspace(_model: ExperienceModel, providers: SystemProviders): SceneNode
    local rows: {SceneNode} = {
        heading("Applications", "Only provider-approved, compatible launch targets appear here.",
            "apps-heading"),
    }
    local applications = providers.apps.compatible
    if not providers_available(providers) then
        table.insert(rows, empty_state("apps-unavailable", "Application provider unavailable",
            "Launching is disabled, but the rest of Stock Base remains usable."))
    elseif #applications == 0 then
        table.insert(rows, empty_state("apps-empty", "No compatible applications",
            "Eligible applications will appear when the provider discovers them."))
    end
    local cards: {SceneNode} = {}
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

local function has_agent_configuration_action(agent: AgentConversation, action: string): boolean
    for _, supported in ipairs(agent.configuration_actions) do
        if supported == action then return true end
    end
    return false
end

local function agent_workspace(model: ExperienceModel, state: ExperienceState, scope: string): SceneNode
    local function agent_id(id: string): string
        if scope == "workspace" then return id end
        return scope .. "-" .. id
    end
    local agent = model.agent
    local children: {SceneNode} = {
        heading("Agent", "Conversation is part of the revision, while credentials stay native.",
            agent_id("agent-heading")),
        group("row", { justify = "between", align = "center" }, {
            text("CONVERSATION", 11, moss),
            text(agent.activity ~= "" and agent.activity
                or (agent.available and "Ready" or "Unavailable"), 10, muted),
        }),
    }
    local messages: {SceneNode} = {}
    for index, message in ipairs(agent.messages) do
        if index > 24 then break end
        table.insert(messages, card(agent_id("agent-message-" .. tostring(index)), {
            text(message.role == "user" and "YOU" or "SOS", 10, moss),
            text(message.text, 13, ink),
        }, { padding = 12, gap = 3 }, message.role == "user" and sand or mint))
    end
    if #messages == 0 then
        table.insert(messages, empty_state(agent_id("agent-empty"), "Make Stock Base yours",
            "Ask the agent to replace navigation, data composition, visuals, or the whole source."))
    end
    table.insert(children, group("column", { gap = 7 }, messages, nil, 0,
        agent_id("agent-messages")))
    local draft = state.agent_draft or ""
    table.insert(children, ({
        id = agent_id("agent-prompt"),
        layout = { height = 50, padding = 10 },
        content = {
            kind = "text_session", state_key = "agent_draft", value = draft,
            placeholder = agent.busy and "SOS is working..." or "Change this experience...",
            submit_action = "agent_submit",
        },
        paint = {{ kind = "fill_bounds", color = white, radius = 14 }},
        semantics = {
            role = "text_field", label = "Ask SOS to change this experience", value = draft,
            hint = "Type a request and press enter",
        },
    } :: SceneNode))
    local controls: {SceneNode} = {}
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
                configuration.capability == "clear_credential" and alert or moss,
                agent_id(configuration.action)))
        end
    end
    if #controls > 0 then
        table.insert(children, group("row", { wrap = true, gap = 7 }, controls,
            nil, 0, agent_id("agent-provider-controls")))
    else
        table.insert(children, text("Provider changes are managed before login.", 10, muted))
    end
    if agent.error then table.insert(children, text(agent.error, 11, alert)) end
    return group("column", { gap = 10 }, children, nil, 0,
        agent_id("workspace-agent"))
end

local function render_workspace(model: ExperienceModel, state: ExperienceState, providers: SystemProviders): SceneNode
    local active = active_workspace(state)
    if active == "agenda" then return agenda_workspace(model, state) end
    if active == "notes" then return notes_workspace(model, state) end
    if active == "media" then return media_workspace(model, providers) end
    if active == "attention" then return attention_workspace(providers) end
    if active == "system" then return system_workspace(providers) end
    if active == "apps" then return apps_workspace(model, providers) end
    if active == "agent" then return agent_workspace(model, state, "workspace") end
    return home_workspace(model, state, providers)
end

local function render(
    model: ExperienceModel,
    state: ExperienceState,
    properties: JsonValue,
    context: ExperienceContext
): SceneNode
    apply_appearance(model.appearance)
    local effective_state: ExperienceState = {}
    for key, value in pairs(state) do effective_state[key] = value end
    local requested = properties and properties.active_workspace or nil
    if type(requested) == "string" then effective_state.active_workspace = requested end
    return group("column", {
        width = context.viewport.width, height = context.viewport.height,
        scroll_y = true, padding = space_lg, gap = space_md,
    }, {
        render_workspace(model, effective_state, model.providers),
        group("row", { justify = "between", gap = space_md }, {
            text("COMPOSED STOCK WORKSPACE", label_size, muted),
            text("APPEARANCE " .. tostring(model.appearance.generation), label_size, muted),
        }),
    }, paper, 0, "stock-workspace-root")
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
    local providers = model.providers
    local audio = providers.audio
    local connectivity = providers.connectivity
    local action = event.action or ""

    local destination = string.match(action, "^navigate_([a-z]+)$")
    if destination then
        for _, workspace in ipairs(workspace_order) do
            if workspace.id == destination then
                state.active_workspace = destination
                return {
                    state = state,
                    effects = {},
                    events = {{ event = "navigate", payload = { workspace = destination } }},
                }
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
        local media = audio.media
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
        local stamp = tostring(providers.clock.unix_time_ms or 0)
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
        state.note_status = "Saved '" .. title .. "'."
        state.note_sequence = (state.note_sequence or 0) + 1
        local stamp = tostring(providers.clock.unix_time_ms or 0)
        return provider_effect("notes", "write", {
            name = "stock-note-" .. stamp .. "-" .. tostring(state.note_sequence),
            content = "# " .. title .. "\n\n" .. draft,
        }, state)
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
    elseif action == "agent_clear_credential" then
        return provider_effect("agent", "clear_credential", nil, state)
    end

    local network_index = tonumber(string.match(action, "^wifi_connect_(%d+)$"))
    local network = network_index and connectivity.wifi_networks[network_index] or nil
    if network then
        return provider_effect("network", "connect", { network_id = network.id }, state)
    end
    local app_index = tonumber(string.match(action, "^app_launch_(%d+)$"))
    local application = app_index and providers.apps.compatible[app_index] or nil
    if application then
        return provider_effect("apps", "launch", { app_id = application.id }, state)
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
