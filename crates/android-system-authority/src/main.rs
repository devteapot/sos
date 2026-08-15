use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use android_authority_protocol::{
    RevisionRequest, RevisionResponse, MAX_REVISION_REQUEST_BYTES, REVISION_ADDRESS,
};
use android_system_authority::{AndroidSystemAuthority, MAX_PROVIDER_REQUEST_BYTES};
use experience_ir::{ProviderRequest, ProviderResponse};

const PROVIDER_ADDRESS: &str = "127.0.0.1:47777";

fn main() {
    if let Err(error) = run() {
        eprintln!("android_system_authority_failed error={error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut root = None;
    let mut state_file = None;
    let mut bootstrap_source = None;
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" => root = args.next().map(PathBuf::from),
            "--state-file" => state_file = args.next().map(PathBuf::from),
            "--bootstrap-source" => bootstrap_source = args.next().map(PathBuf::from),
            _ => return Err(format!("unexpected argument: {argument}").into()),
        }
    }
    let root = root.ok_or("--root requires a path")?;
    let state_file = state_file.ok_or("--state-file requires a path")?;
    let bootstrap_source = bootstrap_source.ok_or("--bootstrap-source requires a path")?;
    let authority = Arc::new(Mutex::new(AndroidSystemAuthority::open(
        root,
        state_file,
        &fs::read(bootstrap_source)?,
    )?));
    let provider = TcpListener::bind(PROVIDER_ADDRESS)?;
    let revisions = TcpListener::bind(REVISION_ADDRESS)?;
    println!(
        "android_system_authority_listening provider={} revision={}",
        PROVIDER_ADDRESS, REVISION_ADDRESS
    );
    let provider_authority = Arc::clone(&authority);
    thread::Builder::new()
        .name("sos-provider-authority".into())
        .spawn(move || serve_provider(provider, provider_authority))?;
    serve_revisions(revisions, authority)
}

fn serve_provider(listener: TcpListener, authority: Arc<Mutex<AndroidSystemAuthority>>) {
    for stream in listener.incoming() {
        match stream.and_then(|stream| handle_provider(stream, &authority)) {
            Ok(()) => {}
            Err(error) => eprintln!("provider_request_failed error={error}"),
        }
    }
}

fn handle_provider(
    mut stream: TcpStream,
    authority: &Arc<Mutex<AndroidSystemAuthority>>,
) -> std::io::Result<()> {
    let mut line = String::new();
    BufReader::new(stream.try_clone()?)
        .take(MAX_PROVIDER_REQUEST_BYTES + 1)
        .read_line(&mut line)?;
    if line.len() as u64 > MAX_PROVIDER_REQUEST_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "provider request exceeded its size limit",
        ));
    }
    let request = serde_json::from_str::<ProviderRequest>(&line).map_err(std::io::Error::other)?;
    let request_id = request.request_id();
    let response: ProviderResponse = authority
        .lock()
        .expect("Android authority lock")
        .dispatch_provider(request);
    serde_json::to_writer(&mut stream, &response).map_err(std::io::Error::other)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    println!(
        "provider_request_completed request_id={request_id} ok={}",
        response.ok
    );
    Ok(())
}

fn serve_revisions(
    listener: TcpListener,
    authority: Arc<Mutex<AndroidSystemAuthority>>,
) -> Result<(), Box<dyn std::error::Error>> {
    for stream in listener.incoming() {
        match stream.and_then(|stream| handle_revision(stream, &authority)) {
            Ok(()) => {}
            Err(error) => eprintln!("revision_request_failed error={error}"),
        }
    }
    Ok(())
}

fn handle_revision(
    mut stream: TcpStream,
    authority: &Arc<Mutex<AndroidSystemAuthority>>,
) -> std::io::Result<()> {
    let mut line = Vec::new();
    BufReader::new(stream.try_clone()?)
        .take(MAX_REVISION_REQUEST_BYTES + 1)
        .read_until(b'\n', &mut line)?;
    if line.len() as u64 > MAX_REVISION_REQUEST_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "revision request exceeded its size limit",
        ));
    }
    let request =
        serde_json::from_slice::<RevisionRequest>(&line).map_err(std::io::Error::other)?;
    let request_id = request.request_id();
    let response: RevisionResponse = authority
        .lock()
        .expect("Android authority lock")
        .dispatch_revision(request);
    serde_json::to_writer(&mut stream, &response).map_err(std::io::Error::other)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    println!(
        "revision_request_completed request_id={request_id} ok={}",
        response.ok
    );
    Ok(())
}
