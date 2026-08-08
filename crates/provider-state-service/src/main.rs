use std::{env, path::PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("provider_state_service_failed error={error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut socket = None;
    let mut state_file = None;
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--socket" => socket = args.next().map(PathBuf::from),
            "--state-file" => state_file = args.next().map(PathBuf::from),
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
    provider_state_service::serve(&socket, &state_file)
}
