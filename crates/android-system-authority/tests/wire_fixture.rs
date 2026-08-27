use experience_package::{
    canonical_sha256, AppearanceProfile, InstanceId, PackageMetadata, ResolvedGraph,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    instance_id: InstanceId,
    package: PackageMetadata,
    appearance: AppearanceProfile,
    graph: ResolvedGraph,
}

#[test]
fn android_authority_decodes_the_shared_v4_wire_fixture() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../tests/fixtures/experience-wire-v4.json"
    ))
    .unwrap();
    fixture.package.validate().unwrap();
    fixture.appearance.validate().unwrap();
    fixture.graph.validate().unwrap();
    assert_eq!(fixture.instance_id.as_str(), "runtime-7f35c9a2");
    assert_eq!(
        canonical_sha256(&fixture.package).unwrap(),
        "251aac888fdacc226c14330e7ccfaae5174da3b6a0f95b23d4a1f3b119edd09b"
    );
    assert_eq!(
        fixture.graph.id().unwrap(),
        "138a8bed4b03f5fdc61bea372acee89945f21a99e4bece24de8343e05999a94c"
    );
}
