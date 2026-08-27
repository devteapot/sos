use std::{
    env, fs,
    io::{self, Read, Write},
    net::{SocketAddrV4, TcpListener},
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    os::unix::net::UnixListener,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

#[cfg(target_os = "android")]
use std::ffi::CString;

use android_authority_protocol::{
    read_provider_request, read_revision_request, write_provider_response, write_revision_response,
    RevisionResponse, CORE_PROVIDER_SOCKET, CORE_REVISION_SOCKET, REVISION_ADDRESS,
};
use android_system_authority::AndroidSystemAuthority;

const PROVIDER_ADDRESS: &str = "127.0.0.1:47777";
const SOCKET_CREATE_STEP: &str = "raw socket step=socket(AF_INET, SOCK_STREAM)";
const SOCKET_REUSE_STEP: &str = "raw socket step=setsockopt(SO_REUSEADDR)";
const SOCKET_BIND_STEP: &str = "raw socket step=bind";
const SOCKET_LISTEN_STEP: &str = "raw socket step=listen";

#[cfg(target_os = "android")]
const ANDROID_LOG_ERROR: libc::c_int = 6;

#[cfg(target_os = "android")]
#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(
        priority: libc::c_int,
        tag: *const libc::c_char,
        text: *const libc::c_char,
    ) -> libc::c_int;
}

fn main() {
    if let Err(error) = run() {
        report_fatal(&fatal_message(error.as_ref()));
        std::process::exit(1);
    }
}

fn fatal_message(error: &dyn std::error::Error) -> String {
    format!("android_system_authority_failed error={error}")
}

fn report_fatal(message: &str) {
    eprintln!("{message}");
    #[cfg(target_os = "android")]
    if let (Ok(tag), Ok(text)) = (CString::new("sos-authority"), CString::new(message)) {
        unsafe {
            __android_log_write(ANDROID_LOG_ERROR, tag.as_ptr(), text.as_ptr());
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut root = None;
    let mut state_file = None;
    let mut bootstrap_source = None;
    let mut bootstrap_package = None;
    let mut bootstrap_assets = Vec::new();
    let mut appearance_writer_file = None;
    let mut install_reference_composition = false;
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" => root = args.next().map(PathBuf::from),
            "--state-file" => state_file = args.next().map(PathBuf::from),
            "--bootstrap-source" => bootstrap_source = args.next().map(PathBuf::from),
            "--bootstrap-package" => bootstrap_package = args.next().map(PathBuf::from),
            "--appearance-writer-file" => appearance_writer_file = args.next().map(PathBuf::from),
            "--install-reference-composition" => install_reference_composition = true,
            "--bootstrap-asset" => {
                let id = args
                    .next()
                    .ok_or("--bootstrap-asset requires ID KIND PATH")?;
                let kind = args
                    .next()
                    .ok_or("--bootstrap-asset requires ID KIND PATH")?;
                let path = args
                    .next()
                    .map(PathBuf::from)
                    .ok_or("--bootstrap-asset requires ID KIND PATH")?;
                bootstrap_assets.push((id, kind, path));
            }
            _ => return Err(format!("unexpected argument: {argument}").into()),
        }
    }
    let root = root.ok_or("--root requires a path")?;
    let state_file = state_file.ok_or("--state-file requires a path")?;
    let bootstrap_source = bootstrap_source.ok_or("--bootstrap-source requires a path")?;
    let bootstrap_source = fs::read(&bootstrap_source).map_err(|error| {
        format!(
            "read bootstrap source {} failed: {error}",
            bootstrap_source.display()
        )
    })?;
    let mut authority = if let Some(package_path) = bootstrap_package {
        let package: experience_package::PackageMetadata =
            serde_json::from_slice(&fs::read(&package_path).map_err(|error| {
                format!(
                    "read bootstrap package {} failed: {error}",
                    package_path.display()
                )
            })?)
            .map_err(|error| {
                format!(
                    "decode bootstrap package {} failed: {error}",
                    package_path.display()
                )
            })?;
        let assets = bootstrap_assets
            .into_iter()
            .map(|(id, kind, path)| {
                Ok(revision_supervisor::RevisionAssetInput {
                    id,
                    kind,
                    bytes: fs::read(&path).map_err(|error| {
                        io::Error::new(
                            error.kind(),
                            format!("read bootstrap asset {} failed: {error}", path.display()),
                        )
                    })?,
                })
            })
            .collect::<Result<Vec<_>, std::io::Error>>()?;
        AndroidSystemAuthority::open_v4(
            root,
            state_file,
            revision_supervisor::RevisionPackageInput {
                revision: revision_supervisor::RevisionInput {
                    source: bootstrap_source,
                    state: serde_json::json!({}),
                    schema_version: 1,
                    experience_api_version: experience_ir::EXPERIENCE_API_VERSION_V4,
                    assets,
                },
                package,
            },
        )
        .map_err(|error| format!("open v4 authority failed: {error}"))?
    } else {
        AndroidSystemAuthority::open(root, state_file, &bootstrap_source)
            .map_err(|error| format!("open legacy authority failed: {error}"))?
    };
    if install_reference_composition {
        authority
            .install_reference_composition()
            .map_err(|error| format!("install reference composition failed: {error}"))?;
    }
    if let Some(path) = appearance_writer_file {
        let capability = fs::read_to_string(&path).map_err(|error| {
            format!("read appearance writer {} failed: {error}", path.display())
        })?;
        authority
            .configure_appearance_writer(capability.trim())
            .map_err(|error| format!("configure appearance writer failed: {error}"))?;
    }
    let authority = Arc::new(Mutex::new(authority));
    let provider = bind_reusable_tcp(PROVIDER_ADDRESS)
        .map_err(|error| format!("bind provider listener {PROVIDER_ADDRESS} failed: {error}"))?;
    let revisions = bind_reusable_tcp(REVISION_ADDRESS)
        .map_err(|error| format!("bind revision listener {REVISION_ADDRESS} failed: {error}"))?;
    match fs::remove_file(CORE_PROVIDER_SOCKET) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "remove core provider socket {CORE_PROVIDER_SOCKET} failed: {error}"
            )
            .into())
        }
    }
    let core_provider = UnixListener::bind(CORE_PROVIDER_SOCKET).map_err(|error| {
        format!("bind core provider listener {CORE_PROVIDER_SOCKET} failed: {error}")
    })?;
    match fs::remove_file(CORE_REVISION_SOCKET) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "remove core revision socket {CORE_REVISION_SOCKET} failed: {error}"
            )
            .into())
        }
    }
    let core_revisions = UnixListener::bind(CORE_REVISION_SOCKET).map_err(|error| {
        format!("bind core revision listener {CORE_REVISION_SOCKET} failed: {error}")
    })?;
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

fn bind_reusable_tcp(address: &str) -> io::Result<TcpListener> {
    let address = address.parse::<SocketAddrV4>().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("parse TCP listener address {address} failed: {error}"),
        )
    })?;
    let raw_fd = unsafe {
        libc::socket(
            libc::AF_INET,
            libc::SOCK_STREAM | libc::SOCK_CLOEXEC,
            libc::IPPROTO_TCP,
        )
    };
    if raw_fd < 0 {
        return Err(socket_step_error(SOCKET_CREATE_STEP, &address));
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
    let enabled: libc::c_int = 1;
    let result = unsafe {
        libc::setsockopt(
            fd.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            (&enabled as *const libc::c_int).cast(),
            std::mem::size_of_val(&enabled) as libc::socklen_t,
        )
    };
    if result < 0 {
        return Err(socket_step_error(SOCKET_REUSE_STEP, &address));
    }
    let socket_address = libc::sockaddr_in {
        sin_family: libc::AF_INET as libc::sa_family_t,
        sin_port: address.port().to_be(),
        sin_addr: libc::in_addr {
            s_addr: u32::from_ne_bytes(address.ip().octets()),
        },
        sin_zero: [0; 8],
    };
    let result = unsafe {
        libc::bind(
            fd.as_raw_fd(),
            (&socket_address as *const libc::sockaddr_in).cast(),
            std::mem::size_of_val(&socket_address) as libc::socklen_t,
        )
    };
    if result < 0 {
        return Err(socket_step_error(SOCKET_BIND_STEP, &address));
    }
    if unsafe { libc::listen(fd.as_raw_fd(), libc::SOMAXCONN) } < 0 {
        return Err(socket_step_error(SOCKET_LISTEN_STEP, &address));
    }
    Ok(TcpListener::from(fd))
}

fn socket_step_error(step: &str, address: &SocketAddrV4) -> io::Error {
    let source = io::Error::last_os_error();
    io::Error::new(
        source.kind(),
        format!("{step} for {address} failed: {source}"),
    )
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
        net::{Shutdown, TcpStream},
        os::unix::net::UnixStream,
        sync::{Arc, Mutex},
        thread,
    };

    use android_authority_protocol::{request_provider_over_stream, RevisionRequest};
    use android_provider_acceptance::{run_probe, ProbeStatus};
    use experience_ir::{ProviderRequest, ProviderResponse};

    use super::{
        bind_reusable_tcp, fatal_message, handle_provider, handle_revision, AndroidSystemAuthority,
    };

    #[test]
    fn fatal_message_preserves_startup_context() {
        let error = std::io::Error::other(
            "install reference composition failed: reference registry incomplete",
        );
        assert_eq!(
            fatal_message(&error),
            "android_system_authority_failed error=install reference composition failed: reference registry incomplete"
        );
    }

    #[test]
    fn tcp_listener_rebinds_immediately_after_a_live_connection() {
        let first = bind_reusable_tcp("127.0.0.1:0").unwrap();
        let address = first.local_addr().unwrap();
        let client = thread::spawn(move || TcpStream::connect(address).unwrap());
        let (server, _) = first.accept().unwrap();
        server.shutdown(Shutdown::Both).unwrap();
        drop(server);
        drop(client.join().unwrap());
        drop(first);

        let rebound = bind_reusable_tcp(&address.to_string()).unwrap();
        assert_eq!(rebound.local_addr().unwrap(), address);
    }

    #[test]
    fn tcp_listener_reports_the_failing_raw_step() {
        let error = bind_reusable_tcp("192.0.2.1:0").unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("raw socket step=bind for 192.0.2.1:0 failed:"),
            "{error}"
        );
    }

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
