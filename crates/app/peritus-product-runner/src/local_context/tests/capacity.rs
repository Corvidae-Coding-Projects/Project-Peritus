//! Required working memory can borrow provider headroom without losing its durable closure.

use super::super::tools;
use super::support::*;
use peritus_agent::estimate_developer_request_tokens;
use serde_json::Value;

#[test]
fn reviewer_packet_leaves_room_for_authoritative_tool_observations() {
    use crate::turn::{ReviewDelivery, ReviewerPrompt, reviewer_system, reviewer_user};
    use peritus_agent::{DeveloperLoopLimits, DeveloperLoopRequest};
    use peritus_model_protocol::Role;

    let system = reviewer_system(std::time::Duration::from_secs(235));
    let diff = "changed source evidence ".repeat(400);
    let developer = "retained command observation ".repeat(4000);
    let mut definitions = crate::developer_tools::read_only_definitions().unwrap();
    definitions.extend(tools::definitions().unwrap());
    for capacity in [32_768, 200_000] {
        let fixture = Fixture::new();
        let mut memory = fixture.open();
        memory.config.retrieved_evidence_max_tokens = 0;
        let prompt = reviewer_user(&ReviewerPrompt {
            system: &system,
            tools: &definitions,
            transcript: "Read input.txt and independently check the exact requested result.",
            diff: &diff,
            gates: "Exact candidate checks: PASS",
            developer_evidence: &developer,
            prior: "",
            max_input_tokens: capacity,
            delivery: ReviewDelivery {
                scope: crate::ProductDeliveryScope::WorkspaceChanges,
                effect_requirement:
                    crate::delivery_requirement::ExternalEffectRequirement::Optional,
            },
            correction: None,
        })
        .unwrap();
        let request = DeveloperLoopRequest {
            request_prefix: "review-evidence-headroom".to_owned(),
            system: system.clone(),
            prompt,
            attachments: vec![],
            tools: definitions.clone(),
            limits: DeveloperLoopLimits::new(4, 16).unwrap(),
            cancellation: peritus_provider_core::CancellationToken::new(),
        };
        assert!(
            request
                .prompt
                .contains("Read input.txt and independently check the exact requested result.")
        );
        memory
            .begin(
                &request,
                &[message(Role::System, &request.system), message(Role::User, &request.prompt)],
            )
            .unwrap();
        let facts = "CURRENT_SOURCE_FACT ".repeat(475);
        observation(&mut memory, "decisive-read", &facts, false);
        let view = memory.prepare_view(&profile(capacity), &definitions).unwrap();
        assert!(estimate_developer_request_tokens(&view, &definitions) <= capacity);
        assert!(
            view.iter().any(|message| message.role() == Role::Tool),
            "the {capacity}-token reviewer must receive its completed authoritative read"
        );
        let expected = canonical(&serde_json::json!({"diagnostic": facts}));
        assert!(view.iter().flat_map(peritus_model_protocol::Message::content).any(|block| {
            matches!(
                block,
                peritus_model_protocol::ContentBlock::ToolResult(result)
                    if result.call_id().expose_for_wire() == "decisive-read"
                        && result.output() == &expected
            )
        }));
        memory.publish(&view).unwrap();
    }
}

#[test]
fn recent_completed_exchange_uses_provider_headroom_above_compaction_target() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "pinned-headroom");
    memory.config.retrieved_evidence_max_tokens = 0;
    let policy = message(peritus_model_protocol::Role::System, "Canonical current policy.");
    observation(&mut memory, "recent-read", "RECENT_VERIFICATION_OK", false);
    let full = memory.prepare_view_with_policy(&profile(32_768), &[], Some(&policy)).unwrap();
    let capacity = estimate_developer_request_tokens(&full, &[]);
    let trigger = capacity * u64::from(memory.config.trigger_percent) / 100;
    assert!(capacity > trigger, "fixture requires the complete exchange above the target");

    let view = memory.prepare_view_with_policy(&profile(capacity), &[], Some(&policy)).unwrap();

    assert_eq!(view.first(), Some(&policy));
    assert!(estimate_developer_request_tokens(&view, &[]) <= capacity);
    assert!(view.iter().any(|message| message.role() == peritus_model_protocol::Role::Assistant));
    assert!(view.iter().any(|message| message.role() == peritus_model_protocol::Role::Tool));
    memory.publish(&view).unwrap();
}

#[test]
fn recent_source_and_result_remain_available_for_comparison() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "source-result-comparison");
    memory.config.retrieved_evidence_max_tokens = 0;
    let policy = message(peritus_model_protocol::Role::System, "Compare source and result.");
    observation(&mut memory, "source-read", "AUTHORITATIVE_SOURCE_FACT", false);
    observation(&mut memory, "result-read", "CANDIDATE_RESULT_FACT", false);
    let complete = memory.prepare_view_with_policy(&profile(32_768), &[], Some(&policy)).unwrap();
    let capacity = estimate_developer_request_tokens(&complete, &[]);
    let view = memory.prepare_view_with_policy(&profile(capacity), &[], Some(&policy)).unwrap();

    assert!(estimate_developer_request_tokens(&view, &[]) <= capacity);
    for (call_id, fact) in
        [("source-read", "AUTHORITATIVE_SOURCE_FACT"), ("result-read", "CANDIDATE_RESULT_FACT")]
    {
        let expected = canonical(&serde_json::json!({"diagnostic": fact}));
        assert!(
            view.iter().flat_map(peritus_model_protocol::Message::content).any(|block| {
                matches!(block, peritus_model_protocol::ContentBlock::ToolResult(output)
                if output.call_id().expose_for_wire() == call_id && output.output() == &expected)
            }),
            "source and result must coexist when their complete exchanges fit"
        );
    }
    memory.publish(&view).unwrap();
    drop(memory);
    let mut recovered = fixture.open();
    recovered.config.retrieved_evidence_max_tokens = 0;
    assert_eq!(
        recovered.prepare_view_with_policy(&profile(capacity), &[], Some(&policy)).unwrap(),
        view
    );
}

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

#[test]
fn retry_and_restart_preserve_the_exact_selected_working_view_and_sources() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "selected-view-source");
    let source = observation(&mut memory, "read", "source for retained working state", false);
    let proposal = update(
        memory.model_revision,
        source,
        "retained-finding",
        "The selected working view must retain this exact source binding.",
    );
    let result = tools::update::execute(&mut memory, proposal.to_string().as_bytes()).unwrap();
    assert!(result.get("rejected").is_none(), "{result}");

    begin(&mut memory, "selected-view-source-next");
    memory.config.retrieved_evidence_max_tokens = 0;
    let provider = profile(32_768);
    let first = memory.prepare_view(&provider, &[]).unwrap();
    let first_validation = memory.prepared.as_ref().unwrap().validation.clone();
    assert!(render(&first).contains("selected working view must retain this exact source binding"));
    assert!(first_validation.selected_observations.contains(&source));

    let retry = memory.prepare_view(&provider, &[]).unwrap();
    assert_eq!(retry, first);
    assert_eq!(memory.prepared.as_ref().unwrap().validation, first_validation);
    memory.publish(&retry).unwrap();
    drop(memory);

    let mut recovered = fixture.open();
    recovered.config.retrieved_evidence_max_tokens = 0;
    let restarted = recovered.prepare_view(&provider, &[]).unwrap();
    assert_eq!(restarted, first);
    assert_eq!(recovered.prepared.as_ref().unwrap().validation, first_validation);
}
