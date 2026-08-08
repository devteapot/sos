use std::{env, fs, process};

use experience_ir::{Content, PaintOp, Scene, SceneNode};
use runtime_luau::LuauRuntime;
use serde_json::json;

fn main() {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: cargo run -p runtime-luau --example validate -- <experience.luau>");
        process::exit(2);
    };
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("could not read {path}: {error}");
            process::exit(2);
        }
    };
    let result = LuauRuntime::compile(&source).and_then(|runtime| {
        runtime.render(
            &providers_fake::snapshot(),
            &json!({
                "draft": "Caffè ☕️ – 明日のデザイン",
            }),
        )
    });
    match result {
        Ok(scene) => {
            let (nodes, inputs, images, paint_nodes, animations, semantics) = statistics(&scene);
            println!(
                "valid source_bytes={} nodes={} inputs={} images={} paint_nodes={} animations={} semantics={}",
                source.len(),
                nodes,
                inputs,
                images,
                paint_nodes,
                animations,
                semantics
            );
        }
        Err(error) => {
            eprintln!("candidate rejected: {error}");
            process::exit(1);
        }
    }
}

fn statistics(scene: &Scene) -> (usize, usize, usize, usize, usize, usize) {
    fn visit(node: &SceneNode, totals: &mut (usize, usize, usize, usize, usize, usize)) {
        totals.0 += 1;
        totals.1 += usize::from(matches!(node.content, Some(Content::TextSession(_))));
        totals.2 += usize::from(matches!(node.content, Some(Content::Image(_))));
        totals.3 += usize::from(
            node.paint
                .iter()
                .any(|op| !matches!(op, PaintOp::FillBounds { .. })),
        );
        totals.4 += usize::from(node.animation.is_some());
        totals.5 += usize::from(node.semantics.is_some());
        for child in &node.children {
            visit(child, totals);
        }
    }
    let mut totals = (0, 0, 0, 0, 0, 0);
    visit(&scene.root, &mut totals);
    totals
}
