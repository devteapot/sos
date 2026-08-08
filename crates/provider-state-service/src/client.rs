use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::{SocketAddr, UnixStream},
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(target_os = "android")]
use std::os::android::net::SocketAddrExt;
#[cfg(target_os = "linux")]
use std::os::linux::net::SocketAddrExt;

use service_protocol::{ServiceRequest, ServiceRequestEnvelope, ServiceResponse, MAX_STATE_BYTES};

pub struct ServiceClient {
    socket: PathBuf,
    timeout: Duration,
}

impl ServiceClient {
    pub fn new(socket: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            socket: socket.into(),
            timeout,
        }
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    pub fn call(&self, request: &ServiceRequest) -> std::io::Result<ServiceResponse> {
        let mut stream = connect(&self.socket)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        serde_json::to_writer(&mut stream, &ServiceRequestEnvelope::new(request.clone()))
            .map_err(std::io::Error::other)?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        let mut response = Vec::new();
        BufReader::new(stream)
            .take((MAX_STATE_BYTES + 512 * 1024) as u64)
            .read_until(b'\n', &mut response)?;
        serde_json::from_slice(&response).map_err(std::io::Error::other)
    }
}

fn connect(socket: &Path) -> std::io::Result<UnixStream> {
    let value = socket.as_os_str().as_encoded_bytes();
    if let Some(name) = value.strip_prefix(b"@") {
        UnixStream::connect_addr(&SocketAddr::from_abstract_name(name)?)
    } else {
        UnixStream::connect(socket)
    }
}
