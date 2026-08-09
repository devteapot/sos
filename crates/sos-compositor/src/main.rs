#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = sos_compositor::run() {
        eprintln!("sos_compositor_failed error={error:#}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("sos-compositor is currently supported only on Linux");
    std::process::exit(1);
}
