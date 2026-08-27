//! Pause/resume and cancellation-dominance settlement matrices.

#![allow(clippy::unwrap_used, reason = "fixed checked cancellation fixtures")]

use crate::{
    CancellationChildClassification, ChildAggregateKind, ChildObservation, DirectiveDestination,
    DirectiveKind, DirectivePayloadBinding, OrchestratorCommandKind, OrchestratorPhase,
    OrchestratorTerminalKind, ResumeReconciliation, cancellation_dominates,
    directive_payload_digest, terminal_is_truthful,
};

use super::support::{Scenario, acknowledge, activate_open_handoff, digest, head, publish};

#[test]
fn pause_acknowledges_every_live_child_and_resumes_only_exact_heads() {
    let mut scenario = Scenario::new();
    activate_open_handoff(&mut scenario, 700);
    let heads = scenario
        .state()
        .active_children()
        .iter()
        .enumerate()
        .map(|(index, kind)| head(*kind, 710 + u16::try_from(index).unwrap(), None))
        .collect::<Vec<_>>();
    let reconciliation = ResumeReconciliation::from_checkpoint(scenario.state(), heads).unwrap();
    scenario.apply_ok(OrchestratorCommandKind::Pause { reconciliation: reconciliation.clone() });
    assert!(matches!(scenario.state().phase(), OrchestratorPhase::Paused(_)));

    let active = scenario.state().active_children().to_vec();
    for child in active {
        let destination = destination(child);
        let payload = directive_payload_digest(
            DirectiveKind::PauseChildren,
            destination,
            DirectivePayloadBinding::Reconciliation(&reconciliation),
        )
        .unwrap();
        let id = publish(
            &mut scenario,
            destination,
            DirectiveKind::PauseChildren,
            payload,
            None,
            None,
            None,
        );
        acknowledge(&mut scenario, id);
    }
    scenario.apply_ok(OrchestratorCommandKind::Resume { reconciliation: reconciliation.clone() });
    assert_eq!(
        scenario.state().phase(),
        OrchestratorPhase::Active(crate::ActivePhase::WriterActive)
    );
    assert!(scenario.state().paused_reconciliation().is_some());
    assert!(scenario.apply(OrchestratorCommandKind::Fail { cause_digest: digest(711) }).is_err());

    let paused = scenario.state().paused_children().to_vec();
    for child in paused {
        let destination = destination(child);
        let payload = directive_payload_digest(
            DirectiveKind::ResumeChildren,
            destination,
            DirectivePayloadBinding::Reconciliation(&reconciliation),
        )
        .unwrap();
        let id = publish(
            &mut scenario,
            destination,
            DirectiveKind::ResumeChildren,
            payload,
            None,
            None,
            None,
        );
        acknowledge(&mut scenario, id);
    }
    assert!(scenario.state().paused_children().is_empty());
    assert!(scenario.state().paused_reconciliation().is_none());
    let replayed = crate::replay(&scenario.events()).unwrap();
    assert_eq!(&replayed, scenario.state());

    let mut stale = scenario.clone();
    let fresh_heads = stale
        .state()
        .active_children()
        .iter()
        .enumerate()
        .map(|(index, kind)| head(*kind, 715 + u16::try_from(index).unwrap(), None))
        .collect::<Vec<_>>();
    let fresh = ResumeReconciliation::from_checkpoint(stale.state(), fresh_heads).unwrap();
    stale.apply_ok(OrchestratorCommandKind::Pause { reconciliation: fresh });
    let wrong = ResumeReconciliation::from_wire(stale.state().state_digest(), Vec::new());
    assert!(wrong.is_ok());
    assert!(
        stale.apply(OrchestratorCommandKind::Resume { reconciliation: wrong.unwrap() }).is_err()
    );
}

#[test]
fn cancellation_settles_owned_children_and_late_success_cannot_win() {
    let mut scenario = Scenario::new();
    activate_open_handoff(&mut scenario, 720);
    let cause = digest(730);
    scenario.apply_ok(OrchestratorCommandKind::Cancel { cause_digest: cause });
    assert_eq!(scenario.state().phase(), OrchestratorPhase::Cancelling);
    assert!(cancellation_dominates(scenario.state()));

    let active = scenario.state().active_children().to_vec();
    for (index, child) in active.into_iter().enumerate() {
        let destination = destination(child);
        let payload = directive_payload_digest(
            DirectiveKind::CancelChildren,
            destination,
            DirectivePayloadBinding::Cancellation(cause),
        )
        .unwrap();
        let id = publish(
            &mut scenario,
            destination,
            DirectiveKind::CancelChildren,
            payload,
            None,
            None,
            None,
        );
        acknowledge(&mut scenario, id);
        let classification = CancellationChildClassification::unreachable(
            child,
            scenario.state().current_candidate().revision(),
            digest(740 + u16::try_from(index).unwrap()),
        )
        .unwrap();
        scenario.apply_ok(OrchestratorCommandKind::ReconcileCancellation {
            observation: ChildObservation::CancellationClassification(classification),
        });
    }
    scenario.apply_ok(OrchestratorCommandKind::Finalize);
    assert_eq!(
        scenario.state().terminal().map(|terminal| terminal.kind()),
        Some(OrchestratorTerminalKind::Cancelled)
    );
    assert!(cancellation_dominates(scenario.state()));
    assert!(terminal_is_truthful(scenario.state()));
    assert!(scenario.apply(OrchestratorCommandKind::Fail { cause_digest: digest(760) }).is_err());
}

#[test]
fn ambiguous_child_settlement_truthfully_requires_human_judgment() {
    let mut scenario = Scenario::new();
    activate_open_handoff(&mut scenario, 770);
    let cause = digest(780);
    scenario.apply_ok(OrchestratorCommandKind::Cancel { cause_digest: cause });
    let active = scenario.state().active_children().to_vec();
    for (index, child) in active.into_iter().enumerate() {
        let destination = destination(child);
        let payload = directive_payload_digest(
            DirectiveKind::CancelChildren,
            destination,
            DirectivePayloadBinding::Cancellation(cause),
        )
        .unwrap();
        let id = publish(
            &mut scenario,
            destination,
            DirectiveKind::CancelChildren,
            payload,
            None,
            None,
            None,
        );
        acknowledge(&mut scenario, id);
        let evidence = digest(790 + u16::try_from(index).unwrap());
        let classification = if index == 0 {
            CancellationChildClassification::ambiguous(
                child,
                scenario.state().current_candidate().revision(),
                evidence,
            )
        } else {
            CancellationChildClassification::unreachable(
                child,
                scenario.state().current_candidate().revision(),
                evidence,
            )
        }
        .unwrap();
        scenario.apply_ok(OrchestratorCommandKind::ReconcileCancellation {
            observation: ChildObservation::CancellationClassification(classification),
        });
    }
    scenario.apply_ok(OrchestratorCommandKind::Finalize);
    assert_eq!(
        scenario.state().terminal().map(|terminal| terminal.kind()),
        Some(OrchestratorTerminalKind::NeedsHuman)
    );
}

const fn destination(child: ChildAggregateKind) -> DirectiveDestination {
    match child {
        ChildAggregateKind::Agent => DirectiveDestination::Agent,
        ChildAggregateKind::Gates => DirectiveDestination::Gates,
        ChildAggregateKind::Review => DirectiveDestination::Review,
        ChildAggregateKind::Scheduler => DirectiveDestination::Scheduler,
        ChildAggregateKind::Collaboration => DirectiveDestination::Collaboration,
        ChildAggregateKind::Kernel => DirectiveDestination::Kernel,
    }
}
