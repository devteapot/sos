use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        OnceLock,
    },
    time::Instant,
};

use experience_ir::{Align, ExperienceModel, Justify, NodeKind, UiEvent, UiNode};
use gpui::{
    div, prelude::*, px, rgb, AnyElement, App, Application, Context, MouseButton, Render,
    SharedString, Window, WindowOptions,
};
use gpui_mobile::{android::jni, packages::deeplink};
use runtime_luau::LuauRuntime;
use serde_json::{json, Value as JsonValue};

use crate::DEFAULT_EXPERIENCE;

static FILES_DIR: OnceLock<PathBuf> = OnceLock::new();
static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);

const ACTIVE_FILE: &str = "experience.active.luau";
const CANDIDATE_FILE: &str = "experience.candidate.luau";
const PREVIOUS_FILE: &str = "experience.previous.luau";
const REJECTED_FILE: &str = "experience.rejected.luau";
const STATE_FILE: &str = "experience-state.json";

#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("sos-experience"),
    );
    jni::install_panic_hook();

    if let Some(path) = app.internal_data_path() {
        let _ = FILES_DIR.set(path);
    } else {
        log::error!("Android did not expose an internal data path");
        return;
    }

    let _platform = jni::init_platform(&app);
    let Some(shared) = jni::shared_platform() else {
        log::error!("shared Android platform is unavailable");
        return;
    };

    deeplink::set_deep_link_handler(|url| {
        if url.starts_with("sos://reload") {
            RELOAD_REQUESTED.store(true, Ordering::Release);
            log::info!("script reload requested");
        }
    });

    Application::with_platform(shared.into_rc()).run(|cx: &mut App| {
        match cx.open_window(
            WindowOptions {
                window_bounds: None,
                ..Default::default()
            },
            |_, cx| cx.new(ExperienceHost::new),
        ) {
            Ok(_) => log::info!("SOS experience window is live"),
            Err(error) => log::error!("failed to open experience window: {error}"),
        }
    });
}

struct ExperienceHost {
    model: ExperienceModel,
    runtime: LuauRuntime,
    tree: UiNode,
    state: JsonValue,
    source: String,
    status: Option<(String, bool)>,
}

impl ExperienceHost {
    fn new(_cx: &mut Context<Self>) -> Self {
        let model = providers_fake::snapshot();
        let state = load_state();
        let source = read_file(ACTIVE_FILE).unwrap_or_else(|| DEFAULT_EXPERIENCE.to_owned());
        let (runtime, tree, source) = match build_candidate(&source, &model, &state) {
            Ok((runtime, tree)) => (runtime, tree, source),
            Err(error) => {
                log::error!("active source rejected at startup: {error}; using embedded source");
                let (runtime, tree) = build_candidate(DEFAULT_EXPERIENCE, &model, &state)
                    .expect("embedded experience must be valid");
                (runtime, tree, DEFAULT_EXPERIENCE.to_owned())
            }
        };
        let mut host = Self {
            model,
            runtime,
            tree,
            state,
            source,
            status: None,
        };
        if file_path(CANDIDATE_FILE).is_file() {
            host.try_reload();
        }
        host
    }

    fn dispatch(&mut self, action: String, cx: &mut Context<Self>) {
        log::info!("experience_action action={action}");
        let event = UiEvent { action };
        match self.runtime.update(&self.model, &self.state, &event) {
            Ok(candidate_state) => match self.runtime.render(&self.model, &candidate_state) {
                Ok(tree) => {
                    self.state = candidate_state;
                    self.tree = tree;
                    persist_state(&self.state);
                    self.status = None;
                    cx.notify();
                }
                Err(error) => {
                    self.status = Some((format!("Action rejected: {error}"), false));
                    log::warn!("action did not produce a valid tree: {error}");
                    cx.notify();
                }
            },
            Err(error) => {
                self.status = Some((format!("Action rejected: {error}"), false));
                log::warn!("action failed: {error}");
                cx.notify();
            }
        }
    }

    fn try_reload(&mut self) {
        let Some(candidate_source) = read_file(CANDIDATE_FILE) else {
            self.status = Some(("No candidate script found".into(), false));
            return;
        };
        let started = Instant::now();
        match build_candidate(&candidate_source, &self.model, &self.state) {
            Ok((runtime, tree)) => {
                if let Err(error) = write_file(PREVIOUS_FILE, &self.source)
                    .and_then(|_| write_file(ACTIVE_FILE, &candidate_source))
                {
                    self.status = Some((format!("Could not persist revision: {error}"), false));
                    return;
                }
                self.runtime = runtime;
                self.tree = tree;
                self.source = candidate_source;
                let elapsed = started.elapsed();
                self.status = Some((
                    format!("Luau revision live in {} ms", elapsed.as_millis()),
                    true,
                ));
                let _ = fs::remove_file(file_path(CANDIDATE_FILE));
                log::info!("script_swap accepted elapsed_us={}", elapsed.as_micros());
            }
            Err(error) => {
                self.status = Some((format!("Candidate rejected: {error}"), false));
                let _ = fs::rename(file_path(CANDIDATE_FILE), file_path(REJECTED_FILE));
                log::warn!("script_swap rejected: {error}");
            }
        }
    }

    fn render_node(
        &mut self,
        node: &UiNode,
        path: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let element_id = node.id.clone().unwrap_or_else(|| path.to_string());
        let mut element = div();
        match node.kind {
            NodeKind::Column => element = element.flex().flex_col(),
            NodeKind::Row => element = element.flex().flex_row(),
            NodeKind::Scroll => element = element.flex().flex_col().size_full(),
            NodeKind::Box | NodeKind::Spacer | NodeKind::Text(_) => {}
        }

        if let Some(background) = node.style.background {
            element = element.bg(rgb(background));
        }
        if let Some(color) = node.style.color {
            element = element.text_color(rgb(color));
        }
        if let Some(padding) = node.style.padding {
            element = element.p(px(padding));
        }
        if let Some(gap) = node.style.gap {
            element = element.gap(px(gap));
        }
        if let Some(radius) = node.style.radius {
            element = element.rounded(px(radius));
        }
        if let Some(text_size) = node.style.text_size {
            element = element.text_size(px(text_size));
        }
        if let Some(width) = node.style.width {
            element = element.w(px(width));
        }
        if let Some(height) = node.style.height {
            element = element.h(px(height));
        }
        if node.style.grow {
            element = element.flex_1();
        }
        element = match node.style.align {
            Some(Align::Start) => element.items_start(),
            Some(Align::Center) => element.items_center(),
            Some(Align::End) => element.items_end(),
            None => element,
        };
        element = match node.style.justify {
            Some(Justify::Start) => element.justify_start(),
            Some(Justify::Center) => element.justify_center(),
            Some(Justify::End) => element.justify_end(),
            Some(Justify::Between) => element.justify_between(),
            None => element,
        };
        if let NodeKind::Text(text) = &node.kind {
            element = element.child(SharedString::from(text.clone()));
        }
        for (index, child) in node.children.iter().enumerate() {
            let child_path = SharedString::from(format!("{path}-{index}"));
            element = element.child(self.render_node(child, child_path, cx));
        }
        if let Some(action) = &node.action {
            let action = action.clone();
            return element
                .id(SharedString::from(element_id))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| this.dispatch(action.clone(), cx)),
                )
                .into_any_element();
        }
        if matches!(node.kind, NodeKind::Scroll) {
            return element
                .id(SharedString::from(element_id))
                .overflow_y_scroll()
                .into_any_element();
        }
        element.into_any_element()
    }
}

impl Render for ExperienceHost {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if RELOAD_REQUESTED.swap(false, Ordering::AcqRel) {
            self.try_reload();
        }
        let tree = self.tree.clone();
        let content = self.render_node(&tree, SharedString::from("root"), cx);
        let mut root = div()
            .flex()
            .flex_col()
            .size_full()
            .pt(px(34.0))
            .pb(px(18.0))
            .bg(rgb(0xF3F1E8))
            .child(content);
        if let Some((message, accepted)) = &self.status {
            root = root.child(
                div()
                    .absolute()
                    .left(px(14.0))
                    .right(px(14.0))
                    .bottom(px(12.0))
                    .p(px(10.0))
                    .rounded(px(12.0))
                    .bg(rgb(if *accepted { 0x2F684B } else { 0x8C3A36 }))
                    .text_color(rgb(0xFFFFFF))
                    .text_size(px(12.0))
                    .child(SharedString::from(message.clone())),
            );
        }
        root
    }
}

fn build_candidate(
    source: &str,
    model: &ExperienceModel,
    state: &JsonValue,
) -> Result<(LuauRuntime, UiNode), String> {
    let runtime = LuauRuntime::compile(source).map_err(|error| error.to_string())?;
    let tree = runtime
        .render(model, state)
        .map_err(|error| error.to_string())?;
    Ok((runtime, tree))
}

fn file_path(name: &str) -> PathBuf {
    FILES_DIR
        .get()
        .expect("files directory initialized")
        .join(name)
}

fn read_file(name: &str) -> Option<String> {
    fs::read_to_string(file_path(name)).ok()
}

fn write_file(name: &str, contents: &str) -> std::io::Result<()> {
    let destination = file_path(name);
    let temporary = destination.with_extension("tmp");
    fs::write(&temporary, contents)?;
    fs::rename(temporary, destination)
}

fn load_state() -> JsonValue {
    read_file(STATE_FILE)
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_else(|| json!({}))
}

fn persist_state(state: &JsonValue) {
    match serde_json::to_string(state) {
        Ok(contents) => {
            if let Err(error) = write_file(STATE_FILE, &contents) {
                log::warn!("could not persist experience state: {error}");
            }
        }
        Err(error) => log::warn!("could not serialize experience state: {error}"),
    }
}
