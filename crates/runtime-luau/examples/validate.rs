use std::{env, fs, process};

use runtime_luau::{LuauRuntime, RevisionAssetInput, ValidationReport};
use serde_json::json;

fn main() {
    let mut path = None;
    let mut json_output = false;
    let mut modules = Vec::new();
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--json" {
            json_output = true;
        } else if argument == "--module" {
            let Some(specification) = arguments.next() else {
                usage();
            };
            modules.push(read_module(&specification));
        } else if let Some(specification) = argument.strip_prefix("--module=") {
            modules.push(read_module(specification));
        } else if path.replace(argument).is_some() {
            usage();
        }
    }
    let Some(path) = path else {
        usage();
    };
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("could not read {path}: {error}");
            process::exit(2);
        }
    };
    let module_count = modules.len();
    let report = LuauRuntime::compile_with_assets(&source, modules).and_then(|runtime| {
        runtime.validate_all(
            &providers_fake::snapshot(),
            &json!({
                "draft": "Caffè ☕️ – 明日のデザイン",
            }),
        )
    });
    match report {
        Ok(report) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "source": path,
                        "source_bytes": source.len(),
                        "module_count": module_count,
                        "report": report,
                    }))
                    .expect("validation report is serializable")
                );
            } else {
                print_report(&path, source.len(), module_count, &report);
            }
            if !report.valid {
                process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("candidate rejected: {error}");
            process::exit(1);
        }
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: cargo run -p runtime-luau --example validate -- <experience.luau> [--module ID=FILE]... [--json]"
    );
    process::exit(2);
}

fn read_module(specification: &str) -> RevisionAssetInput {
    let Some((id, path)) = specification.split_once('=') else {
        usage();
    };
    if id.is_empty() || path.is_empty() {
        usage();
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("could not read module {id} from {path}: {error}");
            process::exit(2);
        }
    };
    RevisionAssetInput {
        id: id.into(),
        kind: "luau".into(),
        bytes,
    }
}

fn print_report(path: &str, source_bytes: usize, module_count: usize, report: &ValidationReport) {
    println!(
        "validation {} source={} source_bytes={} modules={} scenarios={}",
        if report.valid { "passed" } else { "failed" },
        path,
        source_bytes,
        module_count,
        report.scenarios.len()
    );
    for scenario in &report.scenarios {
        if let Some(statistics) = scenario.statistics {
            println!(
                "  PASS scenario={} nodes={} inputs={} images={} paint_nodes={} animations={} semantics={}",
                scenario.name,
                statistics.nodes,
                statistics.text_sessions,
                statistics.images,
                statistics.paint_nodes,
                statistics.animations,
                statistics.semantics,
            );
        } else if let Some(diagnostic) = &scenario.diagnostic {
            println!(
                "  FAIL scenario={} stage={} path={} message={}",
                scenario.name,
                diagnostic.stage,
                diagnostic.path.as_deref().unwrap_or("module"),
                diagnostic.message,
            );
        }
    }
}
