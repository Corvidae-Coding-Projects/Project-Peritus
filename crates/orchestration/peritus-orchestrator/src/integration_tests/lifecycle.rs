//! Complete success, fix-cycle, replay, and terminal-truth routes.

#![allow(clippy::unwrap_used, reason = "fixed checked lifecycle fixtures")]

use crate::{
    CandidateBinding, OrchestratorBinding, OrchestratorCommandKind, OrchestratorPhase,
    OrchestratorTerminalKind, QualityCycleBinding, TerminalCause, candidate_cycles_are_fresh,
    counters_are_bounded, replay, replay_equivalent, roles_are_separated, terminal_is_truthful,
    transition_is_legal,
};

use super::support::{Scenario, bytes, digest, fix_cycle_path, happy_path, next_revision};

#[test]
fn writer_gates_review_b2_and_durable_b0_are_the_only_success_path() {
    let scenario = happy_path();
    let state = scenario.state();

    assert_eq!(state.phase(), OrchestratorPhase::Terminal);
    let terminal = state.terminal().copied().expect("accepted terminal");
    assert_eq!(terminal.kind(), OrchestratorTerminalKind::Accepted);
    assert_eq!(terminal.cause(), TerminalCause::KernelAccepted);
    assert!(roles_are_separated(state));
    assert!(candidate_cycles_are_fresh(state));
    assert!(counters_are_bounded(state));
    assert!(terminal_is_truthful(state));

    let rebuilt = replay(&scenario.events()).expect("exact event replay");
    assert!(replay_equivalent(state, &rebuilt));
    for pair in scenario.steps().windows(2) {
        assert!(transition_is_legal(pair[0].1.state(), &pair[1].1));
    }

    let mut closed = scenario.clone();
    let error = closed
        .apply(OrchestratorCommandKind::Reject { cause_digest: digest(900) })
        .expect_err("terminal aggregate rejects all later commands");
    assert_eq!(error.kind(), crate::OrchestratorErrorKind::InvalidTransition);
}

#[test]
fn blocking_finding_fixer_revision_and_fresh_quality_cycle_reach_acceptance() {
    let scenario = fix_cycle_path();
    let state = scenario.state();

    assert_eq!(state.candidate_history().len(), 2);
    assert_eq!(state.quality_cycle_history().len(), 2);
    assert_eq!(state.counters().revisions(), 2);
    assert_eq!(state.counters().fixer_cycles(), 1);
    assert!(candidate_cycles_are_fresh(state));
    assert!(terminal_is_truthful(state));
    assert_eq!(
        state.terminal().map(|terminal| terminal.kind()),
        Some(OrchestratorTerminalKind::Accepted)
    );
    assert_eq!(replay(&scenario.events()).unwrap(), state.clone());
}

#[test]
fn explicit_rejection_failure_and_exhaustion_preserve_distinct_truth() {
    let cases = [
        (
            OrchestratorCommandKind::Reject { cause_digest: digest(910) },
            OrchestratorTerminalKind::Rejected,
            TerminalCause::ExplicitRejection,
        ),
        (
            OrchestratorCommandKind::Fail { cause_digest: digest(911) },
            OrchestratorTerminalKind::Failed,
            TerminalCause::ExplicitFailure,
        ),
        (
            OrchestratorCommandKind::Exhaust { cause_digest: digest(912) },
            OrchestratorTerminalKind::Exhausted,
            TerminalCause::ExplicitExhaustion,
        ),
    ];

    for (command, kind, cause) in cases {
        let mut scenario = Scenario::new();
        scenario.apply_ok(command);
        let terminal = scenario.state().terminal().copied().expect("terminal truth");
        assert_eq!(terminal.kind(), kind);
        assert_eq!(terminal.cause(), cause);
        assert!(terminal_is_truthful(scenario.state()));
        assert_eq!(replay(&scenario.events()).unwrap(), scenario.state().clone());
    }
}

#[test]
fn one_field_contract_candidate_artifact_and_child_binding_drift_is_rejected() {
    let scenario = Scenario::new();
    let state = scenario.state();
    let binding = state.binding();
    for field in [
        BindingField::Contract,
        BindingField::Revision,
        BindingField::Gate,
        BindingField::Review,
        BindingField::Scheduler,
        BindingField::Collaboration,
    ] {
        assert!(tampered_binding(binding, field).validate().is_err(), "{field:?}");
    }

    let candidate = state.current_candidate();
    for field in [CandidateField::Candidate, CandidateField::Tree, CandidateField::Artifact] {
        assert!(
            tampered_candidate(candidate, field).validate(state.limits()).is_err(),
            "{field:?}"
        );
    }

    let cycle = state.current_quality_cycle();
    for field in
        [CycleField::Gate, CycleField::Review, CycleField::Scheduler, CycleField::Collaboration]
    {
        assert!(tampered_cycle(cycle, field).validate().is_err(), "{field:?}");
    }
}

#[derive(Clone, Copy, Debug)]
enum BindingField {
    Contract,
    Revision,
    Gate,
    Review,
    Scheduler,
    Collaboration,
}

fn tampered_binding(value: &OrchestratorBinding, field: BindingField) -> OrchestratorBinding {
    OrchestratorBinding::from_wire(
        value.id(),
        value.run_id(),
        value.attempt_id(),
        value.contract_id(),
        if matches!(field, BindingField::Contract) { digest(920) } else { value.contract_digest() },
        if matches!(field, BindingField::Revision) {
            next_revision(value.initial_revision())
        } else {
            value.initial_revision()
        },
        value.initial_gate_run_id(),
        value.initial_scheduler_run_id(),
        value.initial_collaboration_run_id(),
        value.contract_gate_cycles(),
        value.contract_review_cycles(),
        if matches!(field, BindingField::Gate) { digest(921) } else { value.gate_plan_digest() },
        if matches!(field, BindingField::Review) {
            digest(922)
        } else {
            value.review_binding_digest()
        },
        value.scheduler_id(),
        if matches!(field, BindingField::Scheduler) {
            digest(923)
        } else {
            value.scheduler_binding_digest()
        },
        value.collaboration_id(),
        if matches!(field, BindingField::Collaboration) {
            digest(924)
        } else {
            value.collaboration_binding_digest()
        },
        value.limits(),
        value.digest(),
    )
}

#[derive(Clone, Copy, Debug)]
enum CandidateField {
    Candidate,
    Tree,
    Artifact,
}

fn tampered_candidate(value: &CandidateBinding, field: CandidateField) -> CandidateBinding {
    let artifact = matches!(field, CandidateField::Artifact)
        .then(|| peritus_types::ArtifactId::new(bytes(925)).unwrap());
    CandidateBinding::from_wire(
        value.revision(),
        value.snapshot_id(),
        if matches!(field, CandidateField::Candidate) {
            digest(926)
        } else {
            value.candidate_digest()
        },
        if matches!(field, CandidateField::Tree) { digest(927) } else { value.tree_digest() },
        value.quality_snapshot_digest(),
        artifact,
        artifact.map(|_| digest(928)),
        value.producer_actors().to_vec(),
        value.producer_ancestries().to_vec(),
        value.digest(),
    )
}

#[derive(Clone, Copy, Debug)]
enum CycleField {
    Gate,
    Review,
    Scheduler,
    Collaboration,
}

fn tampered_cycle(value: &QualityCycleBinding, field: CycleField) -> QualityCycleBinding {
    QualityCycleBinding::from_wire(
        value.revision(),
        value.gate_run_id(),
        value.scheduler_run_id(),
        value.collaboration_run_id(),
        if matches!(field, CycleField::Gate) { digest(929) } else { value.gate_plan_digest() },
        if matches!(field, CycleField::Review) {
            digest(930)
        } else {
            value.review_binding_digest()
        },
        value.scheduler_id(),
        if matches!(field, CycleField::Scheduler) {
            digest(931)
        } else {
            value.scheduler_binding_digest()
        },
        value.collaboration_id(),
        if matches!(field, CycleField::Collaboration) {
            digest(932)
        } else {
            value.collaboration_binding_digest()
        },
        value.digest(),
    )
}
