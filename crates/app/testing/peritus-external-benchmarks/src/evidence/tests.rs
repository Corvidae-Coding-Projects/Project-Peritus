use super::*;

#[test]
fn last_observation_retains_gate_review_and_resource_diagnostics() {
    let directory = tempfile::tempdir().expect("evidence directory");
    let observation = ProductObservation::from_update(ProductRunUpdate {
        phase: ProductRunPhase::Reviewing,
        cycle: 3,
        status: "Fresh typed review completed".to_owned(),
        diff: "exact diff".to_owned(),
        gates: "Exact-target acceptance: PASS".to_owned(),
        review: "Canonical reason remains contradictory".to_owned(),
        summary: "candidate retained".to_owned(),
        finding_state: "{\"cycle\":3}".to_owned(),
        progress: ProductRunProgress::default(),
    });
    let path = observation.publish(directory.path()).expect("publish observation");
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("read")).expect("parse");
    assert_eq!(value["phase"], "reviewing");
    assert_eq!(value["gates"], "Exact-target acceptance: PASS");
    assert_eq!(value["review"], "Canonical reason remains contradictory");
    assert_eq!(value["resources"]["model_requests"], 0);
}

#[test]
fn relocatable_paths_survive_a_sandbox_move() {
    let sandbox = Path::new("/state/workspaces/model/task-before");
    let trace = sandbox.join("peritus-benchmark/developer-round-0001.trace");
    let paths = RelocatablePaths::new(
        sandbox,
        &sandbox.join("workspace"),
        &trace,
        std::slice::from_ref(&trace),
        &sandbox.join("usage-proxy"),
        None,
    )
    .expect("paths");
    assert_eq!(paths.workspace, Path::new("workspace"));
    assert_eq!(paths.trace_path, Path::new("peritus-benchmark/developer-round-0001.trace"));
}

#[test]
fn checked_in_v6_fixture_covers_every_required_schema_field() {
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../benchmarks/schemas/external-invocation-report-v6.schema.json"
    ))
    .expect("schema");
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../benchmarks/external/fixtures/invocation-v6-failed-no-candidate.json"
    ))
    .expect("fixture");
    let required = schema["required"].as_array().expect("required fields");
    for field in required {
        let field = field.as_str().expect("field name");
        assert!(fixture.get(field).is_some(), "fixture is missing {field}");
    }
    assert_eq!(fixture["schema_version"], 6);
    assert_eq!(fixture["disposition"], "failed_no_candidate");
    assert!(fixture["candidate"].is_null());
    assert!(fixture["agent_identity"]["source_revision"].is_null());
}
