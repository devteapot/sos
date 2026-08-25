//! Linux-backed provider adapters for the prototype experience model.

mod system;

use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use experience_ir::{
    CalendarEvent, ExperienceModel, Music, Note, ProviderEffect, ProviderSurface,
    ProviderSurfaceKind, ProviderSurfaceStatus, SystemState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ApplicationLaunch,
    AudioControl,
    CalendarRead,
    CalendarWrite,
    NetworkControl,
    NotesRead,
    NotesWrite,
    MusicRead,
    MusicControl,
    SystemRead,
    VideoRead,
    CameraRead,
    ProtectedSurface,
}

#[derive(Clone, Debug)]
pub struct ProviderContext {
    pub revision_id: String,
    pub grants: BTreeSet<Capability>,
    pub cancellation: CancellationToken,
}

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn check(&self) -> Result<(), ProviderError> {
        if self.0.load(Ordering::Acquire) {
            Err(ProviderError::Cancelled)
        } else {
            Ok(())
        }
    }
}

pub type SystemSnapshot = SystemState;

#[derive(Clone, Debug)]
pub struct ProviderFrame {
    pub surface_id: String,
    pub extension: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

#[derive(Clone, Debug)]
pub struct ProviderSnapshot {
    pub model: ExperienceModel,
    pub frames: Vec<ProviderFrame>,
}

#[derive(Debug, Deserialize)]
struct SurfaceManifest {
    kind: ProviderSurfaceKind,
    width: u32,
    height: u32,
    frame: String,
    #[serde(default)]
    protected: bool,
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider capability denied: {0:?}")]
    Denied(Capability),
    #[error("provider operation cancelled")]
    Cancelled,
    #[error("provider temporarily unavailable: {0}")]
    Unavailable(String),
    #[error("invalid provider path")]
    InvalidPath,
    #[error("provider I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("provider JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug)]
pub struct ProviderHub {
    root: PathBuf,
    system: system::SystemAdapter,
}

impl ProviderHub {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, ProviderError> {
        let root = root.into();
        fs::create_dir_all(root.join("notes"))?;
        fs::create_dir_all(root.join("calendar"))?;
        fs::create_dir_all(root.join("surfaces"))?;
        Ok(Self {
            root,
            system: system::SystemAdapter::connect(),
        })
    }

    pub fn snapshot(&self, context: &ProviderContext) -> Result<ExperienceModel, ProviderError> {
        Ok(self.snapshot_with_frames(context)?.model)
    }

    pub fn snapshot_with_frames(
        &self,
        context: &ProviderContext,
    ) -> Result<ProviderSnapshot, ProviderError> {
        context.cancellation.check()?;
        require(context, Capability::NotesRead)?;
        require(context, Capability::CalendarRead)?;
        require(context, Capability::MusicRead)?;
        let mut model = providers_fake::snapshot();
        model.notes = self.notes()?;
        model.calendar = self.calendar()?;
        model.music = self.music()?;
        model.system = self.system(context)?;
        model.providers = self.system.snapshot(context)?;
        let (surfaces, frames) = self.surfaces(context)?;
        model.surfaces = surfaces;
        Ok(ProviderSnapshot { model, frames })
    }

    pub fn system(&self, context: &ProviderContext) -> Result<SystemSnapshot, ProviderError> {
        context.cancellation.check()?;
        require(context, Capability::SystemRead)?;
        Ok(system_snapshot(
            Path::new("/sys/class/net"),
            Path::new("/sys/class/power_supply"),
            Path::new("/sys/class/drm"),
            Path::new("/sys/class/input"),
        ))
    }

    pub fn write_note(
        &self,
        context: &ProviderContext,
        name: &str,
        content: &str,
    ) -> Result<(), ProviderError> {
        context.cancellation.check()?;
        require(context, Capability::NotesWrite)?;
        let path = safe_child(&self.root.join("notes"), name, "md")?;
        atomic_write(&path, content.as_bytes())
    }

    pub fn append_calendar_event(
        &self,
        context: &ProviderContext,
        name: &str,
        event: &CalendarEvent,
    ) -> Result<(), ProviderError> {
        context.cancellation.check()?;
        require(context, Capability::CalendarWrite)?;
        let path = safe_child(&self.root.join("calendar"), name, "ics")?;
        let body = format!(
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:{}\nSUMMARY:{}\nDESCRIPTION:{}\nEND:VEVENT\nEND:VCALENDAR\n",
            sanitize_line(&event.time),
            sanitize_line(&event.title),
            sanitize_line(&event.detail)
        );
        atomic_write(&path, body.as_bytes())
    }

    pub fn music_command(
        &self,
        context: &ProviderContext,
        command: &str,
    ) -> Result<(), ProviderError> {
        context.cancellation.check()?;
        require(context, Capability::MusicControl)?;
        if !matches!(command, "play-pause" | "next" | "previous") {
            return Err(ProviderError::Unavailable(
                "unsupported MPRIS command".into(),
            ));
        }
        let status = Command::new("playerctl")
            .arg(command)
            .status()
            .map_err(|error| {
                ProviderError::Unavailable(format!("playerctl/MPRIS is unavailable: {error}"))
            })?;
        if status.success() {
            Ok(())
        } else {
            Err(ProviderError::Unavailable(format!(
                "MPRIS command exited with {status}"
            )))
        }
    }

    pub fn execute_effect(
        &self,
        context: &ProviderContext,
        effect: &ProviderEffect,
    ) -> Result<(), ProviderError> {
        context.cancellation.check()?;
        if system::is_system_effect(effect) {
            return self.system.execute(context, effect);
        }
        match (effect.provider.as_str(), effect.action.as_str()) {
            ("notes", "write") => self.write_note(
                context,
                required_string(&effect.payload, "name")?,
                required_string(&effect.payload, "content")?,
            ),
            ("calendar", "append") => self.append_calendar_event(
                context,
                required_string(&effect.payload, "name")?,
                &CalendarEvent {
                    time: required_string(&effect.payload, "time")?.into(),
                    title: required_string(&effect.payload, "title")?.into(),
                    detail: required_string(&effect.payload, "detail")?.into(),
                },
            ),
            ("music", "command") => {
                self.music_command(context, required_string(&effect.payload, "command")?)
            }
            _ => Err(ProviderError::Unavailable(format!(
                "unsupported Linux provider effect: {}.{}",
                effect.provider, effect.action
            ))),
        }
    }

    pub fn generation(&self) -> Result<String, ProviderError> {
        let mut paths = Vec::new();
        collect_files(&self.root, &mut paths)?;
        paths.sort();
        let mut digest = Sha256::new();
        for path in paths {
            digest.update(
                path.strip_prefix(&self.root)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes(),
            );
            digest.update(fs::read(path)?);
        }
        let mut system = system_snapshot(
            Path::new("/sys/class/net"),
            Path::new("/sys/class/power_supply"),
            Path::new("/sys/class/drm"),
            Path::new("/sys/class/input"),
        );
        // Wall-clock movement is read on each snapshot but is not itself an
        // event trigger. Connectivity, audio, power, and device changes are.
        system.unix_time_ms = 0;
        digest.update(serde_json::to_vec(&system)?);
        digest.update(serde_json::to_vec(&self.system.fingerprint()?)?);
        Ok(format!("{:x}", digest.finalize()))
    }

    fn notes(&self) -> Result<Vec<Note>, ProviderError> {
        let mut notes = Vec::new();
        for path in sorted_files(&self.root.join("notes"), "md")? {
            let text = fs::read_to_string(&path)?;
            let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
            let title = lines
                .next()
                .map(|line| line.trim_start_matches('#').trim().to_owned())
                .unwrap_or_else(|| file_stem(&path));
            let preview = lines.next().unwrap_or_default().to_owned();
            notes.push(Note { title, preview });
        }
        Ok(notes)
    }

    fn calendar(&self) -> Result<Vec<CalendarEvent>, ProviderError> {
        let mut events = Vec::new();
        for path in sorted_files(&self.root.join("calendar"), "ics")? {
            let text = fs::read_to_string(path)?;
            for block in text.split("BEGIN:VEVENT").skip(1) {
                let block = block.split("END:VEVENT").next().unwrap_or(block);
                events.push(CalendarEvent {
                    time: ics_value(block, "DTSTART:").unwrap_or_default(),
                    title: ics_value(block, "SUMMARY:").unwrap_or_else(|| "Untitled".into()),
                    detail: ics_value(block, "DESCRIPTION:").unwrap_or_default(),
                });
            }
        }
        Ok(events)
    }

    fn music(&self) -> Result<Music, ProviderError> {
        let fallback = self.root.join("music.json");
        if fallback.exists() {
            return Ok(serde_json::from_slice(&fs::read(fallback)?)?);
        }
        let output = Command::new("playerctl")
            .args(["metadata", "--format", "{{title}}\n{{artist}}\n{{status}}"])
            .output();
        match output {
            Ok(output) if output.status.success() => {
                let text = String::from_utf8_lossy(&output.stdout);
                let mut lines = text.lines();
                Ok(Music {
                    title: lines.next().unwrap_or_default().into(),
                    artist: lines.next().unwrap_or_default().into(),
                    playing: lines.next() == Some("Playing"),
                })
            }
            _ => Ok(Music {
                title: String::new(),
                artist: String::new(),
                playing: false,
            }),
        }
    }

    fn surfaces(
        &self,
        context: &ProviderContext,
    ) -> Result<(Vec<ProviderSurface>, Vec<ProviderFrame>), ProviderError> {
        let root = self.root.join("surfaces");
        let mut surfaces = Vec::new();
        let mut frames = Vec::new();
        for manifest_path in sorted_files(&root, "json")? {
            context.cancellation.check()?;
            let id = file_stem(&manifest_path);
            if !valid_component(&id) {
                continue;
            }
            let manifest: SurfaceManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
            if manifest.width == 0
                || manifest.height == 0
                || manifest.width > 4096
                || manifest.height > 4096
                || !valid_component(&manifest.frame)
            {
                return Err(ProviderError::Unavailable(format!(
                    "invalid surface manifest: {}",
                    manifest_path.display()
                )));
            }
            let read_capability = match manifest.kind {
                ProviderSurfaceKind::Video => Capability::VideoRead,
                ProviderSurfaceKind::Camera => Capability::CameraRead,
            };
            if !context.grants.contains(&read_capability)
                || (manifest.protected && !context.grants.contains(&Capability::ProtectedSurface))
            {
                continue;
            }
            let mut status = if manifest.protected {
                ProviderSurfaceStatus::ProtectedUnavailable
            } else {
                ProviderSurfaceStatus::Ready
            };
            if !manifest.protected {
                let frame_path = root.join(&manifest.frame);
                let extension = frame_path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                match fs::read(&frame_path) {
                    Ok(bytes) if valid_surface_frame(&extension, &bytes) => {
                        frames.push(ProviderFrame {
                            surface_id: id.clone(),
                            extension,
                            sha256: format!("{:x}", Sha256::digest(&bytes)),
                            bytes,
                        });
                    }
                    Ok(_) | Err(_) => status = ProviderSurfaceStatus::Unavailable,
                }
            }
            surfaces.push(ProviderSurface {
                id,
                kind: manifest.kind,
                width: manifest.width,
                height: manifest.height,
                protected: manifest.protected,
                status,
            });
        }
        Ok((surfaces, frames))
    }
}

fn required_string<'a>(
    payload: &'a serde_json::Value,
    name: &str,
) -> Result<&'a str, ProviderError> {
    payload
        .get(name)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ProviderError::Unavailable(format!("provider effect omitted {name}")))
}

pub fn prototype_grants(revision_id: impl Into<String>) -> ProviderContext {
    ProviderContext {
        revision_id: revision_id.into(),
        grants: [
            Capability::ApplicationLaunch,
            Capability::AudioControl,
            Capability::CalendarRead,
            Capability::CalendarWrite,
            Capability::NetworkControl,
            Capability::NotesRead,
            Capability::NotesWrite,
            Capability::MusicRead,
            Capability::MusicControl,
            Capability::SystemRead,
            Capability::VideoRead,
            Capability::CameraRead,
        ]
        .into_iter()
        .collect(),
        cancellation: CancellationToken::default(),
    }
}

pub fn load_grants(
    path: &Path,
    revision_id: &str,
    allow_development_wildcard: bool,
) -> Result<ProviderContext, ProviderError> {
    let metadata = fs::metadata(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(ProviderError::Unavailable(
                "provider grant manifest must not be group/world accessible".into(),
            ));
        }
    }
    if metadata.len() > 64 * 1024 {
        return Err(ProviderError::Unavailable(
            "provider grant manifest exceeds 64 KiB".into(),
        ));
    }
    let grants: std::collections::BTreeMap<String, BTreeSet<Capability>> =
        serde_json::from_slice(&fs::read(path)?)?;
    let selected = grants.get(revision_id).cloned().or_else(|| {
        allow_development_wildcard
            .then(|| grants.get("*").cloned())
            .flatten()
    });
    Ok(ProviderContext {
        revision_id: revision_id.into(),
        grants: selected.unwrap_or_default(),
        cancellation: CancellationToken::default(),
    })
}

fn require(context: &ProviderContext, capability: Capability) -> Result<(), ProviderError> {
    context
        .grants
        .contains(&capability)
        .then_some(())
        .ok_or(ProviderError::Denied(capability))
}

fn safe_child(root: &Path, name: &str, extension: &str) -> Result<PathBuf, ProviderError> {
    let relative = Path::new(name);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(ProviderError::InvalidPath);
    }
    let mut path = root.join(relative);
    path.set_extension(extension);
    Ok(path)
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && !value.contains(['/', '\\'])
}

fn valid_surface_frame(extension: &str, bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes.len() <= 16 * 1024 * 1024
        && match extension {
            "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "jpg" | "jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
            "webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
            _ => false,
        }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ProviderError> {
    let temporary = path.with_extension("sos-tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn sanitize_line(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

fn sorted_files(root: &Path, extension: &str) -> Result<Vec<PathBuf>, ProviderError> {
    let mut files = fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|value| value == extension))
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn ics_value(block: &str, prefix: &str) -> Option<String> {
    block
        .lines()
        .find_map(|line| line.trim().strip_prefix(prefix).map(str::to_owned))
}

fn collect_files(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), ProviderError> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, output)?;
        } else {
            output.push(path);
        }
    }
    Ok(())
}

fn online_interfaces(root: &Path) -> Vec<String> {
    let mut interfaces = fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            (fs::read_to_string(entry.path().join("operstate"))
                .ok()?
                .trim()
                == "up")
                .then(|| entry.file_name().to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    interfaces.sort();
    interfaces
}

fn read_battery_capacity(root: &Path) -> Option<u8> {
    fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            let path = entry.path();
            fs::read_to_string(path.join("type"))
                .ok()
                .is_some_and(|value| value.trim() == "Battery")
                && !fs::read_to_string(path.join("scope"))
                    .ok()
                    .is_some_and(|value| value.trim() == "Device")
        })
        .find_map(|entry| {
            fs::read_to_string(entry.path().join("capacity"))
                .ok()?
                .trim()
                .parse()
                .ok()
        })
}

fn read_ac_online(root: &Path) -> Option<bool> {
    let mut found = false;
    let mut online = false;
    for entry in fs::read_dir(root).ok()?.filter_map(Result::ok) {
        let Ok(value) = fs::read_to_string(entry.path().join("online")) else {
            continue;
        };
        found = true;
        online |= value.trim() == "1";
    }
    found.then_some(online)
}

fn system_snapshot(net: &Path, power: &Path, drm: &Path, input: &Path) -> SystemSnapshot {
    let (audio_volume_percent, audio_muted) = read_audio_state();
    SystemSnapshot {
        unix_time_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
        timezone: std::env::var("TZ")
            .ok()
            .filter(|value| !value.is_empty())
            .or_else(|| {
                fs::read_to_string("/etc/timezone")
                    .ok()
                    .map(|value| value.trim().into())
            })
            .unwrap_or_else(|| "UTC".into()),
        online_interfaces: online_interfaces(net),
        battery_percent: read_battery_capacity(power),
        on_ac_power: read_ac_online(power),
        audio_volume_percent,
        audio_muted,
        connected_displays: named_entries_with_state(drm, "card", "status", "connected"),
        input_devices: input_event_devices(input),
    }
}

fn read_audio_state() -> (Option<u8>, Option<bool>) {
    let output = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output();
    let Ok(output) = output else {
        return (None, None);
    };
    if !output.status.success() {
        return (None, None);
    }
    parse_wpctl_volume(&String::from_utf8_lossy(&output.stdout))
}

fn parse_wpctl_volume(value: &str) -> (Option<u8>, Option<bool>) {
    let volume = value
        .split_whitespace()
        .find_map(|part| part.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| (value * 100.0).round().clamp(0.0, 255.0) as u8);
    let muted = volume.map(|_| value.contains("[MUTED]"));
    (volume, muted)
}

fn named_entries_with_state(
    root: &Path,
    prefix: &str,
    state_file: &str,
    expected: &str,
) -> Vec<String> {
    let mut entries = fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (name.starts_with(prefix)
                && fs::read_to_string(entry.path().join(state_file))
                    .ok()
                    .is_some_and(|state| state.trim() == expected))
            .then_some(name)
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn input_event_devices(root: &Path) -> Vec<String> {
    let mut devices = fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with("event").then_some(name)
        })
        .collect::<Vec<_>>();
    devices.sort();
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_files_drive_three_provider_domains_and_generation() {
        let temp = tempfile::tempdir().unwrap();
        let hub = ProviderHub::open(temp.path()).unwrap();
        let context = prototype_grants("revision-a");
        hub.write_note(
            &context,
            "idea",
            "# Interface thought\nA real file-backed note.",
        )
        .unwrap();
        hub.append_calendar_event(
            &context,
            "review",
            &CalendarEvent {
                time: "20260809T093000".into(),
                title: "Design review".into(),
                detail: "SOS".into(),
            },
        )
        .unwrap();
        fs::write(
            temp.path().join("music.json"),
            r#"{"title":"A Walk","artist":"Tycho","playing":true}"#,
        )
        .unwrap();
        let before = hub.generation().unwrap();
        let snapshot = hub.snapshot(&context).unwrap();
        assert_eq!(snapshot.notes[0].title, "Interface thought");
        assert_eq!(snapshot.calendar[0].title, "Design review");
        assert_eq!(snapshot.music.artist, "Tycho");
        hub.write_note(&context, "idea", "# Changed\nLive event.")
            .unwrap();
        assert_ne!(hub.generation().unwrap(), before);
    }

    #[test]
    fn empty_optional_domains_do_not_hide_live_system_providers() {
        let temp = tempfile::tempdir().unwrap();
        let snapshot = ProviderHub::open(temp.path())
            .unwrap()
            .snapshot(&prototype_grants("empty-provider-root"))
            .unwrap();
        assert_eq!(
            snapshot.providers.abi_version,
            experience_ir::SYSTEM_PROVIDER_ABI_VERSION
        );
        assert!(snapshot.providers.clock.unix_time_ms > 0);
        assert!(snapshot.music.title.is_empty());
        assert!(snapshot.music.artist.is_empty());
        assert!(!snapshot.music.playing);
    }

    #[test]
    fn capabilities_cancellation_and_unavailability_are_explicit() {
        let temp = tempfile::tempdir().unwrap();
        let hub = ProviderHub::open(temp.path()).unwrap();
        let context = ProviderContext {
            revision_id: "limited".into(),
            grants: BTreeSet::new(),
            cancellation: CancellationToken::default(),
        };
        assert!(matches!(
            hub.write_note(&context, "x", "x"),
            Err(ProviderError::Denied(Capability::NotesWrite))
        ));
        let context = prototype_grants("cancelled");
        context.cancellation.cancel();
        assert!(matches!(
            hub.snapshot(&context),
            Err(ProviderError::Cancelled)
        ));
    }

    #[test]
    fn provider_paths_cannot_escape_the_granted_root() {
        let temp = tempfile::tempdir().unwrap();
        let hub = ProviderHub::open(temp.path()).unwrap();
        assert!(matches!(
            hub.write_note(&prototype_grants("r"), "../escape", "bad"),
            Err(ProviderError::InvalidPath)
        ));
    }

    #[test]
    fn typed_generated_effects_execute_under_revision_grants() {
        let temp = tempfile::tempdir().unwrap();
        let hub = ProviderHub::open(temp.path()).unwrap();
        let context = prototype_grants("revision-effects");
        hub.execute_effect(
            &context,
            &ProviderEffect {
                provider: "notes".into(),
                action: "write".into(),
                payload: serde_json::json!({
                    "name": "generated",
                    "content": "# Generated note\nCommitted through Linux provider action."
                }),
            },
        )
        .unwrap();
        hub.execute_effect(
            &context,
            &ProviderEffect {
                provider: "calendar".into(),
                action: "append".into(),
                payload: serde_json::json!({
                    "name": "generated",
                    "time": "20260809T120000",
                    "title": "Generated event",
                    "detail": "Capability-scoped write"
                }),
            },
        )
        .unwrap();
        fs::write(
            temp.path().join("music.json"),
            r#"{"title":"Fixture","artist":"SOS","playing":false}"#,
        )
        .unwrap();
        let snapshot = hub.snapshot(&context).unwrap();
        assert_eq!(snapshot.notes[0].title, "Generated note");
        assert_eq!(snapshot.calendar[0].title, "Generated event");
    }

    #[test]
    fn grant_manifest_is_revision_scoped_and_wildcard_is_opt_in() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("grants.json");
        fs::write(
            &path,
            r#"{"revision-a":["notes_read"],"*":["music_control"]}"#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let exact = load_grants(&path, "revision-a", false).unwrap();
        assert_eq!(exact.grants, [Capability::NotesRead].into_iter().collect());
        assert!(load_grants(&path, "revision-b", false)
            .unwrap()
            .grants
            .is_empty());
        assert!(load_grants(&path, "revision-b", true)
            .unwrap()
            .grants
            .contains(&Capability::MusicControl));
    }

    #[test]
    fn provider_surfaces_are_capability_scoped_and_protected_frames_stay_unmapped() {
        let temp = tempfile::tempdir().unwrap();
        let hub = ProviderHub::open(temp.path()).unwrap();
        fs::write(
            temp.path().join("music.json"),
            r#"{"title":"Fixture","artist":"SOS","playing":false}"#,
        )
        .unwrap();
        let surfaces = temp.path().join("surfaces");
        fs::write(
            surfaces.join("preview.json"),
            r#"{"kind":"video","width":160,"height":90,"frame":"preview.png"}"#,
        )
        .unwrap();
        fs::write(
            surfaces.join("preview.png"),
            b"\x89PNG\r\n\x1a\nprototype-frame",
        )
        .unwrap();
        fs::write(
            surfaces.join("secure-camera.json"),
            r#"{"kind":"camera","width":320,"height":180,"frame":"secure.png","protected":true}"#,
        )
        .unwrap();

        let mut context = prototype_grants("surface-revision");
        context.grants.insert(Capability::ProtectedSurface);
        let snapshot = hub.snapshot_with_frames(&context).unwrap();
        assert_eq!(snapshot.frames.len(), 1);
        assert_eq!(snapshot.frames[0].surface_id, "preview");
        assert_eq!(snapshot.model.surfaces.len(), 2);
        assert_eq!(
            snapshot
                .model
                .surfaces
                .iter()
                .find(|surface| surface.id == "secure-camera")
                .unwrap()
                .status,
            ProviderSurfaceStatus::ProtectedUnavailable
        );

        context.grants.remove(&Capability::VideoRead);
        context.grants.remove(&Capability::ProtectedSurface);
        let limited = hub.snapshot_with_frames(&context).unwrap();
        assert!(limited.model.surfaces.is_empty());
        assert!(limited.frames.is_empty());
    }

    #[test]
    fn system_provider_reads_connectivity_power_audio_and_device_state() {
        let temp = tempfile::tempdir().unwrap();
        let net = temp.path().join("net");
        let power = temp.path().join("power");
        let drm = temp.path().join("drm");
        let input = temp.path().join("input");
        for path in [&net, &power, &drm, &input] {
            fs::create_dir(path).unwrap();
        }
        fs::create_dir(net.join("eth0")).unwrap();
        fs::write(net.join("eth0/operstate"), "up\n").unwrap();
        fs::create_dir(power.join("AC")).unwrap();
        fs::write(power.join("AC/type"), "Mains\n").unwrap();
        fs::write(power.join("AC/capacity"), "0\n").unwrap();
        fs::write(power.join("AC/online"), "1\n").unwrap();
        fs::create_dir(power.join("BAT0")).unwrap();
        fs::write(power.join("BAT0/type"), "Battery\n").unwrap();
        fs::write(power.join("BAT0/capacity"), "73\n").unwrap();
        fs::create_dir(power.join("hid-device-battery")).unwrap();
        fs::write(power.join("hid-device-battery/type"), "Battery\n").unwrap();
        fs::write(power.join("hid-device-battery/scope"), "Device\n").unwrap();
        fs::write(power.join("hid-device-battery/capacity"), "0\n").unwrap();
        fs::create_dir(power.join("USB-C")).unwrap();
        fs::write(power.join("USB-C/online"), "0\n").unwrap();
        fs::create_dir(drm.join("card0-HDMI-A-1")).unwrap();
        fs::write(drm.join("card0-HDMI-A-1/status"), "connected\n").unwrap();
        fs::create_dir(input.join("event4")).unwrap();

        let snapshot = system_snapshot(&net, &power, &drm, &input);
        assert_eq!(snapshot.online_interfaces, ["eth0"]);
        assert_eq!(snapshot.battery_percent, Some(73));
        assert_eq!(snapshot.on_ac_power, Some(true));
        assert_eq!(snapshot.connected_displays, ["card0-HDMI-A-1"]);
        assert_eq!(snapshot.input_devices, ["event4"]);
        assert!(snapshot.unix_time_ms > 0);

        assert_eq!(
            parse_wpctl_volume("Volume: 0.42 [MUTED]\n"),
            (Some(42), Some(true))
        );
        assert_eq!(parse_wpctl_volume("unavailable"), (None, None));
    }
}
