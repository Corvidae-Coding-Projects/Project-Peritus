use peritus_types::Sha256Digest;

use super::{
    STARTUP_KILL_MATRIX, STARTUP_PHASE_COUNT, STARTUP_PHASES, StartupFailpoint,
    StartupFailpointBoundary, StartupFailpoints, StartupNextAction, StartupPlan, StartupPlanError,
};
use crate::StartupPhase;

#[test]
fn checkpoint_prefix_retains_exact_digests_and_next_actions() {
    let mut plan = StartupPlan::new(StartupFailpoints::none());
    assert!(plan.checkpoints().is_empty());

    for (index, expected) in STARTUP_PHASES.iter().copied().enumerate() {
        assert_eq!(plan.begin_next().expect("phase begins"), Some(expected));
        assert_eq!(plan.active_phase(), Some(expected));
        let input = digest(index);
        let output = digest(index + STARTUP_PHASE_COUNT);
        let checkpoint =
            plan.complete_active(expected, input, output).expect("canonical phase checkpoints");
        assert_eq!(usize::from(checkpoint.ordinal()), index + 1);
        assert_eq!(checkpoint.phase(), expected);
        assert_eq!(checkpoint.input_digest(), input);
        assert_eq!(checkpoint.output_digest(), output);
        let expected_action = STARTUP_PHASES
            .get(index + 1)
            .copied()
            .map_or(StartupNextAction::Complete, StartupNextAction::Execute);
        assert_eq!(checkpoint.next_action(), expected_action);
    }

    assert_eq!(plan.begin_next().expect("complete plan has no next phase"), None);
    assert!(plan.checkpoints().is_complete());
    assert_eq!(usize::from(plan.checkpoints().len()), STARTUP_PHASE_COUNT);
    assert_eq!(plan.checkpoints().iter().count(), STARTUP_PHASE_COUNT);
    assert_eq!(
        plan.checkpoints().last().map(|checkpoint| checkpoint.phase()),
        Some(StartupPhase::Ready)
    );
}

#[test]
fn wrong_completion_is_typed_and_does_not_advance() {
    let mut plan = StartupPlan::new(StartupFailpoints::none());
    assert_eq!(
        plan.complete_active(StartupPhase::Validate, digest(0), digest(1)),
        Err(StartupPlanError::NoActivePhase { observed: StartupPhase::Validate })
    );
    assert_eq!(plan.begin_next().expect("validation begins"), Some(StartupPhase::Validate));
    assert_eq!(
        plan.begin_next(),
        Err(StartupPlanError::PhaseAlreadyActive { phase: StartupPhase::Validate })
    );
    assert_eq!(
        plan.complete_active(StartupPhase::Lock, digest(0), digest(1)),
        Err(StartupPlanError::UnexpectedPhase {
            expected: StartupPhase::Validate,
            observed: StartupPhase::Lock,
        })
    );
    assert_eq!(plan.checkpoints().len(), 0);
    assert!(plan.complete_active(StartupPhase::Validate, digest(0), digest(1)).is_ok());
    assert_eq!(plan.checkpoints().len(), 1);
    assert_eq!(StartupPlanError::AlreadyComplete.code(), "PERITUS-STARTUP-COMPLETE-001");
}

#[test]
fn fixed_failpoint_set_tracks_both_boundaries_without_allocation() {
    let before = StartupFailpoint::before(StartupPhase::DomainRecovery);
    let after = StartupFailpoint::after(StartupPhase::DomainRecovery);
    let points = StartupFailpoints::none().with(before).with(after);
    assert!(points.contains(before));
    assert!(points.contains(after));
    assert!(!points.contains(StartupFailpoint::before(StartupPhase::EffectRecovery)));
    assert_eq!(before.phase(), StartupPhase::DomainRecovery);
    assert_eq!(before.boundary(), StartupFailpointBoundary::Before);
    assert_eq!(after.boundary(), StartupFailpointBoundary::After);
}

#[test]
fn every_kill_point_preserves_the_exact_restart_prefix() {
    assert_eq!(STARTUP_KILL_MATRIX.len(), STARTUP_PHASE_COUNT * 2);
    for failpoint in STARTUP_KILL_MATRIX {
        let mut plan = StartupPlan::new(StartupFailpoints::single(failpoint));
        let failure = drive_to_injection(&mut plan);
        let expected_completed = STARTUP_PHASES
            .iter()
            .position(|phase| *phase == failpoint.phase())
            .expect("failpoint phase belongs to canonical startup")
            + usize::from(failpoint.boundary() == StartupFailpointBoundary::After);
        assert_eq!(usize::from(failure.completed_checkpoints()), expected_completed);
        assert_eq!(failure.failpoint(), failpoint);
        assert_eq!(failure.restart_phase(), failpoint.restart_phase());
        assert_eq!(plan.halted(), Some(failure));
        assert_eq!(plan.begin_next(), Err(StartupPlanError::Halted(failure)));

        let mut restarted = StartupPlan::resume(plan.checkpoints(), StartupFailpoints::none());
        assert_eq!(restarted.checkpoints().next_phase(), failpoint.restart_phase());
        complete_plan(&mut restarted);
        assert!(restarted.checkpoints().is_complete());
    }
}

fn drive_to_injection(plan: &mut StartupPlan) -> super::StartupInjection {
    loop {
        let phase = match plan.begin_next() {
            Ok(Some(phase)) => phase,
            Err(StartupPlanError::Injected(injection)) => return injection,
            result => panic!("expected injected startup failure, got {result:?}"),
        };
        let index = STARTUP_PHASES
            .iter()
            .position(|candidate| *candidate == phase)
            .expect("active phase belongs to canonical startup");
        match plan.complete_active(phase, digest(index), digest(index + STARTUP_PHASE_COUNT)) {
            Ok(_) => {}
            Err(StartupPlanError::Injected(injection)) => return injection,
            result => panic!("expected checkpoint or injected failure, got {result:?}"),
        }
    }
}

fn complete_plan(plan: &mut StartupPlan) {
    while let Some(phase) = plan.begin_next().expect("restart begins canonical phase") {
        let index = STARTUP_PHASES
            .iter()
            .position(|candidate| *candidate == phase)
            .expect("active phase belongs to canonical startup");
        plan.complete_active(phase, digest(index), digest(index + STARTUP_PHASE_COUNT))
            .expect("restart checkpoints canonical phase");
    }
}

fn digest(seed: usize) -> Sha256Digest {
    let byte = u8::try_from(seed).expect("test digest seed fits in one byte");
    Sha256Digest::new([byte; Sha256Digest::LENGTH])
}
