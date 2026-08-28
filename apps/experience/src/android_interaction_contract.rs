use experience_ir::Flow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextTapOutcome {
    NoActiveInput,
    ActiveInput,
    OtherInput,
    OutsideInputs,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompatImeFocusLifecycle {
    active: Option<String>,
    epoch: u64,
    pending_blur: Option<(u64, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompatImeFocusTransition {
    pub from: Option<String>,
    pub to: String,
    pub epoch: u64,
}

impl CompatImeFocusLifecycle {
    pub fn focus(&mut self, node_id: &str) -> CompatImeFocusTransition {
        self.epoch = self.epoch.wrapping_add(1);
        let from = self.active.replace(node_id.to_owned());
        self.pending_blur = None;
        CompatImeFocusTransition {
            from,
            to: node_id.to_owned(),
            epoch: self.epoch,
        }
    }

    pub fn begin_blur(&mut self, node_id: &str) -> Option<u64> {
        if self.active.as_deref() != Some(node_id) {
            return None;
        }
        self.epoch = self.epoch.wrapping_add(1);
        self.pending_blur = Some((self.epoch, node_id.to_owned()));
        Some(self.epoch)
    }

    pub fn resolve_blur(&mut self, epoch: u64) -> Option<String> {
        let (pending_epoch, node_id) = self.pending_blur.as_ref()?;
        if *pending_epoch != epoch || self.active.as_deref() != Some(node_id) {
            return None;
        }
        let node_id = node_id.clone();
        self.active = None;
        self.pending_blur = None;
        Some(node_id)
    }

    pub fn active(&self) -> Option<&str> {
        self.active.as_deref()
    }
}

pub fn text_tap_outcome<'a>(
    active: Option<&str>,
    inputs: impl IntoIterator<Item = (&'a str, [f32; 4])>,
    x: f32,
    y: f32,
) -> TextTapOutcome {
    let Some(active) = active else {
        return TextTapOutcome::NoActiveInput;
    };
    for (id, [left, top, width, height]) in inputs {
        if x >= left && x <= left + width && y >= top && y <= top + height {
            return if id == active {
                TextTapOutcome::ActiveInput
            } else {
                TextTapOutcome::OtherInput
            };
        }
    }
    TextTapOutcome::OutsideInputs
}

pub fn semantic_tracker_offset(flow: Flow, padding: Option<f32>) -> f32 {
    if flow == Flow::Overlay {
        -padding.unwrap_or_default()
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INPUTS: [(&str, [f32; 4]); 2] = [
        ("note-draft", [10.0, 20.0, 100.0, 40.0]),
        ("agent-prompt", [10.0, 80.0, 100.0, 40.0]),
    ];

    #[test]
    fn compat_text_taps_distinguish_keep_transfer_and_outside_blur() {
        assert_eq!(
            text_tap_outcome(Some("agent-prompt"), INPUTS, 20.0, 90.0),
            TextTapOutcome::ActiveInput
        );
        assert_eq!(
            text_tap_outcome(Some("agent-prompt"), INPUTS, 20.0, 30.0),
            TextTapOutcome::OtherInput
        );
        assert_eq!(
            text_tap_outcome(Some("agent-prompt"), INPUTS, 200.0, 200.0),
            TextTapOutcome::OutsideInputs
        );
        assert_eq!(
            text_tap_outcome(None, INPUTS, 200.0, 200.0),
            TextTapOutcome::NoActiveInput
        );
    }

    #[test]
    fn compat_ime_focus_epoch_keeps_transfer_active_and_resolves_outside_blur() {
        let mut lifecycle = CompatImeFocusLifecycle::default();
        let first = lifecycle.focus("agent-prompt");
        assert_eq!(first.from, None);
        assert_eq!(first.to, "agent-prompt");

        let stale_blur = lifecycle.begin_blur("agent-prompt").unwrap();
        let transfer = lifecycle.focus("note-draft");
        assert_eq!(transfer.from.as_deref(), Some("agent-prompt"));
        assert_eq!(transfer.to, "note-draft");
        assert_eq!(lifecycle.resolve_blur(stale_blur), None);
        assert_eq!(lifecycle.active(), Some("note-draft"));

        let outside_blur = lifecycle.begin_blur("note-draft").unwrap();
        assert_eq!(
            lifecycle.resolve_blur(outside_blur).as_deref(),
            Some("note-draft")
        );
        assert_eq!(lifecycle.active(), None);
    }

    #[test]
    fn compat_ime_focus_epoch_rejects_unowned_and_stale_blurs() {
        let mut lifecycle = CompatImeFocusLifecycle::default();
        lifecycle.focus("agent-prompt");
        assert_eq!(lifecycle.begin_blur("note-draft"), None);

        let first = lifecycle.begin_blur("agent-prompt").unwrap();
        let second = lifecycle.begin_blur("agent-prompt").unwrap();
        assert_eq!(lifecycle.resolve_blur(first), None);
        assert_eq!(lifecycle.active(), Some("agent-prompt"));
        assert_eq!(
            lifecycle.resolve_blur(second).as_deref(),
            Some("agent-prompt")
        );
    }

    #[test]
    fn overlay_semantic_tracker_cancels_its_padded_content_origin() {
        assert_eq!(semantic_tracker_offset(Flow::Overlay, Some(14.0)), -14.0);
        assert_eq!(semantic_tracker_offset(Flow::Overlay, None), 0.0);
        assert_eq!(semantic_tracker_offset(Flow::Row, Some(14.0)), 0.0);
        assert_eq!(semantic_tracker_offset(Flow::Column, Some(14.0)), 0.0);
    }
}
