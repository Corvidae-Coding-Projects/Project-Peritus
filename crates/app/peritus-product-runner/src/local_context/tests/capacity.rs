//! Required working memory can borrow provider headroom without losing its durable closure.

use super::super::tools;
use super::support::*;
use peritus_agent::estimate_developer_request_tokens;
use serde_json::Value;

#[test]
fn required_working_closure_uses_provider_headroom_and_survives_restart() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "capacity-reproduction");
    let source = observation(&mut memory, "read", "source for the investigation", false);
    for index in 0..8 {
        let label = format!("open-plan-{index}");
        let mut proposal = update(
            memory.model_revision,
            source,
            &label,
            &format!("{label}: {}", "Retain this investigation detail. ".repeat(55)),
        );
        proposal["operations"][0]["kind"] = Value::from("plan");
        proposal["operations"][0]["validity"] = Value::from("task");
        let result = tools::update::execute(&mut memory, proposal.to_string().as_bytes()).unwrap();
        assert!(result.get("rejected").is_none(), "{result}");
    }
    let provider = profile(32_768);
    let view = memory.prepare_view(&provider, &[]).unwrap();
    assert!(estimate_developer_request_tokens(&view, &[]) > 4096);
    assert!(estimate_developer_request_tokens(&view, &[]) <= 32_768);
    for index in 0..8 {
        assert!(render(&view).contains(&format!("open-plan-{index}")));
    }
    memory.publish(&view).unwrap();
    drop(memory);
    let mut recovered = fixture.open();
    assert_eq!(recovered.prepare_view(&provider, &[]).unwrap(), view);
    let generation = recovered.store.generation();
    let error = recovered.prepare_view(&profile(4096), &[]).unwrap_err();
    assert!(error.to_string().contains("required working-state closure exceeds input capacity"));
    assert_eq!(recovered.store.generation(), generation);
    assert_eq!(recovered.last_view, view);
    assert_eq!(recovered.prepare_view(&provider, &[]).unwrap(), view);
}

#[test]
fn optional_working_entries_leave_room_for_policy_tools_and_message_framing() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "framing");
    let source = observation(&mut memory, "read", "observed evidence", false);
    for index in 0..8 {
        let proposal = update(
            memory.model_revision,
            source,
            &format!("optional-{index}"),
            "An optional investigation note.",
        );
        let result = tools::update::execute(&mut memory, proposal.to_string().as_bytes()).unwrap();
        assert!(result.get("rejected").is_none(), "{result}");
    }
    // Start a new invocation so old completed exchanges cannot determine the tight boundary.
    begin(&mut memory, "framing-next");
    memory.config.retrieved_evidence_max_tokens = 0;
    let definitions = tools::definitions().unwrap();
    let policy = message(peritus_model_protocol::Role::System, "Current policy and grounding");
    let full =
        memory.prepare_view_with_policy(&profile(32_768), &definitions, Some(&policy)).unwrap();
    let capacity = estimate_developer_request_tokens(&full, &definitions) - 16;
    let tight =
        memory.prepare_view_with_policy(&profile(capacity), &definitions, Some(&policy)).unwrap();
    assert_eq!(tight.first(), Some(&policy));
    assert!(estimate_developer_request_tokens(&tight, &definitions) <= capacity);
    assert!(memory.prepared.as_ref().unwrap().validation.omitted_entries > 0);
    assert_eq!(memory.state.entries(memory.state.binding()).unwrap().len(), 8);
}
