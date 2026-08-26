-- SOS Experience API v3 and v4 Luau type prelude.
-- This file is prepended by `sosctl typecheck`; the embedded VM erases the
-- matching annotations and the Rust decoder remains the authority boundary.

type JsonValue = any
type ExperienceState = {[string]: any}

type AppearanceTypography = {
    family: string,
    size_milli_points: number,
    weight: number,
    line_height_milli: number,
}
type AppearanceProfile = {
    abi_version: number,
    generation: number,
    scheme: "light" | "dark",
    contrast: "standard" | "high",
    text_scale_milli: number,
    reduce_motion: boolean,
    colors: {[string]: string},
    spacing: {[string]: number},
    radii: {[string]: number},
    typography: {[string]: AppearanceTypography},
}
type ContainerAppearance = {
    colors: {[string]: string}?,
    spacing: {[string]: number}?,
    radii: {[string]: number}?,
}

type ShellCanvas = {
    width: number,
    height: number,
    mirrored: boolean,
}

type ShellOutput = {
    id: string,
    x: number,
    y: number,
    width: number,
    height: number,
    scale: number,
    primary: boolean,
}

type ShellWindow = {
    id: string,
    title: string,
    kind: "native" | "compatibility",
    active: boolean,
    capabilities: {"focus" | "close"},
}

type ShellModel = {
    abi_version: number,
    canvas: ShellCanvas,
    outputs: {ShellOutput},
    windows: {ShellWindow},
    capabilities: {"window_focus" | "window_close"},
}

type ClockProviderState = {
    unix_time_ms: number,
    locale: string,
    timezone: string,
    time_label: string,
    date_label: string,
}

type PowerProviderState = {
    battery_percent: number?,
    charging: boolean?,
    charging_source: string,
    battery_temperature_deci_c: number?,
    thermal_status: string?,
}

type WifiNetwork = {
    id: string,
    label: string,
    signal_level: number,
    saved: boolean,
    connected: boolean,
}

type ConnectivityProviderState = {
    wifi_enabled: boolean,
    connected: boolean,
    validated: boolean,
    transport: string,
    network_label: string,
    signal_level: number?,
    online_interfaces: {string},
    wifi_networks: {WifiNetwork},
}

type MediaProviderState = {
    active: boolean,
    playing: boolean,
    title: string,
    artist: string,
}

type AudioProviderState = {
    volume_percent: number?,
    muted: boolean?,
    media: MediaProviderState,
}

type SystemApplication = { id: string, label: string }
type ApplicationStatusWidget = {
    id: string,
    label: string,
    value: string,
    application_id: string?,
}
type AppsProviderState = {
    compatible: {SystemApplication},
    status_widgets: {ApplicationStatusWidget}?,
}
type AttentionItem = {
    id: string,
    occurred_at_ms: number,
    source: string,
    kind: string,
    urgent: boolean,
    title: string,
    detail: string,
}
type AttentionProviderState = { items: {AttentionItem}, urgent_count: number }

type SystemProviders = {
    abi_version: number,
    observed_at_ms: number,
    clock: ClockProviderState,
    power: PowerProviderState,
    connectivity: ConnectivityProviderState,
    audio: AudioProviderState,
    apps: AppsProviderState,
    attention: AttentionProviderState,
    capabilities: {string},
}

type AgentMessage = { role: "user" | "assistant", text: string }
type AgentConversation = {
    available: boolean,
    busy: boolean,
    activity: string,
    messages: {AgentMessage},
    error: string?,
    configuration_actions: {string},
}
type CalendarEvent = { time: string, title: string, detail: string }
type Note = { title: string, preview: string }
type Music = { title: string, artist: string, playing: boolean }
type LegacyNetwork = {
    wifi_enabled: boolean,
    connected: boolean,
    connected_ssid: string?,
    validated: boolean,
    signal_level: number?,
    networks: {{
        ssid: string,
        signal_level: number,
        security: "open" | "personal" | "enterprise",
        saved: boolean,
    }},
    activity: string,
    error: string?,
}

type ExperienceModel = {
    appearance: AppearanceProfile,
    greeting: string,
    date: string,
    weather: { summary: string, temperature_c: number, high_c: number, low_c: number },
    calendar: {CalendarEvent},
    notes: {Note},
    music: Music,
    system: {[string]: any},
    surfaces: {{ id: string, kind: string, width: number, height: number, protected: boolean, status: string }},
    agent: AgentConversation,
    network: LegacyNetwork,
    providers: SystemProviders,
    shell: ShellModel,
}

type Layout = {
    flow: ("overlay" | "column" | "row")?,
    wrap: boolean?,
    scroll_y: boolean?,
    padding: number?,
    gap: number?,
    width: number?,
    height: number?,
    min_width: number?,
    min_height: number?,
    max_width: number?,
    max_height: number?,
    aspect_ratio: number?,
    position: { x: number, y: number }?,
    program: {
        measure_width: number?,
        measure_height: number?,
        arrange_x: number?,
        arrange_y: number?,
    }?,
    clip_bounds: boolean?,
    grow: boolean?,
    align: ("start" | "center" | "end")?,
    justify: ("start" | "center" | "end" | "between")?,
}

type TextContent = { kind: "text", value: string, size: number, color: number }
type TextSessionContent = {
    kind: "text_session",
    state_key: string,
    value: string,
    placeholder: string?,
    submit_action: string?,
    autofocus: boolean?,
}
type ImageContent = { kind: "image", asset: string }
type ProviderSurfaceContent = { kind: "provider_surface", surface: string }
type ExperienceMountContent = {
    kind: "experience_mount",
    dependency: string,
    properties: JsonValue?,
    container_appearance: ContainerAppearance?,
}
type WindowSpaceContent = {
    kind: "window_space",
    layout: ("floating" | "tiling" | "scrolling")?,
    gap: number?,
    fallback: string?,
}
type ShellOverlayPlacement = {
    horizontal: ("start" | "center" | "end")?,
    vertical: ("start" | "center" | "end")?,
    margin: number?,
}
type ShellOverlayAnchor = { x: number, y: number, width: number, height: number, above: boolean? }
type ShellOverlayContent = {
    kind: "shell_overlay",
    x: number?,
    y: number?,
    width: number,
    height: number,
    placement: ShellOverlayPlacement?,
    anchor: ShellOverlayAnchor?,
}
type ApplicationSurfaceContent = { kind: "application_surface", title: string }
type Content = TextContent | TextSessionContent | ImageContent | ProviderSurfaceContent
    | ExperienceMountContent | WindowSpaceContent | ShellOverlayContent | ApplicationSurfaceContent

type PaintPoint = { x: number, y: number }
type GlyphRun = {
    text: string,
    color: number,
    font_family: string?,
    weight: number?,
    italic: boolean?,
}
type PaintOp =
    { kind: "fill_bounds", color: number, radius: number? }
    | { kind: "path", points: {PaintPoint}, color: number, width: number?, closed: boolean? }
    | { kind: "quad", x: number, y: number, width: number, height: number, radius: number?, color: number }
    | { kind: "glyphs", x: number, y: number, size: number, line_height: number?, max_width: number?, runs: {GlyphRun} }
    | { kind: "shader", asset: string, x: number, y: number, width: number, height: number }
    | { kind: "layer", clip: { x: number, y: number, width: number, height: number }?, transform: {[string]: number}?, opacity: number?, paint: {PaintOp} }

type HitRegion = {
    id: string,
    x: number,
    y: number,
    width: number,
    height: number,
    press_action: string?,
    drag_action: string?,
    drop_action: string?,
    tap_action: string?,
    double_tap_action: string?,
    long_press_action: string?,
    swipe_action: string?,
}
type Interaction = {
    tap_action: string?,
    hover_action: string?,
    surface_drag: boolean?,
    double_tap_action: string?,
    long_press_action: string?,
    swipe_action: string?,
    pointer_action: string?,
    multi_pointer_action: string?,
    capture: ("none" | "pointer" | "surface")?,
    hit_regions: {HitRegion}?,
}
type Semantics = {
    role: "button" | "image" | "text_field" | "header" | "status" | "scroll_area",
    label: string,
    value: string?,
    hint: string?,
}
type Animation = { kind: "pulse" | "fade_in", duration_ms: number, loop: boolean? }

type SceneNode = {
    id: string?,
    layout: Layout?,
    content: Content?,
    paint: {PaintOp}?,
    interaction: Interaction?,
    animation: Animation?,
    semantics: Semantics?,
    children: {SceneNode}?,
}

type SceneEvent = {
    action: string,
    target: string?,
    value: string?,
    focused: boolean?,
    x: number?,
    y: number?,
    delta_x: number?,
    delta_y: number?,
    velocity_x: number?,
    velocity_y: number?,
    phase: string?,
    pointer_id: number?,
    pointer_count: number?,
    pressure: number?,
    scale: number?,
    rotation_degrees: number?,
}

type ProviderEffect = { provider: string, action: string, payload: JsonValue? }
type ExperienceViewport = { width: number, height: number, scale_milli: number }
type ExperienceContext = {
    export_id: string,
    viewport: ExperienceViewport,
    properties: JsonValue,
    container_appearance: ContainerAppearance?,
}
type ExperienceEvent = { dependency: string, event: string, payload: JsonValue }
type ExperienceOutputEvent = { event: string, payload: JsonValue }
type UpdateOutcome = {
    state: ExperienceState,
    effects: {ProviderEffect},
    events: {ExperienceOutputEvent}?,
}
type ExperienceExport = {
    render: (ExperienceModel, ExperienceState, JsonValue, ExperienceContext) -> SceneNode,
    update: ((ExperienceModel, ExperienceState, SceneEvent | ExperienceEvent, JsonValue, ExperienceContext) -> ExperienceState | UpdateOutcome)?,
}
type ValidationScenario = { name: string, state: ExperienceState }
