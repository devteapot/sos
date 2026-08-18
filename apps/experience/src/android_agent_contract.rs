pub const OPENROUTER_MODEL: &str = "deepseek/deepseek-v4-flash-0731";
pub const OPENAI_MODEL: &str = "gpt-5.6-luna";
pub const CODEX_MODEL: &str = "gpt-5.6-sol";
pub const FAUX_MODEL: &str = "faux";
#[cfg(any(feature = "core-native", test))]
pub const FAUX_PI_TIMEOUT_SECONDS: u64 = 30;
#[cfg(any(feature = "core-native", test))]
pub const LIVE_PI_TIMEOUT_SECONDS: u64 = 240;
pub const VERIFIED_ACTIONS: [&str; 3] = [
    "get_experience_context",
    "validate_experience",
    "submit_experience",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentActivationPhase {
    Submitted,
    Validated,
    Staged,
    Committed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentActivationEvidence {
    request_id: u64,
    phase: AgentActivationPhase,
}

impl AgentActivationEvidence {
    pub fn submitted(request_id: u64) -> Self {
        Self {
            request_id,
            phase: AgentActivationPhase::Submitted,
        }
    }

    pub fn advance(&mut self, next: AgentActivationPhase) -> Result<(), &'static str> {
        let allowed = matches!(
            (self.phase, next),
            (
                AgentActivationPhase::Submitted,
                AgentActivationPhase::Validated
            ) | (
                AgentActivationPhase::Validated,
                AgentActivationPhase::Staged
            ) | (
                AgentActivationPhase::Staged,
                AgentActivationPhase::Committed
            )
        );
        if !allowed {
            return Err("agent activation evidence phase is missing or out of order");
        }
        self.phase = next;
        Ok(())
    }

    pub fn request_id(&self) -> u64 {
        self.request_id
    }

    #[cfg(test)]
    pub fn phase(&self) -> AgentActivationPhase {
        self.phase
    }
}

pub fn verified_action_sequence(actions: &[String]) -> Option<&[String]> {
    actions
        .iter()
        .map(String::as_str)
        .eq(VERIFIED_ACTIONS)
        .then_some(actions)
}

pub fn expected_model(provider: &str) -> Option<&'static str> {
    match provider {
        "fake" | "faux" => Some(FAUX_MODEL),
        "openai" => Some(OPENAI_MODEL),
        "openrouter" => Some(OPENROUTER_MODEL),
        "openai-codex" => Some(CODEX_MODEL),
        _ => None,
    }
}

pub fn model_is_exact(provider: &str, model: &str) -> bool {
    expected_model(provider) == Some(model)
}

pub fn reconciled_request_error(
    current: Option<String>,
    intentional_clear: bool,
) -> Option<String> {
    if intentional_clear {
        None
    } else {
        current
    }
}

#[cfg(any(feature = "core-native", test))]
pub fn pi_timeout_seconds(provider: &str) -> Option<u64> {
    expected_model(provider).map(|model| {
        if model == FAUX_MODEL {
            FAUX_PI_TIMEOUT_SECONDS
        } else {
            LIVE_PI_TIMEOUT_SECONDS
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_contract_pins_models_and_live_timeout() {
        assert_eq!(expected_model("fake"), Some("faux"));
        assert_eq!(expected_model("openrouter"), Some(OPENROUTER_MODEL));
        assert!(model_is_exact(
            "openrouter",
            "deepseek/deepseek-v4-flash-0731"
        ));
        for rejected in [
            "deepseek/deepseek-v4-flash",
            "deepseek/deepseek-v4-flash-latest",
            "deepseek/deepseek-v4-flash-0731:free",
            "deepseek/deepseek-v4-flash-0731-extra",
        ] {
            assert!(!model_is_exact("openrouter", rejected));
        }
        assert_eq!(pi_timeout_seconds("fake"), Some(30));
        assert_eq!(pi_timeout_seconds("openrouter"), Some(240));
        assert_eq!(expected_model("unknown"), None);
        assert_eq!(pi_timeout_seconds("unknown"), None);
    }

    #[test]
    fn routine_status_preserves_a_request_error_until_an_intentional_action() {
        let error = Some("Provider request failed (provider/rate_limited).".to_owned());
        assert_eq!(reconciled_request_error(error.clone(), false), error);
        assert_eq!(reconciled_request_error(error, true), None);
    }

    #[test]
    fn action_evidence_requires_the_complete_exact_order() {
        let exact = VERIFIED_ACTIONS.map(str::to_owned);
        assert_eq!(verified_action_sequence(&exact), Some(exact.as_slice()));
        for rejected in [
            vec!["get_experience_context", "validate_experience"],
            vec![
                "validate_experience",
                "get_experience_context",
                "submit_experience",
            ],
            vec![
                "get_experience_context",
                "validate_experience",
                "submit_experience",
                "submit_experience",
            ],
        ] {
            let rejected = rejected.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert_eq!(verified_action_sequence(&rejected), None);
        }
    }

    #[test]
    fn activation_evidence_cannot_claim_commit_from_staged_or_validated_state() {
        let mut evidence = AgentActivationEvidence::submitted(41);
        assert_eq!(evidence.request_id(), 41);
        assert_eq!(
            evidence.advance(AgentActivationPhase::Committed),
            Err("agent activation evidence phase is missing or out of order")
        );
        evidence.advance(AgentActivationPhase::Validated).unwrap();
        assert_eq!(evidence.phase(), AgentActivationPhase::Validated);
        assert!(evidence.advance(AgentActivationPhase::Committed).is_err());
        evidence.advance(AgentActivationPhase::Staged).unwrap();
        assert_eq!(evidence.phase(), AgentActivationPhase::Staged);
        evidence.advance(AgentActivationPhase::Committed).unwrap();
        assert_eq!(evidence.phase(), AgentActivationPhase::Committed);
        assert!(evidence.advance(AgentActivationPhase::Committed).is_err());
    }
}
