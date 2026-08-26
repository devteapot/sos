use anyhow::{bail, Result};
use compositor_control_protocol::{
    WindowLayoutMode, WindowSpaceConfiguration, WindowSpaceGeometry,
};

pub const MAX_COMPATIBILITY_TOPLEVELS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowRectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

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
    queued_submit_sequence: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedRevision {
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
    quiesced_revision: Option<String>,
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
            self.quiesced_revision = None;
        }
    }

    pub fn classify(&self, pid: u32) -> ClientRole {
        if self.shell_pid == Some(pid) {
            ClientRole::Shell
        } else {
            ClientRole::Compatibility
        }
    }

    pub fn is_shell_owner(&self, pid: u32) -> bool {
        self.shell_pid == Some(pid)
    }

    pub fn map(&mut self, role: ClientRole) -> Result<()> {
        match role {
            ClientRole::Shell if self.shell_mapped => bail!("shell surface is already mapped"),
            ClientRole::Shell => self.shell_mapped = true,
            ClientRole::Compatibility if !self.shell_mapped => {
                bail!("compatibility surface requires a mapped shell")
            }
            ClientRole::Compatibility
                if self.compatibility_mapped >= MAX_COMPATIBILITY_TOPLEVELS =>
            {
                bail!("at most {MAX_COMPATIBILITY_TOPLEVELS} compatibility toplevels are allowed")
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
        self.validate_revision(&revision_id)?;
        if self.quiesced_revision.as_deref() != Some(&revision_id) {
            bail!("input is not quiesced for the armed revision");
        }
        let after_commit_sequence = self.shell_commit_sequence;
        self.pending = Some(ArmedPresentation {
            request_id,
            revision_id,
            after_commit_sequence,
            target_commit_sequence: None,
            queued_submit_sequence: None,
        });
        Ok(after_commit_sequence)
    }

    pub fn quiesce_input(&mut self, pid: u32, revision_id: String) -> Result<bool> {
        if self.shell_pid != Some(pid) {
            bail!("input quiesce is not owned by the registered shell");
        }
        self.validate_revision(&revision_id)?;
        match self.quiesced_revision.as_deref() {
            Some(current) if current == revision_id => Ok(false),
            Some(_) => bail!("input is already quiesced for another revision"),
            None => {
                self.quiesced_revision = Some(revision_id);
                Ok(true)
            }
        }
    }

    pub fn resume_input(&mut self, pid: u32, revision_id: &str) -> Result<bool> {
        if self.shell_pid != Some(pid) {
            bail!("input resume is not owned by the registered shell");
        }
        if self.pending.is_some() {
            bail!("cannot resume input while presentation is armed");
        }
        match self.quiesced_revision.as_deref() {
            Some(current) if current == revision_id => {
                self.quiesced_revision = None;
                Ok(true)
            }
            Some(_) => bail!("quiesced revision does not match input resume"),
            None => Ok(false),
        }
    }

    fn validate_revision(&self, revision_id: &str) -> Result<()> {
        if revision_id.len() != 64 || !revision_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("revision ID is not a SHA-256 identity");
        }
        Ok(())
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

    pub fn queued_revision(&self, shell_rendered: bool) -> Option<QueuedRevision> {
        if !shell_rendered {
            return None;
        }
        let pending = self.pending.as_ref()?;
        if pending.queued_submit_sequence.is_some() {
            return None;
        }
        Some(QueuedRevision {
            request_id: pending.request_id,
            revision_id: pending.revision_id.clone(),
            commit_sequence: pending.target_commit_sequence?,
            submit_sequence: self.submit_sequence.saturating_add(1),
        })
    }

    pub fn record_frame_queued(&mut self, queued: Option<&QueuedRevision>) {
        self.submit_sequence = self.submit_sequence.saturating_add(1);
        let Some(queued) = queued else {
            return;
        };
        if let Some(pending) = &mut self.pending {
            if pending.request_id == queued.request_id
                && pending.revision_id == queued.revision_id
                && pending.target_commit_sequence == Some(queued.commit_sequence)
            {
                pending.queued_submit_sequence = Some(queued.submit_sequence);
            }
        }
    }

    pub fn record_presented(&mut self, queued: QueuedRevision) -> Option<QueuedRevision> {
        let pending = self.pending.as_ref()?;
        if pending.request_id != queued.request_id
            || pending.revision_id != queued.revision_id
            || pending.target_commit_sequence != Some(queued.commit_sequence)
            || pending.queued_submit_sequence != Some(queued.submit_sequence)
        {
            return None;
        }
        let pending = self
            .pending
            .take()
            .expect("pending presentation was checked");
        self.quiesced_revision = None;
        debug_assert_eq!(pending.revision_id, queued.revision_id);
        Some(queued)
    }

    pub fn record_successful_submit(&mut self, shell_rendered: bool) -> Option<QueuedRevision> {
        let queued = self.queued_revision(shell_rendered);
        self.record_frame_queued(queued.as_ref());
        queued.and_then(|queued| self.record_presented(queued))
    }

    pub fn input_quiesced(&self) -> bool {
        self.quiesced_revision.is_some()
    }

    pub fn shell_mapped(&self) -> bool {
        self.shell_mapped
    }
}

pub fn compatibility_location(output: (i32, i32), window: (i32, i32)) -> (i32, i32) {
    (
        ((output.0 - window.0) / 2).max(24),
        ((output.1 - window.1) / 2).max(24),
    )
}

pub fn default_window_space(output: (i32, i32)) -> WindowSpaceConfiguration {
    WindowSpaceConfiguration {
        geometry: WindowSpaceGeometry {
            x: 24,
            y: 72,
            width: u32::try_from((output.0 - 48).max(1)).unwrap_or(1),
            height: u32::try_from((output.1 - 96).max(1)).unwrap_or(1),
            gap: 12,
        },
        layout: WindowLayoutMode::Floating,
    }
}

pub fn validate_window_space(
    configuration: WindowSpaceConfiguration,
    output: (i32, i32),
) -> Result<WindowSpaceConfiguration> {
    let geometry = configuration.geometry;
    if geometry.x < 0 || geometry.y < 0 || geometry.width < 160 || geometry.height < 120 {
        bail!("window space must have positive origin and be at least 160x120 logical pixels");
    }
    if geometry.gap > 128 {
        bail!("window space gap exceeds 128 logical pixels");
    }
    let right = i64::from(geometry.x) + i64::from(geometry.width);
    let bottom = i64::from(geometry.y) + i64::from(geometry.height);
    if right > i64::from(output.0) || bottom > i64::from(output.1) {
        bail!("window space exceeds the active output");
    }
    Ok(configuration)
}

pub fn window_rectangles(
    configuration: WindowSpaceConfiguration,
    count: usize,
) -> Vec<WindowRectangle> {
    let count = count.min(MAX_COMPATIBILITY_TOPLEVELS);
    if count == 0 {
        return Vec::new();
    }
    let geometry = configuration.geometry;
    let x = geometry.x;
    let y = geometry.y;
    let width = i32::try_from(geometry.width).unwrap_or(i32::MAX);
    let height = i32::try_from(geometry.height).unwrap_or(i32::MAX);
    let gap = i32::try_from(geometry.gap).unwrap_or(128);
    match configuration.layout {
        WindowLayoutMode::Floating => floating_rectangles(x, y, width, height, gap, count),
        WindowLayoutMode::Tiling => tiled_rectangles(x, y, width, height, gap, count),
        WindowLayoutMode::Scrolling => scrolling_rectangles(x, y, width, height, gap, count),
    }
}

fn floating_rectangles(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    gap: i32,
    count: usize,
) -> Vec<WindowRectangle> {
    let inset_width = (width - gap.saturating_mul(2)).max(1);
    let inset_height = (height - gap.saturating_mul(2)).max(1);
    let window_width = (inset_width * 4 / 5).clamp(1, 960.min(inset_width));
    let window_height = (inset_height * 4 / 5).clamp(1, 720.min(inset_height));
    let x_room = (inset_width - window_width).max(0);
    let y_room = (inset_height - window_height).max(0);
    let divisor = i32::try_from(count.saturating_sub(1)).unwrap_or(1).max(1);
    let x_step = (x_room / divisor).min(28);
    let y_step = (y_room / divisor).min(28);
    let x_start = x + gap + (x_room - x_step * (count as i32 - 1)) / 2;
    let y_start = y + gap + (y_room - y_step * (count as i32 - 1)) / 2;
    (0..count)
        .map(|index| WindowRectangle {
            x: x_start + x_step * index as i32,
            y: y_start + y_step * index as i32,
            width: window_width,
            height: window_height,
        })
        .collect()
}

fn tiled_rectangles(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    gap: i32,
    count: usize,
) -> Vec<WindowRectangle> {
    let columns = match count {
        1 => 1,
        2..=4 => 2,
        _ => 3,
    };
    let rows = count.div_ceil(columns);
    let columns_i32 = columns as i32;
    let rows_i32 = rows as i32;
    let cell_width = ((width - gap * (columns_i32 + 1)) / columns_i32).max(1);
    let cell_height = ((height - gap * (rows_i32 + 1)) / rows_i32).max(1);
    (0..count)
        .map(|index| {
            let column = (index % columns) as i32;
            let row = (index / columns) as i32;
            WindowRectangle {
                x: x + gap + column * (cell_width + gap),
                y: y + gap + row * (cell_height + gap),
                width: if column == columns_i32 - 1 {
                    (x + width - gap) - (x + gap + column * (cell_width + gap))
                } else {
                    cell_width
                },
                height: if row == rows_i32 - 1 {
                    (y + height - gap) - (y + gap + row * (cell_height + gap))
                } else {
                    cell_height
                },
            }
        })
        .collect()
}

fn scrolling_rectangles(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    gap: i32,
    count: usize,
) -> Vec<WindowRectangle> {
    let inset_width = (width - gap.saturating_mul(2)).max(1);
    let window_width = (inset_width * 3 / 4).max(1);
    let room = (inset_width - window_width).max(0);
    let divisor = i32::try_from(count.saturating_sub(1)).unwrap_or(1).max(1);
    let step = (room / divisor).min(96);
    let start = x + gap + (room - step * (count as i32 - 1)) / 2;
    (0..count)
        .map(|index| WindowRectangle {
            x: start + step * index as i32,
            y: y + gap,
            width: window_width,
            height: (height - gap.saturating_mul(2)).max(1),
        })
        .collect()
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
        policy.quiesce_input(41, REVISION.into()).unwrap();
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
    fn direct_queue_does_not_release_input_until_matching_presentation() {
        let mut policy = SurfacePolicy::default();
        policy.register_shell(41).unwrap();
        policy.quiesce_input(41, REVISION.into()).unwrap();
        policy.arm(41, 7, REVISION.into()).unwrap();
        policy.record_shell_commit();

        let queued = policy.queued_revision(true).unwrap();
        policy.record_frame_queued(Some(&queued));
        assert!(policy.input_quiesced());
        assert!(policy.queued_revision(true).is_none());
        let mut wrong_frame = queued.clone();
        wrong_frame.submit_sequence += 1;
        assert!(policy.record_presented(wrong_frame).is_none());
        assert!(policy.input_quiesced());

        assert_eq!(policy.record_presented(queued).unwrap().request_id, 7);
        assert!(!policy.input_quiesced());
    }

    #[test]
    fn input_quiesce_is_revision_bound_and_abortable_before_arm() {
        let mut policy = SurfacePolicy::default();
        policy.register_shell(41).unwrap();
        assert!(policy.quiesce_input(42, REVISION.into()).is_err());
        assert!(policy.quiesce_input(41, REVISION.into()).unwrap());
        assert!(policy.input_quiesced());
        assert!(!policy.quiesce_input(41, REVISION.into()).unwrap());
        assert!(policy.resume_input(41, &"b".repeat(64)).is_err());
        assert!(policy.resume_input(41, REVISION).unwrap());
        assert!(!policy.input_quiesced());
    }

    #[test]
    fn surface_cardinality_and_placement_are_bounded() {
        let mut policy = SurfacePolicy::default();
        policy.map(ClientRole::Shell).unwrap();
        assert!(policy.map(ClientRole::Shell).is_err());
        for _ in 0..MAX_COMPATIBILITY_TOPLEVELS {
            policy.map(ClientRole::Compatibility).unwrap();
        }
        assert!(policy.map(ClientRole::Compatibility).is_err());
        policy.unmap(ClientRole::Compatibility);
        policy.map(ClientRole::Compatibility).unwrap();
        assert_eq!(compatibility_location((1280, 800), (720, 520)), (280, 140));
    }

    #[test]
    fn shell_window_layouts_stay_inside_the_declared_space() {
        let mut configuration = WindowSpaceConfiguration {
            geometry: WindowSpaceGeometry {
                x: 20,
                y: 60,
                width: 1000,
                height: 700,
                gap: 12,
            },
            layout: WindowLayoutMode::Floating,
        };
        assert!(validate_window_space(configuration, (1280, 800)).is_ok());
        for layout in [
            WindowLayoutMode::Floating,
            WindowLayoutMode::Tiling,
            WindowLayoutMode::Scrolling,
        ] {
            configuration.layout = layout;
            let rectangles = window_rectangles(configuration, 8);
            assert_eq!(rectangles.len(), 8);
            for rectangle in rectangles {
                assert!(rectangle.x >= configuration.geometry.x);
                assert!(rectangle.y >= configuration.geometry.y);
                assert!(rectangle.width > 0 && rectangle.height > 0);
                assert!(
                    rectangle.x + rectangle.width
                        <= configuration.geometry.x + configuration.geometry.width as i32
                );
                assert!(
                    rectangle.y + rectangle.height
                        <= configuration.geometry.y + configuration.geometry.height as i32
                );
            }
        }

        configuration.geometry.width = 159;
        assert!(validate_window_space(configuration, (1280, 800)).is_err());
    }
}
