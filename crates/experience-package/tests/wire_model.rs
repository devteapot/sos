use experience_package::{
    canonical_json, canonical_sha256, AppearanceProfile, Error, InstanceId, PackageMetadata,
    ResolvedGraph, MAX_BOUNDARY_VALUE_BYTES, MAX_DEPENDENCIES, MAX_EXPORTS, MAX_GRAPH_DEPTH,
    MAX_GRAPH_INSTANCES, MAX_GRAPH_SCENE_NODES, MAX_PACKAGE_METADATA_BYTES,
};
use serde::Deserialize;

const FIXTURE: &str = include_str!("../../../tests/fixtures/experience-wire-v4.json");
const EXPECTED: &str = include_str!("../../../tests/fixtures/experience-wire-v4.expected.json");

#[derive(Deserialize)]
struct WireFixture {
    fixture_version: u32,
    instance_id: InstanceId,
    limits: WireLimits,
    package: PackageMetadata,
    appearance: AppearanceProfile,
    graph: ResolvedGraph,
}

#[derive(Deserialize)]
struct WireLimits {
    exports: usize,
    dependencies: usize,
    boundary_value_bytes: usize,
    graph_depth: usize,
    graph_instances: usize,
    graph_scene_nodes: usize,
}

#[derive(Deserialize)]
struct ExpectedIdentities {
    contract_digest: String,
    package_sha256: String,
    appearance_sha256: String,
    graph_id: String,
}

#[test]
fn shared_v4_fixture_has_exact_ids_limits_and_canonical_encodings() {
    let fixture: WireFixture = serde_json::from_str(FIXTURE).unwrap();
    let expected: ExpectedIdentities = serde_json::from_str(EXPECTED).unwrap();

    assert_eq!(fixture.fixture_version, 1);
    assert_eq!(fixture.instance_id.as_str(), "runtime-7f35c9a2");
    assert_eq!(fixture.limits.exports, MAX_EXPORTS);
    assert_eq!(fixture.limits.dependencies, MAX_DEPENDENCIES);
    assert_eq!(
        fixture.limits.boundary_value_bytes,
        MAX_BOUNDARY_VALUE_BYTES
    );
    assert_eq!(fixture.limits.graph_depth, MAX_GRAPH_DEPTH);
    assert_eq!(fixture.limits.graph_instances, MAX_GRAPH_INSTANCES);
    assert_eq!(fixture.limits.graph_scene_nodes, MAX_GRAPH_SCENE_NODES);

    fixture.package.validate().unwrap();
    fixture.appearance.validate().unwrap();
    fixture.graph.validate().unwrap();
    assert_eq!(
        fixture.package.contract.digest().unwrap().as_str(),
        expected.contract_digest
    );
    assert_eq!(
        canonical_sha256(&fixture.package).unwrap(),
        expected.package_sha256
    );
    assert_eq!(
        canonical_sha256(&fixture.appearance).unwrap(),
        expected.appearance_sha256
    );
    assert_eq!(fixture.graph.id().unwrap(), expected.graph_id);

    let package_bytes = canonical_json(&fixture.package).unwrap();
    assert_eq!(
        PackageMetadata::from_canonical_bytes(&package_bytes)
            .unwrap()
            .experience_id,
        fixture.package.experience_id
    );
    let graph_bytes = canonical_json(&fixture.graph).unwrap();
    assert_eq!(
        ResolvedGraph::from_canonical_bytes(&graph_bytes)
            .unwrap()
            .id()
            .unwrap(),
        expected.graph_id
    );
}

#[test]
fn canonical_decoders_reject_whitespace_unknown_fields_and_oversized_input() {
    let fixture: WireFixture = serde_json::from_str(FIXTURE).unwrap();
    let mut package_bytes = canonical_json(&fixture.package).unwrap();
    package_bytes.push(b'\n');
    assert!(matches!(
        PackageMetadata::from_canonical_bytes(&package_bytes),
        Err(Error::NonCanonicalJson { kind: "package" })
    ));

    let mut value = serde_json::to_value(&fixture.package).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("unknown".into(), serde_json::Value::Bool(true));
    let package_bytes = canonical_json(&value).unwrap();
    assert!(matches!(
        PackageMetadata::from_canonical_bytes(&package_bytes),
        Err(Error::NonCanonicalJson { kind: "package" })
    ));

    let oversized = vec![b' '; MAX_PACKAGE_METADATA_BYTES + 1];
    assert!(matches!(
        PackageMetadata::from_canonical_bytes(&oversized),
        Err(Error::WirePayloadTooLarge {
            kind: "package",
            ..
        })
    ));
}

#[test]
fn canonical_json_uses_cross_platform_jcs_number_and_utf16_key_rules() {
    let numbers = serde_json::json!([333333333.33333329, 1E30, 4.50, 2e-3, 1e-27]);
    assert_eq!(
        canonical_json(&numbers).unwrap(),
        br#"[333333333.3333333,1e+30,4.5,0.002,1e-27]"#
    );
    let keys = serde_json::json!({
        "\u{fb33}": 7,
        "\u{1f600}": 6,
        "€": 5,
        "ö": 4,
        "\u{80}": 3,
        "1": 2,
        "\r": 1
    });
    assert_eq!(
        String::from_utf8(canonical_json(&keys).unwrap()).unwrap(),
        "{\"\\r\":1,\"1\":2,\"\u{80}\":3,\"ö\":4,\"€\":5,\"\u{1f600}\":6,\"\u{fb33}\":7}"
    );
}
