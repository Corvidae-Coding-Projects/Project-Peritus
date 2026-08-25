//! Complete decoded-checkpoint semantic validation.

use peritus_types::RevisionTuple;

use super::OrchestratorState;
use crate::{
    ChildObservation, OrchestratorError, OrchestratorErrorKind, OrchestratorPhase,
    OrchestratorRecoveryAction,
};

pub(super) fn validate(state: &OrchestratorState) -> Result<(), OrchestratorError> {
    validate_nested(state)?;
    let canonical = state.active_children.windows(2).all(|pair| pair[0] < pair[1])
        && state.paused_children.windows(2).all(|pair| pair[0] < pair[1])
        && state.paused_children.iter().all(|kind| state.active_children.contains(kind));
    let history_valid = state.candidate_history.windows(2).all(|pair| {
        revision_successor(pair[0].revision(), pair[1].revision())
            && !pair[0].materially_equal(&pair[1])
            && !pair[0].reuses_material(&pair[1])
    }) && unique_material_history(state);
    let bounded = bounded_collections(state)?;
    let terminal_truth = terminal_truth(state);
    let digest_valid = state.state_digest.as_bytes().iter().any(|byte| *byte != 0)
        && state.state_digest == crate::canonical::state_digest(state);
    if canonical
        && history_valid
        && unique_cycle_history(state)
        && bounded
        && terminal_truth
        && digest_valid
    {
        Ok(())
    } else {
        Err(integrity("orchestrator checkpoint is noncanonical or internally inconsistent"))
    }
}

fn validate_nested(state: &OrchestratorState) -> Result<(), OrchestratorError> {
    state.binding.validate()?;
    let limits = state.binding.limits();
    state.ownership.validate(limits)?;
    state.current_candidate.validate(limits)?;
    state.current_quality_cycle.validate_for_candidate(&state.current_candidate)?;
    state.counters.validate(&state.binding)?;
    for candidate in &state.candidate_history {
        candidate.validate(limits)?;
    }
    for (candidate, cycle) in state.candidate_history.iter().zip(&state.quality_cycle_history) {
        cycle.validate_for_candidate(candidate)?;
    }
    let first_cycle = state
        .quality_cycle_history
        .first()
        .ok_or_else(|| integrity("quality-cycle history must retain genesis"))?;
    if first_cycle.gate_run_id() != state.binding.initial_gate_run_id()
        || first_cycle.scheduler_run_id() != state.binding.initial_scheduler_run_id()
        || first_cycle.collaboration_run_id() != state.binding.initial_collaboration_run_id()
        || first_cycle.scheduler_id() != state.binding.scheduler_id()
        || first_cycle.scheduler_binding_digest() != state.binding.scheduler_binding_digest()
        || first_cycle.collaboration_id() != state.binding.collaboration_id()
        || first_cycle.collaboration_binding_digest()
            != state.binding.collaboration_binding_digest()
    {
        return Err(integrity(
            "genesis child ownership differs from immutable orchestrator binding",
        ));
    }
    if let Some(candidate) = &state.proposed_candidate {
        candidate.validate(limits)?;
        if !revision_successor(state.current_candidate.revision(), candidate.revision()) {
            return Err(integrity("proposed candidate does not advance exact revision identity"));
        }
    }
    for handoff in &state.handoffs {
        handoff.validate(limits)?;
    }
    if let Some(handoff) = &state.open_handoff {
        handoff.validate(limits)?;
    }
    for activation in &state.activations {
        validate_activation(state, activation)?;
    }
    for observation in &state.observations {
        validate_observation(state, observation)?;
    }
    if let Some(directive) = &state.pending_directive {
        directive.validate()?;
    }
    if let Some(certificate) = &state.acceptance_certificate {
        certificate.validate()?;
    }
    if let Some(terminal) = state.terminal {
        terminal.validate()?;
    }
    if let Some(terminal) = state.pending_terminal {
        terminal.validate()?;
    }
    Ok(())
}

fn unique_material_history(state: &OrchestratorState) -> bool {
    state.candidate_history.iter().enumerate().all(|(index, candidate)| {
        state.candidate_history[..index]
            .iter()
            .all(|prior| prior.digest() != candidate.digest() && !prior.reuses_material(candidate))
    })
}

fn unique_cycle_history(state: &OrchestratorState) -> bool {
    state.quality_cycle_history.iter().enumerate().all(|(index, cycle)| {
        state.quality_cycle_history[..index].iter().all(|prior| {
            prior.gate_run_id() != cycle.gate_run_id()
                && prior.scheduler_run_id() != cycle.scheduler_run_id()
                && prior.collaboration_run_id() != cycle.collaboration_run_id()
                && prior.scheduler_id() != cycle.scheduler_id()
                && prior.collaboration_id() != cycle.collaboration_id()
                && prior.digest() != cycle.digest()
        })
    })
}

fn bounded_collections(state: &OrchestratorState) -> Result<bool, OrchestratorError> {
    let limits = state.binding.limits();
    Ok(state.candidate_history.len() == usize::from(state.counters.revisions)
        && state.quality_cycle_history.len() == state.candidate_history.len()
        && state.handoffs.len() == usize::from(state.counters.handoffs)
        && state.observations.len().saturating_add(state.activations.len())
            == usize::from(state.counters.retained_observations)
        && state.activations.len() <= usize::from(limits.retained_observations())
        && u16::try_from(state.used_commands.len()).is_ok()
        && state.current_candidate
            == *state
                .candidate_history
                .last()
                .ok_or_else(|| integrity("candidate history must retain its exact current tail"))?
        && state.current_quality_cycle
            == *state.quality_cycle_history.last().ok_or_else(|| {
                integrity("quality-cycle history must retain its exact current tail")
            })?
        && state
            .candidate_history
            .iter()
            .zip(&state.quality_cycle_history)
            .all(|(candidate, cycle)| candidate.revision() == cycle.revision()))
}

fn terminal_truth(state: &OrchestratorState) -> bool {
    (state.phase == OrchestratorPhase::Terminal) == state.terminal.is_some()
        && state.terminal.is_none_or(|terminal| {
            terminal.revision() == state.current_candidate.revision()
                && state.active_children.is_empty()
                && state.pending_directive.is_none()
                && state.open_handoff.is_none()
                && state.proposed_candidate.is_none()
                && state.pending_terminal.is_none()
                && state.paused_reconciliation.is_none()
                && state.paused_children.is_empty()
                && ((terminal.kind() == crate::OrchestratorTerminalKind::Cancelled)
                    == state.cancellation_cause.is_some())
        })
        && state.pending_terminal.is_none_or(|terminal| {
            state.phase == OrchestratorPhase::Cancelling
                && terminal.revision() == state.current_candidate.revision()
        })
        && crate::verified::terminal_is_truthful(state)
        && accepted_certificate_truth(state)
}

fn accepted_certificate_truth(state: &OrchestratorState) -> bool {
    let Some(terminal) = state.terminal else {
        return true;
    };
    if terminal.kind() != crate::OrchestratorTerminalKind::Accepted {
        return true;
    }
    let Some(certificate) = state.acceptance_certificate.as_ref() else {
        return false;
    };
    let gate_head = state.observations.iter().rev().find_map(|observation| match observation {
        ChildObservation::Gates(gate) if gate.revision() == certificate.revision() => {
            Some(gate.head().state_digest())
        }
        _ => None,
    });
    let review_head = state.observations.iter().rev().find_map(|observation| match observation {
        ChildObservation::Review(review) if review.revision() == certificate.revision() => {
            Some(review.head().state_digest())
        }
        _ => None,
    });
    [
        certificate.orchestrator_binding_digest() == state.binding.digest(),
        certificate.contract_id() == state.binding.contract_id(),
        certificate.contract_digest() == state.binding.contract_digest(),
        certificate.revision() == state.current_candidate.revision(),
        certificate.candidate_binding_digest() == state.current_candidate.digest(),
        certificate.maximum_gate_attempts() == state.binding.contract_gate_cycles(),
        certificate.maximum_review_cycles() == state.binding.contract_review_cycles(),
        gate_head == Some(certificate.gate_state_digest()),
        review_head == Some(certificate.review_state_digest()),
    ]
    .into_iter()
    .all(|exact| exact)
}

fn validate_activation(
    state: &OrchestratorState,
    activation: &crate::HandoffActivationObservation,
) -> Result<(), OrchestratorError> {
    let handoff = state
        .handoffs
        .iter()
        .find(|handoff| handoff.id() == activation.handoff_id())
        .ok_or_else(|| integrity("D3 activation names an unretained handoff"))?;
    let cycle = state
        .quality_cycle_history
        .iter()
        .find(|cycle| cycle.revision() == activation.revision())
        .ok_or_else(|| integrity("D3 activation revision has no retained child cycle"))?;
    let role = handoff
        .destination_role()
        .harness_role()
        .ok_or_else(|| integrity("D3 activation handoff destination is not executable"))?;
    let exact = [
        activation.task_id() == handoff.task_id(),
        activation.work_id() == handoff.work_id(),
        activation.owner() == handoff.destination_actor(),
        activation.role() == role,
        activation.scheduler_run_id() == cycle.scheduler_run_id(),
        activation.collaboration_run_id() == cycle.collaboration_run_id(),
        activation.revision() == handoff.candidate().revision(),
    ]
    .into_iter()
    .all(|part| part);
    if exact {
        Ok(())
    } else {
        Err(integrity("D3 activation differs from its retained handoff or child cycle"))
    }
}

fn validate_observation(
    state: &OrchestratorState,
    observation: &ChildObservation,
) -> Result<(), OrchestratorError> {
    let cycle = state
        .quality_cycle_history
        .iter()
        .find(|cycle| cycle.revision() == observation.revision())
        .ok_or_else(|| integrity("child observation revision has no retained quality cycle"))?;
    let candidate = state
        .candidate_history
        .iter()
        .find(|candidate| candidate.revision() == observation.revision())
        .ok_or_else(|| integrity("child observation revision has no retained candidate"))?;
    let exact = match observation {
        ChildObservation::Agent(agent) => state.handoffs.iter().any(|handoff| {
            [
                handoff.id() == agent.handoff_id(),
                handoff.task_id() == agent.task_id(),
                handoff.work_id() == agent.work_id(),
                handoff.turn_id() == Some(agent.turn_id()),
                handoff.destination_actor() == agent.actor(),
                agent.run_id() == state.binding.run_id(),
            ]
            .into_iter()
            .all(|part| part)
        }),
        ChildObservation::Gates(gate) => {
            gate.run_id() == state.binding.run_id()
                && gate.gate_run_id() == cycle.gate_run_id()
                && gate.plan_digest() == cycle.gate_plan_digest()
                && gate_snapshot_matches(gate.snapshot_digest(), candidate)
        }
        ChildObservation::Review(review) => {
            review.run_id() == state.binding.run_id()
                && review.binding_digest() == cycle.review_binding_digest()
        }
        ChildObservation::ReviewFixer(review) => {
            review.run_id() == state.binding.run_id()
                && review.binding_digest() == cycle.review_binding_digest()
                && state.handoffs.iter().any(|handoff| handoff.id() == review.handoff_id())
        }
        ChildObservation::Scheduler(child) => child.run_id() == cycle.scheduler_run_id(),
        ChildObservation::Collaboration(child) => child.run_id() == cycle.collaboration_run_id(),
        ChildObservation::HandoffActivation(activation) => {
            validate_activation(state, activation).is_ok()
        }
        ChildObservation::KernelAcceptance(kernel) => kernel.run_id() == state.binding.run_id(),
        ChildObservation::CancellationClassification(classification) => {
            classification.revision() == cycle.revision()
                && classification.evidence_digest().as_bytes().iter().any(|byte| *byte != 0)
        }
    };
    let head_valid = observation
        .head()
        .is_none_or(|head| head.state_digest().as_bytes().iter().any(|byte| *byte != 0));
    if exact && head_valid {
        Ok(())
    } else {
        Err(integrity("child observation is stale or differs from retained E0 bindings"))
    }
}

fn gate_snapshot_matches(
    snapshot: peritus_types::Sha256Digest,
    candidate: &crate::CandidateBinding,
) -> bool {
    snapshot == candidate.quality_snapshot_digest()
}

pub fn revision_successor(before: RevisionTuple, after: RevisionTuple) -> bool {
    before.acceptance_spec_id() == after.acceptance_spec_id()
        && before.harness_id() == after.harness_id()
        && before.workspace_id() == after.workspace_id()
        && before.policy_id() == after.policy_id()
        && before.provider_profile_id() == after.provider_profile_id()
        && (after.workspace_generation() > before.workspace_generation()
            || (after.workspace_generation() == before.workspace_generation()
                && after.workspace_revision() > before.workspace_revision()))
}

const fn integrity(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::Integrity,
        OrchestratorRecoveryAction::Quarantine,
        detail,
    )
}

#[cfg(test)]
mod tests {
    use peritus_types::{
        AcceptanceSpecId, ActorId, Generation, HarnessId, PolicyId, ProviderProfileId,
        RevisionNumber, RevisionTuple, Sha256Digest, SnapshotId, WorkspaceId,
    };

    use super::gate_snapshot_matches;
    use crate::{CandidateBinding, OrchestratorLimits};

    #[test]
    fn historical_gate_snapshot_must_equal_candidate_quality_snapshot() {
        let revision = RevisionTuple::new(
            AcceptanceSpecId::new([1; 16]).expect("acceptance id is nonzero"),
            HarnessId::new([2; 16]).expect("harness id is nonzero"),
            WorkspaceId::new([3; 16]).expect("workspace id is nonzero"),
            Generation::first(),
            RevisionNumber::first(),
            PolicyId::new([4; 16]).expect("policy id is nonzero"),
            ProviderProfileId::new([5; 16]).expect("provider id is nonzero"),
        );
        let limits = OrchestratorLimits::new(4, 4, 4, 4, 4, 16, 16, 32, 8, 16, 65_536, 262_144)
            .expect("fixture limits are valid");
        let candidate = CandidateBinding::new(
            revision,
            SnapshotId::new([6; 16]).expect("snapshot id is nonzero"),
            Sha256Digest::new([7; 32]),
            Sha256Digest::new([8; 32]),
            Sha256Digest::new([9; 32]),
            None,
            None,
            vec![ActorId::new([10; 16]).expect("producer id is nonzero")],
            vec![Sha256Digest::new([11; 32])],
            limits,
        )
        .expect("candidate is valid");

        assert!(gate_snapshot_matches(Sha256Digest::new([9; 32]), &candidate));
        assert!(!gate_snapshot_matches(Sha256Digest::new([12; 32]), &candidate));
    }
}
