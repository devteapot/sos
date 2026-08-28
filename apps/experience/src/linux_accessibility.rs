use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::Duration,
};

use experience_ir::{Content, Scene, SceneNode, SemanticRole};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

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

pub fn start_from_environment_for_experience(
    experience_id: Option<&str>,
) -> Result<Option<Service>, String> {
    let Some(path) = std::env::var_os("SOS_ACCESSIBILITY_SOCKET").map(PathBuf::from) else {
        return Ok(None);
    };
    let path = experience_id.map_or(path.clone(), |experience_id| {
        namespaced_socket_path(&path, experience_id)
    });
    start(&path).map(Some)
}

fn namespaced_socket_path(base: &Path, experience_id: &str) -> PathBuf {
    let digest = format!("{:x}", Sha256::digest(experience_id.as_bytes()));
    base.with_file_name(format!("accessibility-{}.sock", &digest[..16]))
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
            }) => {
                let current = shared.0.lock().expect("accessibility snapshot lock");
                let current = if current.generation <= after_generation {
                    shared
                        .1
                        .wait_timeout(current, Duration::from_millis(timeout_ms.min(30_000)))
                        .expect("accessibility wait")
                        .0
                } else {
                    current
                };
                json!({"ok": true, "snapshot": current.clone()})
            }
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
    },
    Action {
        #[serde(flatten)]
        action: Action,
    },
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
    fn independently_presented_experiences_get_bounded_accessibility_sockets() {
        let base = Path::new("/run/user/1000/sos-session/accessibility.sock");
        let dashboard = namespaced_socket_path(base, "sos.example.dashboard");
        let media = namespaced_socket_path(base, "sos.example.media");
        assert_eq!(dashboard.parent(), base.parent());
        assert_ne!(dashboard, media);
        assert!(dashboard
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("accessibility-"));
        assert!(dashboard.as_os_str().len() < 108);
    }
}
