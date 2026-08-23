//! Independent reducer over the immutable oracle views.

use peritus_approval::{
    ApprovalChoice, ApprovalError, ApprovalPhase, ApprovalTransitionKind, ScopeDimension,
};
use peritus_policy::AuthorityInstant;

use super::views::{
    AcceptedView, AggregateView, AmendmentView, ObservationView, RejectedView, ResolutionView,
    StepView, TransitionView, UseView,
};
use super::{Command, InputView};

const fn phase_for_choice(choice: ApprovalChoice) -> ApprovalPhase {
    match choice {
        ApprovalChoice::ApproveOnce => ApprovalPhase::ApprovedOnce,
        ApprovalChoice::Deny => ApprovalPhase::Denied,
        ApprovalChoice::Amend(_) => ApprovalPhase::AmendmentAuthorized,
    }
}

fn rejected(
    before: &AggregateView,
    error: ApprovalError,
    observation: Option<ObservationView>,
) -> StepView {
    StepView { after: before.clone(), result: Err(RejectedView { error, observation }) }
}

fn resolve_step(before: &AggregateView, observation: ObservationView) -> StepView {
    if before.phase != ApprovalPhase::Pending {
        return match before.resolution {
            Some(resolution) if resolution.decision_digest == observation.decision_digest => {
                StepView {
                    after: before.clone(),
                    result: Ok(AcceptedView::Transition(TransitionView {
                        kind: ApprovalTransitionKind::Idempotent,
                        from: before.phase,
                        to: before.phase,
                        decision_digest: Some(observation.decision_digest),
                        registry_revision: Some(observation.registry_revision),
                    })),
                }
            }
            _ => rejected(before, ApprovalError::AlreadyResolved, Some(observation)),
        };
    }
    let valid_until = [
        before.request.validity.expires_at(),
        before.request.scope_validity.expires_at(),
        before.request.requirement_validity.expires_at(),
        observation.credential_validity.expires_at(),
        observation.decision_expires_at,
    ]
    .into_iter()
    .min_by_key(|instant| instant.tick_millis())
    .expect("five expiry bounds");
    if observation.observed_at.tick_millis() >= valid_until.tick_millis() {
        return rejected(before, ApprovalError::Expired, Some(observation));
    }
    let resolution = ResolutionView {
        decision_digest: observation.decision_digest,
        command_id: observation.command_id,
        choice: observation.choice,
        registry_revision: observation.registry_revision,
        credential_generation: observation.credential_generation,
        valid_until,
    };
    let mut after = before.clone();
    after.phase = phase_for_choice(observation.choice);
    after.resolution = Some(resolution);
    after.request.authority_epoch = observation.observed_at.epoch();
    after.request.authority_tick = observation.observed_at.tick_millis();
    StepView {
        after,
        result: Ok(AcceptedView::Transition(TransitionView {
            kind: ApprovalTransitionKind::Resolved,
            from: ApprovalPhase::Pending,
            to: phase_for_choice(observation.choice),
            decision_digest: Some(observation.decision_digest),
            registry_revision: Some(observation.registry_revision),
        })),
    }
}

pub fn oracle_step(before: &AggregateView, input: InputView) -> StepView {
    match input.command {
        Command::ResolveApprove | Command::ResolveDeny | Command::ResolveAmend => {
            resolve_step(before, input.observation.expect("resolve observation"))
        }
        Command::Consume | Command::ConsumeWrongDigest => consume_step(before, input),
        Command::Amend | Command::AmendWrongCandidate => amendment_step(before, input),
        Command::Expire | Command::ExpireEarly => expiry_step(before, input.observed_at),
        Command::Cancel => cancel_step(before),
    }
}

fn consume_step(before: &AggregateView, input: InputView) -> StepView {
    let Some(resolution) = before.resolution else {
        return rejected(
            before,
            ApprovalError::IllegalPhase {
                expected: ApprovalPhase::ApprovedOnce,
                actual: before.phase,
            },
            None,
        );
    };
    if before.phase == ApprovalPhase::Consumed {
        return rejected(before, ApprovalError::AlreadyConsumed, None);
    }
    if before.phase != ApprovalPhase::ApprovedOnce {
        return rejected(
            before,
            ApprovalError::IllegalPhase {
                expected: ApprovalPhase::ApprovedOnce,
                actual: before.phase,
            },
            None,
        );
    }
    if input.action_id != before.request.action_id {
        return rejected(before, ApprovalError::BindingMismatch(ScopeDimension::Action), None);
    }
    if input.action_digest != before.request.action_digest {
        return rejected(
            before,
            ApprovalError::BindingMismatch(ScopeDimension::ActionDigest),
            None,
        );
    }
    if input.observed_at.tick_millis() >= resolution.valid_until.tick_millis() {
        return rejected(before, ApprovalError::Expired, None);
    }
    let mut after = before.clone();
    after.phase = ApprovalPhase::Consumed;
    after.request.authority_epoch = input.observed_at.epoch();
    after.request.authority_tick = input.observed_at.tick_millis();
    StepView {
        after,
        result: Ok(AcceptedView::Use(UseView {
            request_id: before.request.request_id,
            request_digest: before.request.digest,
            action_id: input.action_id,
            action_digest: input.action_digest,
            revision: before.request.revision,
            decision_digest: resolution.decision_digest,
            command_id: resolution.command_id,
            registry_revision: resolution.registry_revision,
            valid_until: resolution.valid_until,
            consumed_request_id: before.request.request_id,
            consumed_decision_digest: resolution.decision_digest,
            consumed_action_id: input.action_id,
        })),
    }
}

fn amendment_step(before: &AggregateView, input: InputView) -> StepView {
    let Some(resolution) = before.resolution else {
        return rejected(
            before,
            ApprovalError::IllegalPhase {
                expected: ApprovalPhase::AmendmentAuthorized,
                actual: before.phase,
            },
            None,
        );
    };
    if before.phase != ApprovalPhase::AmendmentAuthorized {
        return rejected(
            before,
            ApprovalError::IllegalPhase {
                expected: ApprovalPhase::AmendmentAuthorized,
                actual: before.phase,
            },
            None,
        );
    }
    let ApprovalChoice::Amend(identity) = resolution.choice else {
        return rejected(before, ApprovalError::CorruptState, None);
    };
    if identity != input.candidate {
        return rejected(before, ApprovalError::BindingMismatch(ScopeDimension::Choice), None);
    }
    if input.observed_at.tick_millis() >= resolution.valid_until.tick_millis() {
        return rejected(before, ApprovalError::Expired, None);
    }
    let mut after = before.clone();
    after.phase = ApprovalPhase::Amended;
    after.request.authority_epoch = input.observed_at.epoch();
    after.request.authority_tick = input.observed_at.tick_millis();
    StepView {
        after,
        result: Ok(AcceptedView::Amendment(AmendmentView {
            identity,
            registry_revision: resolution.registry_revision,
        })),
    }
}

fn expiry_step(before: &AggregateView, observed_at: AuthorityInstant) -> StepView {
    let expiry = match before.phase {
        ApprovalPhase::Pending => [
            before.request.validity.expires_at(),
            before.request.scope_validity.expires_at(),
            before.request.requirement_validity.expires_at(),
        ]
        .into_iter()
        .min_by_key(|instant| instant.tick_millis())
        .expect("three request bounds"),
        ApprovalPhase::ApprovedOnce | ApprovalPhase::AmendmentAuthorized => {
            before.resolution.expect("resolved phase").valid_until
        }
        actual => {
            return rejected(
                before,
                ApprovalError::IllegalPhase { expected: ApprovalPhase::Pending, actual },
                None,
            );
        }
    };
    if observed_at.tick_millis() < expiry.tick_millis() {
        return rejected(before, ApprovalError::NotYetValid, None);
    }
    let mut after = before.clone();
    after.phase = ApprovalPhase::Expired;
    after.request.authority_epoch = observed_at.epoch();
    after.request.authority_tick = observed_at.tick_millis();
    StepView {
        after,
        result: Ok(AcceptedView::Transition(TransitionView {
            kind: ApprovalTransitionKind::Expired,
            from: before.phase,
            to: ApprovalPhase::Expired,
            decision_digest: before.resolution.map(|value| value.decision_digest),
            registry_revision: before.resolution.map(|value| value.registry_revision),
        })),
    }
}

fn cancel_step(before: &AggregateView) -> StepView {
    if before.phase != ApprovalPhase::Pending {
        return rejected(
            before,
            ApprovalError::IllegalPhase { expected: ApprovalPhase::Pending, actual: before.phase },
            None,
        );
    }
    let mut after = before.clone();
    after.phase = ApprovalPhase::Cancelled;
    StepView {
        after,
        result: Ok(AcceptedView::Transition(TransitionView {
            kind: ApprovalTransitionKind::Cancelled,
            from: ApprovalPhase::Pending,
            to: ApprovalPhase::Cancelled,
            decision_digest: None,
            registry_revision: None,
        })),
    }
}
