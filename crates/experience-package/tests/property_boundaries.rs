use std::collections::{BTreeMap, BTreeSet};

use experience_package::{
    canonical_json, DependencyAlias, ExperienceId, ExportId, FieldSchema, GraphNodeId,
    ResolvedGraph, ResolvedGraphNode, RevisionId, ValueSchema, GRAPH_FORMAT_VERSION,
    MAX_GRAPH_DEPTH, MAX_GRAPH_INSTANCES, MAX_SCHEMA_DEPTH,
};
use serde_json::{json, Value};

const FIXTURE: &str = include_str!("../../../tests/fixtures/experience-wire-v4.json");

#[derive(Clone, Copy)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn usize(&mut self, upper: usize) -> usize {
        (self.next() as usize) % upper
    }
}

fn generated_schema(rng: &mut Rng, depth: usize) -> ValueSchema {
    let variant = if depth == MAX_SCHEMA_DEPTH {
        rng.usize(4)
    } else {
        rng.usize(6)
    };
    match variant {
        0 => ValueSchema::Null,
        1 => ValueSchema::Boolean,
        2 => {
            let lower = (rng.next() % 101) as i64 - 50;
            let upper = lower + (rng.next() % 101) as i64;
            ValueSchema::Integer {
                minimum: Some(lower),
                maximum: Some(upper),
            }
        }
        3 => {
            let lower = (rng.next() % 10_000) as f64 / 100.0 - 50.0;
            let upper = lower + (rng.next() % 10_000) as f64 / 100.0;
            ValueSchema::Number {
                minimum: Some(lower),
                maximum: Some(upper),
            }
        }
        4 => {
            let max_bytes = 1 + rng.usize(128);
            let choices = (0..rng.usize(5))
                .map(|index| format!("choice-{index}"))
                .filter(|choice| choice.len() <= max_bytes)
                .collect::<BTreeSet<_>>();
            ValueSchema::String { max_bytes, choices }
        }
        _ if rng.usize(2) == 0 => ValueSchema::List {
            max_items: rng.usize(16),
            items: Box::new(generated_schema(rng, depth + 1)),
        },
        _ => ValueSchema::Record {
            fields: (0..rng.usize(7))
                .map(|index| {
                    (
                        format!("field-{depth}-{index}"),
                        FieldSchema {
                            required: rng.usize(2) == 0,
                            value: generated_schema(rng, depth + 1),
                        },
                    )
                })
                .collect(),
        },
    }
}

fn wrong_type(schema: &ValueSchema) -> Value {
    match schema {
        ValueSchema::Null => Value::Bool(false),
        ValueSchema::Boolean => Value::String("wrong".into()),
        ValueSchema::Integer { .. } | ValueSchema::Number { .. } => Value::String("wrong".into()),
        ValueSchema::String { .. } => Value::Bool(false),
        ValueSchema::List { .. } => json!({}),
        ValueSchema::Record { .. } => json!([]),
    }
}

fn revision(seed: u64) -> RevisionId {
    RevisionId::parse(format!("{seed:064x}")).unwrap()
}

fn generated_graph(rng: &mut Rng) -> ResolvedGraph {
    let count = 1 + rng.usize(MAX_GRAPH_INSTANCES);
    let ids = (0..count)
        .map(|index| GraphNodeId::parse(format!("node-{index}")).unwrap())
        .collect::<Vec<_>>();
    let mut depths = vec![0_usize];
    let mut nodes = BTreeMap::new();
    nodes.insert(
        ids[0].clone(),
        ResolvedGraphNode {
            experience_id: ExperienceId::parse("experience-0").unwrap(),
            revision_id: revision(rng.next()),
            export_id: ExportId::parse("main").unwrap(),
            parent: None,
            dependency: None,
        },
    );
    for index in 1..count {
        let candidates = (0..index)
            .filter(|candidate| depths[*candidate] < MAX_GRAPH_DEPTH)
            .collect::<Vec<_>>();
        let parent_index = candidates[rng.usize(candidates.len())];
        depths.push(depths[parent_index] + 1);
        nodes.insert(
            ids[index].clone(),
            ResolvedGraphNode {
                experience_id: ExperienceId::parse(format!("experience-{index}")).unwrap(),
                revision_id: revision(rng.next()),
                export_id: ExportId::parse("main").unwrap(),
                parent: Some(ids[parent_index].clone()),
                dependency: Some(DependencyAlias::parse(format!("dependency-{index}")).unwrap()),
            },
        );
    }
    ResolvedGraph {
        format_version: GRAPH_FORMAT_VERSION,
        root: ids[0].clone(),
        nodes,
    }
}

#[test]
fn generated_schema_examples_round_trip_and_reject_wrong_types() {
    let mut rng = Rng(0x736f_732d_7363_6865);
    for _ in 0..10_000 {
        let schema = generated_schema(&mut rng, 0);
        schema.validate_definition().unwrap();
        schema.validate_value(&schema.example_value()).unwrap();
        assert!(schema.validate_value(&wrong_type(&schema)).is_err());

        let bytes = canonical_json(&schema).unwrap();
        let decoded: ValueSchema = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded, schema);
    }
}

#[test]
fn generated_graphs_round_trip_and_structural_corruption_fails_closed() {
    let mut rng = Rng(0x736f_732d_6772_6170);
    for _ in 0..10_000 {
        let graph = generated_graph(&mut rng);
        graph.validate().unwrap();
        let id = graph.id().unwrap();
        let bytes = canonical_json(&graph).unwrap();
        let decoded = ResolvedGraph::from_canonical_bytes(&bytes).unwrap();
        assert_eq!(decoded, graph);
        assert_eq!(decoded.id().unwrap(), id);

        let mut bad_root = graph.clone();
        let root = bad_root.root.clone();
        bad_root.nodes.get_mut(&root).unwrap().parent = Some(root.clone());
        assert!(bad_root.validate().is_err());

        if graph.nodes.len() > 1 {
            let mut missing_parent = graph.clone();
            let child = missing_parent
                .nodes
                .keys()
                .find(|node| **node != root)
                .unwrap()
                .clone();
            missing_parent.nodes.get_mut(&child).unwrap().parent =
                Some(GraphNodeId::parse("missing-parent").unwrap());
            assert!(missing_parent.validate().is_err());
        }
    }
}

#[test]
fn canonical_package_and_graph_decoders_survive_a_deterministic_mutation_corpus() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let package = canonical_json(&fixture["package"]).unwrap();
    let graph = canonical_json(&fixture["graph"]).unwrap();
    let mut rng = Rng(0x736f_732d_7769_7265);

    for (is_package, original) in [(true, package.as_slice()), (false, graph.as_slice())] {
        for _ in 0..5_000 {
            let mut bytes = original.to_vec();
            match rng.usize(4) {
                0 if !bytes.is_empty() => {
                    let index = rng.usize(bytes.len());
                    bytes[index] ^= 1 << rng.usize(7);
                }
                1 if !bytes.is_empty() => {
                    let index = rng.usize(bytes.len());
                    bytes.remove(index);
                }
                2 => {
                    let index = rng.usize(bytes.len() + 1);
                    bytes.insert(index, (rng.next() & 0xff) as u8);
                }
                _ => bytes.truncate(rng.usize(bytes.len() + 1)),
            }

            if is_package {
                if let Ok(decoded) =
                    experience_package::PackageMetadata::from_canonical_bytes(&bytes)
                {
                    assert_eq!(canonical_json(&decoded).unwrap(), bytes);
                }
            } else if let Ok(decoded) = ResolvedGraph::from_canonical_bytes(&bytes) {
                assert_eq!(canonical_json(&decoded).unwrap(), bytes);
            }
        }
    }
}
