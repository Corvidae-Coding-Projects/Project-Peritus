//! Closed D2 command application separated from fence/replay orchestration.

mod finding_lifecycle;

use peritus_types::{EventId, ReviewCycleId, Sha256Digest};

use crate::error::{ReviewError, ReviewErrorKind, reject};
use crate::state::mutation;
use crate::{
    DispositionKind, DispositionRecord, ReviewCommandKind, ReviewCyclePhase, ReviewEventKind,
    ReviewRunPhase, ReviewRunState, ReviewTerminalKind,
};

use super::illegal;

#[allow(
    clippy::too_many_lines,
    reason = "the closed command-to-event dispatch table is clearest as one exhaustive match"
)]
pub(super) fn apply(
    state: &mut ReviewRunState,
    event_id: EventId,
    command: &ReviewCommandKind,
) -> Result<ReviewEventKind, ReviewError> {
    if state.phase() == ReviewRunPhase::Paused
        && !matches!(
            command,
            ReviewCommandKind::ResumeRun
                | ReviewCommandKind::CancelRun
                | ReviewCommandKind::ExhaustBudget { .. }
                | ReviewCommandKind::FailRun { .. }
        )
    {
        return Err(illegal("paused review run admits only resume or terminal control"));
    }
    match command {
        ReviewCommandKind::StartRun { .. } => Err(illegal("review run already started")),
        ReviewCommandKind::AdvanceRevision { binding } => advance_revision(state, binding),
        ReviewCommandKind::AssignReviewer { assignment } => assign(state, assignment),
        ReviewCommandKind::SubmitReview { submission } => submit(state, event_id, submission),
        ReviewCommandKind::ReconcileDuplicates { canonical, duplicates, reconciliation_digest } => {
            crate::reconciliation::reconcile_duplicates(
                state,
                event_id,
                *canonical,
                duplicates,
                *reconciliation_digest,
            )?;
            Ok(ReviewEventKind::DuplicatesReconciled {
                canonical: *canonical,
                duplicates: duplicates.clone(),
                reconciliation_digest: *reconciliation_digest,
            })
        }
        ReviewCommandKind::RecordFixerResponse { finding_id, response } => {
            finding_lifecycle::record_response(state, event_id, *finding_id, response, false)?;
            Ok(ReviewEventKind::FixerResponseRecorded {
                finding_id: *finding_id,
                response: response.clone(),
            })
        }
        ReviewCommandKind::ConfirmResolution {
            finding_id,
            reviewer_cycle,
            pending_response_digest,
            evidence,
            confirmation_digest,
        } => {
            finding_lifecycle::confirm(
                state,
                event_id,
                *finding_id,
                *reviewer_cycle,
                *pending_response_digest,
                evidence,
                *confirmation_digest,
                DispositionKind::Fixed,
                DispositionKind::ResolutionConfirmed,
                None,
            )?;
            Ok(ReviewEventKind::ResolutionConfirmed {
                finding_id: *finding_id,
                reviewer_cycle: *reviewer_cycle,
                pending_response_digest: *pending_response_digest,
                evidence: evidence.clone(),
                confirmation_digest: *confirmation_digest,
            })
        }
        ReviewCommandKind::ConfirmInvalidation {
            finding_id,
            reviewer_cycle,
            pending_response_digest,
            evidence,
            confirmation_digest,
        } => {
            finding_lifecycle::confirm(
                state,
                event_id,
                *finding_id,
                *reviewer_cycle,
                *pending_response_digest,
                evidence,
                *confirmation_digest,
                DispositionKind::Disputed,
                DispositionKind::InvalidationConfirmed,
                None,
            )?;
            Ok(ReviewEventKind::InvalidationConfirmed {
                finding_id: *finding_id,
                reviewer_cycle: *reviewer_cycle,
                pending_response_digest: *pending_response_digest,
                evidence: evidence.clone(),
                confirmation_digest: *confirmation_digest,
            })
        }
        ReviewCommandKind::ConfirmSupersession {
            finding_id,
            superseding,
            reviewer_cycle,
            pending_response_digest,
            evidence,
            confirmation_digest,
        } => {
            finding_lifecycle::validate_confirmation(
                state,
                *finding_id,
                *reviewer_cycle,
                *pending_response_digest,
                evidence,
                DispositionKind::SupersessionProposed,
                Some(*superseding),
            )?;
            crate::reconciliation::confirm_supersession(
                state,
                event_id,
                *finding_id,
                *superseding,
                *reviewer_cycle,
                evidence.clone(),
                *confirmation_digest,
            )?;
            Ok(ReviewEventKind::SupersessionConfirmed {
                finding_id: *finding_id,
                superseding: *superseding,
                reviewer_cycle: *reviewer_cycle,
                pending_response_digest: *pending_response_digest,
                evidence: evidence.clone(),
                confirmation_digest: *confirmation_digest,
            })
        }
        ReviewCommandKind::RequestWaiver { finding_id, request } => {
            finding_lifecycle::record_response(state, event_id, *finding_id, request, true)?;
            Ok(ReviewEventKind::WaiverRequested {
                finding_id: *finding_id,
                request: request.clone(),
            })
        }
        ReviewCommandKind::ObserveWaiver { waiver } => {
            finding_lifecycle::observe_waiver(state, event_id, *waiver)
        }
        ReviewCommandKind::CancelCycle { cycle_id } => cancel_cycle(state, *cycle_id),
        ReviewCommandKind::CancelRun => {
            terminate(state, ReviewTerminalKind::Cancelled, Sha256Digest::new([0; 32]));
            Ok(ReviewEventKind::RunCancelled)
        }
        ReviewCommandKind::PauseRun => pause(state),
        ReviewCommandKind::ResumeRun => resume(state),
        ReviewCommandKind::ExhaustBudget { reason_digest } => {
            terminate(state, ReviewTerminalKind::NeedsHuman, *reason_digest);
            Ok(ReviewEventKind::BudgetExhausted { reason_digest: *reason_digest })
        }
        ReviewCommandKind::FailRun { failure_digest } => {
            terminate(state, ReviewTerminalKind::Failed, *failure_digest);
            Ok(ReviewEventKind::RunFailed { failure_digest: *failure_digest })
        }
        ReviewCommandKind::FinalizeRun => finalize(state),
    }
}

fn pause(state: &mut ReviewRunState) -> Result<ReviewEventKind, ReviewError> {
    if state.phase() != ReviewRunPhase::Active {
        return Err(illegal("only an active review run can pause"));
    }
    mutation::set_phase(state, ReviewRunPhase::Paused);
    Ok(ReviewEventKind::RunPaused)
}

fn resume(state: &mut ReviewRunState) -> Result<ReviewEventKind, ReviewError> {
    if state.phase() != ReviewRunPhase::Paused {
        return Err(illegal("only a paused review run can resume"));
    }
    mutation::set_phase(state, ReviewRunPhase::Active);
    Ok(ReviewEventKind::RunResumed)
}

fn advance_revision(
    state: &mut ReviewRunState,
    binding: &crate::ReviewBinding,
) -> Result<ReviewEventKind, ReviewError> {
    binding.validate(state.limits())?;
    let current = state.binding();
    if binding.contract_id() != current.contract_id()
        || binding.contract_digest() != current.contract_digest()
        || binding.required_categories() != current.required_categories()
        || binding.reviewer_quorum() != current.reviewer_quorum()
        || binding.independence() != current.independence()
        || binding.blocking_severity() != current.blocking_severity()
        || binding.maximum_cycles() != current.maximum_cycles()
        || binding.waiver_policy() != current.waiver_policy()
        || binding.same_candidate(current)
    {
        return Err(reject(
            ReviewErrorKind::BindingMismatch,
            "new revision binding changes contract policy or does not change freshness",
        ));
    }
    mutation::replace_binding(state, binding.clone());
    Ok(ReviewEventKind::RevisionAdvanced { binding: binding.clone() })
}

fn assign(
    state: &mut ReviewRunState,
    assignment: &crate::ReviewAssignment,
) -> Result<ReviewEventKind, ReviewError> {
    assignment.validate(state.binding(), state.limits())?;
    let expected =
        state.cycles().len().checked_add(1).ok_or_else(|| {
            reject(ReviewErrorKind::LimitExceeded, "review cycle ordinal overflowed")
        })?;
    if state.cycles().len() >= usize::from(state.limits().assignments())
        || state.cycles().len() >= usize::from(state.limits().cycles())
        || state.cycles().len() >= usize::from(state.binding().maximum_cycles())
        || usize::from(assignment.ordinal().get()) != expected
        || state.cycle(assignment.cycle_id()).is_some()
    {
        return Err(reject(
            ReviewErrorKind::LimitExceeded,
            "assignment identity, ordinal, or cycle limit is exhausted",
        ));
    }
    mutation::push_cycle(state, crate::ReviewCycle::assigned(assignment.clone()));
    Ok(ReviewEventKind::ReviewerAssigned { assignment: assignment.clone() })
}

fn submit(
    state: &mut ReviewRunState,
    event_id: EventId,
    submission: &crate::ReviewSubmission,
) -> Result<ReviewEventKind, ReviewError> {
    submission.validate(state.binding().blocking_severity(), state.limits())?;
    let cycle = state.cycle(submission.cycle_id()).ok_or_else(|| {
        reject(ReviewErrorKind::UnknownIdentity, "submission cycle is not assigned")
    })?;
    if !state.cycle_is_current(cycle)
        || cycle.phase() != ReviewCyclePhase::Assigned
        || submission.revision() != state.binding().revision()
        || submission
            .categories()
            .iter()
            .any(|category| cycle.assignment().categories().binary_search(category).is_err())
        || !submission.reviewer_matches(cycle.assignment().reviewer())
    {
        return Err(reject(
            ReviewErrorKind::IllegalTransition,
            "submission is stale, repeated, or outside its assignment",
        ));
    }
    let submissions = state.cycles().iter().filter(|cycle| cycle.submission().is_some()).count();
    if submissions >= usize::from(state.limits().submissions())
        || state.findings().len().saturating_add(submission.findings().len())
            > state.limits().findings() as usize
        || submission.findings().iter().any(|finding| state.finding(finding.id()).is_some())
    {
        return Err(reject(
            ReviewErrorKind::LimitExceeded,
            "submission/finding limit or stable finding identity is exhausted",
        ));
    }
    let reviewer = cycle.assignment().reviewer().actor_id();
    let mut findings = submission.findings().to_vec();
    for finding in &mut findings {
        mutation::push_disposition(
            finding,
            DispositionRecord::from_wire(
                event_id,
                DispositionKind::Open,
                Some(reviewer),
                Some(submission.cycle_id()),
                submission.revision(),
                Vec::new(),
                None,
                None,
                None,
                None,
                finding.normalized_digest(),
            ),
        );
    }
    mutation::insert_findings(state, findings);
    let cycle = mutation::cycle_mut(state, submission.cycle_id())
        .ok_or_else(|| reject(ReviewErrorKind::UnknownIdentity, "submission cycle disappeared"))?;
    mutation::set_cycle_submission(cycle, submission.clone());
    Ok(ReviewEventKind::ReviewSubmitted { submission: submission.clone() })
}

fn cancel_cycle(
    state: &mut ReviewRunState,
    cycle_id: ReviewCycleId,
) -> Result<ReviewEventKind, ReviewError> {
    let current = state.cycle(cycle_id).is_some_and(|cycle| state.cycle_is_current(cycle));
    let cycle = mutation::cycle_mut(state, cycle_id).ok_or_else(|| {
        reject(ReviewErrorKind::UnknownIdentity, "cancelled cycle does not exist")
    })?;
    if !current || cycle.phase() != ReviewCyclePhase::Assigned {
        return Err(illegal("only an unsubmitted current cycle can be cancelled"));
    }
    mutation::set_cycle_phase(cycle, ReviewCyclePhase::Cancelled);
    Ok(ReviewEventKind::CycleCancelled { cycle_id })
}

fn finalize(state: &mut ReviewRunState) -> Result<ReviewEventKind, ReviewError> {
    let unconserved = state.unconserved_current_findings();
    if state.oscillation().triggered() {
        terminate(state, ReviewTerminalKind::NeedsHuman, Sha256Digest::new([0; 32]));
        return Ok(ReviewEventKind::RunFinalized);
    }
    if !state.quorum().complete() {
        return Err(reject(
            ReviewErrorKind::QuorumIncomplete,
            "all independent current review-quorum dimensions must pass",
        ));
    }
    if !unconserved.is_empty() {
        return Err(reject(
            ReviewErrorKind::FindingUnconserved,
            "one or more current findings lack a permitted closure",
        ));
    }
    terminate(state, ReviewTerminalKind::Completed, Sha256Digest::new([0; 32]));
    Ok(ReviewEventKind::RunFinalized)
}

fn terminate(state: &mut ReviewRunState, kind: ReviewTerminalKind, cause: Sha256Digest) {
    let terminal = mutation::make_terminal(
        kind,
        state.unconserved_current_findings(),
        state.quorum().clone(),
        state.oscillation().clone(),
        cause,
    );
    mutation::terminal(state, terminal);
}
