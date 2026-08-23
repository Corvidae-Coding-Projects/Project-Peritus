//! Canonical digest of the complete B0 successor aggregate.

use peritus_budget::{BudgetAmounts, BudgetDimension, BudgetLimits};
use peritus_kernel::{
    AcceptancePhase, ActionPhase, AttemptPhase, KernelAggregate, ReviewPhase, RunPhase,
    SessionPhase, TurnPhase, WaiverPhase,
};
use peritus_policy::ActorRole;
use peritus_types::Sha256Digest;

use crate::domain::encoding;

pub(super) fn kernel_state_digest(aggregate: &KernelAggregate) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(2_048);
    bytes.extend_from_slice(b"PERITUS-C0-KERNEL-STATE-V1\0");
    bytes.extend_from_slice(aggregate.project_id().as_bytes());
    encoding::revision(&mut bytes, aggregate.revision());
    let binding = aggregate.contract_binding();
    bytes.extend_from_slice(binding.contract_id().as_bytes());
    encoding::digest(&mut bytes, binding.contract_digest());
    let session = aggregate.session();
    bytes.extend_from_slice(session.id().as_bytes());
    encoding::u8_value(&mut bytes, session_phase(session.phase()));
    bytes.extend_from_slice(aggregate.head_event_id().as_bytes());
    encoding::u64_value(&mut bytes, aggregate.last_sequence().get());
    encoding::u64_value(&mut bytes, aggregate.runs().len() as u64);
    for run in aggregate.runs() {
        bytes.extend_from_slice(run.id().as_bytes());
        encoding::revision(&mut bytes, run.revision());
        bytes.extend_from_slice(run.budget_id().as_bytes());
        limits(&mut bytes, run.budget_limits());
        encoding::u8_value(&mut bytes, run_phase(run.phase()));
        encoding::u8_value(&mut bytes, acceptance_phase(run.acceptance()));
        option_id(&mut bytes, run.current_attempt_id().map(peritus_types::AttemptId::into_bytes));
    }
    encoding::u64_value(&mut bytes, aggregate.attempts().len() as u64);
    for attempt in aggregate.attempts() {
        bytes.extend_from_slice(attempt.id().as_bytes());
        bytes.extend_from_slice(attempt.run_id().as_bytes());
        bytes.extend_from_slice(attempt.budget_id().as_bytes());
        limits(&mut bytes, attempt.budget_limits());
        encoding::u8_value(&mut bytes, attempt_phase(attempt.phase()));
    }
    encoding::u64_value(&mut bytes, aggregate.turns().len() as u64);
    for turn in aggregate.turns() {
        bytes.extend_from_slice(turn.id().as_bytes());
        bytes.extend_from_slice(turn.attempt_id().as_bytes());
        encoding::u8_value(&mut bytes, turn_phase(turn.phase()));
    }
    encoding::u64_value(&mut bytes, aggregate.actions().len() as u64);
    for action in aggregate.actions() {
        bytes.extend_from_slice(action.id().as_bytes());
        bytes.extend_from_slice(action.turn_id().as_bytes());
        encoding::digest(&mut bytes, action.digest());
        bytes.extend_from_slice(action.actor_id().as_bytes());
        encoding::u8_value(&mut bytes, role_tag(action.role()));
        bytes.extend_from_slice(action.environment_id().as_bytes());
        encoding::u8_value(&mut bytes, action_phase(action.phase()));
        match action.authorization() {
            Some(value) => {
                encoding::u8_value(&mut bytes, 1);
                encoding::digest(&mut bytes, value.transition_digest());
                bytes.extend_from_slice(value.resource_id().as_bytes());
                encoding::bytes_value(&mut bytes, value.capability_name().as_str().as_bytes());
            }
            None => encoding::u8_value(&mut bytes, 0),
        }
    }
    encoding::u64_value(&mut bytes, aggregate.reviews().len() as u64);
    for review in aggregate.reviews() {
        bytes.extend_from_slice(review.id().as_bytes());
        bytes.extend_from_slice(review.run_id().as_bytes());
        bytes.extend_from_slice(review.attempt_id().as_bytes());
        encoding::u8_value(&mut bytes, review_phase(review.phase()));
    }
    encoding::u64_value(&mut bytes, aggregate.waivers().len() as u64);
    for waiver in aggregate.waivers() {
        bytes.extend_from_slice(waiver.finding_id().as_bytes());
        bytes.extend_from_slice(waiver.review_cycle_id().as_bytes());
        bytes.extend_from_slice(waiver.run_id().as_bytes());
        encoding::u8_value(&mut bytes, waiver_phase(waiver.phase()));
    }
    peritus_codec::sha256(&bytes)
}

fn limits(bytes: &mut Vec<u8>, value: BudgetLimits) {
    amounts(bytes, value.amounts());
}

fn amounts(bytes: &mut Vec<u8>, value: BudgetAmounts) {
    for dimension in [
        BudgetDimension::ModelTokens,
        BudgetDimension::ProviderCostMicrounits,
        BudgetDimension::ActiveEffectMilliseconds,
        BudgetDimension::Attempts,
        BudgetDimension::Retries,
    ] {
        encoding::u64_value(bytes, value.get(dimension).get());
    }
}

fn option_id(bytes: &mut Vec<u8>, value: Option<[u8; 16]>) {
    match value {
        Some(value) => {
            encoding::u8_value(bytes, 1);
            bytes.extend_from_slice(&value);
        }
        None => encoding::u8_value(bytes, 0),
    }
}

const fn session_phase(value: SessionPhase) -> u8 {
    match value {
        SessionPhase::Open => 1,
        SessionPhase::Paused => 2,
        SessionPhase::Closed => 3,
    }
}

const fn run_phase(value: RunPhase) -> u8 {
    match value {
        RunPhase::Pending => 1,
        RunPhase::Running => 2,
        RunPhase::Paused => 3,
        RunPhase::Reviewing => 4,
        RunPhase::Fixing => 5,
        RunPhase::Accepted => 6,
        RunPhase::Rejected => 7,
        RunPhase::Cancelled => 8,
        RunPhase::Failed => 9,
        RunPhase::Exhausted => 10,
    }
}

const fn acceptance_phase(value: AcceptancePhase) -> u8 {
    match value {
        AcceptancePhase::Pending => 1,
        AcceptancePhase::Evaluating => 2,
        AcceptancePhase::NeedsChanges => 3,
        AcceptancePhase::Accepted => 4,
        AcceptancePhase::Terminated => 5,
    }
}

const fn attempt_phase(value: AttemptPhase) -> u8 {
    match value {
        AttemptPhase::Active => 1,
        AttemptPhase::Submitted => 2,
        AttemptPhase::Reviewing => 3,
        AttemptPhase::Fixing => 4,
        AttemptPhase::Accepted => 5,
        AttemptPhase::Failed => 6,
        AttemptPhase::Cancelled => 7,
        AttemptPhase::Exhausted => 8,
    }
}

const fn turn_phase(value: TurnPhase) -> u8 {
    match value {
        TurnPhase::Active => 1,
        TurnPhase::Completed => 2,
        TurnPhase::Failed => 3,
        TurnPhase::Cancelled => 4,
    }
}

const fn action_phase(value: ActionPhase) -> u8 {
    match value {
        ActionPhase::Proposed => 1,
        ActionPhase::Authorized => 2,
        ActionPhase::Dispatched => 3,
        ActionPhase::Succeeded => 4,
        ActionPhase::Failed => 5,
        ActionPhase::Cancelled => 6,
    }
}

const fn review_phase(value: ReviewPhase) -> u8 {
    match value {
        ReviewPhase::Requested => 1,
        ReviewPhase::Active => 2,
        ReviewPhase::Submitted => 3,
        ReviewPhase::Invalidated => 4,
    }
}

const fn waiver_phase(value: WaiverPhase) -> u8 {
    match value {
        WaiverPhase::Requested => 1,
        WaiverPhase::Granted => 2,
        WaiverPhase::Denied => 3,
        WaiverPhase::Invalidated => 4,
    }
}

const fn role_tag(value: ActorRole) -> u8 {
    match value {
        ActorRole::Writer => 1,
        ActorRole::Fixer => 2,
        ActorRole::Reviewer => 3,
        ActorRole::Evaluator => 4,
        ActorRole::GateRunner => 5,
        ActorRole::Orchestrator => 6,
        ActorRole::EvolutionAgent => 7,
        ActorRole::HumanAuthority => 8,
        ActorRole::DaemonService => 9,
        ActorRole::ProviderToolWorker => 10,
        ActorRole::Plugin => 11,
    }
}
