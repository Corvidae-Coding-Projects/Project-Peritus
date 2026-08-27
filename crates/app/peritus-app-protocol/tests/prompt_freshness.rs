//! Prompt correlation and freshness integration tests.

mod support;

use peritus_app_protocol::{
    ApprovalChallenge, CorrelationId, PromptAnswer, PromptAnswerPayload, PromptBinding,
    PromptChoice, PromptConstraint, PromptCorrelation, PromptErrorKind, PromptId, PromptKind,
    PromptState, RequestId, SignedApprovalDecisionFrame, UserInputValue,
};
use peritus_types::{ActorId, CommandId, Generation, RevisionNumber, SessionId, Sha256Digest};
use support::{fixture_id, revision};

fn correlation(freshness: u8) -> PromptCorrelation {
    PromptCorrelation::new(
        fixture_id(50, RequestId::new),
        fixture_id(51, PromptId::new),
        fixture_id(52, SessionId::new),
        fixture_id(53, ActorId::new),
        revision(5),
        Sha256Digest::new([freshness; 32]),
        Generation::new(2).expect("positive cancellation generation"),
    )
}

fn selection_binding() -> PromptBinding {
    PromptBinding::new(
        PromptKind::UserInput,
        correlation(54),
        vec![
            PromptChoice::new("no".to_owned(), "No".to_owned(), 16, 32).unwrap(),
            PromptChoice::new("yes".to_owned(), "Yes".to_owned(), 16, 32).unwrap(),
        ],
        vec![PromptConstraint::BoundChoiceOnly],
        4,
        4,
    )
    .expect("canonical prompt binding")
}

fn selection_answer(correlation: PromptCorrelation, selected: &str) -> PromptAnswer {
    PromptAnswer::new(
        correlation,
        PromptAnswerPayload::UserInput(
            UserInputValue::selection(selected.to_owned(), 16).expect("bounded selection"),
        ),
        64,
    )
    .expect("bounded correlated answer")
}

#[test]
fn answers_require_complete_binding_and_live_revision() {
    let binding = selection_binding();
    let answer = selection_answer(binding.correlation(), "yes");
    let mut accepted = PromptState::new(binding.clone(), 64).expect("positive answer bound");
    accepted
        .answer(answer.clone(), revision(5))
        .expect("complete matching fresh answer is accepted");
    assert_eq!(
        accepted
            .answer(answer, revision(5))
            .expect_err("a second terminal answer is rejected")
            .kind(),
        PromptErrorKind::AlreadyTerminal,
    );

    let mut mismatched = PromptState::new(binding.clone(), 64).unwrap();
    assert_eq!(
        mismatched
            .answer(selection_answer(correlation(55), "yes"), revision(5))
            .expect_err("freshness digest is part of complete correlation")
            .kind(),
        PromptErrorKind::BindingMismatch,
    );

    let mut stale = PromptState::new(binding.clone(), 64).unwrap();
    assert_eq!(
        stale
            .answer(selection_answer(binding.correlation(), "yes"), revision(6))
            .expect_err("caller-supplied live revision must match exactly")
            .kind(),
        PromptErrorKind::StaleRevision,
    );

    let mut unknown_choice = PromptState::new(binding.clone(), 64).unwrap();
    assert_eq!(
        unknown_choice
            .answer(selection_answer(binding.correlation(), "maybe"), revision(5))
            .expect_err("selection must name a bound choice")
            .kind(),
        PromptErrorKind::UnknownChoice,
    );

    let approval_payload = PromptAnswerPayload::signed_approval(
        SignedApprovalDecisionFrame::new(vec![1], 64).expect("bounded signed decision"),
        None,
        64,
    )
    .expect("bounded signed approval response");
    let approval_answer = PromptAnswer::new(binding.correlation(), approval_payload, 64)
        .expect("bounded approval answer");
    let mut wrong_kind = PromptState::new(binding.clone(), 64).unwrap();
    assert_eq!(
        wrong_kind
            .answer(approval_answer, revision(5))
            .expect_err("approval response cannot answer an input prompt")
            .kind(),
        PromptErrorKind::WrongAnswerKind,
    );

    let mut cancelled = PromptState::new(binding.clone(), 64).unwrap();
    let cancellation = peritus_app_protocol::PromptCancellation::new(
        binding.correlation(),
        fixture_id(56, CorrelationId::new),
    );
    cancelled.cancel(cancellation).expect("matching cancellation applies");
    assert_eq!(
        cancelled
            .answer(selection_answer(binding.correlation(), "no"), revision(5))
            .expect_err("answer after cancellation is rejected")
            .kind(),
        PromptErrorKind::AlreadyTerminal,
    );
}

#[test]
fn approval_prompts_require_challenges_and_accept_only_signed_or_cancelled_answers() {
    let correlation = correlation(60);
    assert_eq!(
        PromptBinding::new(PromptKind::Approval, correlation, Vec::new(), Vec::new(), 1, 1)
            .expect_err("approval prompt without challenge is invalid")
            .kind(),
        PromptErrorKind::InvalidInput,
    );
    let challenge = ApprovalChallenge::new(
        fixture_id(61, CommandId::new),
        RevisionNumber::first(),
        vec![1, 2, 3],
        64,
    )
    .expect("bounded approval challenge");
    let binding = PromptBinding::approval(correlation, challenge.clone(), Vec::new(), 2)
        .expect("challenge-bound approval prompt");
    assert_eq!(binding.approval_challenge(), Some(&challenge));

    let signed = PromptAnswerPayload::signed_approval(
        SignedApprovalDecisionFrame::new(vec![4, 5, 6], 64).unwrap(),
        Some("reviewed".to_owned()),
        64,
    )
    .unwrap();
    let answer = PromptAnswer::new(correlation, signed, 64).unwrap();
    let mut state = PromptState::new(binding.clone(), 64).unwrap();
    state.answer(answer, revision(5)).expect("signed response is fresh protocol input");

    let cancel = PromptAnswerPayload::cancel_approval(None, 64).unwrap();
    let cancel_answer = PromptAnswer::new(correlation, cancel, 64).unwrap();
    let mut cancelled = PromptState::new(binding, 64).unwrap();
    cancelled
        .answer(cancel_answer, revision(5))
        .expect("approval cancellation is fresh protocol input");
}
