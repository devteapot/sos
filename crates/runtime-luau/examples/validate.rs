use std::{env, fs, process};

use experience_ir::{NodeKind, UiNode};
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
        Ok(tree) => {
            let (nodes, inputs, images, canvases, animations, semantics) = statistics(&tree);
            println!(
                "valid source_bytes={} nodes={} inputs={} images={} canvases={} animations={} semantics={}",
                source.len(),
                nodes,
                inputs,
                images,
                canvases,
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

fn statistics(root: &UiNode) -> (usize, usize, usize, usize, usize, usize) {
    fn visit(node: &UiNode, totals: &mut (usize, usize, usize, usize, usize, usize)) {
        totals.0 += 1;
        totals.1 += usize::from(matches!(node.kind, NodeKind::TextInput(_)));
        totals.2 += usize::from(matches!(node.kind, NodeKind::Image(_)));
        totals.3 += usize::from(matches!(node.kind, NodeKind::Canvas(_)));
        totals.4 += usize::from(node.animation.is_some());
        totals.5 += usize::from(node.accessibility.is_some());
        for child in &node.children {
            visit(child, totals);
        }
    }
    let mut totals = (0, 0, 0, 0, 0, 0);
    visit(root, &mut totals);
    totals
}
