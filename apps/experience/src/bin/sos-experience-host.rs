#[cfg(target_os = "linux")]
fn main() {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--graph-runtime-worker"))
    {
        if let Err(error) = runtime_luau::run_graph_worker_stdio() {
            eprintln!("sos_graph_runtime_worker_failed error={error}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = sos_experience::run_linux_host() {
        eprintln!("sos_experience_host_failed error={error:#}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("sos-experience-host is currently supported only on Linux");
    std::process::exit(1);
}
