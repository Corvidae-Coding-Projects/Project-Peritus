//! Reviewer isolation excludes derived memory without discarding exact observations.

use super::*;
use peritus_context::working::WorkingBinding;
use peritus_role::HarnessRole;
use serde_json::Value;

#[test]
fn failed_reviewer_observations_remain_readable_without_derived_memory() {
    let fixture = Fixture::new();
    let writer = binding();
    let reviewer = WorkingBinding::new(
        writer.run(),
        writer.workspace(),
        writer.task(),
        HarnessRole::Reviewer,
        1,
    );
    let mut memory = memory::LocalMemory::load(
        &fixture.state.path().join("reviewer"),
        fixture.workspace.path(),
        &fixture.state.path().join("reviewer.trace"),
        reviewer,
        LocalContextConfig::default(),
    )
    .unwrap();
    begin(&mut memory, "review");
    let source = observation(&mut memory, "failed-read", "EXACT_REVIEWER_DIAGNOSTIC", true);
    let proposal = update(memory.model_revision, source, "hypothesis", "Reviewer speculation");
    assert!(
        tools::update::execute(&mut memory, proposal.to_string().as_bytes())
            .unwrap()
            .get("rejected")
            .is_some()
    );
    let view = memory.prepare_view(&profile(32768), &[]).unwrap();
    memory.publish(&view).unwrap();
    assert!(memory.state.entries(reviewer).unwrap().is_empty());
    let request = Value::from_iter([
        ("observation_ids", Value::Array(vec![Value::from(tools::source_handle(&memory, source))])),
        ("query", Value::Null),
        ("cursor", Value::Null),
        ("offset", Value::from(0)),
        ("max_bytes", Value::from(16384)),
    ]);
    let result = tools::read::execute(&mut memory, request.to_string().as_bytes()).unwrap();
    assert!(result.to_string().contains("EXACT_REVIEWER_DIAGNOSTIC"));
    assert!(!render(&view).contains("Reviewer speculation"));
}
