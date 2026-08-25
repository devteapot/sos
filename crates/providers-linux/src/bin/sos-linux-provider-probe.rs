use std::{env, path::PathBuf, process::ExitCode};

use providers_linux::{load_grants, ProviderHub};

#[derive(Debug)]
struct Options {
    root: PathBuf,
    grants: PathBuf,
    revision: String,
    development_wildcard: bool,
}

fn parse_options() -> Result<Options, String> {
    let mut arguments = env::args().skip(1);
    let mut root = None;
    let mut grants = None;
    let mut revision = None;
    let mut development_wildcard = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--root" => root = arguments.next().map(PathBuf::from),
            "--grants" => grants = arguments.next().map(PathBuf::from),
            "--revision" => revision = arguments.next(),
            "--development-wildcard" => development_wildcard = true,
            _ => return Err(format!("unexpected argument: {argument}")),
        }
    }
    let root = root.ok_or_else(|| "--root is required".to_owned())?;
    let grants = grants.ok_or_else(|| "--grants is required".to_owned())?;
    let revision = revision
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(|| "--revision must be non-empty and at most 128 bytes".to_owned())?;
    if !root.is_absolute() || !grants.is_absolute() {
        return Err("--root and --grants must be absolute paths".into());
    }
    Ok(Options {
        root,
        grants,
        revision,
        development_wildcard,
    })
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
    let context = load_grants(
        &options.grants,
        &options.revision,
        options.development_wildcard,
    )?;
    let snapshot = ProviderHub::open(&options.root)?.snapshot_with_frames(&context)?;
    serde_json::to_writer_pretty(
        std::io::stdout().lock(),
        &serde_json::json!({
            "revision_id": options.revision,
            "providers": snapshot.model.providers,
            "compatibility_system": snapshot.model.system,
            "provider_frame_count": snapshot.frames.len(),
        }),
    )?;
    println!();
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sos_linux_provider_probe_failed error={error}");
            ExitCode::FAILURE
        }
    }
}
