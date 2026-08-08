mod control;
mod handlers;
mod input;
pub mod policy;
mod state;
mod winit;

use std::{collections::BTreeMap, env, path::PathBuf};

use anyhow::{bail, Context as _, Result};
use smithay::reexports::{
    calloop::{EventLoop, LoopSignal},
    wayland_server::{Display, DisplayHandle},
};
use state::SosCompositor;

pub struct CompositorData {
    state: SosCompositor,
    display_handle: DisplayHandle,
    loop_signal: LoopSignal,
}

pub fn run() -> Result<()> {
    init_tracing();
    let options = Options::parse(env::args().skip(1).collect())?;
    let socket_name = options.required("--socket")?;
    if !socket_name.starts_with("wayland-") || socket_name.contains('/') {
        bail!("--socket must be a Wayland socket basename such as wayland-sos");
    }
    let control_socket = PathBuf::from(options.required("--control-socket")?);
    let shell_token = options.required("--shell-token")?.to_owned();
    options.ensure_only(&["--socket", "--control-socket", "--shell-token"])?;

    let mut event_loop: EventLoop<CompositorData> = EventLoop::try_new()?;
    let display: Display<SosCompositor> = Display::new()?;
    let display_handle = display.handle();
    let loop_signal = event_loop.get_signal();
    let state = SosCompositor::new(&mut event_loop, display, socket_name)?;
    let mut data = CompositorData {
        state,
        display_handle,
        loop_signal,
    };
    let _control_guard = control::init_control(&mut event_loop, &control_socket, shell_token)?;
    winit::init_winit(&mut event_loop, &mut data)?;

    println!(
        "sos_compositor_ready wayland_display={} control_socket={} evidence=nested_backend_submit",
        data.state.socket_name.to_string_lossy(),
        control_socket.display()
    );
    event_loop.run(None, &mut data, |_| {})?;
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
    "usage: sos-compositor --socket NAME --control-socket PATH --shell-token TOKEN"
}
