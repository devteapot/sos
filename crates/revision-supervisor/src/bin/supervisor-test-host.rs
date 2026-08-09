use std::{
    fs,
    io::{self, BufRead, Write},
    process, thread,
    time::Duration,
};

use revision_supervisor::{HostEvent, HostRequest};

fn main() {
    let stdin = io::stdin();
    let mut prepared: Option<(String, String)> = None;
    let mut quiesced_revision: Option<String> = None;
    for line in stdin.lock().lines() {
        let request: HostRequest = serde_json::from_str(&line.expect("read host request"))
            .expect("deserialize host request");
        match request {
            HostRequest::Boot {
                request_id,
                revision_id,
                revision_path,
                experience_api_version,
            } => {
                if !matches!(experience_api_version, 1 | 3) {
                    emit(HostEvent::Rejected {
                        request_id,
                        revision_id,
                        error: format!(
                            "unsupported experience API version {experience_api_version}"
                        ),
                    });
                    continue;
                }
                let mode = source_mode(&revision_path);
                emit(HostEvent::Presented {
                    request_id,
                    revision_id,
                });
                exit_later_if_requested(&mode);
            }
            HostRequest::Prepare {
                request_id,
                revision_id,
                revision_path,
                experience_api_version,
            } => {
                let mode = source_mode(&revision_path);
                if !matches!(experience_api_version, 1 | 3) {
                    emit(HostEvent::Rejected {
                        request_id,
                        revision_id,
                        error: format!(
                            "unsupported experience API version {experience_api_version}"
                        ),
                    });
                } else if mode == "reject" {
                    emit(HostEvent::Rejected {
                        request_id,
                        revision_id,
                        error: "synthetic Luau validation rejection".into(),
                    });
                } else if mode != "no-response" {
                    prepared = Some((revision_id.clone(), mode));
                    emit(HostEvent::Prepared {
                        request_id,
                        revision_id,
                    });
                }
            }
            HostRequest::QuiesceInput {
                request_id,
                revision_id,
            } => {
                if prepared.as_ref().map(|(revision, _)| revision) != Some(&revision_id) {
                    emit(HostEvent::Rejected {
                        request_id,
                        revision_id,
                        error: "cannot quiesce without the matching prepared revision".into(),
                    });
                } else {
                    quiesced_revision = Some(revision_id.clone());
                    emit(HostEvent::InputQuiesced {
                        request_id,
                        revision_id,
                    });
                }
            }
            HostRequest::Present {
                request_id,
                revision_id,
            } => {
                let Some((prepared_revision, mode)) = prepared.take() else {
                    emit(HostEvent::Rejected {
                        request_id,
                        revision_id,
                        error: "no prepared revision".into(),
                    });
                    continue;
                };
                if prepared_revision != revision_id {
                    emit(HostEvent::Rejected {
                        request_id,
                        revision_id,
                        error: "prepared revision mismatch".into(),
                    });
                } else if quiesced_revision.as_deref() != Some(&revision_id) {
                    emit(HostEvent::Rejected {
                        request_id,
                        revision_id,
                        error: "input was not quiesced for prepared revision".into(),
                    });
                } else if mode == "exit-before-present" {
                    process::exit(42);
                } else {
                    quiesced_revision = None;
                    emit(HostEvent::Presented {
                        request_id,
                        revision_id,
                    });
                    exit_later_if_requested(&mode);
                }
            }
            HostRequest::Confirm {
                request_id,
                revision_id,
            } => emit(HostEvent::Confirmed {
                request_id,
                revision_id,
            }),
            HostRequest::Discard {
                request_id,
                revision_id,
            } => {
                prepared = None;
                if quiesced_revision.as_deref() == Some(&revision_id) {
                    quiesced_revision = None;
                }
                emit(HostEvent::Discarded {
                    request_id,
                    revision_id,
                });
            }
            HostRequest::Shutdown { request_id } => {
                emit(HostEvent::Shutdown { request_id });
                return;
            }
        }
    }
}

fn source_mode(revision_path: &std::path::Path) -> String {
    fs::read_to_string(revision_path.join("source.luau"))
        .expect("read revision source")
        .strip_prefix("host:")
        .unwrap_or("stay")
        .trim()
        .to_owned()
}

fn emit(event: HostEvent) {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &event).expect("serialize host event");
    stdout.write_all(b"\n").expect("write host event");
    stdout.flush().expect("flush host event");
}

fn exit_later_if_requested(mode: &str) {
    if mode == "exit-immediately-after-present" {
        process::exit(44);
    }
    let delay = match mode {
        "exit-after-present" => Some(Duration::from_millis(40)),
        "crash-later" => Some(Duration::from_millis(250)),
        _ => None,
    };
    if let Some(delay) = delay {
        thread::spawn(move || {
            thread::sleep(delay);
            process::exit(43);
        });
    }
}
