#[cfg(target_os = "linux")]
fn main() {
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
