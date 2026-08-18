use zeroize::Zeroize;

pub const MIN_CREDENTIAL_BYTES: usize = 20;
pub const MAX_CREDENTIAL_BYTES: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CeremonySnapshot {
    pub visible: bool,
    pub masked: String,
    pub error: Option<&'static str>,
}

#[derive(Default)]
pub struct CredentialState {
    active: Vec<u8>,
    draft: Vec<u8>,
    ceremony_visible: bool,
    openrouter_selected: bool,
    error: Option<&'static str>,
}

impl CredentialState {
    pub fn begin(&mut self) {
        self.clear_draft();
        self.ceremony_visible = true;
        self.error = None;
    }

    pub fn cancel(&mut self) {
        self.clear_draft();
        self.ceremony_visible = false;
        self.error = None;
    }

    pub fn use_faux(&mut self) {
        self.cancel();
        self.openrouter_selected = false;
    }

    pub fn clear(&mut self) {
        self.cancel();
        self.active.zeroize();
        self.active.clear();
        self.openrouter_selected = false;
    }

    pub fn apply_input(&mut self, text: &str) -> bool {
        if !self.ceremony_visible {
            return false;
        }
        match text {
            "\u{8}" => {
                self.draft.pop();
                self.error = None;
                true
            }
            "\n" => self.save(),
            value
                if value.len() == 1
                    && value.as_bytes()[0].is_ascii_graphic()
                    && self.draft.len() < MAX_CREDENTIAL_BYTES =>
            {
                self.draft.push(value.as_bytes()[0]);
                self.error = None;
                true
            }
            _ => false,
        }
    }

    pub fn save(&mut self) -> bool {
        if !valid_credential(&self.draft) {
            self.error = Some("Enter 20–512 visible ASCII characters");
            return false;
        }
        self.active.zeroize();
        self.active.clear();
        std::mem::swap(&mut self.active, &mut self.draft);
        self.draft.zeroize();
        self.draft.clear();
        self.ceremony_visible = false;
        self.openrouter_selected = true;
        self.error = None;
        true
    }

    pub fn accept_refreshed(&mut self, provider: &str, key: &[u8]) -> bool {
        if provider != "openrouter" || !self.openrouter_selected || !valid_credential(key) {
            return false;
        }
        self.active.zeroize();
        self.active.clear();
        self.active.extend_from_slice(key);
        true
    }

    pub fn credential(&self) -> Option<Vec<u8>> {
        (self.openrouter_selected && valid_credential(&self.active)).then(|| self.active.clone())
    }

    pub fn configured(&self) -> bool {
        self.openrouter_selected && valid_credential(&self.active)
    }

    pub fn snapshot(&self) -> CeremonySnapshot {
        CeremonySnapshot {
            visible: self.ceremony_visible,
            masked: "•".repeat(self.draft.len()),
            error: self.error,
        }
    }

    fn clear_draft(&mut self) {
        self.draft.zeroize();
        self.draft.clear();
    }
}

impl Drop for CredentialState {
    fn drop(&mut self) {
        self.active.zeroize();
        self.draft.zeroize();
    }
}

fn valid_credential(value: &[u8]) -> bool {
    (MIN_CREDENTIAL_BYTES..=MAX_CREDENTIAL_BYTES).contains(&value.len())
        && value.iter().all(u8::is_ascii_graphic)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: &str = "ABCDEFGHIJKLMNOPQRSTUVWX";
    const SECOND: &str = "zyxwvutsrqponmlkjihgfedc";

    fn enter(state: &mut CredentialState, value: &str) {
        for byte in value.bytes() {
            assert!(state.apply_input(&char::from(byte).to_string()));
        }
    }

    #[test]
    fn ceremony_masks_input_and_has_fixed_cancel_semantics() {
        let mut state = CredentialState::default();
        state.begin();
        enter(&mut state, FIRST);
        let snapshot = state.snapshot();
        assert!(snapshot.visible);
        assert_eq!(snapshot.masked.chars().count(), FIRST.len());
        assert!(!snapshot.masked.contains(FIRST));
        state.cancel();
        assert!(!state.snapshot().visible);
        assert!(!state.configured());
        assert!(state.credential().is_none());
    }

    #[test]
    fn save_replace_clear_and_refresh_are_provider_scoped() {
        let mut state = CredentialState::default();
        state.begin();
        enter(&mut state, FIRST);
        assert!(state.save());
        assert_eq!(state.credential().as_deref(), Some(FIRST.as_bytes()));

        state.begin();
        enter(&mut state, SECOND);
        assert!(state.save());
        assert_eq!(state.credential().as_deref(), Some(SECOND.as_bytes()));
        assert!(!state.accept_refreshed("openai", FIRST.as_bytes()));
        assert_eq!(state.credential().as_deref(), Some(SECOND.as_bytes()));
        assert!(state.accept_refreshed("openrouter", FIRST.as_bytes()));
        assert_eq!(state.credential().as_deref(), Some(FIRST.as_bytes()));

        state.use_faux();
        assert!(!state.configured());
        assert!(state.credential().is_none());

        state.clear();
        assert!(!state.configured());
        assert!(state.credential().is_none());
    }

    #[test]
    fn ceremony_rejects_whitespace_short_and_overlong_values() {
        let mut state = CredentialState::default();
        state.begin();
        enter(&mut state, "short");
        assert!(!state.save());
        assert!(state.snapshot().error.is_some());
        assert!(!state.apply_input(" "));
        state.cancel();
        state.begin();
        for _ in 0..MAX_CREDENTIAL_BYTES {
            assert!(state.apply_input("x"));
        }
        assert!(!state.apply_input("x"));
    }
}
