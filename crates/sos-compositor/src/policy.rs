use std::collections::BTreeSet;

use anyhow::{bail, Result};
use compositor_control_protocol::{
    ShellOverlayConfiguration, WindowLayoutMode, WindowSpaceConfiguration, WindowSpaceGeometry,
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
    ShellOverlay,
    NativeApplication,
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
    native_application_pids: BTreeSet<u32>,
    shell_mapped: bool,
    shell_overlay_mapped: bool,
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

    pub fn register_native_application(&mut self, pid: u32) -> Result<()> {
        if pid == 0 || self.shell_pid == Some(pid) {
            bail!("native application PID is invalid or owns the shell");
        }
        self.native_application_pids.insert(pid);
        Ok(())
    }

    pub fn unregister_client(&mut self, pid: u32) {
        self.unregister_shell(pid);
        self.native_application_pids.remove(&pid);
    }

    pub fn classify(&self, pid: u32) -> ClientRole {
        if self.shell_pid == Some(pid) {
            ClientRole::Shell
        } else if self.native_application_pids.contains(&pid) {
            ClientRole::NativeApplication
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
            ClientRole::ShellOverlay if !self.shell_mapped => {
                bail!("shell overlay requires a mapped shell")
            }
            ClientRole::ShellOverlay if self.shell_overlay_mapped => {
                bail!("shell overlay surface is already mapped")
            }
            ClientRole::ShellOverlay => self.shell_overlay_mapped = true,
            ClientRole::NativeApplication | ClientRole::Compatibility if !self.shell_mapped => {
                bail!("application surface requires a mapped shell")
            }
            ClientRole::NativeApplication | ClientRole::Compatibility
                if self.compatibility_mapped >= MAX_COMPATIBILITY_TOPLEVELS =>
            {
                bail!("at most {MAX_COMPATIBILITY_TOPLEVELS} application toplevels are allowed")
            }
            ClientRole::NativeApplication | ClientRole::Compatibility => {
                self.compatibility_mapped += 1
            }
        }
        Ok(())
    }

    pub fn unmap(&mut self, role: ClientRole) {
        match role {
            ClientRole::Shell => self.shell_mapped = false,
            ClientRole::ShellOverlay => self.shell_overlay_mapped = false,
            ClientRole::NativeApplication | ClientRole::Compatibility => {
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
        if self.quiesced_revision.is_none() {
            bail!("input is not quiesced for the armed presentation");
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

pub fn default_shell_overlay(output: (i32, i32)) -> ShellOverlayConfiguration {
    const SIZE: i32 = 72;
    const MARGIN: i32 = 18;
    ShellOverlayConfiguration {
        x: (output.0 - SIZE - MARGIN).max(0),
        y: (output.1 - SIZE - MARGIN).max(0),
        width: SIZE as u32,
        height: SIZE as u32,
    }
}

pub fn validate_shell_overlay(
    configuration: ShellOverlayConfiguration,
    output: (i32, i32),
) -> Result<ShellOverlayConfiguration> {
    if configuration.x < 0
        || configuration.y < 0
        || configuration.width < 48
        || configuration.height < 48
    {
        bail!("shell overlay must have a positive origin and be at least 48x48 logical pixels");
    }
    if configuration.width > 720 || configuration.height > 360 {
        bail!("shell overlay exceeds its 720x360 logical-pixel bound");
    }
    let right = i64::from(configuration.x) + i64::from(configuration.width);
    let bottom = i64::from(configuration.y) + i64::from(configuration.height);
    if right > i64::from(output.0) || bottom > i64::from(output.1) {
        bail!("shell overlay exceeds the active output");
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
    let split_depth = usize::BITS - count.saturating_sub(1).leading_zeros();
    let gap_divisor = i32::try_from(split_depth)
        .unwrap_or(i32::MAX)
        .saturating_add(2);
    // Reserve two outer insets plus enough space for every recursive split on
    // a path. This keeps even an impossible requested gap bounded while every
    // leaf retains at least one logical pixel.
    let horizontal_gap = gap.min((width.saturating_sub(1) / gap_divisor).max(0));
    let vertical_gap = gap.min((height.saturating_sub(1) / gap_divisor).max(0));
    let inner_x = x + horizontal_gap;
    let inner_y = y + vertical_gap;
    let inner_width = (width - horizontal_gap.saturating_mul(2)).max(1);
    let inner_height = (height - vertical_gap.saturating_mul(2)).max(1);
    let mut rectangles = Vec::with_capacity(count);
    split_balanced_tile(
        WindowRectangle {
            x: inner_x,
            y: inner_y,
            width: inner_width,
            height: inner_height,
        },
        count,
        horizontal_gap,
        vertical_gap,
        &mut rectangles,
    );
    // Geometry order is also managed-window identity. Keep it row-major so a
    // focus raise cannot reassign windows when the next relayout occurs.
    rectangles.sort_by_key(|rectangle| (rectangle.y, rectangle.x));
    rectangles
}

fn split_balanced_tile(
    rectangle: WindowRectangle,
    count: usize,
    horizontal_gap: i32,
    vertical_gap: i32,
    rectangles: &mut Vec<WindowRectangle>,
) {
    if count == 1 {
        rectangles.push(rectangle);
        return;
    }

    let first_count = count.div_ceil(2);
    let second_count = count / 2;
    if rectangle.width >= rectangle.height {
        let available = (rectangle.width - horizontal_gap).max(2);
        let first_width = i32::try_from(
            i64::from(available) * i64::try_from(first_count).unwrap_or(i64::MAX)
                / i64::try_from(count).unwrap_or(i64::MAX),
        )
        .unwrap_or(i32::MAX)
        .clamp(1, available - 1);
        let second_width = available - first_width;
        split_balanced_tile(
            WindowRectangle {
                width: first_width,
                ..rectangle
            },
            first_count,
            horizontal_gap,
            vertical_gap,
            rectangles,
        );
        split_balanced_tile(
            WindowRectangle {
                x: rectangle.x + first_width + horizontal_gap,
                width: second_width,
                ..rectangle
            },
            second_count,
            horizontal_gap,
            vertical_gap,
            rectangles,
        );
    } else {
        let available = (rectangle.height - vertical_gap).max(2);
        let first_height = i32::try_from(
            i64::from(available) * i64::try_from(first_count).unwrap_or(i64::MAX)
                / i64::try_from(count).unwrap_or(i64::MAX),
        )
        .unwrap_or(i32::MAX)
        .clamp(1, available - 1);
        let second_height = available - first_height;
        split_balanced_tile(
            WindowRectangle {
                height: first_height,
                ..rectangle
            },
            first_count,
            horizontal_gap,
            vertical_gap,
            rectangles,
        );
        split_balanced_tile(
            WindowRectangle {
                y: rectangle.y + first_height + vertical_gap,
                height: second_height,
                ..rectangle
            },
            second_count,
            horizontal_gap,
            vertical_gap,
            rectangles,
        );
    }
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
    fn authenticated_application_pid_is_native_without_receiving_shell_authority() {
        let mut policy = SurfacePolicy::default();
        policy.register_shell(41).unwrap();
        policy.register_native_application(42).unwrap();
        assert_eq!(policy.classify(42), ClientRole::NativeApplication);
        assert!(policy.arm(42, 1, REVISION.into()).is_err());
        policy.unregister_client(42);
        assert_eq!(policy.classify(42), ClientRole::Compatibility);
    }

    #[test]
    fn presented_input_stays_quiesced_until_explicit_finalize() {
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
        assert!(policy.input_quiesced());
        assert!(policy.resume_input(41, REVISION).unwrap());
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
        assert!(policy.input_quiesced());
        assert!(policy.resume_input(41, REVISION).unwrap());
        assert!(!policy.input_quiesced());
    }

    #[test]
    fn rollback_presentation_reuses_the_quiesced_candidate_epoch() {
        let mut policy = SurfacePolicy::default();
        let restored = "b".repeat(64);
        policy.register_shell(41).unwrap();
        policy.quiesce_input(41, REVISION.into()).unwrap();
        policy.arm(41, 7, REVISION.into()).unwrap();
        policy.record_shell_commit();
        assert!(policy.record_successful_submit(true).is_some());

        policy.arm(41, 8, restored.clone()).unwrap();
        policy.record_shell_commit();
        let presented = policy.record_successful_submit(true).unwrap();
        assert_eq!(presented.revision_id, restored);
        assert!(policy.input_quiesced());
        assert!(policy.resume_input(41, REVISION).unwrap());
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
        policy.map(ClientRole::NativeApplication).unwrap();
        for _ in 1..MAX_COMPATIBILITY_TOPLEVELS {
            policy.map(ClientRole::Compatibility).unwrap();
        }
        assert!(policy.map(ClientRole::Compatibility).is_err());
        policy.unmap(ClientRole::NativeApplication);
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

    #[test]
    fn three_tiled_windows_use_balanced_recursive_splits() {
        let configuration = WindowSpaceConfiguration {
            geometry: WindowSpaceGeometry {
                x: 0,
                y: 0,
                width: 1000,
                height: 700,
                gap: 10,
            },
            layout: WindowLayoutMode::Tiling,
        };

        let rectangles = window_rectangles(configuration, 3);

        assert_eq!(rectangles.len(), 3);
        assert_eq!(
            rectangles[0],
            WindowRectangle {
                x: 10,
                y: 10,
                width: 646,
                height: 335,
            }
        );
        assert_eq!(
            rectangles[1],
            WindowRectangle {
                x: 666,
                y: 10,
                width: 324,
                height: 680,
            }
        );
        assert_eq!(
            rectangles[2],
            WindowRectangle {
                x: 10,
                y: 355,
                width: 646,
                height: 335,
            }
        );
    }

    #[test]
    fn four_tiled_windows_form_a_balanced_quad() {
        let configuration = WindowSpaceConfiguration {
            geometry: WindowSpaceGeometry {
                x: 0,
                y: 0,
                width: 1000,
                height: 700,
                gap: 10,
            },
            layout: WindowLayoutMode::Tiling,
        };

        let rectangles = window_rectangles(configuration, 4);

        assert_eq!(rectangles.len(), 4);
        assert_eq!(
            rectangles,
            vec![
                WindowRectangle {
                    x: 10,
                    y: 10,
                    width: 485,
                    height: 335,
                },
                WindowRectangle {
                    x: 505,
                    y: 10,
                    width: 485,
                    height: 335,
                },
                WindowRectangle {
                    x: 10,
                    y: 355,
                    width: 485,
                    height: 335,
                },
                WindowRectangle {
                    x: 505,
                    y: 355,
                    width: 485,
                    height: 335,
                },
            ]
        );
    }

    #[test]
    fn tiled_layout_reduces_an_impossible_gap_without_escaping_its_space() {
        let configuration = WindowSpaceConfiguration {
            geometry: WindowSpaceGeometry {
                x: 20,
                y: 30,
                width: 160,
                height: 120,
                gap: 128,
            },
            layout: WindowLayoutMode::Tiling,
        };

        let rectangles = window_rectangles(configuration, MAX_COMPATIBILITY_TOPLEVELS);

        assert_eq!(rectangles.len(), MAX_COMPATIBILITY_TOPLEVELS);
        for rectangle in rectangles {
            assert!(rectangle.x >= configuration.geometry.x);
            assert!(rectangle.y >= configuration.geometry.y);
            assert!(rectangle.width > 0 && rectangle.height > 0);
            assert!(rectangle.x + rectangle.width <= 180);
            assert!(rectangle.y + rectangle.height <= 150);
        }
    }
}
