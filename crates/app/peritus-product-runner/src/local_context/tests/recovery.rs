//! Restart and trace/journal publication gaps never redispatch archived tool effects.

use super::{super::*, support::*};
use peritus_agent::{DeveloperToolObservation, DeveloperTrace, DeveloperTraceEvent};
use serde_json::Value;

#[test]
fn a_trace_committed_before_memory_is_ingested_once_after_restart() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "first");
    let view = memory.prepare_view(&profile(32_768), &[]).unwrap();
    memory.publish(&view).unwrap();
    let call = call("effect");
    proposal(&mut memory, &call);
    let observation = DeveloperToolObservation {
        output: canonical(&Value::from_iter([(
            "diagnostic",
            Value::from("middle-tail exact result"),
        )])),
        is_error: true,
    };
    let mut trace = crate::trace::FileDeveloperTrace::new(fixture.state.path().join("run.trace"))
        .with_memory_scope(memory.store.scope_digest(), memory.transcript.invocation);
    trace
        .record(DeveloperTraceEvent::ToolObservation { call: &call, observation: &observation })
        .unwrap();
    let prior = memory.state.through_observation();
    drop(memory);
    let mut reopened = fixture.open();
    assert_eq!(reopened.state.through_observation(), prior + 1);
    assert!(reopened.transcript.pending.is_empty());
    assert_eq!(reopened.artifact(prior + 1).unwrap(), observation.output.canonical_bytes());
    let proposal = update(
        reopened.model_revision,
        prior + 1,
        "cause",
        "The old approach failed for the preserved reason.",
    );
    assert!(
        tools::update::execute(&mut reopened, proposal.to_string().as_bytes())
            .unwrap()
            .get("rejected")
            .is_none()
    );
    let state = reopened.state.clone();
    drop(reopened);
    let mut reopened = fixture.open();
    assert_eq!(reopened.state, state);
    begin(&mut reopened, "different-provider");
    assert_eq!(reopened.transcript.invocation, 2);
    assert!(reopened.transcript.pending.is_empty());
    assert!(
        render(&reopened.prepare_view(&profile(32_768), &[]).unwrap()).contains("preserved reason")
    );
}

#[test]
fn reused_provider_call_ids_are_distinct_completed_episodes() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "one");
    let first = observation(&mut memory, "same-provider-call-id", "first", false);
    let second = observation(&mut memory, "same-provider-call-id", "second", false);
    assert_ne!(first, second);
    let state = memory.state.clone();
    drop(memory);
    assert_eq!(fixture.open().state, state);
}

#[test]
fn checkpoint_and_uncovered_pending_proposal_survive_without_claiming_execution() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "one");
    let id = observation(&mut memory, "read", "exact input", false);
    let proposal_json = update(memory.model_revision, id, "cause", "A preserved hypothesis");
    assert!(
        tools::update::execute(&mut memory, proposal_json.to_string().as_bytes())
            .unwrap()
            .get("rejected")
            .is_none()
    );
    let view = memory.prepare_view(&profile(32_768), &[]).unwrap();
    memory.publish(&view).unwrap();
    proposal(&mut memory, &call("unfinished"));
    drop(memory);
    let mut memory = fixture.open();
    assert_eq!(memory.last_view, view);
    assert_eq!(memory.transcript.pending.len(), 1);
    begin(&mut memory, "two");
    assert_eq!(memory.transcript.pending[0].state, record::PendingState::Unknown);
    let next = memory.prepare_view(&profile(32_768), &[]).unwrap();
    assert!(render(&next).contains("DO NOT REDISPATCH"));
    assert!(
        next.iter()
            .flat_map(peritus_model_protocol::Message::content)
            .all(|block| !matches!(block, peritus_model_protocol::ContentBlock::ToolCall(_)))
    );
}
