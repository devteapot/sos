use std::{
    env, fs,
    io::{self, BufRead, Write},
};

use revision_supervisor::{ExperienceLifecycleOperation, HostEvent, HostRequest};

fn main() {
    let mut arguments = env::args().skip(1);
    let mut lifecycle = None;
    while let Some(argument) = arguments.next() {
        if argument == "--emit-present-from" || argument == "--emit-dismiss-from" {
            let emitter = arguments.next().expect("missing lifecycle emitter");
            let target = arguments.next().expect("missing lifecycle target");
            let operation = if argument == "--emit-present-from" {
                ExperienceLifecycleOperation::Present
            } else {
                ExperienceLifecycleOperation::Dismiss
            };
            lifecycle = Some((emitter, target, operation));
        } else {
            panic!("unknown test-host argument: {argument}");
        }
    }
    let stdin = io::stdin();
    let mut prepared_graph: Option<String> = None;
    let mut quiesced_graph: Option<String> = None;
    for line in stdin.lock().lines() {
        let request: HostRequest = serde_json::from_str(&line.expect("read host request"))
            .expect("deserialize host request");
        match request {
            HostRequest::BootGraph {
                request_id,
                graph_id,
                graph_path,
                ..
            } => {
                if !graph_path.is_file() {
                    emit(HostEvent::GraphRejected {
                        request_id,
                        graph_id,
                        error: "graph file is missing".into(),
                    });
                } else {
                    emit(HostEvent::GraphPresented {
                        request_id,
                        graph_id,
                    });
                    if let Some((emitter, target, operation)) = &lifecycle {
                        let graph: experience_package::ResolvedGraph = serde_json::from_slice(
                            &fs::read(&graph_path).expect("read test graph"),
                        )
                        .expect("decode test graph");
                        if graph.nodes[&graph.root].experience_id.as_str() == emitter {
                            emit(HostEvent::ExperienceLifecycleRequested {
                                request_id: 1_u64 << 63,
                                experience_id: target.clone(),
                                operation: *operation,
                            });
                        }
                    }
                }
            }
            HostRequest::PrepareGraph {
                request_id,
                graph_id,
                graph_path,
                ..
            } => {
                if !graph_path.is_file() {
                    emit(HostEvent::GraphRejected {
                        request_id,
                        graph_id,
                        error: "graph file is missing".into(),
                    });
                } else {
                    prepared_graph = Some(graph_id.clone());
                    emit(HostEvent::GraphPrepared {
                        request_id,
                        graph_id,
                    });
                }
            }
            HostRequest::QuiesceGraphInput {
                request_id,
                graph_id,
            } => {
                if prepared_graph.as_ref() != Some(&graph_id) {
                    emit(HostEvent::GraphRejected {
                        request_id,
                        graph_id,
                        error: "cannot quiesce without the matching prepared graph".into(),
                    });
                } else {
                    quiesced_graph = Some(graph_id.clone());
                    emit(HostEvent::GraphInputQuiesced {
                        request_id,
                        graph_id,
                    });
                }
            }
            HostRequest::PresentGraph {
                request_id,
                graph_id,
            } => {
                if prepared_graph.as_ref() != Some(&graph_id)
                    || quiesced_graph.as_ref() != Some(&graph_id)
                {
                    emit(HostEvent::GraphRejected {
                        request_id,
                        graph_id,
                        error: "graph was not prepared and quiesced".into(),
                    });
                } else {
                    prepared_graph = None;
                    quiesced_graph = None;
                    emit(HostEvent::GraphPresented {
                        request_id,
                        graph_id,
                    });
                }
            }
            HostRequest::ConfirmGraph {
                request_id,
                graph_id,
            } => emit(HostEvent::GraphConfirmed {
                request_id,
                graph_id,
            }),
            HostRequest::FinalizeGraph {
                request_id,
                graph_id,
            } => emit(HostEvent::GraphFinalized {
                request_id,
                graph_id,
            }),
            HostRequest::DiscardGraph {
                request_id,
                graph_id,
            } => {
                prepared_graph = None;
                quiesced_graph = None;
                emit(HostEvent::GraphDiscarded {
                    request_id,
                    graph_id,
                });
            }
            HostRequest::Shutdown { request_id } => {
                emit(HostEvent::Shutdown { request_id });
                return;
            }
        }
    }
}

fn emit(event: HostEvent) {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &event).expect("serialize host event");
    stdout.write_all(b"\n").expect("write host event");
    stdout.flush().expect("flush host event");
}
