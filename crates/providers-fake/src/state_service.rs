use std::collections::HashMap;

use experience_ir::{StateEnvelope, StateFaultPoint, MAX_STATE_BYTES};
use serde_json::json;

#[derive(Debug)]
pub struct StateService {
    current: StateEnvelope,
    staged: HashMap<u64, StateEnvelope>,
    next_stage_id: u64,
    fault: Option<StateFaultPoint>,
}

impl Default for StateService {
    fn default() -> Self {
        Self::new(StateEnvelope {
            revision: 0,
            schema_version: 1,
            state: json!({}),
        })
    }
}

impl StateService {
    pub fn new(current: StateEnvelope) -> Self {
        Self {
            current,
            staged: HashMap::new(),
            next_stage_id: 1,
            fault: None,
        }
    }

    pub fn load(&self) -> StateEnvelope {
        self.current.clone()
    }

    pub fn configure_fault(&mut self, point: Option<StateFaultPoint>) {
        self.fault = point;
    }

    pub fn stage(
        &mut self,
        expected_revision: u64,
        schema_version: u64,
        state: serde_json::Value,
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
        let stage_id = self.next_stage_id;
        self.next_stage_id = self.next_stage_id.saturating_add(1);
        self.staged.insert(
            stage_id,
            StateEnvelope {
                revision: self.current.revision.saturating_add(1),
                schema_version,
                state,
            },
        );
        self.inject(StateFaultPoint::AfterStage)?;
        Ok(stage_id)
    }

    pub fn promote(&mut self, stage_id: u64) -> Result<StateEnvelope, String> {
        self.inject(StateFaultPoint::BeforePromote)?;
        let staged = self
            .staged
            .remove(&stage_id)
            .ok_or_else(|| format!("unknown state stage: {stage_id}"))?;
        if staged.revision != self.current.revision.saturating_add(1) {
            return Err("staged state is stale".into());
        }
        self.current = staged;
        self.inject(StateFaultPoint::AfterPromote)?;
        Ok(self.current.clone())
    }

    pub fn abort(&mut self, stage_id: u64) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_promotes_and_rejects_stale_writers() {
        let mut service = StateService::default();
        let stage = service.stage(0, 2, json!({ "migrated": true })).unwrap();
        assert_eq!(service.load().revision, 0);
        let promoted = service.promote(stage).unwrap();
        assert_eq!(promoted.revision, 1);
        assert_eq!(promoted.schema_version, 2);
        assert!(service.stage(0, 2, json!({})).is_err());
    }

    #[test]
    fn faults_preserve_or_expose_the_expected_transaction_phase() {
        let mut service = StateService::default();
        service.configure_fault(Some(StateFaultPoint::BeforeStage));
        assert!(service.stage(0, 1, json!({ "value": 1 })).is_err());
        assert_eq!(service.load().revision, 0);

        service.configure_fault(Some(StateFaultPoint::AfterStage));
        assert!(service.stage(0, 1, json!({ "value": 2 })).is_err());
        assert_eq!(service.load().revision, 0);

        let stage = service.stage(0, 1, json!({ "value": 3 })).unwrap();
        service.configure_fault(Some(StateFaultPoint::BeforePromote));
        assert!(service.promote(stage).is_err());
        assert_eq!(service.load().revision, 0);

        let stage = service.stage(0, 1, json!({ "value": 4 })).unwrap();
        service.configure_fault(Some(StateFaultPoint::AfterPromote));
        assert!(service.promote(stage).is_err());
        assert_eq!(service.load().revision, 1);
        assert_eq!(service.load().state, json!({ "value": 4 }));
    }
}
