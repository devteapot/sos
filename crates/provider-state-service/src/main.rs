use std::{env, fs, os::unix::fs::PermissionsExt as _, path::PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("provider_state_service_failed error={error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut socket = None;
    let mut state_file = None;
    let mut appearance_capability_file = None;
    let mut grant_capability_file = None;
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--socket" => socket = args.next().map(PathBuf::from),
            "--state-file" => state_file = args.next().map(PathBuf::from),
            "--appearance-capability-file" => {
                appearance_capability_file = args.next().map(PathBuf::from)
            }
            "--grant-capability-file" => grant_capability_file = args.next().map(PathBuf::from),
            _ => return Err(format!("unexpected argument: {argument}").into()),
        }
    }
    let socket = socket.ok_or("--socket requires a path")?;
    let state_file = state_file.ok_or("--state-file requires a path")?;
    println!(
        "provider_state_service_listening socket={} state_file={}",
        socket.display(),
        state_file.display()
    );
    let appearance_capability = load_capability(appearance_capability_file, "appearance")?;
    let grant_capability = load_capability(grant_capability_file, "grant-review")?;
    provider_state_service::serve_with_writers(
        &socket,
        &state_file,
        appearance_capability.as_deref(),
        grant_capability.as_deref(),
    )
}

fn load_capability(
    path: Option<PathBuf>,
    name: &str,
) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    path.map(
        |path| -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            let metadata = fs::metadata(&path)?;
            if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 257 {
                return Err(format!("{name} capability file must contain 1 to 256 bytes").into());
            }
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(
                    format!("{name} capability file must not be group/world accessible").into(),
                );
            }
            let capability = fs::read_to_string(path)?;
            let capability = capability.trim_end_matches(['\r', '\n']).to_owned();
            if capability.is_empty() || capability.len() > 256 {
                return Err(format!("{name} capability file must contain 1 to 256 bytes").into());
            }
            Ok(capability)
        },
    )
    .transpose()
}
