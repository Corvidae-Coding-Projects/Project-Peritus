//! Lifecycle, replay, expiry, and move-only consumption behavior.

mod support;

use peritus_approval::{
    ApprovalAggregate, ApprovalChoice, ApprovalError, ApprovalPhase, ApprovalTransitionKind,
    ScopeDimension, verify_signed_decision,
};

#[test]
fn approve_once_binds_one_exact_use_and_rejects_reuse() {
    let fixture = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let observation = verify_signed_decision(
        &fixture.request,
        &fixture.signed,
        &fixture.registry,
        fixture.observed_at,
    )
    .expect("valid signed approval");
    let action_id = fixture.request.action_id();
    let action_digest = fixture.request.action_digest();

    let resolved = ApprovalAggregate::new(fixture.request)
        .resolve(observation, &fixture.registry)
        .expect("first resolution");
    assert_eq!(resolved.transition().kind(), ApprovalTransitionKind::Resolved);
    assert_eq!(resolved.aggregate().phase(), ApprovalPhase::ApprovedOnce);
    let (aggregate, _) = resolved.into_parts();
    let used =
        aggregate.consume_once(action_id, action_digest, support::instant(40)).expect("exact use");
    assert_eq!(used.aggregate().phase(), ApprovalPhase::Consumed);
    assert_eq!(used.transition().action_id(), action_id);
    assert_eq!(used.transition().action_digest(), action_digest);
    assert_eq!(used.consumed().request_id(), used.transition().request_id());

    let (aggregate, _, _) = used.into_parts();
    let failure = aggregate
        .consume_once(action_id, action_digest, support::instant(41))
        .expect_err("approve-once cannot be reused");
    assert_eq!(*failure.error(), ApprovalError::AlreadyConsumed);
    assert_eq!(failure.aggregate().phase(), ApprovalPhase::Consumed);
}

#[test]
fn use_binding_failures_preserve_the_authorization() {
    let fixture = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let observation = verify_signed_decision(
        &fixture.request,
        &fixture.signed,
        &fixture.registry,
        fixture.observed_at,
    )
    .expect("valid signed approval");
    let action_id = fixture.request.action_id();
    let action_digest = fixture.request.action_digest();
    let aggregate = ApprovalAggregate::new(fixture.request)
        .resolve(observation, &fixture.registry)
        .expect("resolution")
        .into_parts()
        .0;

    let wrong_digest =
        peritus_approval::ActionDigest::from_sha256(peritus_types::Sha256Digest::new([0x99; 32]));
    let failure = aggregate
        .consume_once(action_id, wrong_digest, support::instant(40))
        .expect_err("digest mismatch");
    assert_eq!(*failure.error(), ApprovalError::BindingMismatch(ScopeDimension::ActionDigest));
    let (_, aggregate) = failure.into_parts();
    assert_eq!(aggregate.phase(), ApprovalPhase::ApprovedOnce);
    assert!(aggregate.consume_once(action_id, action_digest, support::instant(40)).is_ok());
}

#[test]
fn exact_replay_survives_expiry_but_conflict_does_not() {
    let fixture = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let exact = verify_signed_decision(
        &fixture.request,
        &fixture.signed,
        &fixture.registry,
        fixture.observed_at,
    )
    .expect("first observation");
    let replay = verify_signed_decision(
        &fixture.request,
        &fixture.signed,
        &fixture.registry,
        fixture.observed_at,
    )
    .expect("same signed observation");
    let conflicting = support::signed_decision(
        &fixture.request,
        ApprovalChoice::Deny,
        fixture.ids.responder,
        peritus_policy::ActorRole::HumanAuthority,
        support::approval_key_id(),
        peritus_types::Generation::first(),
        peritus_types::RevisionNumber::first(),
        17,
        support::instant(75),
    );
    let conflict = verify_signed_decision(
        &fixture.request,
        &conflicting,
        &fixture.registry,
        fixture.observed_at,
    )
    .expect("independently valid conflicting decision");
    let changed_command = support::signed_decision(
        &fixture.request,
        ApprovalChoice::ApproveOnce,
        fixture.ids.responder,
        peritus_policy::ActorRole::HumanAuthority,
        support::approval_key_id(),
        peritus_types::Generation::first(),
        peritus_types::RevisionNumber::first(),
        18,
        support::instant(75),
    );
    let changed_command = verify_signed_decision(
        &fixture.request,
        &changed_command,
        &fixture.registry,
        fixture.observed_at,
    )
    .expect("same choice with a distinct command authenticates");

    let aggregate = ApprovalAggregate::new(fixture.request)
        .resolve(exact, &fixture.registry)
        .expect("resolution")
        .into_parts()
        .0
        .expire(support::instant(75))
        .expect("exclusive expiry")
        .into_parts()
        .0;
    let replayed = aggregate.resolve(replay, &fixture.registry).expect("exact terminal replay");
    assert_eq!(replayed.transition().kind(), ApprovalTransitionKind::Idempotent);
    assert_eq!(replayed.aggregate().phase(), ApprovalPhase::Expired);
    let (aggregate, _) = replayed.into_parts();
    let failure = aggregate.resolve(conflict, &fixture.registry).expect_err("terminal conflict");
    assert_eq!(*failure.error(), ApprovalError::AlreadyResolved);
    assert_eq!(failure.aggregate().phase(), ApprovalPhase::Expired);
    let (_, aggregate, _) = failure.into_parts();
    let failure = aggregate
        .resolve(changed_command, &fixture.registry)
        .expect_err("digest-semantic replay rejects any changed signed field");
    assert_eq!(*failure.error(), ApprovalError::AlreadyResolved);
    assert_eq!(failure.aggregate().phase(), ApprovalPhase::Expired);
}

#[test]
fn deny_cancel_and_pending_expiry_are_closed_terminal_states() {
    let denied = support::signed_fixture(ApprovalChoice::Deny);
    let observation = verify_signed_decision(
        &denied.request,
        &denied.signed,
        &denied.registry,
        denied.observed_at,
    )
    .expect("deny authenticates");
    let aggregate = ApprovalAggregate::new(denied.request)
        .resolve(observation, &denied.registry)
        .expect("deny resolves")
        .into_parts()
        .0;
    assert_eq!(aggregate.phase(), ApprovalPhase::Denied);
    let failure = aggregate.cancel().expect_err("denial is terminal");
    assert!(matches!(failure.error(), ApprovalError::IllegalPhase { .. }));

    let cancelled = ApprovalAggregate::new(support::request(1, Vec::new()))
        .cancel()
        .expect("pending cancellation");
    assert_eq!(cancelled.aggregate().phase(), ApprovalPhase::Cancelled);
    let (aggregate, _) = cancelled.into_parts();
    assert!(matches!(
        aggregate.expire(support::instant(90)).expect_err("cancelled terminal").error(),
        ApprovalError::IllegalPhase { .. }
    ));

    let expired = ApprovalAggregate::new(support::request(1, Vec::new()))
        .expire(support::instant(90))
        .expect("pending expires at exclusive bound");
    assert_eq!(expired.aggregate().phase(), ApprovalPhase::Expired);
}

#[test]
fn amendment_authorization_binds_the_exact_previewed_candidate() {
    let (candidate, identity) = support::amendment_candidate();
    let fixture = support::signed_fixture(ApprovalChoice::Amend(identity));
    let observation = verify_signed_decision(
        &fixture.request,
        &fixture.signed,
        &fixture.registry,
        fixture.observed_at,
    )
    .expect("amendment signature");
    let aggregate = ApprovalAggregate::new(fixture.request)
        .resolve(observation, &fixture.registry)
        .expect("amendment resolution")
        .into_parts()
        .0;
    assert_eq!(aggregate.phase(), ApprovalPhase::AmendmentAuthorized);
    let amended = aggregate
        .consume_amendment(&candidate, support::instant(40))
        .expect("exact candidate consumption");
    assert_eq!(amended.aggregate().phase(), ApprovalPhase::Amended);
    assert!(amended.approval().matches_candidate(&candidate));
}
