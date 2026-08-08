use std::{
    fs,
    io::{BufRead, BufReader},
    os::unix::net::UnixListener,
    process::{Child, ExitStatus},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Error, Result, VerifiedRevision};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CandidateEvent {
    FirstFrame { token: String, revision_id: String },
}

#[derive(Debug)]
pub struct ManagedCandidate {
    child: Child,
    pub revision_id: String,
}

impl ManagedCandidate {
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        Ok(self.child.try_wait()?)
    }

    pub fn terminate(mut self) -> Result<()> {
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        self.child.wait()?;
        Ok(())
    }
}

impl Drop for ManagedCandidate {
    fn drop(&mut self) {
        if self.child.try_wait().is_ok_and(|status| status.is_none()) {
            self.child.kill().ok();
        }
        self.child.wait().ok();
    }
}

pub fn launch_until_first_frame(
    revision: &VerifiedRevision,
    timeout: Duration,
) -> Result<ManagedCandidate> {
    let run_directory = revision
        .directory
        .parent()
        .and_then(|path| path.parent())
        .expect("verified revision is rooted in store")
        .join("run");
    fs::create_dir_all(&run_directory)?;
    let token_seed = format!(
        "{}:{}:{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        revision.manifest.revision_id
    );
    let token = format!("{:x}", Sha256::digest(token_seed.as_bytes()));
    let socket = run_directory.join(format!("ready-{}", &token[..20]));
    let listener = UnixListener::bind(&socket)?;
    listener.set_nonblocking(true)?;

    let executable = revision.directory.join(&revision.manifest.executable.path);
    let mut child = std::process::Command::new(executable)
        .args(&revision.manifest.args)
        .current_dir(&revision.directory)
        .env("SOS_SUPERVISOR_SOCKET", &socket)
        .env("SOS_SUPERVISOR_TOKEN", &token)
        .env("SOS_REVISION_ID", &revision.manifest.revision_id)
        .spawn()?;
    let deadline = Instant::now() + timeout;
    let result = loop {
        if let Some(status) = child.try_wait()? {
            break Err(Error::CandidateExitedBeforeFirstFrame(status));
        }
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_read_timeout(Some(deadline.saturating_duration_since(Instant::now())))?;
                let mut line = String::new();
                BufReader::new(stream).read_line(&mut line)?;
                let event: CandidateEvent = serde_json::from_str(&line)?;
                match event {
                    CandidateEvent::FirstFrame {
                        token: received_token,
                        revision_id,
                    } if received_token == token
                        && revision_id == revision.manifest.revision_id =>
                    {
                        break Ok(())
                    }
                    _ => break Err(Error::InvalidCandidateEvent),
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => break Err(error.into()),
        }
        if Instant::now() >= deadline {
            break Err(Error::FirstFrameTimeout(timeout));
        }
        thread::sleep(Duration::from_millis(2));
    };
    fs::remove_file(&socket).ok();
    if let Err(error) = result {
        if child.try_wait()?.is_none() {
            child.kill()?;
        }
        child.wait()?;
        return Err(error);
    }
    Ok(ManagedCandidate {
        child,
        revision_id: revision.manifest.revision_id.clone(),
    })
}
