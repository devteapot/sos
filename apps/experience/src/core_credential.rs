use zeroize::Zeroize;

pub const MIN_CREDENTIAL_BYTES: usize = 20;
pub const MAX_CREDENTIAL_BYTES: usize = 512;
pub const OPENROUTER_KEY_PREFIX: &str = "sk-or-v1-";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CeremonySnapshot {
    pub visible: bool,
    pub masked: String,
    pub suffix_count: usize,
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
        self.draft
            .extend_from_slice(OPENROUTER_KEY_PREFIX.as_bytes());
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
                if self.draft.len() > OPENROUTER_KEY_PREFIX.len() {
                    self.draft.pop();
                }
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
            self.error = Some("Enter 11–503 visible ASCII characters after sk-or-v1-");
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

    #[cfg(any(feature = "core-dev-credential", test))]
    pub fn install_openrouter(&mut self, key: &[u8]) -> bool {
        if !valid_credential(key) {
            return false;
        }
        self.cancel();
        self.active.zeroize();
        self.active.clear();
        self.active.extend_from_slice(key);
        self.openrouter_selected = true;
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
            masked: self
                .ceremony_visible
                .then(|| masked_credential(self.draft.len()))
                .unwrap_or_default(),
            suffix_count: self
                .ceremony_visible
                .then(|| self.draft.len().saturating_sub(OPENROUTER_KEY_PREFIX.len()))
                .unwrap_or_default(),
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
        && value.starts_with(OPENROUTER_KEY_PREFIX.as_bytes())
        && value.iter().all(u8::is_ascii_graphic)
}

fn masked_credential(length: usize) -> String {
    let suffix = length.saturating_sub(OPENROUTER_KEY_PREFIX.len());
    let mut masked = String::from(OPENROUTER_KEY_PREFIX);
    for index in 0..suffix {
        if index > 0 && index % 4 == 0 {
            masked.push(' ');
        }
        masked.push('•');
    }
    masked
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: &str = "sk-or-v1-0123456789abcdef01234567";
    const SECOND: &str = "sk-or-v1-fedcba9876543210fedcba98";

    fn enter(state: &mut CredentialState, value: &str) {
        for byte in value.bytes() {
            assert!(state.apply_input(&char::from(byte).to_string()));
        }
    }

    fn enter_key(state: &mut CredentialState, value: &str) {
        enter(
            state,
            value
                .strip_prefix(OPENROUTER_KEY_PREFIX)
                .expect("test key must use the fixed OpenRouter prefix"),
        );
    }

    #[test]
    fn ceremony_masks_input_and_has_fixed_cancel_semantics() {
        let mut state = CredentialState::default();
        state.begin();
        enter_key(&mut state, FIRST);
        let snapshot = state.snapshot();
        assert!(snapshot.visible);
        assert!(snapshot.masked.starts_with(OPENROUTER_KEY_PREFIX));
        assert_eq!(
            snapshot.suffix_count,
            FIRST.len() - OPENROUTER_KEY_PREFIX.len()
        );
        assert_eq!(snapshot.masked.matches(' ').count(), 5);
        assert!(!snapshot.masked.contains(FIRST));
        state.cancel();
        let cancelled = state.snapshot();
        assert!(!cancelled.visible);
        assert!(cancelled.masked.is_empty());
        assert_eq!(cancelled.suffix_count, 0);
        assert!(!state.configured());
        assert!(state.credential().is_none());
    }

    #[test]
    fn save_replace_clear_and_refresh_are_provider_scoped() {
        let mut state = CredentialState::default();
        state.begin();
        enter_key(&mut state, FIRST);
        assert!(state.save());
        assert_eq!(state.credential().as_deref(), Some(FIRST.as_bytes()));

        state.begin();
        enter_key(&mut state, SECOND);
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
        for _ in OPENROUTER_KEY_PREFIX.len()..MAX_CREDENTIAL_BYTES {
            assert!(state.apply_input("x"));
        }
        assert!(!state.apply_input("x"));
    }

    #[test]
    fn ceremony_prefills_and_protects_the_exact_openrouter_prefix() {
        let mut state = CredentialState::default();
        state.begin();
        assert_eq!(state.snapshot().masked, OPENROUTER_KEY_PREFIX);
        assert_eq!(state.snapshot().suffix_count, 0);
        for _ in 0..OPENROUTER_KEY_PREFIX.len() + 3 {
            assert!(state.apply_input("\u{8}"));
        }
        assert_eq!(state.snapshot().masked, OPENROUTER_KEY_PREFIX);
        enter(&mut state, "0123456789abcdef");
        assert_eq!(state.snapshot().suffix_count, 16);
        assert_eq!(state.snapshot().masked, "sk-or-v1-•••• •••• •••• ••••");
    }

    #[test]
    fn development_install_is_bounded_provider_scoped_and_clearable() {
        let mut state = CredentialState::default();
        assert!(!state.install_openrouter(b"too-short"));
        assert!(!state.install_openrouter(b"sk-or-v1-line\nbreak-is-rejected"));
        assert!(!state.configured());
        assert!(state.install_openrouter(FIRST.as_bytes()));
        assert_eq!(state.credential().as_deref(), Some(FIRST.as_bytes()));
        state.clear();
        assert!(!state.configured());
        assert!(state.credential().is_none());
    }

    #[test]
    fn failed_request_refresh_retains_memory_only_credential_until_explicit_clear() {
        let mut state = CredentialState::default();
        assert!(state.install_openrouter(FIRST.as_bytes()));
        assert!(!state.accept_refreshed("openrouter", b"invalid-request-refresh"));
        assert!(state.configured());
        assert_eq!(state.credential().as_deref(), Some(FIRST.as_bytes()));
        state.clear();
        assert!(!state.configured());
        assert!(state.credential().is_none());
    }
}
