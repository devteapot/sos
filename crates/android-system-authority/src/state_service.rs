use std::collections::HashMap;

use experience_ir::{StateEnvelope, StateFaultPoint, MAX_STATE_BYTES};

#[derive(Debug)]
pub(crate) struct StateService {
    current: StateEnvelope,
    staged: HashMap<u64, StateEnvelope>,
    next_stage_id: u64,
    fault: Option<StateFaultPoint>,
}

impl StateService {
    pub(crate) fn new(current: StateEnvelope) -> Self {
        Self {
            current,
            staged: HashMap::new(),
            next_stage_id: 1,
            fault: None,
        }
    }

    pub(crate) fn load(&self) -> StateEnvelope {
        self.current.clone()
    }

    pub(crate) fn configure_fault(&mut self, point: Option<StateFaultPoint>) {
        self.fault = point;
    }

    pub(crate) fn stage(
        &mut self,
        expected_revision: u64,
        schema_version: u64,
        state: serde_json::Value,
        source_sha256: String,
    ) -> Result<u64, String> {
        self.inject(StateFaultPoint::BeforeStage)?;
        if expected_revision != self.current.revision {
            return Err(format!(
                "state revision conflict: expected {expected_revision}, current {}",
                self.current.revision
            ));
        }
        if schema_version == 0 {
            return Err("state schema version must be positive".into());
        }
        if serde_json::to_vec(&state)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_STATE_BYTES
        {
            return Err("state is larger than the service limit".into());
        }
        let source_sha256 = if source_sha256.is_empty() {
            self.current.source_sha256.clone()
        } else if source_sha256.len() == 64
            && source_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            source_sha256
        } else {
            return Err("source SHA-256 must be 64 hexadecimal characters".into());
        };
        let stage_id = self.next_stage_id;
        self.next_stage_id = self.next_stage_id.saturating_add(1);
        self.staged.insert(
            stage_id,
            StateEnvelope {
                revision: self.current.revision.saturating_add(1),
                schema_version,
                source_sha256,
                state,
            },
        );
        self.inject(StateFaultPoint::AfterStage)?;
        Ok(stage_id)
    }

    pub(crate) fn promote(&mut self, stage_id: u64) -> Result<StateEnvelope, String> {
        self.inject(StateFaultPoint::BeforePromote)?;
        self.validate_promotion(stage_id)?;
        let staged = self
            .staged
            .remove(&stage_id)
            .ok_or_else(|| format!("unknown state stage: {stage_id}"))?;
        self.current = staged;
        self.inject(StateFaultPoint::AfterPromote)?;
        Ok(self.current.clone())
    }

    pub(crate) fn validate_promotion(&self, stage_id: u64) -> Result<StateEnvelope, String> {
        let staged = self
            .staged
            .get(&stage_id)
            .ok_or_else(|| format!("unknown state stage: {stage_id}"))?;
        if staged.revision != self.current.revision.saturating_add(1) {
            return Err("staged state is stale".into());
        }
        Ok(staged.clone())
    }

    pub(crate) fn abort(&mut self, stage_id: u64) -> bool {
        self.staged.remove(&stage_id).is_some()
    }

    fn inject(&mut self, point: StateFaultPoint) -> Result<(), String> {
        if self.fault == Some(point) {
            self.fault = None;
            Err(format!("injected state fault: {point:?}"))
        } else {
            Ok(())
        }
    }
}
