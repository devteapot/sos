use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use android_authority_protocol::{RevisionAssetWire, RevisionRequest, RevisionResponse};
use experience_ir::{
    ProviderEffect, ProviderRequest, ProviderResponse, StateEnvelope, MAX_STATE_BYTES,
};
use providers_fake::state_service::StateService;
use revision_supervisor::{RevisionAssetInput, RevisionInput, RevisionStore, VerifiedRevision};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ActivationJournal {
    revision_id: String,
    state_stage_id: u64,
}

pub struct AndroidSystemAuthority {
    revisions: RevisionStore,
    state: StateService,
    staged_effects: HashMap<u64, Vec<ProviderEffect>>,
    state_file: PathBuf,
    journal_file: PathBuf,
}

impl AndroidSystemAuthority {
    pub fn open(
        revision_root: impl Into<PathBuf>,
        state_file: impl Into<PathBuf>,
        bootstrap_source: &[u8],
    ) -> Result<Self, String> {
        let revision_root = revision_root.into();
        let state_file = state_file.into();
        let revisions = RevisionStore::open(&revision_root).map_err(|error| error.to_string())?;
        let current = match revisions.current().map_err(|error| error.to_string())? {
            Some(current) => current,
            None => {
                let revision = revisions
                    .install(RevisionInput {
                        source: bootstrap_source.to_vec(),
                        state: json!({}),
                        schema_version: 1,
                        experience_api_version: 3,
                        assets: Vec::new(),
                    })
                    .map_err(|error| error.to_string())?;
                revisions
                    .set_current(&revision.manifest.revision_id)
                    .map_err(|error| error.to_string())?;
                revision
            }
        };
        let initial = if state_file.exists() {
            serde_json::from_slice::<StateEnvelope>(
                &fs::read(&state_file).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?
        } else {
            StateEnvelope {
                revision: 0,
                schema_version: current.manifest.schema_version,
                source_sha256: current.manifest.source.sha256.clone(),
                state: json!({}),
            }
        };
        let journal_file = revision_root.join("activation-journal.json");
        let mut authority = Self {
            revisions,
            state: StateService::new(initial),
            staged_effects: HashMap::new(),
            state_file,
            journal_file,
        };
        authority.persist_state()?;
        authority.recover_activation()?;
        authority.ensure_consistent()?;
        Ok(authority)
    }

    pub fn dispatch_provider(&mut self, request: ProviderRequest) -> ProviderResponse {
        let request_id = request.request_id();
        match request {
            ProviderRequest::Snapshot { .. } => ProviderResponse {
                model: Some(providers_fake::snapshot()),
                ..provider_response(request_id, true)
            },
            ProviderRequest::Action {
                provider,
                action,
                payload,
                ..
            } if provider == "notes" && action == "attach_to_event" => {
                let note_id = payload.get("note_id").and_then(|value| value.as_str());
                let event_title = payload.get("event_title").and_then(|value| value.as_str());
                match (note_id, event_title) {
                    (Some(note_id), Some(event_title)) => ProviderResponse {
                        result: Some(json!({
                            "receipt": format!("notes:{note_id}->{event_title}"),
                        })),
                        ..provider_response(request_id, true)
                    },
                    _ => provider_failure(request_id, "note_id and event_title are required"),
                }
            }
            ProviderRequest::Action {
                provider, action, ..
            } => provider_failure(
                request_id,
                &format!("unsupported provider action: {provider}.{action}"),
            ),
            ProviderRequest::LoadState { .. } => ProviderResponse {
                state: Some(self.state.load()),
                ..provider_response(request_id, true)
            },
            ProviderRequest::StageState {
                expected_revision,
                schema_version,
                mut state,
                source_sha256,
                effects,
                ..
            } => {
                if let Err(error) = validate_effects(&effects, &mut state) {
                    return provider_failure(request_id, &error);
                }
                match self
                    .state
                    .stage(expected_revision, schema_version, state, source_sha256)
                {
                    Ok(stage_id) => {
                        self.staged_effects.insert(stage_id, effects);
                        ProviderResponse {
                            stage_id: Some(stage_id),
                            ..provider_response(request_id, true)
                        }
                    }
                    Err(error) => provider_failure(request_id, &error),
                }
            }
            ProviderRequest::PromoteState { stage_id, .. } => match self.promote_state(stage_id) {
                Ok(state) => ProviderResponse {
                    state: Some(state),
                    ..provider_response(request_id, true)
                },
                Err(error) => provider_failure(request_id, &error),
            },
            ProviderRequest::AbortState { stage_id, .. } => {
                let removed = self.state.abort(stage_id);
                self.staged_effects.remove(&stage_id);
                ProviderResponse {
                    result: Some(json!({ "removed": removed })),
                    ..provider_response(request_id, true)
                }
            }
            ProviderRequest::ConfigureStateFault { point, .. } => {
                self.state.configure_fault(point);
                provider_response(request_id, true)
            }
        }
    }

    pub fn dispatch_revision(&mut self, request: RevisionRequest) -> RevisionResponse {
        let request_id = request.request_id();
        let result = match request {
            RevisionRequest::Current { .. } => self.current_response(request_id),
            RevisionRequest::Install {
                source,
                state,
                schema_version,
                experience_api_version,
                assets,
                ..
            } => self.install_response(
                request_id,
                source,
                state,
                schema_version,
                experience_api_version,
                assets,
            ),
            RevisionRequest::Activate {
                revision_id,
                state_stage_id,
                ..
            } => self.activate_response(request_id, &revision_id, state_stage_id),
        };
        result.unwrap_or_else(|error| RevisionResponse {
            request_id,
            ok: false,
            error: Some(error),
            ..RevisionResponse::default()
        })
    }

    fn current_response(&self, request_id: u64) -> Result<RevisionResponse, String> {
        let current = self
            .revisions
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "revision authority has no current revision".to_owned())?;
        revision_response(request_id, &current, Some(self.state.load()))
    }

    fn install_response(
        &self,
        request_id: u64,
        source: String,
        state: serde_json::Value,
        schema_version: u64,
        experience_api_version: u32,
        assets: Vec<RevisionAssetWire>,
    ) -> Result<RevisionResponse, String> {
        let revision = self
            .revisions
            .install(RevisionInput {
                source: source.into_bytes(),
                state,
                schema_version,
                experience_api_version,
                assets: assets
                    .into_iter()
                    .map(|asset| RevisionAssetInput {
                        id: asset.id,
                        kind: asset.kind,
                        bytes: asset.bytes,
                    })
                    .collect(),
            })
            .map_err(|error| error.to_string())?;
        revision_response(request_id, &revision, None)
    }

    fn activate_response(
        &mut self,
        request_id: u64,
        revision_id: &str,
        state_stage_id: u64,
    ) -> Result<RevisionResponse, String> {
        let revision = self
            .revisions
            .verify(revision_id)
            .map_err(|error| error.to_string())?;
        let staged = self
            .state
            .staged(state_stage_id)
            .ok_or_else(|| format!("unknown state stage: {state_stage_id}"))?;
        if staged.source_sha256 != revision.manifest.source.sha256
            || staged.schema_version != revision.manifest.schema_version
        {
            return Err("staged state does not match the immutable revision".into());
        }
        self.write_journal(&ActivationJournal {
            revision_id: revision_id.into(),
            state_stage_id,
        })?;
        let state = self
            .promote_state(state_stage_id)
            .unwrap_or_else(|error| fatal_activation(error));
        self.revisions
            .set_current(revision_id)
            .map_err(|error| error.to_string())
            .unwrap_or_else(|error| fatal_activation(error));
        self.remove_journal()
            .unwrap_or_else(|error| fatal_activation(error));
        revision_response(request_id, &revision, Some(state))
    }

    fn promote_state(&mut self, stage_id: u64) -> Result<StateEnvelope, String> {
        let before_revision = self.state.load().revision;
        let promoted = self.state.promote(stage_id);
        let current = self.state.load();
        if current.revision > before_revision {
            if let Some(effects) = self.staged_effects.remove(&stage_id) {
                execute_effects(current.revision, &effects);
            }
            self.persist_state()?;
        }
        promoted
    }

    fn recover_activation(&mut self) -> Result<(), String> {
        let Ok(bytes) = fs::read(&self.journal_file) else {
            return Ok(());
        };
        let journal: ActivationJournal =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let revision = self
            .revisions
            .verify(&journal.revision_id)
            .map_err(|error| error.to_string())?;
        if self.state.load().source_sha256 == revision.manifest.source.sha256 {
            self.revisions
                .set_current(&journal.revision_id)
                .map_err(|error| error.to_string())?;
            println!(
                "android_authority_recovered revision_id={} stage_id={}",
                journal.revision_id, journal.state_stage_id
            );
        }
        self.remove_journal()
    }

    fn ensure_consistent(&self) -> Result<(), String> {
        let current = self
            .revisions
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "revision authority has no current revision".to_owned())?;
        if current.manifest.source.sha256 != self.state.load().source_sha256 {
            return Err("revision pointer and provider/state authority disagree".into());
        }
        Ok(())
    }

    fn persist_state(&self) -> Result<(), String> {
        let parent = self
            .state_file
            .parent()
            .ok_or_else(|| "state file must have a parent".to_owned())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let temporary = self.state_file.with_extension("tmp");
        write_synced_atomic(
            &temporary,
            &self.state_file,
            &serde_json::to_vec_pretty(&self.state.load()).map_err(|error| error.to_string())?,
        )
    }

    fn write_journal(&self, journal: &ActivationJournal) -> Result<(), String> {
        let temporary = self.journal_file.with_extension("tmp");
        write_synced_atomic(
            &temporary,
            &self.journal_file,
            &serde_json::to_vec_pretty(journal).map_err(|error| error.to_string())?,
        )
    }

    fn remove_journal(&self) -> Result<(), String> {
        match fs::remove_file(&self.journal_file) {
            Ok(()) => sync_parent(&self.journal_file),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

fn fatal_activation(error: String) -> ! {
    eprintln!("android_authority_fatal_activation error={error}");
    std::process::abort()
}

fn write_synced_atomic(temporary: &Path, destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(temporary)
        .map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    fs::rename(temporary, destination).map_err(|error| error.to_string())?;
    sync_parent(destination)
}

fn sync_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "durable file must have a parent".to_owned())?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())
}

fn revision_response(
    request_id: u64,
    revision: &VerifiedRevision,
    state: Option<StateEnvelope>,
) -> Result<RevisionResponse, String> {
    let source = fs::read_to_string(revision.directory.join(&revision.manifest.source.path))
        .map_err(|error| error.to_string())?;
    let assets = revision
        .manifest
        .assets
        .iter()
        .map(|asset| {
            Ok(RevisionAssetWire {
                id: asset.id.clone(),
                kind: asset.kind.clone(),
                bytes: fs::read(revision.directory.join(&asset.file.path))
                    .map_err(|error| error.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(RevisionResponse {
        request_id,
        ok: true,
        revision_id: Some(revision.manifest.revision_id.clone()),
        source: Some(source),
        state,
        assets,
        error: None,
    })
}

fn provider_response(request_id: u64, ok: bool) -> ProviderResponse {
    ProviderResponse {
        request_id,
        ok,
        model: None,
        result: None,
        state: None,
        stage_id: None,
        error: None,
    }
}

fn provider_failure(request_id: u64, error: &str) -> ProviderResponse {
    ProviderResponse {
        error: Some(error.into()),
        ..provider_response(request_id, false)
    }
}

fn validate_effects(
    effects: &[ProviderEffect],
    state: &mut serde_json::Value,
) -> Result<(), String> {
    if effects.len() > experience_ir::MAX_EFFECTS {
        return Err("too many staged provider effects".into());
    }
    for effect in effects {
        if (effect.provider.as_str(), effect.action.as_str()) != ("notes", "attach_to_event") {
            return Err(format!(
                "unsupported staged provider action: {}.{}",
                effect.provider, effect.action
            ));
        }
        let note_id = effect
            .payload
            .get("note_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "note_id is required".to_owned())?;
        let event_title = effect
            .payload
            .get("event_title")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "event_title is required".to_owned())?;
        if !state.is_object() {
            *state = json!({});
        }
        state.as_object_mut().expect("state object").insert(
            "provider_receipt".into(),
            json!({ "receipt": format!("notes:{note_id}->{event_title}") }),
        );
    }
    Ok(())
}

fn execute_effects(revision: u64, effects: &[ProviderEffect]) {
    for effect in effects {
        println!(
            "provider_effect_promoted revision={revision} provider={} action={} payload={}",
            effect.provider, effect.action, effect.payload
        );
    }
}

pub const MAX_PROVIDER_REQUEST_BYTES: u64 = (MAX_STATE_BYTES + 64 * 1024) as u64;

#[cfg(test)]
mod tests {
    use super::*;
    use android_authority_protocol::RevisionRequest;

    fn install_and_stage(authority: &mut AndroidSystemAuthority, source: &str) -> (String, u64) {
        let installed = authority.dispatch_revision(RevisionRequest::Install {
            request_id: 1,
            source: source.to_owned(),
            state: json!({ "candidate": true }),
            schema_version: 1,
            experience_api_version: 3,
            assets: Vec::new(),
        });
        assert!(installed.ok);
        let revision_id = installed.revision_id.unwrap();
        let source_sha256 = authority
            .revisions
            .verify(&revision_id)
            .unwrap()
            .manifest
            .source
            .sha256;
        let staged = authority.dispatch_provider(ProviderRequest::StageState {
            request_id: 2,
            expected_revision: 0,
            schema_version: 1,
            state: json!({ "candidate": true }),
            source_sha256,
            effects: Vec::new(),
        });
        assert!(staged.ok);
        (revision_id, staged.stage_id.unwrap())
    }

    #[test]
    fn presentation_activation_commits_state_and_revision_together() {
        let temporary = tempfile::tempdir().unwrap();
        let mut authority = AndroidSystemAuthority::open(
            temporary.path().join("revisions"),
            temporary.path().join("provider.json"),
            b"return { api_version = 3, revision = 1 }",
        )
        .unwrap();
        let source = "return { api_version = 3, revision = 2 }".to_owned();
        let (revision_id, stage_id) = install_and_stage(&mut authority, &source);
        let activated = authority.dispatch_revision(RevisionRequest::Activate {
            request_id: 3,
            revision_id,
            state_stage_id: stage_id,
        });
        assert!(activated.ok, "{:?}", activated.error);
        assert_eq!(activated.state.unwrap().revision, 1);
        assert_eq!(authority.state.load().state, json!({ "candidate": true }));
    }

    #[test]
    fn restart_recovers_state_first_activation_gap() {
        let temporary = tempfile::tempdir().unwrap();
        let revision_root = temporary.path().join("revisions");
        let state_file = temporary.path().join("provider.json");
        let bootstrap = b"return { api_version = 3, revision = 1 }";
        let revision_id = {
            let mut authority =
                AndroidSystemAuthority::open(&revision_root, &state_file, bootstrap).unwrap();
            let (revision_id, stage_id) =
                install_and_stage(&mut authority, "return { api_version = 3, revision = 2 }");
            authority
                .write_journal(&ActivationJournal {
                    revision_id: revision_id.clone(),
                    state_stage_id: stage_id,
                })
                .unwrap();
            authority.promote_state(stage_id).unwrap();
            revision_id
        };

        let recovered =
            AndroidSystemAuthority::open(&revision_root, &state_file, bootstrap).unwrap();
        assert_eq!(
            recovered
                .revisions
                .current()
                .unwrap()
                .unwrap()
                .manifest
                .revision_id,
            revision_id
        );
        assert_eq!(recovered.state.load().state, json!({ "candidate": true }));
        assert!(!revision_root.join("activation-journal.json").exists());
    }

    #[test]
    fn activation_rejects_state_for_another_source() {
        let temporary = tempfile::tempdir().unwrap();
        let mut authority = AndroidSystemAuthority::open(
            temporary.path().join("revisions"),
            temporary.path().join("provider.json"),
            b"return { api_version = 3, revision = 1 }",
        )
        .unwrap();
        let installed = authority.dispatch_revision(RevisionRequest::Install {
            request_id: 1,
            source: "return { api_version = 3, revision = 2 }".into(),
            state: json!({}),
            schema_version: 1,
            experience_api_version: 3,
            assets: Vec::new(),
        });
        let staged = authority.dispatch_provider(ProviderRequest::StageState {
            request_id: 2,
            expected_revision: 0,
            schema_version: 1,
            state: json!({}),
            source_sha256: "0".repeat(64),
            effects: Vec::new(),
        });
        let activated = authority.dispatch_revision(RevisionRequest::Activate {
            request_id: 3,
            revision_id: installed.revision_id.unwrap(),
            state_stage_id: staged.stage_id.unwrap(),
        });
        assert!(!activated.ok);
        assert_eq!(authority.state.load().revision, 0);
    }
}
