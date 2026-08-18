#[cfg(any(
    all(target_os = "android", feature = "core-dev-credential"),
    core_dev_credential_protocol_host_test
))]
use std::io::{Read, Write};
#[cfg(all(target_os = "android", feature = "core-dev-credential"))]
use std::{
    mem::{size_of, zeroed},
    os::fd::{FromRawFd, OwnedFd},
    os::unix::net::{UnixListener, UnixStream},
    thread,
    time::Duration,
};

use zeroize::{Zeroize, Zeroizing};

use crate::core_credential::{MAX_CREDENTIAL_BYTES, MIN_CREDENTIAL_BYTES, OPENROUTER_KEY_PREFIX};
#[cfg(all(target_os = "android", feature = "core-dev-credential"))]
use crate::core_dev_product::{validate_dev_product, DevProductMarkers};

#[cfg(all(target_os = "android", feature = "core-dev-credential"))]
const SOCKET_NAME: &[u8] = b"sos_core_dev_credential_v1";
#[allow(dead_code)]
mod v1 {
    include!(concat!(
        env!("OUT_DIR"),
        "/core_dev_credential_protocol_v1.rs"
    ));
}
const MAGIC: [u8; 4] = [
    v1::MAGIC_0 as u8,
    v1::MAGIC_1 as u8,
    v1::MAGIC_2 as u8,
    v1::MAGIC_3 as u8,
];
const VERSION: u8 = v1::VERSION as u8;
const OP_PROBE: u8 = v1::OP_PROBE as u8;
const OP_SET: u8 = v1::OP_SET as u8;
const OP_CLEAR: u8 = v1::OP_CLEAR as u8;
const OP_STATUS: u8 = v1::OP_STATUS as u8;
const OP_AGENT_SMOKE: u8 = v1::OP_AGENT_SMOKE as u8;
const STATUS_OK: u8 = v1::STATUS_OK as u8;
#[cfg(any(
    all(target_os = "android", feature = "core-dev-credential"),
    core_dev_credential_protocol_host_test
))]
const STATUS_REJECTED: u8 = v1::STATUS_REJECTED as u8;
#[cfg(any(
    all(target_os = "android", feature = "core-dev-credential"),
    core_dev_credential_protocol_host_test
))]
const STATUS_WRONG_PEER: u8 = v1::STATUS_WRONG_PEER as u8;
#[cfg(any(
    all(target_os = "android", feature = "core-dev-credential"),
    core_dev_credential_protocol_host_test
))]
const STATUS_PROTOCOL_MISMATCH: u8 = v1::STATUS_PROTOCOL_MISMATCH as u8;
const STATUS_CONFIGURED: u8 = v1::STATUS_CONFIGURED as u8;
const STATUS_EMPTY: u8 = v1::STATUS_EMPTY as u8;
const HEADER_BYTES: usize = v1::REQUEST_HEADER_BYTES;
#[cfg(any(
    all(target_os = "android", feature = "core-dev-credential"),
    core_dev_credential_protocol_host_test
))]
const ACK_BYTES: usize = v1::ACK_BYTES;
const MAX_FRAME_BYTES: usize = HEADER_BYTES + MAX_CREDENTIAL_BYTES;
const SHELL_UID: u32 = 2000;
const DEV_CLIENT_CONTEXT: &[u8] = b"u:r:sos_core_dev_credential:s0";
#[cfg(all(target_os = "android", feature = "core-dev-credential"))]
const IO_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, PartialEq, Eq)]
enum Request {
    Probe,
    Set(Zeroizing<Vec<u8>>),
    Clear,
    Status,
    AgentSmoke,
}

fn decode_request(mut frame: Zeroizing<Vec<u8>>) -> Result<Request, &'static str> {
    if frame.len() < HEADER_BYTES {
        return Err("short_io");
    }
    if frame[..4] != MAGIC {
        return Err("bad_magic");
    }
    if frame[4] != VERSION {
        return Err("bad_version");
    }
    let operation = frame[5];
    let payload_length = u16::from_be_bytes([frame[6], frame[7]]) as usize;
    if frame.len() != HEADER_BYTES + payload_length {
        return Err("length");
    }
    match operation {
        OP_PROBE if payload_length == 0 => Ok(Request::Probe),
        OP_SET => {
            if !(MIN_CREDENTIAL_BYTES..=MAX_CREDENTIAL_BYTES).contains(&payload_length) {
                return Err("credential_length");
            }
            let mut key = Zeroizing::new(frame.split_off(HEADER_BYTES));
            frame.zeroize();
            if !key.starts_with(OPENROUTER_KEY_PREFIX.as_bytes())
                || !key.iter().all(u8::is_ascii_graphic)
            {
                key.zeroize();
                return Err("credential_format");
            }
            Ok(Request::Set(key))
        }
        OP_CLEAR if payload_length == 0 => Ok(Request::Clear),
        OP_STATUS if payload_length == 0 => Ok(Request::Status),
        OP_AGENT_SMOKE if payload_length == 0 => Ok(Request::AgentSmoke),
        _ => Err("operation"),
    }
}

#[cfg(any(
    all(target_os = "android", feature = "core-dev-credential"),
    core_dev_credential_protocol_host_test
))]
fn read_request(stream: &mut impl Read) -> Result<Request, &'static str> {
    let mut header = [0_u8; HEADER_BYTES];
    stream.read_exact(&mut header).map_err(|_| "short_io")?;
    if header[..4] != MAGIC {
        return Err("bad_magic");
    }
    if header[4] != VERSION {
        return Err("bad_version");
    }
    let payload_length = u16::from_be_bytes([header[6], header[7]]) as usize;
    if HEADER_BYTES + payload_length > MAX_FRAME_BYTES {
        return Err("request_too_large");
    }
    let mut frame = Zeroizing::new(Vec::with_capacity(HEADER_BYTES + payload_length));
    frame.extend_from_slice(&header);
    frame.resize(HEADER_BYTES + payload_length, 0);
    stream
        .read_exact(&mut frame[HEADER_BYTES..])
        .map_err(|_| "short_io")?;
    let mut trailing = [0_u8; 1];
    match stream.read(&mut trailing) {
        Ok(0) => decode_request(frame),
        Ok(_) => Err("trailing_data"),
        Err(_) => Err("short_io"),
    }
}

fn authorized_peer(uid: u32, context: &[u8]) -> bool {
    uid == SHELL_UID && context == DEV_CLIENT_CONTEXT
}

fn apply_request(
    request: Request,
    mut set: impl FnMut(&[u8]) -> bool,
    mut clear: impl FnMut(),
    mut configured: impl FnMut() -> bool,
    mut agent_smoke: impl FnMut() -> bool,
) -> u8 {
    match request {
        Request::Probe => STATUS_OK,
        Request::Set(key) => {
            if set(&key) {
                STATUS_OK
            } else {
                STATUS_REJECTED
            }
        }
        Request::Clear => {
            clear();
            STATUS_OK
        }
        Request::Status => {
            if configured() {
                STATUS_CONFIGURED
            } else {
                STATUS_EMPTY
            }
        }
        Request::AgentSmoke => {
            if configured() && agent_smoke() {
                STATUS_OK
            } else {
                STATUS_REJECTED
            }
        }
    }
}

#[cfg(target_os = "android")]
fn property(name: &str) -> String {
    use std::ffi::{c_char, c_int, CStr, CString};
    unsafe extern "C" {
        fn __system_property_get(name: *const c_char, value: *mut c_char) -> c_int;
    }
    let Ok(name) = CString::new(name) else {
        return String::new();
    };
    let mut value = [0 as c_char; 92];
    // SAFETY: bionic writes at most PROP_VALUE_MAX bytes and both buffers are valid.
    let length = unsafe { __system_property_get(name.as_ptr(), value.as_mut_ptr()) };
    if length <= 0 {
        return String::new();
    }
    unsafe { CStr::from_ptr(value.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(all(target_os = "android", feature = "core-dev-credential"))]
struct OwnedProductMarkerFailure {
    name: &'static str,
    expected: &'static str,
    actual: String,
}

#[cfg(all(target_os = "android", feature = "core-dev-credential"))]
fn validate_running_dev_product() -> Result<(), OwnedProductMarkerFailure> {
    let revision = property("ro.build.version.incremental");
    let build_variant = property("ro.sos.build_variant");
    let dev_credential = property("ro.sos.dev_credential");
    let build_type = property("ro.build.type");
    let debuggable = property("ro.debuggable");
    validate_dev_product(DevProductMarkers {
        revision: &revision,
        build_variant: &build_variant,
        dev_credential: &dev_credential,
        build_type: &build_type,
        debuggable: &debuggable,
    })
    .map_err(|failure| OwnedProductMarkerFailure {
        name: failure.name,
        expected: failure.expected,
        actual: failure.actual.to_owned(),
    })
}

#[cfg(all(target_os = "android", feature = "core-dev-credential"))]
fn bind_listener() -> std::io::Result<UnixListener> {
    // SAFETY: every raw descriptor is transferred to OwnedFd exactly once.
    let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
    if raw < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let owned = unsafe { OwnedFd::from_raw_fd(raw) };
    let mut address = unsafe { zeroed::<libc::sockaddr_un>() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    if SOCKET_NAME.len() + 1 > address.sun_path.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "socket name",
        ));
    }
    for (index, byte) in SOCKET_NAME.iter().enumerate() {
        address.sun_path[index + 1] = *byte as libc::c_char;
    }
    let address_length =
        (size_of::<libc::sa_family_t>() + 1 + SOCKET_NAME.len()) as libc::socklen_t;
    if unsafe {
        libc::bind(
            raw,
            &address as *const libc::sockaddr_un as *const libc::sockaddr,
            address_length,
        )
    } != 0
        || unsafe { libc::listen(raw, 1) } != 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(UnixListener::from(owned))
}

#[cfg(all(target_os = "android", feature = "core-dev-credential"))]
fn peer_identity(stream: &UnixStream) -> Result<(u32, Vec<u8>), &'static str> {
    use std::os::fd::AsRawFd;
    let mut credentials = unsafe { zeroed::<libc::ucred>() };
    let mut credentials_length = size_of::<libc::ucred>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut credentials as *mut libc::ucred as *mut libc::c_void,
            &mut credentials_length,
        )
    } != 0
    {
        return Err("peer_credentials");
    }
    let mut context = vec![0_u8; 128];
    let mut context_length = context.len() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERSEC,
            context.as_mut_ptr().cast(),
            &mut context_length,
        )
    } != 0
    {
        return Err("peer_context");
    }
    context.truncate(context_length as usize);
    while context.last() == Some(&0) {
        context.pop();
    }
    Ok((credentials.uid, context))
}

#[cfg(all(target_os = "android", feature = "core-dev-credential"))]
fn serve_connection(mut stream: UnixStream) -> Result<(), &'static str> {
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|_| "timeout")?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|_| "timeout")?;
    let (uid, context) = peer_identity(&stream)?;
    if !authorized_peer(uid, &context) {
        write_status(&mut stream, STATUS_WRONG_PEER)?;
        return Err("peer_rejected");
    }
    serve_protocol(
        &mut stream,
        crate::android::install_dev_openrouter_credential,
        crate::android::clear_dev_openrouter_credential,
        crate::android::dev_openrouter_credential_configured,
        crate::android::request_dev_agent_smoke,
    )
}

#[cfg(any(
    all(target_os = "android", feature = "core-dev-credential"),
    core_dev_credential_protocol_host_test
))]
fn serve_protocol(
    stream: &mut (impl Read + Write),
    set: impl FnMut(&[u8]) -> bool,
    clear: impl FnMut(),
    configured: impl FnMut() -> bool,
    agent_smoke: impl FnMut() -> bool,
) -> Result<(), &'static str> {
    let request = match read_request(stream) {
        Ok(request) => request,
        Err(category) => {
            write_status(stream, STATUS_PROTOCOL_MISMATCH)?;
            return Err(category);
        }
    };
    let status = apply_request(request, set, clear, configured, agent_smoke);
    write_status(stream, status)
}

#[cfg(any(
    all(target_os = "android", feature = "core-dev-credential"),
    core_dev_credential_protocol_host_test
))]
fn write_status(stream: &mut impl Write, status: u8) -> Result<(), &'static str> {
    let response = [MAGIC[0], MAGIC[1], MAGIC[2], MAGIC[3], VERSION, status];
    debug_assert_eq!(response.len(), ACK_BYTES);
    stream.write_all(&response).map_err(|_| "response_io")
}

#[cfg(all(target_os = "android", feature = "core-dev-credential"))]
pub fn start() -> Result<(), &'static str> {
    if let Err(failure) = validate_running_dev_product() {
        log::warn!(
            "core_dev_credential state=unavailable marker={} expected={} actual={}",
            failure.name,
            failure.expected,
            failure.actual
        );
        return Err("build_gate");
    }
    let listener = bind_listener().map_err(|_| "bind")?;
    thread::Builder::new()
        .name("sos-dev-credential".into())
        .spawn(move || {
            log::info!(
                "core_dev_credential state=ready transport=local peer=sos_core_dev_credential"
            );
            for connection in listener.incoming() {
                let result = connection.map_err(|_| "accept").and_then(serve_connection);
                if let Err(category) = result {
                    log::warn!("core_dev_credential state=rejected category={category}");
                }
            }
        })
        .map_err(|_| "thread")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    #[cfg(core_dev_credential_protocol_host_test)]
    use std::io::Cursor;
    #[cfg(core_dev_credential_protocol_host_test)]
    use std::{
        fs,
        net::Shutdown,
        os::unix::net::{UnixListener, UnixStream},
        path::PathBuf,
        process::{Command, Output, Stdio},
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc,
        },
        thread,
    };

    const SYNTHETIC_KEY: &[u8] = b"sk-or-v1-0123456789abcdef01234567";
    const PROBE_GOLDEN: &[u8] = b"SOSK\x01\x00\x00\x00";
    const CLEAR_GOLDEN: &[u8] = b"SOSK\x01\x02\x00\x00";
    const STATUS_GOLDEN: &[u8] = b"SOSK\x01\x03\x00\x00";
    const AGENT_SMOKE_GOLDEN: &[u8] = b"SOSK\x01\x04\x00\x00";
    const SET_GOLDEN: &[u8] = b"SOSK\x01\x01\x00\x21sk-or-v1-0123456789abcdef01234567";
    const OK_GOLDEN: &[u8] = b"SOSK\x01\x01";
    #[cfg(core_dev_credential_protocol_host_test)]
    static SOCKET_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

    #[cfg(core_dev_credential_protocol_host_test)]
    struct OneByteIo(UnixStream);

    #[cfg(core_dev_credential_protocol_host_test)]
    impl std::io::Read for OneByteIo {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let limit = buffer.len().min(1);
            self.0.read(&mut buffer[..limit])
        }
    }

    #[cfg(core_dev_credential_protocol_host_test)]
    impl std::io::Write for OneByteIo {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.0.write(&buffer[..buffer.len().min(1)])
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.0.flush()
        }
    }

    #[cfg(core_dev_credential_protocol_host_test)]
    struct FailingAck(Cursor<Vec<u8>>);

    #[cfg(core_dev_credential_protocol_host_test)]
    impl std::io::Read for FailingAck {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.0.read(buffer)
        }
    }

    #[cfg(core_dev_credential_protocol_host_test)]
    impl std::io::Write for FailingAck {
        fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn frame(operation: u8, payload: &[u8]) -> Zeroizing<Vec<u8>> {
        let mut frame = Zeroizing::new(Vec::new());
        frame.extend_from_slice(&MAGIC);
        frame.push(VERSION);
        frame.push(operation);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    #[test]
    fn exact_v1_frames_decode_without_exposing_payload() {
        let key = SYNTHETIC_KEY;
        assert_eq!(&*frame(OP_PROBE, b""), PROBE_GOLDEN);
        assert_eq!(&*frame(OP_CLEAR, b""), CLEAR_GOLDEN);
        assert_eq!(&*frame(OP_STATUS, b""), STATUS_GOLDEN);
        assert_eq!(&*frame(OP_AGENT_SMOKE, b""), AGENT_SMOKE_GOLDEN);
        assert_eq!(&*frame(OP_SET, key), SET_GOLDEN);
        assert_eq!(
            [MAGIC[0], MAGIC[1], MAGIC[2], MAGIC[3], VERSION, STATUS_OK],
            OK_GOLDEN
        );
        assert_eq!(MAX_CREDENTIAL_BYTES, v1::MAX_PAYLOAD_BYTES);
        assert_eq!(
            decode_request(frame(OP_SET, key)),
            Ok(Request::Set(Zeroizing::new(key.to_vec())))
        );
        assert_eq!(decode_request(frame(OP_CLEAR, b"")), Ok(Request::Clear));
        assert_eq!(decode_request(frame(OP_PROBE, b"")), Ok(Request::Probe));
        assert_eq!(decode_request(frame(OP_STATUS, b"")), Ok(Request::Status));
        assert_eq!(
            decode_request(frame(OP_AGENT_SMOKE, b"")),
            Ok(Request::AgentSmoke)
        );
    }

    #[test]
    fn protocol_rejects_wrong_magic_version_length_operation_and_credentials() {
        assert_eq!(MAX_FRAME_BYTES, HEADER_BYTES + MAX_CREDENTIAL_BYTES);
        let key = SYNTHETIC_KEY;
        let mut wrong_magic = frame(OP_SET, key);
        wrong_magic[0] = b'X';
        assert_eq!(decode_request(wrong_magic), Err("bad_magic"));
        let mut wrong_version = frame(OP_SET, key);
        wrong_version[4] = 2;
        assert_eq!(decode_request(wrong_version), Err("bad_version"));
        let mut wrong_length = frame(OP_SET, key);
        wrong_length[7] -= 1;
        assert_eq!(decode_request(wrong_length), Err("length"));
        assert_eq!(decode_request(frame(9, b"")), Err("operation"));
        assert_eq!(decode_request(frame(OP_PROBE, b"x")), Err("operation"));
        assert_eq!(decode_request(frame(OP_STATUS, b"x")), Err("operation"));
        assert_eq!(
            decode_request(frame(OP_AGENT_SMOKE, b"x")),
            Err("operation")
        );
        assert_eq!(
            decode_request(frame(OP_SET, b"short")),
            Err("credential_length")
        );
        assert_eq!(
            decode_request(frame(OP_SET, b"sk-or-v1-line\nbreak-is-rejected")),
            Err("credential_format")
        );
        assert_eq!(
            decode_request(frame(OP_SET, &vec![b'x'; MAX_CREDENTIAL_BYTES + 1])),
            Err("credential_length")
        );
    }

    #[test]
    fn only_the_exact_dedicated_client_peer_is_authorized() {
        assert!(authorized_peer(SHELL_UID, DEV_CLIENT_CONTEXT));
        assert!(!authorized_peer(0, DEV_CLIENT_CONTEXT));
        assert!(!authorized_peer(SHELL_UID, b"u:r:shell:s0"));
    }

    #[test]
    fn probe_does_not_read_or_mutate_credential_state() {
        let set_calls = Cell::new(0);
        let clear_calls = Cell::new(0);
        assert_eq!(
            apply_request(
                Request::Probe,
                |_| {
                    set_calls.set(set_calls.get() + 1);
                    true
                },
                || clear_calls.set(clear_calls.get() + 1),
                || false,
                || false,
            ),
            STATUS_OK
        );
        assert_eq!((set_calls.get(), clear_calls.get()), (0, 0));
    }

    #[test]
    fn clear_remains_available_after_a_rejected_set() {
        let set_calls = Cell::new(0);
        let clear_calls = Cell::new(0);
        assert_eq!(
            apply_request(
                Request::Set(Zeroizing::new(
                    b"sk-or-v1-0123456789abcdef01234567".to_vec()
                )),
                |_| {
                    set_calls.set(set_calls.get() + 1);
                    false
                },
                || clear_calls.set(clear_calls.get() + 1),
                || false,
                || false,
            ),
            STATUS_REJECTED
        );
        assert_eq!(
            apply_request(
                Request::Clear,
                |_| true,
                || clear_calls.set(clear_calls.get() + 1),
                || false,
                || false,
            ),
            STATUS_OK
        );
        assert_eq!((set_calls.get(), clear_calls.get()), (1, 1));
    }

    #[test]
    fn status_is_secret_free_and_does_not_mutate_state() {
        let set_calls = Cell::new(0);
        let clear_calls = Cell::new(0);
        let status_calls = Cell::new(0);
        for (configured, expected) in [(false, STATUS_EMPTY), (true, STATUS_CONFIGURED)] {
            assert_eq!(
                apply_request(
                    Request::Status,
                    |_| {
                        set_calls.set(set_calls.get() + 1);
                        true
                    },
                    || clear_calls.set(clear_calls.get() + 1),
                    || {
                        status_calls.set(status_calls.get() + 1);
                        configured
                    },
                    || false,
                ),
                expected
            );
        }
        assert_eq!(
            (set_calls.get(), clear_calls.get(), status_calls.get()),
            (0, 0, 2)
        );
    }

    #[test]
    fn agent_smoke_requires_configured_state_and_queues_only_the_fixed_action() {
        let smoke_calls = Cell::new(0);
        assert_eq!(
            apply_request(
                Request::AgentSmoke,
                |_| false,
                || {},
                || false,
                || {
                    smoke_calls.set(smoke_calls.get() + 1);
                    true
                },
            ),
            STATUS_REJECTED
        );
        assert_eq!(smoke_calls.get(), 0);
        assert_eq!(
            apply_request(
                Request::AgentSmoke,
                |_| false,
                || {},
                || true,
                || {
                    smoke_calls.set(smoke_calls.get() + 1);
                    true
                },
            ),
            STATUS_OK
        );
        assert_eq!(smoke_calls.get(), 1);
        assert_eq!(
            apply_request(Request::AgentSmoke, |_| false, || {}, || true, || false,),
            STATUS_REJECTED
        );
    }

    #[test]
    fn rejected_agent_smoke_retains_the_configured_credential_state() {
        let configured = Cell::new(true);
        let clear_calls = Cell::new(0);
        assert_eq!(
            apply_request(
                Request::AgentSmoke,
                |_| false,
                || {
                    clear_calls.set(clear_calls.get() + 1);
                    configured.set(false);
                },
                || configured.get(),
                || false,
            ),
            STATUS_REJECTED
        );
        assert!(configured.get());
        assert_eq!(clear_calls.get(), 0);
        assert_eq!(
            apply_request(
                Request::Status,
                |_| false,
                || clear_calls.set(clear_calls.get() + 1),
                || configured.get(),
                || false,
            ),
            STATUS_CONFIGURED
        );
        assert_eq!(clear_calls.get(), 0);
    }

    #[cfg(core_dev_credential_protocol_host_test)]
    fn socket_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "sos-core-dev-protocol-{}-{}.sock",
            std::process::id(),
            SOCKET_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[cfg(core_dev_credential_protocol_host_test)]
    fn run_cpp_client(operation: &str, stdin: &[u8], serve: impl FnOnce(UnixStream)) -> Output {
        let path = socket_path();
        let listener = UnixListener::bind(&path).unwrap();
        let mut child = Command::new(env!("CORE_DEV_CPP_CLIENT"))
            .arg(&path)
            .arg(operation)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(stdin).unwrap();
        let (stream, _) = listener.accept().unwrap();
        serve(stream);
        let output = child.wait_with_output().unwrap();
        drop(listener);
        fs::remove_file(path).unwrap();
        output
    }

    #[test]
    #[cfg(core_dev_credential_protocol_host_test)]
    fn production_cpp_client_and_rust_endpoint_interoperate_without_payload_output() {
        let set_calls = Cell::new(0);
        let clear_calls = Cell::new(0);
        let configured = Cell::new(false);
        let smoke_calls = Cell::new(0);

        let probe = run_cpp_client("probe", b"", |mut stream| {
            serve_protocol(
                &mut stream,
                |_| {
                    set_calls.set(set_calls.get() + 1);
                    true
                },
                || clear_calls.set(clear_calls.get() + 1),
                || configured.get(),
                || false,
            )
            .unwrap();
        });
        assert!(probe.status.success());
        assert_eq!(probe.stdout, b"core_dev_credential=READY\n");
        assert!(probe.stderr.is_empty());
        assert_eq!((set_calls.get(), clear_calls.get()), (0, 0));

        let mut input = SYNTHETIC_KEY.to_vec();
        input.push(b'\n');
        let set = run_cpp_client("set", &input, |mut stream| {
            serve_protocol(
                &mut stream,
                |key| {
                    assert_eq!(key, SYNTHETIC_KEY);
                    set_calls.set(set_calls.get() + 1);
                    configured.set(true);
                    true
                },
                || clear_calls.set(clear_calls.get() + 1),
                || configured.get(),
                || false,
            )
            .unwrap();
        });
        input.zeroize();
        assert!(set.status.success());
        assert_eq!(set.stdout, b"core_dev_credential=SET\n");
        assert!(set.stderr.is_empty());
        assert!(!set
            .stdout
            .windows(SYNTHETIC_KEY.len())
            .any(|w| w == SYNTHETIC_KEY));

        let status = run_cpp_client("status", b"", |mut stream| {
            serve_protocol(&mut stream, |_| false, || {}, || configured.get(), || false).unwrap();
        });
        assert!(status.status.success());
        assert_eq!(status.stdout, b"core_dev_credential=CONFIGURED\n");
        assert!(status.stderr.is_empty());

        let smoke = run_cpp_client("agent-smoke", b"", |mut stream| {
            serve_protocol(
                &mut stream,
                |_| false,
                || {},
                || configured.get(),
                || {
                    smoke_calls.set(smoke_calls.get() + 1);
                    true
                },
            )
            .unwrap();
        });
        assert!(smoke.status.success());
        assert_eq!(smoke.stdout, b"core_dev_agent_smoke=SUBMITTED\n");
        assert!(smoke.stderr.is_empty());
        assert_eq!(smoke_calls.get(), 1);

        let clear = run_cpp_client("clear", b"", |mut stream| {
            serve_protocol(
                &mut stream,
                |_| true,
                || {
                    clear_calls.set(clear_calls.get() + 1);
                    configured.set(false);
                },
                || configured.get(),
                || false,
            )
            .unwrap();
        });
        assert!(clear.status.success());
        assert_eq!(clear.stdout, b"core_dev_credential=CLEARED\n");
        assert!(clear.stderr.is_empty());
        let status = run_cpp_client("status", b"", |mut stream| {
            serve_protocol(&mut stream, |_| false, || {}, || configured.get(), || false).unwrap();
        });
        assert!(status.status.success());
        assert_eq!(status.stdout, b"core_dev_credential=EMPTY\n");
        assert!(status.stderr.is_empty());
        assert_eq!((set_calls.get(), clear_calls.get()), (1, 1));
    }

    #[test]
    #[cfg(core_dev_credential_protocol_host_test)]
    fn cpp_client_emits_golden_frames_and_handles_fragmented_acks_and_statuses() {
        for (operation, expected) in [
            ("probe", PROBE_GOLDEN),
            ("clear", CLEAR_GOLDEN),
            ("set", SET_GOLDEN),
            ("agent-smoke", AGENT_SMOKE_GOLDEN),
        ] {
            let input = if operation == "set" {
                [SYNTHETIC_KEY, b"\n"].concat()
            } else {
                Vec::new()
            };
            let output = run_cpp_client(operation, &input, |mut stream| {
                let mut actual = Vec::new();
                stream.read_to_end(&mut actual).unwrap();
                assert_eq!(actual, expected);
                for byte in OK_GOLDEN {
                    stream.write_all(&[*byte]).unwrap();
                }
            });
            assert!(output.status.success());
        }

        for (status, expected_output) in [
            (
                STATUS_CONFIGURED,
                b"core_dev_credential=CONFIGURED\n".as_slice(),
            ),
            (STATUS_EMPTY, b"core_dev_credential=EMPTY\n".as_slice()),
        ] {
            let output = run_cpp_client("status", b"", |mut stream| {
                let mut actual = Vec::new();
                stream.read_to_end(&mut actual).unwrap();
                assert_eq!(actual, STATUS_GOLDEN);
                let mut ack = OK_GOLDEN.to_vec();
                ack[5] = status;
                stream.write_all(&ack).unwrap();
            });
            assert!(output.status.success());
            assert_eq!(output.stdout, expected_output);
            assert!(output.stderr.is_empty());
        }

        for status in [STATUS_CONFIGURED, STATUS_EMPTY] {
            let output = run_cpp_client("probe", b"", |mut stream| {
                let mut request = Vec::new();
                stream.read_to_end(&mut request).unwrap();
                let mut ack = OK_GOLDEN.to_vec();
                ack[5] = status;
                stream.write_all(&ack).unwrap();
            });
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("unexpected_status"));
        }

        for (status, category) in [
            (STATUS_REJECTED, "request_rejected"),
            (STATUS_WRONG_PEER, "wrong_peer"),
            (STATUS_PROTOCOL_MISMATCH, "protocol_mismatch_status"),
            (0xff, "bad_status"),
        ] {
            let output = run_cpp_client("probe", b"", |mut stream| {
                let mut request = Vec::new();
                stream.read_to_end(&mut request).unwrap();
                let mut ack = OK_GOLDEN.to_vec();
                ack[5] = status;
                stream.write_all(&ack).unwrap();
            });
            assert!(!output.status.success());
            assert_eq!(
                String::from_utf8_lossy(&output.stderr),
                format!("error: Core development credential request failed ({category})\n")
            );
        }
    }

    #[test]
    #[cfg(core_dev_credential_protocol_host_test)]
    fn endpoint_ack_write_failure_is_reported() {
        let mut stream = FailingAck(Cursor::new(PROBE_GOLDEN.to_vec()));
        assert_eq!(
            serve_protocol(&mut stream, |_| true, || {}, || false, || false),
            Err("response_io")
        );
    }

    #[cfg(core_dev_credential_protocol_host_test)]
    fn endpoint_ack_for_fragments(fragments: &[&[u8]]) -> ([u8; ACK_BYTES], &'static str) {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let endpoint = thread::spawn(move || {
            serve_protocol(&mut server, |_| true, || {}, || false, || false).unwrap_err()
        });
        for fragment in fragments {
            client.write_all(fragment).unwrap();
        }
        client.shutdown(Shutdown::Write).unwrap();
        let mut ack = [0_u8; ACK_BYTES];
        client.read_exact(&mut ack).unwrap();
        (ack, endpoint.join().unwrap())
    }

    #[test]
    #[cfg(core_dev_credential_protocol_host_test)]
    fn rust_endpoint_handles_fragmentation_and_rejects_invalid_or_short_frames() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let endpoint = thread::spawn(move || {
            let mut fragmented = OneByteIo(server);
            serve_protocol(
                &mut fragmented,
                |key| key == SYNTHETIC_KEY,
                || {},
                || false,
                || false,
            )
            .unwrap()
        });
        for byte in SET_GOLDEN {
            client.write_all(&[*byte]).unwrap();
        }
        client.shutdown(Shutdown::Write).unwrap();
        let mut ack = [0_u8; ACK_BYTES];
        client.read_exact(&mut ack).unwrap();
        assert_eq!(ack, OK_GOLDEN);
        endpoint.join().unwrap();

        let mut wrong_version = PROBE_GOLDEN.to_vec();
        wrong_version[4] = 2;
        let (ack, category) = endpoint_ack_for_fragments(&[&wrong_version]);
        assert_eq!(ack[5], STATUS_PROTOCOL_MISMATCH);
        assert_eq!(category, "bad_version");

        let mut wrong_operation = PROBE_GOLDEN.to_vec();
        wrong_operation[5] = 9;
        let (ack, category) = endpoint_ack_for_fragments(&[&wrong_operation]);
        assert_eq!(ack[5], STATUS_PROTOCOL_MISMATCH);
        assert_eq!(category, "operation");

        let oversized = b"SOSK\x01\x01\x02\x01";
        let (ack, category) = endpoint_ack_for_fragments(&[oversized]);
        assert_eq!(ack[5], STATUS_PROTOCOL_MISMATCH);
        assert_eq!(category, "request_too_large");

        let (ack, category) = endpoint_ack_for_fragments(&[b"SOS"]);
        assert_eq!(ack[5], STATUS_PROTOCOL_MISMATCH);
        assert_eq!(category, "short_io");
    }

    #[test]
    #[cfg(core_dev_credential_protocol_host_test)]
    fn concurrent_status_reads_are_non_mutating() {
        let configured = Arc::new(AtomicBool::new(true));
        let mut requests = Vec::new();
        for _ in 0..8 {
            let (mut client, mut server) = UnixStream::pair().unwrap();
            let state = Arc::clone(&configured);
            let endpoint = thread::spawn(move || {
                serve_protocol(
                    &mut server,
                    |_| false,
                    || state.store(false, Ordering::Release),
                    || state.load(Ordering::Acquire),
                    || false,
                )
                .unwrap()
            });
            let request = thread::spawn(move || {
                client.write_all(STATUS_GOLDEN).unwrap();
                client.shutdown(Shutdown::Write).unwrap();
                let mut ack = [0_u8; ACK_BYTES];
                client.read_exact(&mut ack).unwrap();
                ack
            });
            requests.push((endpoint, request));
        }
        for (endpoint, request) in requests {
            assert_eq!(request.join().unwrap()[5], STATUS_CONFIGURED);
            endpoint.join().unwrap();
        }
        assert!(configured.load(Ordering::Acquire));
    }

    #[test]
    #[cfg(core_dev_credential_protocol_host_test)]
    fn cpp_client_classifies_bad_ack_fields_and_disconnects_without_frame_output() {
        for (ack, category) in [
            (b"XOSK\x01\x01".as_slice(), "bad_magic"),
            (b"SOSK\x02\x01".as_slice(), "bad_version"),
            (b"SOS".as_slice(), "short_io"),
            (b"".as_slice(), "short_io"),
        ] {
            let output = run_cpp_client("probe", b"", |mut stream| {
                let mut request = Vec::new();
                stream.read_to_end(&mut request).unwrap();
                stream.write_all(ack).unwrap();
            });
            assert!(!output.status.success());
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                diagnostic,
                format!("error: Core development credential request failed ({category})\n")
            );
            assert!(!diagnostic.contains("SOSK"));
        }

        let output = run_cpp_client("probe-closed-stdout", b"", |mut stream| {
            serve_protocol(&mut stream, |_| true, || {}, || false, || false).unwrap();
        });
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("stdout_io"));
    }
}
