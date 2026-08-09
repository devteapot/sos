//! Deterministic compose policy used by the SOS Wayland input-method-v2 client.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Preedit(String),
    Commit(String),
    Clear,
    None,
}

#[derive(Clone, Debug, Default)]
pub struct ComposeEngine {
    preedit: String,
    candidates: Vec<&'static str>,
    selected: usize,
    dead_acute: bool,
}

impl ComposeEngine {
    pub fn preedit(&self) -> &str {
        &self.preedit
    }

    pub fn candidates(&self) -> &[&'static str] {
        &self.candidates
    }

    pub fn selected_candidate(&self) -> Option<&'static str> {
        self.candidates.get(self.selected).copied()
    }

    pub fn cursor_left(&mut self) -> Edit {
        if self.candidates.is_empty() {
            return Edit::None;
        }
        self.selected = self
            .selected
            .checked_sub(1)
            .unwrap_or(self.candidates.len() - 1);
        Edit::Preedit(self.preedit.clone())
    }

    pub fn cursor_right(&mut self) -> Edit {
        if self.candidates.is_empty() {
            return Edit::None;
        }
        self.selected = (self.selected + 1) % self.candidates.len();
        Edit::Preedit(self.preedit.clone())
    }

    pub fn backspace(&mut self) -> Edit {
        if self.dead_acute {
            self.dead_acute = false;
            return Edit::Clear;
        }
        if self.preedit.pop().is_none() {
            return Edit::None;
        }
        self.refresh_candidates();
        if self.preedit.is_empty() {
            Edit::Clear
        } else {
            Edit::Preedit(self.preedit.clone())
        }
    }

    pub fn acute(&mut self) -> Edit {
        self.dead_acute = true;
        Edit::Preedit("´".into())
    }

    pub fn letter(&mut self, letter: char) -> Edit {
        if self.dead_acute {
            self.dead_acute = false;
            let composed = match letter {
                'a' => Some('á'),
                'e' => Some('é'),
                'i' => Some('í'),
                'o' => Some('ó'),
                'u' => Some('ú'),
                _ => None,
            };
            return Edit::Commit(composed.map_or_else(|| format!("´{letter}"), |c| c.to_string()));
        }
        self.preedit.push(letter);
        self.refresh_candidates();
        Edit::Preedit(self.preedit.clone())
    }

    pub fn accept(&mut self) -> Edit {
        if self.dead_acute {
            self.dead_acute = false;
            return Edit::Commit("´".into());
        }
        if self.preedit.is_empty() {
            return Edit::Commit(" ".into());
        }
        let text = self
            .selected_candidate()
            .map(str::to_owned)
            .unwrap_or_else(|| self.preedit.clone());
        self.reset();
        Edit::Commit(text)
    }

    pub fn cancel(&mut self) -> Edit {
        self.reset();
        Edit::Clear
    }

    fn refresh_candidates(&mut self) {
        self.candidates = match self.preedit.as_str() {
            "ni" => vec!["你", "尼", "呢"],
            "hao" => vec!["好", "号", "浩"],
            "nihao" => vec!["你好", "你号"],
            "zhongwen" => vec!["中文", "中文字"],
            _ => Vec::new(),
        };
        self.selected = 0;
    }

    fn reset(&mut self) {
        self.preedit.clear();
        self.candidates.clear();
        self.selected = 0;
        self.dead_acute = false;
    }
}

#[cfg(test)]
mod tests {
    use super::{ComposeEngine, Edit};

    #[test]
    fn non_latin_preedit_candidate_selection_and_commit() {
        let mut engine = ComposeEngine::default();
        for letter in "nihao".chars() {
            engine.letter(letter);
        }
        assert_eq!(engine.candidates(), &["你好", "你号"]);
        engine.cursor_right();
        assert_eq!(engine.accept(), Edit::Commit("你号".into()));
        assert!(engine.preedit().is_empty());
    }

    #[test]
    fn dead_key_commits_composed_character() {
        let mut engine = ComposeEngine::default();
        assert_eq!(engine.acute(), Edit::Preedit("´".into()));
        assert_eq!(engine.letter('e'), Edit::Commit("é".into()));
    }

    #[test]
    fn cancellation_clears_composition() {
        let mut engine = ComposeEngine::default();
        engine.letter('n');
        engine.letter('i');
        assert_eq!(engine.cancel(), Edit::Clear);
        assert!(engine.preedit().is_empty());
        assert!(engine.candidates().is_empty());
    }
}
