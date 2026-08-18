use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

use experience_ir::{Content, Scene, SceneNode, SemanticRole};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Action {
    pub kind: String,
    pub target: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Snapshot {
    pub generation: u64,
    pub focused: Option<String>,
    pub nodes: Vec<Node>,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Node {
    pub id: String,
    pub parent: Option<String>,
    pub role: String,
    pub label: String,
    pub value: Option<String>,
    pub hint: Option<String>,
    pub activate: bool,
    pub editable: bool,
    pub scrollable: bool,
}

#[derive(Clone)]
pub struct Service {
    shared: Arc<(Mutex<Snapshot>, Condvar)>,
    actions: async_channel::Receiver<Action>,
}

pub fn start_from_environment() -> Result<Option<Service>, String> {
    let Some(path) = std::env::var_os("SOS_ACCESSIBILITY_SOCKET").map(PathBuf::from) else {
        return Ok(None);
    };
    start(&path).map(Some)
}

pub fn start(path: &Path) -> Result<Service, String> {
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            return Err(format!(
                "accessibility service is already listening: {}",
                path.display()
            ));
        }
        fs::remove_file(path).map_err(|error| {
            format!(
                "remove refused accessibility socket {}: {error}",
                path.display()
            )
        })?;
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let listener = UnixListener::bind(path).map_err(|error| error.to_string())?;
    let shared = Arc::new((Mutex::new(Snapshot::default()), Condvar::new()));
    let (actions_tx, actions) = async_channel::bounded(64);
    let service = Service {
        shared: shared.clone(),
        actions,
    };
    let socket = path.to_owned();
    thread::Builder::new()
        .name("sos-accessibility".into())
        .spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => handle(stream, &shared, &actions_tx),
                    Err(error) => {
                        eprintln!("sos_accessibility_accept_failed error={error}");
                        break;
                    }
                }
            }
            fs::remove_file(socket).ok();
        })
        .map_err(|error| error.to_string())?;
    eprintln!("sos_accessibility_listening socket={}", path.display());
    Ok(service)
}

impl Service {
    pub fn actions(&self) -> async_channel::Receiver<Action> {
        self.actions.clone()
    }

    pub fn publish(&self, scene: &Scene, focused: Option<String>, status: Option<String>) {
        let (lock, changed) = &*self.shared;
        let mut current = lock.lock().expect("accessibility snapshot lock");
        let next_nodes = semantic_nodes(scene);
        if current.nodes != next_nodes || current.focused != focused || current.status != status {
            current.generation = current.generation.wrapping_add(1).max(1);
            current.nodes = next_nodes;
            current.focused = focused;
            current.status = status;
            changed.notify_all();
        }
    }
}

fn handle(
    mut stream: UnixStream,
    shared: &Arc<(Mutex<Snapshot>, Condvar)>,
    actions: &async_channel::Sender<Action>,
) {
    let mut line = String::new();
    let response = match BufReader::new(&stream).read_line(&mut line) {
        Ok(0) => json!({"ok": false, "error": "empty request"}),
        Ok(_) => match serde_json::from_str::<Request>(&line) {
            Ok(Request::Snapshot) => {
                let snapshot = shared
                    .0
                    .lock()
                    .expect("accessibility snapshot lock")
                    .clone();
                json!({"ok": true, "snapshot": snapshot})
            }
            Ok(Request::Wait {
                after_generation,
                timeout_ms,
                focused,
            }) => wait_response(shared, after_generation, timeout_ms, focused.as_deref()),
            Ok(Request::Action { action }) if valid_action(&action) => {
                match actions.try_send(action) {
                    Ok(()) => json!({"ok": true}),
                    Err(_) => json!({"ok": false, "error": "action queue is full"}),
                }
            }
            Ok(Request::Action { .. }) => {
                json!({"ok": false, "error": "unsupported semantic action"})
            }
            Err(error) => json!({"ok": false, "error": error.to_string()}),
        },
        Err(error) => json!({"ok": false, "error": error.to_string()}),
    };
    let _ = serde_json::to_writer(&mut stream, &response);
    let _ = stream.write_all(b"\n");
}

#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
enum Request {
    Snapshot,
    Wait {
        after_generation: u64,
        timeout_ms: u64,
        #[serde(default)]
        focused: Option<String>,
    },
    Action {
        #[serde(flatten)]
        action: Action,
    },
}

#[derive(Debug)]
struct WaitOutcome {
    snapshot: Snapshot,
    elapsed_ms: u64,
    timed_out: bool,
}

fn snapshot_satisfies_wait(
    snapshot: &Snapshot,
    after_generation: u64,
    focused: Option<&str>,
) -> bool {
    snapshot.generation > after_generation
        && focused.is_none_or(|expected| snapshot.focused.as_deref() == Some(expected))
}

fn wait_for_snapshot(
    shared: &Arc<(Mutex<Snapshot>, Condvar)>,
    after_generation: u64,
    timeout_ms: u64,
    focused: Option<&str>,
) -> WaitOutcome {
    let started = Instant::now();
    let timeout = Duration::from_millis(timeout_ms.min(30_000));
    let current = shared.0.lock().expect("accessibility snapshot lock");
    let current = shared
        .1
        .wait_timeout_while(current, timeout, |snapshot| {
            !snapshot_satisfies_wait(snapshot, after_generation, focused)
        })
        .expect("accessibility wait")
        .0;
    let timed_out = !snapshot_satisfies_wait(&current, after_generation, focused);
    WaitOutcome {
        snapshot: current.clone(),
        elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        timed_out,
    }
}

fn wait_response(
    shared: &Arc<(Mutex<Snapshot>, Condvar)>,
    after_generation: u64,
    timeout_ms: u64,
    focused: Option<&str>,
) -> serde_json::Value {
    let outcome = wait_for_snapshot(shared, after_generation, timeout_ms, focused);
    let wait = json!({
        "after_generation": after_generation,
        "expected_focus": focused,
        "elapsed_ms": outcome.elapsed_ms,
        "timed_out": outcome.timed_out,
    });
    if outcome.timed_out {
        json!({
            "ok": false,
            "error": "accessibility wait timed out",
            "snapshot": outcome.snapshot,
            "wait": wait,
        })
    } else {
        json!({"ok": true, "snapshot": outcome.snapshot, "wait": wait})
    }
}

fn valid_action(action: &Action) -> bool {
    !action.target.is_empty()
        && matches!(
            action.kind.as_str(),
            "focus"
                | "next"
                | "previous"
                | "activate"
                | "scroll_forward"
                | "scroll_backward"
                | "set_value"
                | "submit"
                | "set_selection"
                | "copy"
                | "cut"
                | "paste"
        )
}

fn semantic_nodes(scene: &Scene) -> Vec<Node> {
    fn visit(node: &SceneNode, parent: Option<&str>, output: &mut Vec<Node>) {
        let mut next_parent = parent;
        if let Some(id) = node
            .id
            .as_deref()
            .filter(|_| node.semantics.is_some() || node.layout.scroll_y)
        {
            let semantics = node.semantics.as_ref();
            output.push(Node {
                id: id.into(),
                parent: parent.map(str::to_owned),
                role: semantics
                    .map(|value| role_name(value.role))
                    .unwrap_or("scroll_area")
                    .into(),
                label: semantics
                    .map(|value| value.label.clone())
                    .unwrap_or_else(|| "Scrollable content".into()),
                value: match &node.content {
                    Some(Content::TextSession(input)) => Some(input.value.clone()),
                    _ => semantics.and_then(|value| value.value.clone()),
                },
                hint: semantics.and_then(|value| value.hint.clone()),
                activate: node.interaction.tap_action.is_some(),
                editable: matches!(node.content, Some(Content::TextSession(_))),
                scrollable: node.layout.scroll_y,
            });
            next_parent = Some(id);
        }
        for child in &node.children {
            visit(child, next_parent, output);
        }
    }
    let mut nodes = Vec::new();
    visit(&scene.root, None, &mut nodes);
    nodes
}

fn role_name(role: SemanticRole) -> &'static str {
    match role {
        SemanticRole::Button => "button",
        SemanticRole::Image => "image",
        SemanticRole::TextField => "text_field",
        SemanticRole::Header => "header",
        SemanticRole::Status => "status",
        SemanticRole::ScrollArea => "scroll_area",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use experience_ir::{Interaction, Semantics};

    fn publish_test_snapshot(
        shared: &Arc<(Mutex<Snapshot>, Condvar)>,
        generation: u64,
        focused: Option<&str>,
    ) {
        let mut snapshot = shared.0.lock().unwrap();
        snapshot.generation = generation;
        snapshot.focused = focused.map(str::to_owned);
        shared.1.notify_all();
    }

    #[test]
    fn semantic_tree_preserves_hierarchy_and_actions() {
        let scene = Scene {
            root: SceneNode {
                id: Some("root".into()),
                layout: experience_ir::Layout {
                    scroll_y: true,
                    ..Default::default()
                },
                children: vec![SceneNode {
                    id: Some("save".into()),
                    semantics: Some(Semantics {
                        role: SemanticRole::Button,
                        label: "Save".into(),
                        value: None,
                        hint: None,
                    }),
                    interaction: Interaction {
                        tap_action: Some("save".into()),
                        ..Default::default()
                    },
                    ..Default::default()
                }],
                ..Default::default()
            },
        };
        let nodes = semantic_nodes(&scene);
        assert_eq!(nodes[1].parent.as_deref(), Some("root"));
        assert!(nodes[0].scrollable);
        assert!(nodes[1].activate);
    }

    #[test]
    fn focus_wait_skips_a_stale_generation_before_the_focused_generation() {
        let shared = Arc::new((
            Mutex::new(Snapshot {
                generation: 41,
                focused: Some("daily-flow-root".into()),
                ..Default::default()
            }),
            Condvar::new(),
        ));
        let waiting = shared.clone();
        let waiter =
            thread::spawn(move || wait_for_snapshot(&waiting, 41, 1_000, Some("note-draft")));

        publish_test_snapshot(&shared, 42, Some("music-toggle"));
        publish_test_snapshot(&shared, 43, Some("note-draft"));

        let outcome = waiter.join().unwrap();
        assert!(!outcome.timed_out);
        assert_eq!(outcome.snapshot.generation, 43);
        assert_eq!(outcome.snapshot.focused.as_deref(), Some("note-draft"));
    }

    #[test]
    fn focus_wait_times_out_on_a_new_generation_with_the_wrong_focus() {
        let shared = Arc::new((
            Mutex::new(Snapshot {
                generation: 42,
                focused: Some("music-toggle".into()),
                ..Default::default()
            }),
            Condvar::new(),
        ));

        let response = wait_response(&shared, 41, 0, Some("note-draft"));
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"], "accessibility wait timed out");
        assert_eq!(response["snapshot"]["generation"], 42);
        assert_eq!(response["snapshot"]["focused"], "music-toggle");
        assert_eq!(response["wait"]["after_generation"], 41);
        assert_eq!(response["wait"]["expected_focus"], "note-draft");
        assert_eq!(response["wait"]["timed_out"], true);
        assert!(response["wait"]["elapsed_ms"].is_u64());
    }

    #[test]
    fn focus_wait_requires_a_new_generation_even_when_focus_already_matches() {
        let shared = Arc::new((
            Mutex::new(Snapshot {
                generation: 41,
                focused: Some("note-draft".into()),
                ..Default::default()
            }),
            Condvar::new(),
        ));

        let response = wait_response(&shared, 41, 0, Some("note-draft"));
        assert_eq!(response["ok"], false);
        assert_eq!(response["snapshot"]["generation"], 41);
        assert_eq!(response["wait"]["timed_out"], true);
    }
}
