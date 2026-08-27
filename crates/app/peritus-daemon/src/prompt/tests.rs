//! Broker settlement ordering tests.

use peritus_app_protocol::{
    CorrelationId, PromptAnswer, PromptAnswerPayload, PromptBinding, PromptCancellation,
    PromptCorrelation, PromptKind, RequestId, UserInputValue,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, SessionId, Sha256Digest, WorkspaceId,
};

use super::{
    PromptAcceptance, PromptAdmission, PromptBroker, PromptBrokerErrorKind, PromptBrokerLimits,
    PromptCancellationAcceptance, PromptSettlementToken, PromptTerminalStatus,
};

#[test]
fn prepared_answer_remains_awaiting_until_durable_settlement() {
    let (mut broker, binding) = broker();
    let answer = answer(binding.correlation());

    let prepared = broker
        .prepare_answer(admission(binding.correlation()), answer.clone(), None)
        .expect("fresh user input prepares");
    assert_eq!(
        broker.status(binding.correlation()).expect("status"),
        PromptTerminalStatus::AwaitingAnswer,
    );

    let (acceptance, settlement) = prepared.into_parts();
    assert!(matches!(acceptance, PromptAcceptance::UserInput(value) if value == answer));
    assert_eq!(
        broker.commit_settlement(settlement).expect("durable settlement commits broker state"),
        PromptTerminalStatus::Answered,
    );
    assert_eq!(
        broker.status(binding.correlation()).expect("terminal status"),
        PromptTerminalStatus::Answered,
    );
    assert_eq!(
        broker
            .commit_settlement(PromptSettlementToken::answer(answer))
            .expect("exact settlement replay is idempotent"),
        PromptTerminalStatus::Answered,
    );
}

#[test]
fn abandoned_preparation_can_be_retried_exactly() {
    let (broker, binding) = broker();
    let answer = answer(binding.correlation());
    let first = broker
        .prepare_answer(admission(binding.correlation()), answer.clone(), None)
        .expect("first preparation");
    drop(first);
    let second = broker
        .prepare_answer(admission(binding.correlation()), answer, None)
        .expect("unsettled preparation does not poison retry");
    drop(second);
    assert_eq!(
        broker.status(binding.correlation()).expect("status"),
        PromptTerminalStatus::AwaitingAnswer,
    );
}

#[test]
fn cancellation_is_inert_until_its_target_settles() {
    let (mut broker, binding) = broker();
    let cancellation = PromptCancellation::new(
        binding.correlation(),
        CorrelationId::new([0x31; 16]).expect("correlation identity"),
    );
    let prepared = broker
        .prepare_cancel(admission(binding.correlation()), cancellation)
        .expect("fresh cancellation prepares");
    assert_eq!(
        broker.status(binding.correlation()).expect("status"),
        PromptTerminalStatus::AwaitingAnswer,
    );
    let (acceptance, settlement) = prepared.into_parts();
    assert!(matches!(
        acceptance,
        PromptAcceptance::Cancelled(PromptCancellationAcceptance::Control(value))
            if value == cancellation
    ));
    assert_eq!(
        broker.commit_settlement(settlement).expect("settled cancellation commits"),
        PromptTerminalStatus::Cancelled,
    );
}

#[test]
fn correlation_listing_is_exact_bounded_and_owner_scoped() {
    let first = binding(0x12, 0x14, 0x13);
    let second = binding(0x22, 0x14, 0x13);
    let other_actor = binding(0x32, 0x24, 0x13);
    let other_session = binding(0x42, 0x14, 0x23);
    let mut broker = PromptBroker::new(PromptBrokerLimits::new(4).expect("broker limits"));
    for value in [&first, &second, &other_actor, &other_session] {
        broker.register(value.clone(), 64).expect("register prompt");
    }

    let correlations = broker
        .correlations_for(first.correlation().actor_id(), first.correlation().session_id(), 2)
        .expect("exact owned correlation list");
    assert_eq!(correlations, vec![first.correlation(), second.correlation()]);
    assert_eq!(
        broker
            .correlations_for(first.correlation().actor_id(), first.correlation().session_id(), 1)
            .expect_err("exact list cannot be truncated")
            .kind(),
        PromptBrokerErrorKind::ListingLimitExceeded,
    );
    assert_eq!(
        broker
            .correlations_for(first.correlation().actor_id(), first.correlation().session_id(), 0)
            .expect_err("zero result bound")
            .kind(),
        PromptBrokerErrorKind::InvalidLimit,
    );
}

#[test]
fn only_terminal_prompts_can_be_retired() {
    let (mut broker, binding) = broker();
    assert_eq!(
        broker
            .retire_terminal(binding.correlation())
            .expect_err("awaiting prompt cannot retire")
            .kind(),
        PromptBrokerErrorKind::StillAwaiting,
    );
    let cancellation = PromptCancellation::new(
        binding.correlation(),
        CorrelationId::new([0x51; 16]).expect("cancellation correlation"),
    );
    let prepared = broker
        .prepare_cancel(admission(binding.correlation()), cancellation)
        .expect("prepare cancellation");
    let (_, settlement) = prepared.into_parts();
    broker.commit_settlement(settlement).expect("settle cancellation");

    assert_eq!(
        broker.retire_terminal(binding.correlation()).expect("retire terminal prompt"),
        PromptTerminalStatus::Cancelled,
    );
    assert_eq!(
        broker.status(binding.correlation()).expect_err("retired prompt is absent").kind(),
        PromptBrokerErrorKind::NotFound,
    );
}

fn broker() -> (PromptBroker, PromptBinding) {
    let binding = binding(0x12, 0x14, 0x13);
    let mut broker = PromptBroker::new(PromptBrokerLimits::new(4).expect("broker limits"));
    broker.register(binding.clone(), 64).expect("register prompt");
    (broker, binding)
}

fn binding(prompt: u8, actor: u8, session: u8) -> PromptBinding {
    let correlation = PromptCorrelation::new(
        RequestId::new([0x11; 16]).expect("request identity"),
        peritus_app_protocol::PromptId::new([prompt; 16]).expect("prompt identity"),
        SessionId::new([session; 16]).expect("session identity"),
        ActorId::new([actor; 16]).expect("actor identity"),
        revision(),
        Sha256Digest::new([0x15; 32]),
        Generation::new(2).expect("cancellation generation"),
    );
    PromptBinding::new(PromptKind::UserInput, correlation, Vec::new(), Vec::new(), 1, 1)
        .expect("user-input binding")
}

fn answer(correlation: PromptCorrelation) -> PromptAnswer {
    PromptAnswer::new(
        correlation,
        PromptAnswerPayload::UserInput(UserInputValue::Confirmation(true)),
        64,
    )
    .expect("bounded answer")
}

fn admission(correlation: PromptCorrelation) -> PromptAdmission {
    PromptAdmission::new(
        correlation.actor_id(),
        correlation.session_id(),
        correlation.revision(),
        correlation.cancellation_generation(),
    )
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new([1; 16]).expect("acceptance identity"),
        HarnessId::new([2; 16]).expect("harness identity"),
        WorkspaceId::new([3; 16]).expect("workspace identity"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([4; 16]).expect("policy identity"),
        ProviderProfileId::new([5; 16]).expect("provider identity"),
    )
}
