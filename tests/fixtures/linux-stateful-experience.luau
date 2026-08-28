--!strict
-- Test-only Linux host fixture for native text input, media, provider surfaces,
-- animation, focus, and accessibility. It is never packaged as an experience.

local ink = 0x171A21
local paper = 0xF5F0E7
local muted = 0x666B73
local white = 0xFFFFFF

type GroupOptions = {
    id: string?,
    background: number?,
    radius: number?,
    tap_action: string?,
    semantics: Semantics?,
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
    children: {SceneNode}?,
    options: GroupOptions?
): SceneNode
    local resolved_options: GroupOptions = options or {}
    local resolved_layout: Layout = layout or {}
    resolved_layout.flow = flow
    local paint: {PaintOp}? = nil
    if resolved_options.background ~= nil then
        paint = ({{
            kind = "fill_bounds",
            color = resolved_options.background,
            radius = resolved_options.radius or 0,
        }} :: {PaintOp})
    end
    local interaction: Interaction? = nil
    if resolved_options.tap_action ~= nil then
        interaction = ({ tap_action = resolved_options.tap_action } :: Interaction)
    end
    return {
        id = resolved_options.id,
        layout = resolved_layout,
        paint = paint,
        interaction = interaction,
        semantics = resolved_options.semantics,
        children = children,
    }
end

local function render(model: ExperienceModel, state: ExperienceState): SceneNode
    local draft = state.draft
    if draft == nil then draft = "Caffè ☕️ – 明日のデザイン" end
    local playing = state.playing
    if playing == nil then playing = model.music.playing end

    local provider_surfaces: {SceneNode} = {}
    for _, surface in ipairs(model.surfaces) do
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

    return group("column", { scroll_y = true }, {
        group("column", { padding = 22, gap = 18 }, {
            text("Stateful Linux gate", 30, ink, {
                role = "header", label = "Stateful Linux gate",
            }, "stateful-gate-header"),
            group("row", { gap = 14, align = "center" }, {
                {
                    id = "album-art",
                    layout = { width = 76, height = 76 },
                    content = { kind = "image", asset = "album-orbit" },
                    paint = {{ kind = "fill_bounds", color = white, radius = 22 }},
                    animation = { kind = "pulse", duration_ms = 1800, loop = true },
                    semantics = { role = "image", label = "Animated album artwork" },
                },
                group("column", { gap = 4, grow = true }, {
                    text(model.music.title, 19, ink),
                    text(model.music.artist, 13, muted),
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
                        },
                    }),
                }),
            }),
            group("row", { gap = 12 }, provider_surfaces),
            group("column", { gap = 8, padding = 16 }, {
                text("Native editor", 18, ink),
                {
                    id = "note-draft",
                    layout = { height = 38, padding = 7 },
                    content = {
                        kind = "text_session",
                        state_key = "draft",
                        value = draft,
                        placeholder = "Write a note…",
                        submit_action = "save_note",
                        autofocus = true,
                    },
                    paint = {{ kind = "fill_bounds", color = 0xF0ECE4, radius = 10 }},
                    semantics = {
                        role = "text_field",
                        label = "Note draft",
                        value = draft,
                        hint = "Edit a note and press enter to save",
                    },
                },
                text(state.last_saved and "Saved" or "Editing", 12, muted, {
                    role = "status",
                    label = "Note status",
                    value = state.last_saved and "Saved" or "Editing",
                }, "note-status"),
            }, { background = white, radius = 20 }),
            text(model.weather.temperature_c .. "° · " .. model.weather.summary, 18, ink, {
                role = "status",
                label = "Weather",
                value = model.weather.temperature_c .. " degrees, " .. model.weather.summary,
            }, "weather-status"),
        }),
    }, { id = "stateful-gate-root", background = paper })
end

local function update(
    model: ExperienceModel,
    state: ExperienceState,
    event: SceneEvent
): ExperienceState
    if event.action == "text_changed" and event.target == "note-draft" then
        state.draft = event.value
    elseif event.action == "save_note" then
        state.draft = event.value or state.draft
        state.last_saved = true
    elseif event.action == "toggle_music" then
        if state.playing == nil then
            state.playing = not model.music.playing
        else
            state.playing = not state.playing
        end
    elseif event.action == "focus_changed" then
        state.editor_focused = event.focused
    end
    return state
end

return {
    api_version = 4,
    state_version = 1,
    exports = { main = { render = render, update = update } },
}
