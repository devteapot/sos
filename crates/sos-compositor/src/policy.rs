use anyhow::{bail, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientRole {
    Shell,
    Compatibility,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArmedPresentation {
    pub request_id: u64,
    pub revision_id: String,
    pub after_commit_sequence: u64,
    target_commit_sequence: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentedRevision {
    pub request_id: u64,
    pub revision_id: String,
    pub commit_sequence: u64,
    pub submit_sequence: u64,
}

#[derive(Debug, Default)]
pub struct SurfacePolicy {
    shell_pid: Option<u32>,
    shell_mapped: bool,
    compatibility_mapped: usize,
    shell_commit_sequence: u64,
    submit_sequence: u64,
    pending: Option<ArmedPresentation>,
    input_quiesced: bool,
}

impl SurfacePolicy {
    pub fn register_shell(&mut self, pid: u32) -> Result<()> {
        if pid == 0 {
            bail!("shell PID must be non-zero");
        }
        match self.shell_pid {
            Some(current) if current == pid => Ok(()),
            Some(current) => bail!("shell PID {current} is already registered"),
            None => {
                self.shell_pid = Some(pid);
                Ok(())
            }
        }
    }

    pub fn unregister_shell(&mut self, pid: u32) {
        if self.shell_pid == Some(pid) {
            self.shell_pid = None;
            self.pending = None;
            self.input_quiesced = false;
        }
    }

    pub fn classify(&self, pid: u32) -> ClientRole {
        if self.shell_pid == Some(pid) {
            ClientRole::Shell
        } else {
            ClientRole::Compatibility
        }
    }

    pub fn map(&mut self, role: ClientRole) -> Result<()> {
        match role {
            ClientRole::Shell if self.shell_mapped => bail!("shell surface is already mapped"),
            ClientRole::Shell => self.shell_mapped = true,
            ClientRole::Compatibility if !self.shell_mapped => {
                bail!("compatibility surface requires a mapped shell")
            }
            ClientRole::Compatibility if self.compatibility_mapped >= 1 => {
                bail!("only one compatibility toplevel is allowed")
            }
            ClientRole::Compatibility => self.compatibility_mapped += 1,
        }
        Ok(())
    }

    pub fn unmap(&mut self, role: ClientRole) {
        match role {
            ClientRole::Shell => self.shell_mapped = false,
            ClientRole::Compatibility => {
                self.compatibility_mapped = self.compatibility_mapped.saturating_sub(1)
            }
        }
    }

    pub fn arm(&mut self, pid: u32, request_id: u64, revision_id: String) -> Result<u64> {
        if self.shell_pid != Some(pid) {
            bail!("presentation fence is not owned by the registered shell");
        }
        if self.pending.is_some() {
            bail!("another shell presentation is already armed");
        }
        if revision_id.len() != 64 || !revision_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("revision ID is not a SHA-256 identity");
        }
        let after_commit_sequence = self.shell_commit_sequence;
        self.pending = Some(ArmedPresentation {
            request_id,
            revision_id,
            after_commit_sequence,
            target_commit_sequence: None,
        });
        self.input_quiesced = true;
        Ok(after_commit_sequence)
    }

    pub fn record_shell_commit(&mut self) -> u64 {
        self.shell_commit_sequence = self.shell_commit_sequence.saturating_add(1);
        if let Some(pending) = &mut self.pending {
            if pending.target_commit_sequence.is_none()
                && self.shell_commit_sequence > pending.after_commit_sequence
            {
                pending.target_commit_sequence = Some(self.shell_commit_sequence);
            }
        }
        self.shell_commit_sequence
    }

    pub fn record_successful_submit(&mut self, shell_rendered: bool) -> Option<PresentedRevision> {
        self.submit_sequence = self.submit_sequence.saturating_add(1);
        if !shell_rendered {
            return None;
        }
        let target_commit_sequence = self.pending.as_ref()?.target_commit_sequence?;
        let pending = self
            .pending
            .take()
            .expect("pending presentation was checked");
        self.input_quiesced = false;
        Some(PresentedRevision {
            request_id: pending.request_id,
            revision_id: pending.revision_id,
            commit_sequence: target_commit_sequence,
            submit_sequence: self.submit_sequence,
        })
    }

    pub fn input_quiesced(&self) -> bool {
        self.input_quiesced
    }
}

pub fn compatibility_location(output: (i32, i32), window: (i32, i32)) -> (i32, i32) {
    (
        ((output.0 - window.0) / 2).max(24),
        ((output.1 - window.1) / 2).max(24),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const REVISION: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn only_registered_pid_can_be_shell_or_arm_a_fence() {
        let mut policy = SurfacePolicy::default();
        policy.register_shell(41).unwrap();
        assert_eq!(policy.classify(41), ClientRole::Shell);
        assert_eq!(policy.classify(42), ClientRole::Compatibility);
        assert!(policy.arm(42, 1, REVISION.into()).is_err());
        assert!(policy.register_shell(42).is_err());
    }

    #[test]
    fn input_stays_quiesced_until_an_armed_commit_is_submitted() {
        let mut policy = SurfacePolicy::default();
        policy.register_shell(41).unwrap();
        assert_eq!(policy.arm(41, 7, REVISION.into()).unwrap(), 0);
        assert!(policy.input_quiesced());
        assert!(policy.record_successful_submit(true).is_none());
        assert_eq!(policy.record_shell_commit(), 1);
        assert!(policy.record_successful_submit(false).is_none());
        let presented = policy.record_successful_submit(true).unwrap();
        assert_eq!(presented.request_id, 7);
        assert_eq!(presented.revision_id, REVISION);
        assert_eq!(presented.commit_sequence, 1);
        assert_eq!(presented.submit_sequence, 3);
        assert!(!policy.input_quiesced());
    }

    #[test]
    fn surface_cardinality_and_placement_are_bounded() {
        let mut policy = SurfacePolicy::default();
        policy.map(ClientRole::Shell).unwrap();
        assert!(policy.map(ClientRole::Shell).is_err());
        policy.map(ClientRole::Compatibility).unwrap();
        assert!(policy.map(ClientRole::Compatibility).is_err());
        policy.unmap(ClientRole::Compatibility);
        policy.map(ClientRole::Compatibility).unwrap();
        assert_eq!(compatibility_location((1280, 800), (720, 520)), (280, 140));
    }
}
