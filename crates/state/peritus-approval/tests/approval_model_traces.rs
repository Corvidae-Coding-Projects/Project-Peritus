//! Seeded command traces checked step-by-step against an independent executable oracle.

#[path = "approval_model_traces/model.rs"]
mod model;
mod support;

use peritus_approval::{
    ActionDigest, ApprovalAggregate, ApprovalChoice, ApprovalPhase,
    AuthenticatedApprovalObservation, CredentialRegistrySnapshot, verify_signed_decision,
};
use peritus_policy::ActorRole;
use peritus_types::{Generation, RevisionNumber, Sha256Digest};

use model::{
    AcceptedView, AggregateView, Command, InputView, RejectedView, StepView, aggregate_view,
    amendment_view, initial_view, observation_view, oracle_step, transition_view, use_view,
};

const SEEDS: [u64; 4] =
    [0x6d5a_56da_4f31_29c7, 0x9e37_79b9_7f4a_7c15, 0xd1b5_4a32_d192_ed03, 0x94d0_49bb_1331_11eb];
const CASES: usize = 12;
const STEPS: usize = 24;

struct Prepared {
    command: Command,
    observation: Option<AuthenticatedApprovalObservation>,
    input: InputView,
}

fn resolve_choice(command: Command) -> Option<(ApprovalChoice, u8)> {
    match command {
        Command::ResolveApprove => Some((ApprovalChoice::ApproveOnce, 0x30)),
        Command::ResolveDeny => Some((ApprovalChoice::Deny, 0x31)),
        Command::ResolveAmend => {
            let (_, identity) = support::amendment_candidate();
            Some((ApprovalChoice::Amend(identity), 0x32))
        }
        _ => None,
    }
}

fn prepare(command: Command, fixture: &support::SignedFixture) -> Prepared {
    let (candidate, candidate_identity) = if matches!(command, Command::AmendWrongCandidate) {
        support::amendment_candidate_with(0x42, 0x43)
    } else {
        support::amendment_candidate()
    };
    let _ = candidate;
    let action_digest = if matches!(command, Command::ConsumeWrongDigest) {
        ActionDigest::from_sha256(Sha256Digest::new([0xa5; 32]))
    } else {
        fixture.request.action_digest()
    };
    let observed_at = if matches!(command, Command::Expire) {
        support::instant(90)
    } else if resolve_choice(command).is_some() {
        fixture.observed_at
    } else {
        support::instant(40)
    };
    let observation = resolve_choice(command).map(|(choice, command_byte)| {
        let signed = support::signed_decision(
            &fixture.request,
            choice,
            fixture.ids.responder,
            ActorRole::HumanAuthority,
            support::approval_key_id(),
            Generation::first(),
            RevisionNumber::first(),
            command_byte,
            support::instant(75),
        );
        verify_signed_decision(&fixture.request, &signed, &fixture.registry, fixture.observed_at)
            .expect("generated signed command authenticates before execution")
    });
    let input = InputView {
        command,
        observation: observation.as_ref().map(observation_view),
        action_id: fixture.request.action_id(),
        action_digest,
        candidate: candidate_identity,
        observed_at,
    };
    Prepared { command, observation, input }
}

fn apply_actual(
    aggregate: ApprovalAggregate,
    prepared: Prepared,
    registry: &CredentialRegistrySnapshot,
) -> (ApprovalAggregate, StepView) {
    let Prepared { command, observation, input } = prepared;
    match command {
        Command::ResolveApprove | Command::ResolveDeny | Command::ResolveAmend => {
            apply_resolve(aggregate, observation.expect("resolve observation"), registry)
        }
        Command::Consume | Command::ConsumeWrongDigest => apply_consume(aggregate, &input),
        Command::Amend | Command::AmendWrongCandidate => apply_amendment(aggregate, command),
        Command::Expire | Command::ExpireEarly => apply_expire(aggregate, command),
        Command::Cancel => apply_cancel(aggregate),
    }
}

fn apply_resolve(
    aggregate: ApprovalAggregate,
    observation: AuthenticatedApprovalObservation,
    registry: &CredentialRegistrySnapshot,
) -> (ApprovalAggregate, StepView) {
    match aggregate.resolve(observation, registry) {
        Ok(outcome) => {
            let (aggregate, transition) = outcome.into_parts();
            let view = StepView {
                after: aggregate_view(&aggregate),
                result: Ok(AcceptedView::Transition(transition_view(&transition))),
            };
            (aggregate, view)
        }
        Err(failure) => {
            let (error, aggregate, observation) = failure.into_parts();
            let view = StepView {
                after: aggregate_view(&aggregate),
                result: Err(RejectedView {
                    error,
                    observation: observation.as_ref().map(observation_view),
                }),
            };
            (aggregate, view)
        }
    }
}

fn apply_consume(aggregate: ApprovalAggregate, input: &InputView) -> (ApprovalAggregate, StepView) {
    let action_id = aggregate.request().action_id();
    match aggregate.consume_once(action_id, input.action_digest, input.observed_at) {
        Ok(outcome) => {
            let (aggregate, transition, consumed) = outcome.into_parts();
            let view = StepView {
                after: aggregate_view(&aggregate),
                result: Ok(AcceptedView::Use(Box::new(use_view(&transition, &consumed)))),
            };
            (aggregate, view)
        }
        Err(failure) => {
            let (error, aggregate) = failure.into_parts();
            let view = StepView {
                after: aggregate_view(&aggregate),
                result: Err(RejectedView { error, observation: None }),
            };
            (aggregate, view)
        }
    }
}

fn apply_amendment(
    aggregate: ApprovalAggregate,
    command: Command,
) -> (ApprovalAggregate, StepView) {
    let (candidate, _) = if matches!(command, Command::Amend) {
        support::amendment_candidate()
    } else {
        support::amendment_candidate_with(0x42, 0x43)
    };
    match aggregate.consume_amendment(&candidate, support::instant(40)) {
        Ok(outcome) => {
            let (aggregate, approval) = outcome.into_parts();
            let view = StepView {
                after: aggregate_view(&aggregate),
                result: Ok(AcceptedView::Amendment(amendment_view(&approval))),
            };
            (aggregate, view)
        }
        Err(failure) => {
            let (error, aggregate, _) = failure.into_parts();
            let view = StepView {
                after: aggregate_view(&aggregate),
                result: Err(RejectedView { error, observation: None }),
            };
            (aggregate, view)
        }
    }
}

fn apply_expire(aggregate: ApprovalAggregate, command: Command) -> (ApprovalAggregate, StepView) {
    let observed_at = if matches!(command, Command::Expire) {
        support::instant(90)
    } else {
        support::instant(40)
    };
    match aggregate.expire(observed_at) {
        Ok(outcome) => {
            let (aggregate, transition) = outcome.into_parts();
            let view = StepView {
                after: aggregate_view(&aggregate),
                result: Ok(AcceptedView::Transition(transition_view(&transition))),
            };
            (aggregate, view)
        }
        Err(failure) => {
            let (error, aggregate, _) = failure.into_parts();
            let view = StepView {
                after: aggregate_view(&aggregate),
                result: Err(RejectedView { error, observation: None }),
            };
            (aggregate, view)
        }
    }
}

fn apply_cancel(aggregate: ApprovalAggregate) -> (ApprovalAggregate, StepView) {
    match aggregate.cancel() {
        Ok(outcome) => {
            let (aggregate, transition) = outcome.into_parts();
            let view = StepView {
                after: aggregate_view(&aggregate),
                result: Ok(AcceptedView::Transition(transition_view(&transition))),
            };
            (aggregate, view)
        }
        Err(failure) => {
            let (error, aggregate, _) = failure.into_parts();
            let view = StepView {
                after: aggregate_view(&aggregate),
                result: Err(RejectedView { error, observation: None }),
            };
            (aggregate, view)
        }
    }
}

fn forced(case: usize, step: usize) -> Option<Command> {
    let sequence: &[Command] = match case % 6 {
        0 => &[
            Command::ResolveApprove,
            Command::ResolveApprove,
            Command::ResolveDeny,
            Command::ConsumeWrongDigest,
            Command::Consume,
            Command::Consume,
            Command::Expire,
            Command::Cancel,
            Command::Amend,
        ],
        1 => &[
            Command::ResolveAmend,
            Command::AmendWrongCandidate,
            Command::Amend,
            Command::Amend,
            Command::Consume,
            Command::Expire,
            Command::Cancel,
        ],
        2 => &[
            Command::ResolveDeny,
            Command::ResolveApprove,
            Command::Consume,
            Command::Amend,
            Command::Expire,
            Command::Cancel,
        ],
        3 => &[
            Command::Cancel,
            Command::Cancel,
            Command::Expire,
            Command::Consume,
            Command::Amend,
            Command::ResolveApprove,
        ],
        4 => &[
            Command::ExpireEarly,
            Command::ResolveApprove,
            Command::ExpireEarly,
            Command::Expire,
            Command::ResolveApprove,
            Command::ResolveDeny,
            Command::Consume,
            Command::Amend,
            Command::Cancel,
        ],
        _ => &[
            Command::Expire,
            Command::ResolveApprove,
            Command::ResolveDeny,
            Command::Consume,
            Command::Amend,
            Command::Cancel,
        ],
    };
    sequence.get(step).copied()
}

const fn generated_command(state: &mut u64) -> Command {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    match *state % 10 {
        0 => Command::ResolveApprove,
        1 => Command::ResolveDeny,
        2 => Command::ResolveAmend,
        3 => Command::Consume,
        4 => Command::ConsumeWrongDigest,
        5 => Command::Amend,
        6 => Command::AmendWrongCandidate,
        7 => Command::ExpireEarly,
        8 => Command::Expire,
        _ => Command::Cancel,
    }
}

const fn terminal_index(phase: ApprovalPhase) -> Option<usize> {
    match phase {
        ApprovalPhase::Consumed => Some(0),
        ApprovalPhase::Amended => Some(1),
        ApprovalPhase::Denied => Some(2),
        ApprovalPhase::Expired => Some(3),
        ApprovalPhase::Cancelled => Some(4),
        _ => None,
    }
}

#[test]
fn generated_traces_match_independent_model_after_every_step() {
    let mut saw_accept = false;
    let mut saw_reject = false;
    let mut terminal_rejections = [false; 5];
    for case in 0..CASES {
        let seed = SEEDS[case % SEEDS.len()] ^ case as u64;
        let mut random = seed;
        let commands: Vec<_> = (0..STEPS)
            .map(|step| forced(case, step).unwrap_or_else(|| generated_command(&mut random)))
            .collect();
        let fixture = support::signed_fixture(ApprovalChoice::ApproveOnce);
        #[allow(
            clippy::needless_collect,
            reason = "commands must borrow the fixture before its move-only request is consumed"
        )]
        let prepared: Vec<_> =
            commands.into_iter().map(|command| prepare(command, &fixture)).collect();
        let mut oracle: AggregateView = initial_view(&fixture.request);
        let mut aggregate = ApprovalAggregate::new(fixture.request);

        for (step, input) in prepared.into_iter().enumerate() {
            let command = input.command;
            let expected = oracle_step(&oracle, &input.input);
            let before = oracle.phase;
            let (next_aggregate, actual) = apply_actual(aggregate, input, &fixture.registry);
            if expected.result.is_ok() {
                saw_accept = true;
            } else {
                saw_reject = true;
                if let Some(index) = terminal_index(before) {
                    terminal_rejections[index] = true;
                }
            }
            assert_eq!(
                actual, expected,
                "full oracle divergence seed={seed:#x} case={case} step={step} command={command:?}"
            );
            oracle = expected.after;
            aggregate = next_aggregate;
            assert_eq!(
                aggregate_view(&aggregate),
                oracle,
                "full snapshot divergence seed={seed:#x} case={case} step={step} command={command:?}"
            );
        }
    }
    assert!(saw_accept && saw_reject, "corpus must exercise accepted and rejected commands");
    assert!(
        terminal_rejections.into_iter().all(|seen| seen),
        "every terminal phase must reject at least one generated command"
    );
}
