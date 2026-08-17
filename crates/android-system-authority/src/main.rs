use std::{
    env, fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::net::UnixListener,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use android_authority_protocol::{
    read_provider_request, read_revision_request, write_provider_response, write_revision_response,
    RevisionResponse, CORE_PROVIDER_SOCKET, CORE_REVISION_SOCKET, REVISION_ADDRESS,
};
use android_system_authority::AndroidSystemAuthority;

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
    match fs::remove_file(CORE_PROVIDER_SOCKET) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let core_provider = UnixListener::bind(CORE_PROVIDER_SOCKET)?;
    match fs::remove_file(CORE_REVISION_SOCKET) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let core_revisions = UnixListener::bind(CORE_REVISION_SOCKET)?;
    println!(
        "android_system_authority_listening provider={} revision={} core_provider={} core_revision={}",
        PROVIDER_ADDRESS, REVISION_ADDRESS, CORE_PROVIDER_SOCKET, CORE_REVISION_SOCKET
    );
    let provider_authority = Arc::clone(&authority);
    thread::Builder::new()
        .name("sos-provider-authority".into())
        .spawn(move || serve_provider(provider, provider_authority))?;
    let core_provider_authority = Arc::clone(&authority);
    thread::Builder::new()
        .name("sos-core-provider-authority".into())
        .spawn(move || serve_core_provider(core_provider, core_provider_authority))?;
    let core_authority = Arc::clone(&authority);
    thread::Builder::new()
        .name("sos-core-revision-authority".into())
        .spawn(move || serve_core_revisions(core_revisions, core_authority))?;
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

fn serve_core_provider(listener: UnixListener, authority: Arc<Mutex<AndroidSystemAuthority>>) {
    for stream in listener.incoming() {
        match stream.and_then(|stream| handle_provider(stream, &authority)) {
            Ok(()) => {}
            Err(error) => eprintln!("core_provider_request_failed error={error}"),
        }
    }
}

fn handle_provider<S: Read + Write>(
    mut stream: S,
    authority: &Arc<Mutex<AndroidSystemAuthority>>,
) -> std::io::Result<()> {
    let request = read_provider_request(&mut stream)?;
    let request_id = request.request_id();
    let response = authority
        .lock()
        .expect("Android authority lock")
        .dispatch_provider(request);
    write_provider_response(&mut stream, &response)?;
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

fn serve_core_revisions(listener: UnixListener, authority: Arc<Mutex<AndroidSystemAuthority>>) {
    for stream in listener.incoming() {
        match stream.and_then(|stream| handle_revision(stream, &authority)) {
            Ok(()) => {}
            Err(error) => eprintln!("core_revision_request_failed error={error}"),
        }
    }
}

fn handle_revision<S: Read + Write>(
    mut stream: S,
    authority: &Arc<Mutex<AndroidSystemAuthority>>,
) -> std::io::Result<()> {
    let request = read_revision_request(&mut stream)?;
    let request_id = request.request_id();
    let response: RevisionResponse = authority
        .lock()
        .expect("Android authority lock")
        .dispatch_revision(request);
    write_revision_response(&mut stream, &response)?;
    println!(
        "revision_request_completed request_id={request_id} ok={}",
        response.ok
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read as _, Write as _},
        net::Shutdown,
        os::unix::net::UnixStream,
        sync::{Arc, Mutex},
        thread,
    };

    use android_authority_protocol::{request_provider_over_stream, RevisionRequest};
    use android_provider_acceptance::{run_probe, ProbeStatus};
    use experience_ir::{ProviderRequest, ProviderResponse};

    use super::{handle_provider, handle_revision, AndroidSystemAuthority};

    fn test_authority() -> (tempfile::TempDir, Arc<Mutex<AndroidSystemAuthority>>) {
        let temporary = tempfile::tempdir().unwrap();
        let authority = AndroidSystemAuthority::open(
            temporary.path().join("revisions"),
            temporary.path().join("provider.json"),
            b"return { api_version = 3 }",
        )
        .unwrap();
        (temporary, Arc::new(Mutex::new(authority)))
    }

    fn round_trip(
        authority: &Arc<Mutex<AndroidSystemAuthority>>,
        request: ProviderRequest,
    ) -> Result<ProviderResponse, String> {
        let (client, server) = UnixStream::pair().unwrap();
        let server_authority = Arc::clone(authority);
        let server = thread::spawn(move || handle_provider(server, &server_authority));
        let response = request_provider_over_stream(client, request);
        let server_result = server.join().expect("provider handler thread");
        if let Err(error) = server_result {
            return Err(error.to_string());
        }
        response
    }

    fn revision_exchange(
        authority: &Arc<Mutex<AndroidSystemAuthority>>,
        bytes: &[u8],
    ) -> (std::io::Result<()>, Vec<u8>) {
        let (mut client, server) = UnixStream::pair().unwrap();
        let server_authority = Arc::clone(authority);
        let worker = thread::spawn(move || handle_revision(server, &server_authority));
        client.write_all(bytes).unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        (worker.join().unwrap(), response)
    }

    #[test]
    fn malformed_revision_connections_do_not_poison_the_authority() {
        let (_temporary, authority) = test_authority();

        let (empty, response) = revision_exchange(&authority, b"");
        assert_eq!(empty.unwrap_err().kind(), std::io::ErrorKind::UnexpectedEof);
        assert!(response.is_empty());

        let (truncated, response) =
            revision_exchange(&authority, br#"{"action":"current","request_id":2}"#);
        assert_eq!(
            truncated.unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
        assert!(response.is_empty());

        let (malformed, response) = revision_exchange(&authority, b"{\n");
        assert_eq!(
            malformed.unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        assert!(response.is_empty());

        let mut valid = serde_json::to_vec(&RevisionRequest::Current { request_id: 3 }).unwrap();
        valid.push(b'\n');
        let (result, response) = revision_exchange(&authority, &valid);
        result.unwrap();
        let decoded: android_authority_protocol::RevisionResponse =
            serde_json::from_slice(&response).unwrap();
        assert!(decoded.ok);
        assert_eq!(decoded.request_id, 3);
    }

    #[test]
    fn probe_snapshot_uses_the_real_authority_handler_and_framing() {
        let (_temporary, authority) = test_authority();
        let report = run_probe("snapshot", |request| round_trip(&authority, request));
        assert_eq!(report.status, ProbeStatus::Pass, "{:?}", report.lines);
    }

    #[test]
    fn probe_security_maps_the_real_expected_rejection() {
        let (_temporary, authority) = test_authority();
        let report = run_probe("security", |request| round_trip(&authority, request));
        assert_eq!(report.status, ProbeStatus::Pass, "{:?}", report.lines);
        assert!(report.lines[0].contains("injected_privileged_capability=rejected"));
    }

    #[test]
    fn probe_unavailable_mode_uses_the_real_authority_rejection() {
        let (_temporary, authority) = test_authority();
        let report = run_probe("unavailable", |request| round_trip(&authority, request));
        assert_eq!(report.status, ProbeStatus::Pass, "{:?}", report.lines);
        assert!(report.lines[0].contains("semantics=explicit_rejection"));
    }

    #[test]
    fn authority_reports_broken_pipe_when_client_closes_before_response() {
        let (_temporary, authority) = test_authority();
        let (mut client, server) = UnixStream::pair().unwrap();
        serde_json::to_writer(&mut client, &ProviderRequest::Snapshot { request_id: 77 }).unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();
        drop(client);

        let error = handle_provider(server, &authority).unwrap_err();
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
        ));
    }
}
