//! Fail-closed qualification of exact public obligations against current typed evidence.

use crate::{
    AlternativeBranchId, AlternativeGroupId, ConditionId, ConditionObservation, ConditionState,
    ObligationError, ObligationErrorKind, ObligationSpec, RequirementEntry, RequirementEvidence,
    RequirementLedger,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;

/// Outcome of matching one requirement against its evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvidenceVerdict {
    /// No evidence was supplied.
    Missing,
    /// Evidence belongs to another ledger or candidate revision.
    Stale,
    /// Current evidence has the wrong shape or does not satisfy the obligation.
    Invalid,
    /// Current evidence exactly satisfies the obligation.
    Satisfied,
}

/// Complete deterministic result of qualifying one requirement ledger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualificationReport {
    qualified: bool,
    required_count: usize,
    satisfied_count: usize,
    missing_count: usize,
    stale_count: usize,
    invalid_count: usize,
    unsatisfied_requirements: Vec<RequirementId>,
    unresolved_conditions: Vec<ConditionId>,
    incomplete_alternatives: Vec<AlternativeGroupId>,
}

impl QualificationReport {
    /// Whether all active obligations have current satisfying evidence.
    #[must_use]
    pub const fn qualified(&self) -> bool {
        self.qualified
    }

    /// Active ordinary obligations plus alternative groups.
    #[must_use]
    pub const fn required_count(&self) -> usize {
        self.required_count
    }

    /// Active obligations or groups that are completely satisfied.
    #[must_use]
    pub const fn satisfied_count(&self) -> usize {
        self.satisfied_count
    }

    /// Missing evidence observations.
    #[must_use]
    pub const fn missing_count(&self) -> usize {
        self.missing_count
    }

    /// Evidence observations bound to an older ledger or candidate.
    #[must_use]
    pub const fn stale_count(&self) -> usize {
        self.stale_count
    }

    /// Current observations that do not meet their typed obligation.
    #[must_use]
    pub const fn invalid_count(&self) -> usize {
        self.invalid_count
    }

    /// Active non-alternative requirements that remain unsatisfied.
    #[must_use]
    pub const fn unsatisfied_requirements(&self) -> &[RequirementId] {
        self.unsatisfied_requirements.as_slice()
    }

    /// Public conditions that were not resolved.
    #[must_use]
    pub const fn unresolved_conditions(&self) -> &[ConditionId] {
        self.unresolved_conditions.as_slice()
    }

    /// Alternative groups lacking one complete branch.
    #[must_use]
    pub const fn incomplete_alternatives(&self) -> &[AlternativeGroupId] {
        self.incomplete_alternatives.as_slice()
    }
}

/// Qualifies one exact ledger against current candidate-bound evidence.
///
/// # Errors
///
/// Rejects oversized, duplicate, unordered, or unknown evidence and conflicting condition
/// observations. Missing, stale, and unsatisfied known evidence are represented in the report.
pub fn qualify(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    conditions: &[ConditionObservation],
    evidence: &[RequirementEvidence],
) -> Result<QualificationReport, ObligationError> {
    validate_conditions(conditions)?;
    validate_evidence(ledger, evidence)?;
    let mut report = QualificationReport {
        qualified: false,
        required_count: 0,
        satisfied_count: 0,
        missing_count: 0,
        stale_count: 0,
        invalid_count: 0,
        unsatisfied_requirements: Vec::new(),
        unresolved_conditions: Vec::new(),
        incomplete_alternatives: Vec::new(),
    };

    for entry in ledger.entries() {
        if entry.specification().is_example() || entry.specification().alternative().is_some() {
            continue;
        }
        if let Some(condition_id) = entry.specification().condition_id() {
            match condition_state(conditions, condition_id) {
                Some(ConditionState::DoesNotHold) => continue,
                Some(ConditionState::Holds) => {}
                Some(ConditionState::Unknown) | None => {
                    report.unresolved_conditions.push(condition_id);
                    continue;
                }
            }
        }
        report.required_count += 1;
        let verdict =
            evidence_verdict(ledger, candidate, entry, evidence_for(evidence, entry.id()));
        record_verdict(&mut report, entry.id(), verdict);
    }

    qualify_alternatives(ledger, candidate, evidence, &mut report);
    let required_current = report.satisfied_count == report.required_count
        && report.missing_count == 0
        && report.stale_count == 0
        && report.invalid_count == 0;
    report.qualified = crate::verified::qualification_allowed(
        required_current,
        report.incomplete_alternatives.is_empty(),
        report.unresolved_conditions.is_empty(),
    );
    Ok(report)
}

fn validate_conditions(conditions: &[ConditionObservation]) -> Result<(), ObligationError> {
    for pair in conditions.windows(2) {
        if pair[0].condition_id() == pair[1].condition_id() {
            return Err(ObligationError::plain(ObligationErrorKind::InvalidCondition));
        }
        if pair[0].condition_id() > pair[1].condition_id() {
            return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
        }
    }
    Ok(())
}

fn validate_evidence(
    ledger: &RequirementLedger,
    evidence: &[RequirementEvidence],
) -> Result<(), ObligationError> {
    if evidence.len() > ledger.limits().max_evidence() {
        return Err(ObligationError::numbers(
            ObligationErrorKind::LimitExceeded,
            ledger.limits().max_evidence() as u64,
            evidence.len() as u64,
        ));
    }
    for pair in evidence.windows(2) {
        if pair[0].requirement_id() == pair[1].requirement_id() {
            return Err(ObligationError::plain(ObligationErrorKind::DuplicateValue));
        }
        if pair[0].requirement_id() > pair[1].requirement_id() {
            return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
        }
    }
    if let Some(unknown) =
        evidence.iter().find(|item| ledger.entry(item.requirement_id()).is_none())
    {
        return Err(ObligationError::requirement(
            ObligationErrorKind::UnknownRequirement,
            unknown.requirement_id(),
        ));
    }
    Ok(())
}

fn condition_state(conditions: &[ConditionObservation], id: ConditionId) -> Option<ConditionState> {
    conditions
        .binary_search_by_key(&id, |observation| observation.condition_id())
        .ok()
        .map(|index| conditions[index].state())
}

fn evidence_for(
    evidence: &[RequirementEvidence],
    id: RequirementId,
) -> Option<&RequirementEvidence> {
    evidence
        .binary_search_by_key(&id, RequirementEvidence::requirement_id)
        .ok()
        .map(|index| &evidence[index])
}

fn evidence_verdict(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    entry: &RequirementEntry,
    evidence: Option<&RequirementEvidence>,
) -> EvidenceVerdict {
    let Some(evidence) = evidence else { return EvidenceVerdict::Missing };
    if !evidence.binding().is_current_for(entry.id(), ledger.digest(), candidate) {
        return EvidenceVerdict::Stale;
    }
    if entry
        .paths()
        .iter()
        .filter(|path| path.role().requires_candidate_evidence())
        .any(|path| !evidence.binding().contains_path(path.id()))
    {
        return EvidenceVerdict::Invalid;
    }
    let satisfied = match (entry.specification(), evidence) {
        (
            ObligationSpec::Hard
            | ObligationSpec::Conditional { .. }
            | ObligationSpec::Alternative { .. }
            | ObligationSpec::GeneratedOutput,
            RequirementEvidence::Direct(value),
        ) => value.satisfied(),
        (ObligationSpec::Performance(requirement), RequirementEvidence::Performance(value)) => {
            value.satisfies(*requirement)
        }
        (ObligationSpec::LifecycleIngress(requirement), RequirementEvidence::Lifecycle(value)) => {
            value.satisfies(*requirement)
        }
        (
            ObligationSpec::RequestSchema(requirement)
            | ObligationSpec::ResponseSchema(requirement),
            RequirementEvidence::Schema(value),
        ) => value.covers(requirement),
        (ObligationSpec::BrowserSemantics(requirement), RequirementEvidence::Browser(value)) => {
            value.satisfies(*requirement)
        }
        (
            ObligationSpec::ExternalEffect { effect_identity },
            RequirementEvidence::ExternalEffect(value),
        ) => {
            value.effect_identity() == *effect_identity
                && value.observed_at_public_boundary()
                && value.completed()
        }
        (ObligationSpec::Example, _) => true,
        _ => false,
    };
    if satisfied { EvidenceVerdict::Satisfied } else { EvidenceVerdict::Invalid }
}

fn record_verdict(
    report: &mut QualificationReport,
    requirement_id: RequirementId,
    verdict: EvidenceVerdict,
) {
    match verdict {
        EvidenceVerdict::Satisfied => report.satisfied_count += 1,
        EvidenceVerdict::Missing => {
            report.missing_count += 1;
            report.unsatisfied_requirements.push(requirement_id);
        }
        EvidenceVerdict::Stale => {
            report.stale_count += 1;
            report.unsatisfied_requirements.push(requirement_id);
        }
        EvidenceVerdict::Invalid => {
            report.invalid_count += 1;
            report.unsatisfied_requirements.push(requirement_id);
        }
    }
}

fn qualify_alternatives(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    evidence: &[RequirementEvidence],
    report: &mut QualificationReport,
) {
    let mut seen_groups = Vec::new();
    for entry in ledger.entries() {
        let Some((group, _)) = entry.specification().alternative() else { continue };
        if seen_groups.contains(&group) {
            continue;
        }
        seen_groups.push(group);
        report.required_count += 1;
        if any_branch_complete(ledger, candidate, evidence, group) {
            report.satisfied_count += 1;
        } else {
            report.incomplete_alternatives.push(group);
        }
    }
}

fn any_branch_complete(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    evidence: &[RequirementEvidence],
    group: AlternativeGroupId,
) -> bool {
    let mut seen_branches: Vec<AlternativeBranchId> = Vec::new();
    for entry in ledger.entries() {
        let Some((entry_group, branch)) = entry.specification().alternative() else { continue };
        if entry_group != group || seen_branches.contains(&branch) {
            continue;
        }
        seen_branches.push(branch);
        let complete = ledger
            .entries()
            .iter()
            .filter(|candidate_entry| {
                candidate_entry.specification().alternative() == Some((group, branch))
            })
            .all(|candidate_entry| {
                evidence_verdict(
                    ledger,
                    candidate,
                    candidate_entry,
                    evidence_for(evidence, candidate_entry.id()),
                ) == EvidenceVerdict::Satisfied
            });
        if complete {
            return true;
        }
    }
    false
}
