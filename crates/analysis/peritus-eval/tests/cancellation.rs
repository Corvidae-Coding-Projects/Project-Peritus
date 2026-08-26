//! Per-rollout cancellation reconciliation and terminal-dominance coverage.

mod support;

use peritus_eval::{
    EvaluationCommand, EvaluationCommandKind, EvaluationPhase, EvaluationPlan, EvaluationState,
    PlanBatch, PlanRecord, PlannedRolloutBinding, RolloutStatus, decide,
};
use peritus_scheduler::{
    ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerLimits,
};
use peritus_types::{ActorId, CommandId, EventId};

use support::{artifact, bytes, campaign_id, digest, frozen_profile, revision};

#[test]
#[allow(clippy::too_many_lines, reason = "one linear scenario proves cancellation dominance")]
fn cancellation_waits_for_each_external_rollout_before_terminal_completion() {
    let profile = frozen_profile();
    let plan = EvaluationPlan::build(campaign_id(), &profile).expect("plan");
    let genesis = EvaluationCommand::new(
        CommandId::new(bytes(100)).expect("command"),
        EventId::new(bytes(101)).expect("event"),
        campaign_id(),
        0,
        None,
        digest(0),
        profile.digest(),
        EvaluationCommandKind::CreateCampaign {
            revision: revision(),
            dataset_digest: profile.dataset().digest(),
            dataset_artifact: artifact(102),
            profile_artifact: artifact(103),
        },
    )
    .expect("genesis");
    let mut state = decide(None, &genesis).expect("create").state().clone();

    let mut bindings: Vec<_> = plan
        .specs()
        .iter()
        .map(|spec| PlannedRolloutBinding::new(spec.id(), spec.work_id(), spec.request_digest()))
        .collect();
    bindings.sort_unstable_by_key(|binding| binding.rollout_id());
    state = advance(
        &state,
        104,
        EvaluationCommandKind::RecordPlanBatch {
            plan_id: plan.id(),
            plan_digest: plan.digest(),
            batch: PlanBatch::new(1, 1, artifact(105), bindings).expect("batch"),
        },
        profile.digest(),
    );
    state = advance(
        &state,
        106,
        EvaluationCommandKind::CompletePlan {
            plan: PlanRecord::new(plan.id(), plan.digest(), artifact(107), 8, 1)
                .expect("plan record"),
        },
        profile.digest(),
    );

    let rollout = plan.specs()[0].id();
    let work = plan.specs()[0]
        .work_spec(
            ActorId::new(bytes(108)).expect("owner"),
            revision(),
            resources(),
            3,
            scheduler_limits(),
        )
        .expect("work");
    state = advance(
        &state,
        109,
        EvaluationCommandKind::RequestSchedule { rollout_id: rollout, work },
        profile.digest(),
    );
    state = advance(
        &state,
        111,
        EvaluationCommandKind::CancelCampaign { reason_digest: digest(112) },
        profile.digest(),
    );
    assert_eq!(state.phase(), EvaluationPhase::Cancelling);

    let premature =
        command(&state, 113, EvaluationCommandKind::CompleteCancellation, profile.digest());
    assert!(decide(Some(&state), &premature).is_err());

    state = advance(
        &state,
        115,
        EvaluationCommandKind::SettleCancellation {
            rollout_id: rollout,
            observation_digest: digest(116),
        },
        profile.digest(),
    );
    assert_eq!(
        state.rollout(rollout).expect("rollout").status(),
        RolloutStatus::Cancelled { reason_digest: digest(112), observation_digest: digest(116) }
    );

    state = advance(&state, 117, EvaluationCommandKind::CompleteCancellation, profile.digest());
    assert_eq!(state.phase(), EvaluationPhase::Cancelled);
    assert!(state.counts().complete());

    let late = command(
        &state,
        119,
        EvaluationCommandKind::StartAnalysis { counts: state.counts() },
        profile.digest(),
    );
    assert!(decide(Some(&state), &late).is_err());
}

fn advance(
    prior: &EvaluationState,
    seed: u8,
    kind: EvaluationCommandKind,
    profile: peritus_eval::ProfileDigest,
) -> EvaluationState {
    let command = command(prior, seed, kind, profile);
    decide(Some(prior), &command).expect("legal transition").state().clone()
}

fn command(
    prior: &EvaluationState,
    seed: u8,
    kind: EvaluationCommandKind,
    profile: peritus_eval::ProfileDigest,
) -> EvaluationCommand {
    EvaluationCommand::new(
        CommandId::new(bytes(seed)).expect("command"),
        EventId::new(bytes(seed.saturating_add(1))).expect("event"),
        campaign_id(),
        prior.sequence(),
        Some(prior.last_event_id()),
        prior.state_digest(),
        profile,
        kind,
    )
    .expect("command")
}

fn resources() -> ResourceVector {
    ResourceVector::new(
        vec![ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(1).expect("quantity"))],
        4,
    )
    .expect("resources")
}

fn scheduler_limits() -> SchedulerLimits {
    SchedulerLimits::new(10, 10, 2, 4, 4, 2, 3, 8, 2, 1_024, 1_048_576).expect("scheduler limits")
}
