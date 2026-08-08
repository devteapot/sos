use std::{env, fs, process};

use experience_ir::{Canvas, CanvasCommand, NodeKind, UiEvent, UiNode};
use runtime_luau::LuauRuntime;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Deserialize)]
struct Case {
    id: String,
    required_text: Vec<String>,
    conditional_music: bool,
    require_canvas: bool,
    min_paths: usize,
    min_quads: usize,
    require_drag_effect: bool,
}

#[derive(Debug, Serialize)]
struct Check {
    name: String,
    passed: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
struct Grade {
    case_id: String,
    source_bytes: usize,
    compiled: bool,
    score: usize,
    possible: usize,
    checks: Vec<Check>,
    error: Option<String>,
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        eprintln!("usage: eval_grade <cases.json> <case-id> <source.luau>");
        process::exit(2);
    }
    let manifest = fs::read_to_string(&args[1]).unwrap_or_else(|error| fail(&error.to_string()));
    let cases: Vec<Case> =
        serde_json::from_str(&manifest).unwrap_or_else(|error| fail(&error.to_string()));
    let case = cases
        .into_iter()
        .find(|case| case.id == args[2])
        .unwrap_or_else(|| fail("unknown case id"));
    let source = fs::read_to_string(&args[3]).unwrap_or_else(|error| fail(&error.to_string()));
    let grade = grade(&case, &source);
    println!("{}", serde_json::to_string(&grade).unwrap());
    if !grade.compiled {
        process::exit(1);
    }
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    process::exit(2)
}

fn grade(case: &Case, source: &str) -> Grade {
    let runtime = match LuauRuntime::compile(source) {
        Ok(runtime) => runtime,
        Err(error) => {
            return Grade {
                case_id: case.id.clone(),
                source_bytes: source.len(),
                compiled: false,
                score: 0,
                possible: expected_checks(case),
                checks: Vec::new(),
                error: Some(error.to_string()),
            };
        }
    };
    let model = providers_fake::snapshot();
    let initial_state = json!({});
    let tree = match runtime.render(&model, &initial_state) {
        Ok(tree) => tree,
        Err(error) => {
            return Grade {
                case_id: case.id.clone(),
                source_bytes: source.len(),
                compiled: false,
                score: 0,
                possible: expected_checks(case),
                checks: Vec::new(),
                error: Some(error.to_string()),
            };
        }
    };

    let mut checks = vec![Check {
        name: "compile_render_validate".into(),
        passed: true,
        detail: "source compiled and produced a bounded tree".into(),
    }];
    let all_text = collect_text(&tree);
    for required in &case.required_text {
        let passed = all_text.contains(&required.to_lowercase());
        checks.push(Check {
            name: format!("text:{required}"),
            passed,
            detail: if passed {
                "required content is visible".into()
            } else {
                "required content was not found in text nodes".into()
            },
        });
    }

    let canvases = collect_canvases(&tree);
    if case.require_canvas {
        checks.push(Check {
            name: "low_level_canvas".into(),
            passed: !canvases.is_empty(),
            detail: format!("decoded {} canvas node(s)", canvases.len()),
        });
        let path_count = canvases
            .iter()
            .flat_map(|(_, canvas)| &canvas.commands)
            .filter(|command| matches!(command, CanvasCommand::Path { .. }))
            .count();
        checks.push(Check {
            name: "generated_paths".into(),
            passed: path_count >= case.min_paths,
            detail: format!("{path_count} paths; minimum {}", case.min_paths),
        });
        let quad_count = canvases
            .iter()
            .flat_map(|(_, canvas)| &canvas.commands)
            .filter(|command| matches!(command, CanvasCommand::Quad { .. }))
            .count();
        checks.push(Check {
            name: "generated_quads".into(),
            passed: quad_count >= case.min_quads,
            detail: format!("{quad_count} quads; minimum {}", case.min_quads),
        });
        let (passed, detail) = interactive_bounds(&canvases);
        checks.push(Check {
            name: "phone_safe_hit_bounds".into(),
            passed,
            detail,
        });
    }

    if case.conditional_music {
        let paused = runtime.render(&model, &json!({ "playing": false }));
        let passed = paused
            .as_ref()
            .map(|tree| {
                let text = collect_text(tree);
                !text.contains(&model.music.title.to_lowercase())
                    && !text.contains(&model.music.artist.to_lowercase())
            })
            .unwrap_or(false);
        checks.push(Check {
            name: "music_hidden_when_paused".into(),
            passed,
            detail: if passed {
                "track title and artist are absent with playing=false".into()
            } else {
                "music remains visible or paused render failed".into()
            },
        });
    }

    if case.require_drag_effect {
        let passed = find_drag_effect(&runtime, &model, &tree);
        checks.push(Check {
            name: "reachable_drag_provider_effect".into(),
            passed,
            detail: if passed {
                "a generated drag/drop sequence emitted notes.attach_to_event".into()
            } else {
                "no tested source/target geometry reached the provider effect".into()
            },
        });
    }

    let score = checks.iter().filter(|check| check.passed).count();
    Grade {
        case_id: case.id.clone(),
        source_bytes: source.len(),
        compiled: true,
        score,
        possible: checks.len(),
        checks,
        error: None,
    }
}

fn expected_checks(case: &Case) -> usize {
    1 + case.required_text.len()
        + usize::from(case.require_canvas) * 4
        + usize::from(case.conditional_music)
        + usize::from(case.require_drag_effect)
}

fn collect_text(root: &UiNode) -> String {
    fn visit(node: &UiNode, output: &mut String) {
        if let NodeKind::Text(text) = &node.kind {
            output.push_str(text);
            output.push('\n');
        }
        for child in &node.children {
            visit(child, output);
        }
    }
    let mut output = String::new();
    visit(root, &mut output);
    output.to_lowercase()
}

fn collect_canvases(root: &UiNode) -> Vec<(&UiNode, &Canvas)> {
    fn visit<'a>(node: &'a UiNode, output: &mut Vec<(&'a UiNode, &'a Canvas)>) {
        if let NodeKind::Canvas(canvas) = &node.kind {
            output.push((node, canvas));
        }
        for child in &node.children {
            visit(child, output);
        }
    }
    let mut output = Vec::new();
    visit(root, &mut output);
    output
}

fn interactive_bounds(canvases: &[(&UiNode, &Canvas)]) -> (bool, String) {
    let mut regions = 0usize;
    for (node, canvas) in canvases {
        let Some(width) = node.style.width else {
            return (false, "canvas width is not explicit".into());
        };
        let Some(height) = node.style.height else {
            return (false, "canvas height is not explicit".into());
        };
        for region in &canvas.hit_regions {
            regions += 1;
            if region.x < 0.0
                || region.y < 0.0
                || region.x + region.width > width
                || region.y + region.height > height
                || (region.drag_action.is_some() && region.y + region.height > 400.0)
            {
                return (
                    false,
                    format!(
                        "region {} is outside canvas or the y<=400 interactive safe area",
                        region.id
                    ),
                );
            }
        }
    }
    (
        true,
        format!("{regions} hit regions remain within declared/safe bounds"),
    )
}

fn find_drag_effect(
    runtime: &LuauRuntime,
    model: &experience_ir::ExperienceModel,
    tree: &UiNode,
) -> bool {
    for (node, canvas) in collect_canvases(tree) {
        let Some(width) = node.style.width else {
            continue;
        };
        let Some(height) = node.style.height else {
            continue;
        };
        for source in canvas
            .hit_regions
            .iter()
            .filter(|region| region.drop_action.is_some())
        {
            let mut targets = canvas
                .hit_regions
                .iter()
                .map(|region| {
                    (
                        region.x + region.width / 2.0,
                        region.y + region.height / 2.0,
                    )
                })
                .collect::<Vec<_>>();
            for row in 0..=8 {
                for column in 0..=8 {
                    targets.push((
                        width * (column as f32 + 0.5) / 9.0,
                        height.min(400.0) * (row as f32 + 0.5) / 9.0,
                    ));
                }
            }
            for (target_x, target_y) in targets {
                let mut state = json!({});
                if let Some(action) = &source.press_action {
                    let Ok(outcome) = runtime.update_with_effects(
                        model,
                        &state,
                        &event(action, source, source.x, source.y),
                    ) else {
                        continue;
                    };
                    state = outcome.state;
                }
                if let Some(action) = &source.drag_action {
                    let Ok(outcome) = runtime.update_with_effects(
                        model,
                        &state,
                        &event(action, source, target_x, target_y),
                    ) else {
                        continue;
                    };
                    state = outcome.state;
                }
                let action = source.drop_action.as_ref().unwrap();
                let Ok(outcome) = runtime.update_with_effects(
                    model,
                    &state,
                    &event(action, source, target_x, target_y),
                ) else {
                    continue;
                };
                if outcome
                    .effects
                    .iter()
                    .any(|effect| effect.provider == "notes" && effect.action == "attach_to_event")
                {
                    return true;
                }
            }
        }
    }
    false
}

fn event(action: &str, source: &experience_ir::HitRegion, x: f32, y: f32) -> UiEvent {
    UiEvent {
        action: action.into(),
        target: Some(source.id.clone()),
        x: Some(x),
        y: Some(y),
        ..Default::default()
    }
}
