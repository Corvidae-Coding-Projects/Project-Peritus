use super::*;
use peritus_model_protocol::{ReducedItem, TerminalOutcome};

#[derive(Debug, Eq, PartialEq)]
struct CompletionFailure {
    kind: peritus_model_protocol::ProtocolErrorKind,
    path: String,
    detail: &'static str,
    accepted_semantics: Vec<u8>,
    rejected_semantic: u8,
}

// Record deletion must regenerate local sequence numbers and the count header: deleting
// raw bytes would otherwise manufacture an ordering failure or fallback-generated events.
fn encode_records(records: &[[u8; EVENT_BYTES]]) -> Vec<u8> {
    assert!(!records.is_empty() && records.len() <= EVENT_LIMIT);
    let mut bytes = vec![u8::try_from(records.len() - 1).expect("bounded count")];
    bytes.extend(records.iter().flatten());
    bytes.push(1); // Preserve the explicit no-EOF schedule independently of the last record.
    bytes
}

fn completion_failure(records: &[[u8; EVENT_BYTES]]) -> Option<CompletionFailure> {
    let bytes = encode_records(records);
    let mut reducer = ResponseReducer::new(
        ProviderName::new("discovery-provider".to_owned()).expect("provider"),
        limits(),
    );
    let mut accepted_semantics = Vec::new();
    for (record, event) in records.iter().zip(events(&bytes, limits())) {
        let semantic = record[1] % 12;
        match reducer.push(event) {
            Ok(ReducerTransition::Applied) => {
                if semantic != 1 {
                    accepted_semantics.push(semantic);
                }
            }
            Err(error) => {
                // Observe the first rejection, never a later post-terminal error. These
                // assertions mirror the owner's malformed_completed_fragments contract.
                assert!(matches!(reducer.terminal(), Some(TerminalOutcome::Failed(_))));
                assert!(reducer.completed_items().is_empty());
                assert_eq!(
                    reducer.finish_eof().expect("failure remains terminal"),
                    reducer.terminal().expect("terminal").clone(),
                );
                return Some(CompletionFailure {
                    kind: error.kind(),
                    path: error.path().to_owned(),
                    detail: error.detail(),
                    accepted_semantics,
                    rejected_semantic: semantic,
                });
            }
            Ok(ReducerTransition::DuplicateIgnored | ReducerTransition::Terminal(_)) => {
                return None;
            }
        }
    }
    None
}

fn minimize_records(
    mut records: Vec<[u8; EVENT_BYTES]>,
    expected: &CompletionFailure,
    budget: usize,
) -> (Vec<[u8; EVENT_BYTES]>, usize) {
    assert_eq!(completion_failure(&records).as_ref(), Some(expected));
    let mut attempts = 0;
    let mut index = 0;
    while records.len() > 1 && index < records.len() && attempts < budget {
        let mut candidate = records.clone();
        candidate.remove(index);
        attempts += 1;
        if completion_failure(&candidate).as_ref() == Some(expected) {
            records = candidate;
            index = 0;
        } else {
            index += 1;
        }
    }
    (records, attempts)
}

#[test]
fn structured_minimization_preserves_failure_signature_and_completion_reachability() {
    // Qualification fixture, not a newly discovered product defect. Independent expected
    // behavior comes from peritus-model-protocol/tests/reducer_matrix.rs:
    // malformed_completed_fragments_irreversibly_fail_the_response.
    let padded = include_bytes!("../../corpus/provider_sequence/incomplete-utf8-padded");
    let records = padded[1..padded.len() - 1]
        .chunks_exact(EVENT_BYTES)
        .map(|chunk| chunk.try_into().expect("complete encoded record"))
        .collect::<Vec<[u8; EVENT_BYTES]>>();
    assert_eq!(encode_records(&records), padded);
    let expected = CompletionFailure {
        kind: peritus_model_protocol::ProtocolErrorKind::InvalidEvent,
        path: "stream".to_owned(),
        detail: "text ended with incomplete UTF-8",
        accepted_semantics: vec![0, 2, 3], // response start, message start, text accepted
        rejected_semantic: 4,              // explicit item completion reached
    };
    assert_eq!(completion_failure(&records).as_ref(), Some(&expected));
    let (unchanged, attempts) = minimize_records(records.clone(), &expected, 0);
    assert_eq!((unchanged, attempts), (records.clone(), 0));
    let (bounded, attempts) = minimize_records(records.clone(), &expected, 1);
    assert_eq!(attempts, 1);
    assert_eq!(completion_failure(&bounded).as_ref(), Some(&expected));

    let budget = EVENT_LIMIT * EVENT_LIMIT;
    let (minimal, attempts) = minimize_records(records.clone(), &expected, budget);
    assert!(attempts > 1 && attempts < budget, "minimizer exhausted its budget");
    assert!(minimal.len() < records.len());
    assert_eq!(minimal.len(), 4);
    assert_eq!(completion_failure(&minimal).as_ref(), Some(&expected));
    let encoded = encode_records(&minimal);
    assert_eq!(encoded, include_bytes!("../../corpus/provider_sequence/incomplete-utf8-minimal"));
    super::super::check_input(super::super::DiscoveryTarget::ProviderSequence, &encoded);
    for index in 0..minimal.len() {
        let mut candidate = minimal.clone();
        candidate.remove(index);
        assert_ne!(completion_failure(&candidate).as_ref(), Some(&expected));
    }
}

#[test]
fn checked_seeds_reach_complete_message_and_tool_lifecycles() {
    let message = reduce(include_bytes!("../../corpus/provider_sequence/message-lifecycle"));
    assert!(matches!(message.terminal(), Some(TerminalOutcome::Succeeded { .. })));
    match message.completed_items() {
        [ReducedItem::Text { index, text, .. }] => {
            assert_eq!((*index, text.expose_for_wire()), (0, "a"));
        }
        items => panic!("message lifecycle did not complete exactly once: {items:?}"),
    }

    let tool = reduce(include_bytes!("../../corpus/provider_sequence/tool-lifecycle"));
    assert!(
        matches!(tool.terminal(), Some(TerminalOutcome::RequiresAction { .. })),
        "unexpected tool terminal: {:?}; items: {:?}",
        tool.terminal(),
        tool.completed_items(),
    );
    match tool.completed_items() {
        [ReducedItem::ToolCall { index, call, .. }] => {
            assert_eq!(*index, 3);
            assert_eq!(call.name().as_str(), "tool-0");
            assert_eq!(call.arguments().canonical_bytes(), b"{}");
        }
        items => panic!("tool lifecycle did not complete exactly once: {items:?}"),
    }
}
