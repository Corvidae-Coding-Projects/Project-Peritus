use core::fmt::Write;
use std::sync::Arc;

use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    BoundedJson, BoundedText, CallLimits, FailureCategory, IdempotencyKey, JsonLimits,
    PreparedToolCall, RecoveryRoute, ResponsibleSubsystem, ResultStatus, Retryability, ToolCall,
    ToolFailure, ToolResult, ToolTiming, Truncation, TruncationMetadata, prepare_call,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, WorkspaceId,
};

use super::*;

#[test]
fn structured_quality_terminal_decodes_all_required_provenance() {
    let value = structured_value("passed", true, false);
    let decoded = decode_structured(&value).expect("valid structured terminal");
    assert_eq!(decoded.gate_id, GateId::new([1; 16]).expect("gate"));
    assert!(matches!(decoded.outcome, DecodedOutcome::Passed));
    assert_eq!(decoded.result_digest, Sha256Digest::new([2; 32]));
    assert_eq!(decoded.plan_digest, Sha256Digest::new([3; 32]));
    assert_eq!(decoded.process_id, ProcessId::new([4; 16]).expect("process"));
    assert!(decoded.execution_complete);
    assert!(!decoded.progress_truncated);
}

#[test]
fn malformed_or_open_terminal_values_fail_closed() {
    let unknown = structured_value("future-outcome", true, false);
    assert!(decode_structured(&unknown).is_none());
    let missing = BoundedJson::parse("{}", JsonLimits::PRODUCTION).expect("json");
    assert!(decode_structured(&missing).is_none());
}

#[test]
fn contradictory_failure_envelopes_are_malformed_and_never_retryable() {
    let prepared = prepared();
    let gate_id = GateId::new([1; 16]).expect("gate");
    let cases = [
        (
            ResultStatus::TimedOut,
            FailureCategory::Cancelled,
            Retryability::NewAction,
            RecoveryRoute::Reauthorize,
            "infrastructure",
        ),
        (
            ResultStatus::Failed,
            FailureCategory::Execution,
            Retryability::AfterRecovery,
            RecoveryRoute::None,
            "unsuccessful-exit",
        ),
        (
            ResultStatus::Failed,
            FailureCategory::Execution,
            Retryability::Never,
            RecoveryRoute::None,
            "passed",
        ),
    ];
    for (status, category, retryability, recovery, outcome) in cases {
        let result = failure_result(&prepared, status, category, retryability, recovery, outcome);
        let terminal = decode_quality_result(
            &result,
            QualityResultBinding::new(
                prepared.call().action_id(),
                prepared.prepared_digest(),
                prepared.replay_identity(),
                gate_id,
            ),
        )
        .expect("invocation binding");
        assert_eq!(terminal.kind(), QualityTerminalKind::MalformedOutput);
        assert_eq!(terminal.retryability(), Retryability::Never);
        assert_eq!(terminal.recovery(), RecoveryRoute::None);
    }
}

#[test]
fn status_failure_and_structured_outcome_matrix_is_closed() {
    let execution_terminal =
        failure(FailureCategory::Execution, Retryability::Never, RecoveryRoute::None);
    let timeout =
        failure(FailureCategory::Timeout, Retryability::NewAction, RecoveryRoute::Reauthorize);
    assert!(contract::terminal_contract_consistent(
        ResultStatus::Succeeded,
        None,
        Some(DecodedOutcome::Passed),
    ));
    assert!(contract::terminal_contract_consistent(
        ResultStatus::Failed,
        Some(&execution_terminal),
        Some(DecodedOutcome::PredicateFailed),
    ));
    assert!(contract::terminal_contract_consistent(
        ResultStatus::TimedOut,
        Some(&timeout),
        Some(DecodedOutcome::Infrastructure),
    ));
    assert!(!contract::terminal_contract_consistent(
        ResultStatus::Cancelled,
        Some(&timeout),
        Some(DecodedOutcome::Infrastructure),
    ));
}

fn structured_value(
    outcome: &str,
    execution_complete: bool,
    progress_truncated: bool,
) -> BoundedJson {
    let json = format!(
        "{{\"candidate\":{{\"gate_id\":\"{}\",\"outcome\":\"{outcome}\",\"result_digest\":\"{}\"}},\"execution\":{{\"plan_digest\":\"{}\",\"process_id\":\"{}\",\"complete\":{execution_complete}}},\"progress_truncated\":{progress_truncated}}}",
        hex(&[1; 16]),
        hex(&[2; 32]),
        hex(&[3; 32]),
        hex(&[4; 16]),
    );
    BoundedJson::parse(&json, JsonLimits::PRODUCTION).expect("structured json")
}

fn hex(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(value, "{byte:02x}").expect("writing to a String cannot fail");
    }
    value
}

fn failure_result(
    prepared: &PreparedToolCall,
    status: ResultStatus,
    category: FailureCategory,
    retryability: Retryability,
    recovery: RecoveryRoute,
    outcome: &str,
) -> ToolResult {
    ToolResult::failure(
        prepared,
        status,
        failure(category, retryability, recovery),
        Some(structured_value(outcome, true, false)),
        text("failure"),
        text("failure"),
        Vec::new(),
        ToolTiming::new(instant(1), instant(2)).expect("timing"),
        TruncationMetadata {
            output: Truncation::Complete,
            model: Truncation::Complete,
            human: Truncation::Complete,
        },
        0,
    )
    .expect("failure result")
}

fn failure(
    category: FailureCategory,
    retryability: Retryability,
    recovery: RecoveryRoute,
) -> ToolFailure {
    ToolFailure::new(
        category,
        text("quality-failure"),
        ResponsibleSubsystem::Tool,
        retryability,
        recovery,
        text("quality failure"),
    )
}

fn prepared() -> PreparedToolCall {
    let descriptor = Arc::new(run_descriptor().expect("run descriptor"));
    let call = ToolCall::new(
        ActionId::new([9; 16]).expect("action"),
        descriptor.name().clone(),
        descriptor.version(),
        BoundedJson::parse("{\"gate\":\"gate-1\"}", JsonLimits::PRODUCTION).expect("arguments"),
        CallLimits::new(10_000, 4_096, 1_024, 1_024, 8, 1).expect("limits"),
        revision(),
        instant(10_000),
        IdempotencyKey::new("quality-result-contract".to_owned()).expect("idempotency"),
    );
    prepare_call(descriptor, call).expect("prepared call")
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new([10; 16]).expect("acceptance"),
        HarnessId::new([11; 16]).expect("harness"),
        WorkspaceId::new([12; 16]).expect("workspace"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([13; 16]).expect("policy"),
        ProviderProfileId::new([14; 16]).expect("provider"),
    )
}

fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}

fn text(value: &str) -> BoundedText {
    BoundedText::new(value.to_owned()).expect("bounded text")
}
