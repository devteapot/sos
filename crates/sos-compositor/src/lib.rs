mod control;
#[cfg(feature = "direct-backend")]
mod direct;
mod handlers;
mod input;
pub mod policy;
mod recovery;
mod render;
mod state;
#[cfg(feature = "nested-backend")]
mod winit;
mod xwayland;

use std::{collections::BTreeMap, env, fs, io::Write as _, path::PathBuf};

use anyhow::{bail, Context as _, Result};
use compositor_control_protocol::read_shell_token_file;
use smithay::reexports::{
    calloop::{EventLoop, LoopSignal},
    wayland_server::{Display, DisplayHandle},
};
use state::SosCompositor;

pub struct CompositorData {
    state: SosCompositor,
    display_handle: DisplayHandle,
    loop_signal: LoopSignal,
    #[cfg(feature = "direct-backend")]
    direct: Option<direct::DirectBackend>,
    ready_file: Option<PathBuf>,
    backend_ready: bool,
    #[cfg(feature = "direct-backend")]
    last_recovery_view: Option<bool>,
}

pub fn run() -> Result<()> {
    init_tracing();
    let options = Options::parse(env::args().skip(1).collect())?;
    let socket_name = options.required("--socket")?;
    if !socket_name.starts_with("wayland-") || socket_name.contains('/') {
        bail!("--socket must be a Wayland socket basename such as wayland-sos");
    }
    let control_socket = PathBuf::from(options.required("--control-socket")?);
    let ready_file = options.optional("--ready-file").map(PathBuf::from);
    let xwayland_display_file = options
        .optional("--xwayland-display-file")
        .map(PathBuf::from);
    let shell_token = match (
        options.optional("--shell-token"),
        options.optional("--shell-token-file"),
    ) {
        (Some(token), None) => token.to_owned(),
        (None, Some(path)) => read_shell_token_file(PathBuf::from(path).as_path())
            .with_context(|| format!("read compositor shell credential {path}"))?,
        (Some(_), Some(_)) => bail!("use exactly one of --shell-token and --shell-token-file"),
        (None, None) => bail!("one of --shell-token or --shell-token-file is required"),
    };
    let backend = options
        .0
        .get("--backend")
        .map(String::as_str)
        .unwrap_or("nested");
    options.ensure_only(&[
        "--socket",
        "--control-socket",
        "--shell-token",
        "--shell-token-file",
        "--ready-file",
        "--backend",
        "--xwayland-display-file",
    ])?;

    let mut event_loop: EventLoop<'static, CompositorData> = EventLoop::try_new()?;
    let display: Display<SosCompositor> = Display::new()?;
    let display_handle = display.handle();
    let loop_signal = event_loop.get_signal();
    let state = SosCompositor::new(&mut event_loop, display, socket_name)?;
    let mut data = CompositorData {
        state,
        display_handle,
        loop_signal,
        #[cfg(feature = "direct-backend")]
        direct: None,
        ready_file,
        backend_ready: false,
        #[cfg(feature = "direct-backend")]
        last_recovery_view: None,
    };
    let _control_guard = control::init_control(&mut event_loop, &control_socket, shell_token)?;
    let evidence: &'static str = match backend {
        #[cfg(feature = "nested-backend")]
        "nested" => {
            winit::init_winit(&mut event_loop, &mut data)?;
            "nested_backend_submit"
        }
        #[cfg(feature = "direct-backend")]
        "drm" => {
            direct::init_direct(&mut event_loop, &mut data)?;
            "drm_page_flip"
        }
        #[cfg(not(feature = "nested-backend"))]
        "nested" => bail!("nested backend was not compiled in"),
        #[cfg(not(feature = "direct-backend"))]
        "drm" => bail!("DRM backend was not compiled in; enable direct-backend"),
        other => bail!("unsupported compositor backend: {other}"),
    };
    if let Some(display_file) = xwayland_display_file {
        xwayland::start(&mut event_loop, &mut data, display_file)?;
    }

    println!(
        "sos_compositor_ready wayland_display={} control_socket={} backend={} evidence={}",
        data.state.socket_name.to_string_lossy(),
        control_socket.display(),
        backend,
        evidence,
    );
    event_loop.run(None, &mut data, |data| {
        data.state.space.refresh();
        data.state.popups.cleanup();
        data.state.input_method_popups.retain(|popup| popup.alive());
        if let Err(error) = data.display_handle.flush_clients() {
            tracing::warn!(%error, "could not flush Wayland clients");
        }
    })?;
    Ok(())
}

fn init_tracing() {
    let builder = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(std::io::stderr);
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        builder.with_env_filter(filter).init();
    } else {
        builder
            .with_env_filter(tracing_subscriber::EnvFilter::new("sos_compositor=info"))
            .init();
    }
}

fn mark_backend_ready(data: &mut CompositorData, evidence: &str) {
    if data.backend_ready {
        return;
    }
    if let Some(path) = &data.ready_file {
        let result = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .and_then(|mut file| writeln!(file, "evidence={evidence}"));
        if let Err(error) = result {
            tracing::error!(%error, path = %path.display(), "could not publish compositor readiness");
            data.loop_signal.stop();
            return;
        }
    }
    data.backend_ready = true;
    println!("sos_compositor_presenting evidence={evidence}");
}

struct Options(BTreeMap<String, String>);

impl Options {
    fn parse(arguments: Vec<String>) -> Result<Self> {
        if !arguments.len().is_multiple_of(2) {
            bail!("every compositor option requires a value\n{}", usage());
        }
        let mut options = BTreeMap::new();
        for pair in arguments.chunks_exact(2) {
            if !pair[0].starts_with("--") {
                bail!("expected compositor option, got {}", pair[0]);
            }
            if options.insert(pair[0].clone(), pair[1].clone()).is_some() {
                bail!("duplicate compositor option: {}", pair[0]);
            }
        }
        Ok(Self(options))
    }

    fn required(&self, name: &str) -> Result<&str> {
        self.0
            .get(name)
            .map(String::as_str)
            .with_context(|| format!("missing required compositor option: {name}\n{}", usage()))
    }

    fn optional(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    fn ensure_only(&self, allowed: &[&str]) -> Result<()> {
        for name in self.0.keys() {
            if !allowed.contains(&name.as_str()) {
                bail!("unexpected compositor option: {name}\n{}", usage());
            }
        }
        Ok(())
    }
}

fn usage() -> &'static str {
    "usage: sos-compositor --socket NAME --control-socket PATH (--shell-token TOKEN | --shell-token-file PATH) [--ready-file PATH] [--backend nested|drm] [--xwayland-display-file PATH]"
}
