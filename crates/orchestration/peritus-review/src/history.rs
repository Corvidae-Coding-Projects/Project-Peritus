//! Bounded hydration of command-referenced facts from durable D2 indexes.

pub(crate) mod index;

use std::collections::{BTreeMap, BTreeSet};

use peritus_types::{FindingId, ReviewCycleId};

use crate::error::{ReviewError, ReviewErrorKind, reject};
use crate::state::mutation;
use crate::{
    DispositionKind, DispositionRecord, Finding, ReviewCommand, ReviewCommandKind, ReviewCycle,
    ReviewBinding, ReviewCyclePhase, ReviewEvent, ReviewEventKind, ReviewRunState,
};

/// Only the historical entities named by one bounded command.
pub(crate) struct ReviewHistoryHydration {
    finding_targets: BTreeSet<FindingId>,
    cycle_targets: BTreeSet<ReviewCycleId>,
    findings: BTreeMap<FindingId, Finding>,
    cycles: BTreeMap<ReviewCycleId, ReviewCycle>,
    noncurrent_findings: BTreeSet<FindingId>,
    indexed_finding_ids: BTreeSet<FindingId>,
    indexed_cycle_ids: BTreeSet<ReviewCycleId>,
    uses_index: bool,
    index_projection_missing: bool,
    new_findings: BTreeSet<FindingId>,
    new_cycle: Option<ReviewCycleId>,
    finding_identity_conflict: bool,
    cycle_identity_conflict: bool,
    cycle_status_conflict: bool,
    waiver_request_conflict: bool,
    waiver_observation_conflict: bool,
    submission_cycle_consumed: bool,
}

impl ReviewHistoryHydration {
    pub(crate) fn for_command(command: &ReviewCommand) -> Self {
        let mut finding_targets = BTreeSet::new();
        let mut cycle_targets = BTreeSet::new();
        let mut new_findings = BTreeSet::new();
        let mut new_cycle = None;
        match command.kind() {
            ReviewCommandKind::AssignReviewer { assignment } => {
                new_cycle = Some(assignment.cycle_id());
            }
            ReviewCommandKind::SubmitReview { submission } => {
                cycle_targets.insert(submission.cycle_id());
                new_findings.extend(submission.findings().iter().map(Finding::id));
            }
            ReviewCommandKind::ReconcileDuplicates { canonical, duplicates, .. } => {
                finding_targets.insert(*canonical);
                finding_targets.extend(duplicates.iter().copied());
            }
            ReviewCommandKind::RecordFixerResponse { finding_id, response }
            | ReviewCommandKind::RequestWaiver { finding_id, request: response } => {
                finding_targets.insert(*finding_id);
                if let Some(superseding) = response.superseding() {
                    finding_targets.insert(superseding);
                }
            }
            ReviewCommandKind::ConfirmResolution { finding_id, reviewer_cycle, .. }
            | ReviewCommandKind::ConfirmInvalidation { finding_id, reviewer_cycle, .. } => {
                finding_targets.insert(*finding_id);
                cycle_targets.insert(*reviewer_cycle);
            }
            ReviewCommandKind::ConfirmSupersession {
                finding_id,
                superseding,
                reviewer_cycle,
                ..
            } => {
                finding_targets.insert(*finding_id);
                finding_targets.insert(*superseding);
                cycle_targets.insert(*reviewer_cycle);
            }
            ReviewCommandKind::ObserveWaiver { waiver } => {
                finding_targets.insert(waiver.finding_id());
            }
            ReviewCommandKind::CancelCycle { cycle_id } => {
                cycle_targets.insert(*cycle_id);
            }
            ReviewCommandKind::StartRun { .. }
            | ReviewCommandKind::AdvanceRevision { .. }
            | ReviewCommandKind::CancelRun
            | ReviewCommandKind::PauseRun
            | ReviewCommandKind::ResumeRun
            | ReviewCommandKind::ExhaustBudget { .. }
            | ReviewCommandKind::FailRun { .. }
            | ReviewCommandKind::FinalizeRun => {}
        }
        Self {
            finding_targets,
            cycle_targets,
            findings: BTreeMap::new(),
            cycles: BTreeMap::new(),
            noncurrent_findings: BTreeSet::new(),
            indexed_finding_ids: BTreeSet::new(),
            indexed_cycle_ids: BTreeSet::new(),
            uses_index: false,
            index_projection_missing: false,
            new_findings,
            new_cycle,
            finding_identity_conflict: false,
            cycle_identity_conflict: false,
            cycle_status_conflict: false,
            waiver_request_conflict: false,
            waiver_observation_conflict: false,
            submission_cycle_consumed: false,
        }
    }

    pub(crate) fn from_index(
        command: &ReviewCommand,
        binding: &ReviewBinding,
        selection: index::ReviewIndexSelection,
    ) -> Self {
        let mut hydration = Self::for_command(command);
        hydration.uses_index = true;
        hydration.waiver_request_conflict = selection.waiver_request_exists;
        hydration.waiver_observation_conflict = selection.waiver_observation_exists;
        for (cycle_id, cycle) in selection.cycles {
            hydration.indexed_cycle_ids.insert(cycle_id);
            if hydration.new_cycle == Some(cycle_id) {
                hydration.cycle_identity_conflict = true;
            }
            if hydration.cycle_targets.contains(&cycle_id) {
                match (command.kind(), cycle.phase()) {
                    (
                        ReviewCommandKind::SubmitReview { .. },
                        index::IndexedCyclePhase::Submitted,
                    ) => hydration.submission_cycle_consumed = true,
                    (
                        ReviewCommandKind::SubmitReview { .. }
                        | ReviewCommandKind::CancelCycle { .. },
                        index::IndexedCyclePhase::Cancelled
                        | index::IndexedCyclePhase::Invalidated,
                    )
                    | (
                        ReviewCommandKind::CancelCycle { .. },
                        index::IndexedCyclePhase::Submitted,
                    ) => hydration.cycle_status_conflict = true,
                    _ => {
                        hydration.cycles.insert(cycle_id, cycle.hydration_cycle());
                    }
                }
            }
        }
        for (finding_id, finding) in selection.findings {
            hydration.indexed_finding_ids.insert(finding_id);
            if hydration.new_findings.contains(&finding_id) {
                hydration.finding_identity_conflict = true;
            }
            if hydration.finding_targets.contains(&finding_id) {
                if finding.binding_digest() == binding.digest() {
                    hydration.findings.insert(finding_id, finding.into_finding());
                } else {
                    hydration.noncurrent_findings.insert(finding_id);
                }
            }
        }
        hydration
    }

    pub(crate) fn observe(&mut self, event: &ReviewEvent, command: &ReviewCommand) {
        match event.kind() {
            ReviewEventKind::RunStarted { .. } => {}
            ReviewEventKind::RevisionAdvanced { binding } => {
                for cycle in self.cycles.values_mut() {
                    if cycle.assignment().binding_digest() != binding.digest() {
                        cycle.phase = ReviewCyclePhase::Invalidated;
                    }
                }
            }
            ReviewEventKind::ReviewerAssigned { assignment } => {
                if self.new_cycle == Some(assignment.cycle_id()) {
                    self.cycle_identity_conflict = true;
                }
                if self.cycle_targets.contains(&assignment.cycle_id()) {
                    self.cycles
                        .insert(assignment.cycle_id(), ReviewCycle::assigned(assignment.clone()));
                }
            }
            ReviewEventKind::ReviewSubmitted { submission } => {
                if matches!(command.kind(), ReviewCommandKind::SubmitReview { .. })
                    && self.cycle_targets.contains(&submission.cycle_id())
                {
                    self.submission_cycle_consumed = true;
                }
                if matches!(command.kind(), ReviewCommandKind::CancelCycle { .. }) {
                    if let Some(cycle) = self.cycles.get_mut(&submission.cycle_id()) {
                        cycle.phase = ReviewCyclePhase::Cancelled;
                    }
                }
                for finding in submission.findings() {
                    if self.new_findings.contains(&finding.id()) {
                        self.finding_identity_conflict = true;
                    }
                    if self.finding_targets.contains(&finding.id()) {
                        let mut finding = finding.clone();
                        let record = DispositionRecord::from_wire(
                            event.id(),
                            DispositionKind::Open,
                            Some(finding.origin().reviewer()),
                            Some(submission.cycle_id()),
                            submission.revision(),
                            Vec::new(),
                            None,
                            None,
                            None,
                            None,
                            finding.normalized_digest(),
                        );
                        set_current_disposition(&mut finding, record);
                        self.findings.insert(finding.id(), finding);
                    }
                }
            }
            ReviewEventKind::DuplicatesReconciled {
                canonical,
                duplicates,
                reconciliation_digest,
            } => {
                if let Some(finding) = self.findings.get_mut(canonical) {
                    set_current_disposition(
                        finding,
                        reconciliation_record(
                            event,
                            DispositionKind::Open,
                            None,
                            *reconciliation_digest,
                        ),
                    );
                }
                for duplicate in duplicates {
                    if let Some(finding) = self.findings.get_mut(duplicate) {
                        finding.superseded_by = Some(*canonical);
                        set_current_disposition(
                            finding,
                            reconciliation_record(
                                event,
                                DispositionKind::Superseded,
                                Some(*canonical),
                                *reconciliation_digest,
                            ),
                        );
                    }
                }
            }
            ReviewEventKind::FixerResponseRecorded { finding_id, response }
            | ReviewEventKind::WaiverRequested { finding_id, request: response } => {
                if let ReviewCommandKind::RequestWaiver { request, .. } = command.kind() {
                    if response.approval_request_id() == request.approval_request_id() {
                        self.waiver_request_conflict = true;
                    }
                }
                if let Some(finding) = self.findings.get_mut(finding_id) {
                    let kind = match response {
                        crate::FixerResponse::Fixed { .. } => DispositionKind::Fixed,
                        crate::FixerResponse::Disputed { .. } => DispositionKind::Disputed,
                        crate::FixerResponse::SupersessionProposed { .. } => {
                            DispositionKind::SupersessionProposed
                        }
                        crate::FixerResponse::WaiverRequested { .. } => {
                            DispositionKind::WaiverRequested
                        }
                    };
                    set_current_disposition(
                        finding,
                        DispositionRecord::from_wire(
                            event.id(),
                            kind,
                            Some(response.actor()),
                            None,
                            response.revision(),
                            response.evidence().to_vec(),
                            response.superseding(),
                            response.approval_request_id(),
                            response.authority(),
                            response.evidence_requirement_id(),
                            response.digest(),
                        ),
                    );
                }
            }
            ReviewEventKind::ResolutionConfirmed {
                finding_id,
                reviewer_cycle,
                evidence,
                confirmation_digest,
                ..
            } => self.confirm(
                event,
                *finding_id,
                *reviewer_cycle,
                evidence.clone(),
                *confirmation_digest,
                DispositionKind::ResolutionConfirmed,
                None,
            ),
            ReviewEventKind::InvalidationConfirmed {
                finding_id,
                reviewer_cycle,
                evidence,
                confirmation_digest,
                ..
            } => self.confirm(
                event,
                *finding_id,
                *reviewer_cycle,
                evidence.clone(),
                *confirmation_digest,
                DispositionKind::InvalidationConfirmed,
                None,
            ),
            ReviewEventKind::SupersessionConfirmed {
                finding_id,
                superseding,
                reviewer_cycle,
                evidence,
                confirmation_digest,
                ..
            } => {
                self.confirm(
                    event,
                    *finding_id,
                    *reviewer_cycle,
                    evidence.clone(),
                    *confirmation_digest,
                    DispositionKind::Superseded,
                    Some(*superseding),
                );
                if let Some(finding) = self.findings.get_mut(finding_id) {
                    finding.superseded_by = Some(*superseding);
                }
                if let Some(target) = self.findings.get_mut(superseding) {
                    set_current_disposition(
                        target,
                        reconciliation_record(
                            event,
                            DispositionKind::Open,
                            Some(*finding_id),
                            *confirmation_digest,
                        ),
                    );
                }
            }
            ReviewEventKind::WaiverObserved { waiver } => {
                if let ReviewCommandKind::ObserveWaiver { waiver: requested } = command.kind() {
                    if waiver.finding_id() == requested.finding_id()
                        && waiver.revision() == requested.revision()
                    {
                        self.waiver_observation_conflict = true;
                    }
                }
                if let Some(finding) = self.findings.get_mut(&waiver.finding_id()) {
                    set_current_disposition(
                        finding,
                        DispositionRecord::from_wire(
                            event.id(),
                            DispositionKind::Waived,
                            None,
                            None,
                            waiver.revision(),
                            Vec::new(),
                            None,
                            Some(waiver.observation().approval_request_id()),
                            Some(waiver.observation().authority()),
                            Some(waiver.observation().evidence_requirement_id()),
                            waiver.observation().waiver_digest(),
                        ),
                    );
                }
            }
            ReviewEventKind::CycleCancelled { cycle_id } => {
                if let Some(cycle) = self.cycles.get_mut(cycle_id) {
                    cycle.phase = ReviewCyclePhase::Cancelled;
                }
            }
            ReviewEventKind::RunCancelled
            | ReviewEventKind::RunPaused
            | ReviewEventKind::RunResumed
            | ReviewEventKind::BudgetExhausted { .. }
            | ReviewEventKind::RunFailed { .. }
            | ReviewEventKind::RunFinalized => {}
        }
    }

    pub(crate) fn capture_active(&mut self, state: &ReviewRunState) {
        for finding in state.findings() {
            if self.uses_index
                && (self.finding_targets.contains(&finding.id())
                    || self.new_findings.contains(&finding.id()))
                && !self.indexed_finding_ids.contains(&finding.id())
            {
                self.index_projection_missing = true;
            }
            if self.finding_targets.contains(&finding.id())
                && !self.noncurrent_findings.contains(&finding.id())
            {
                self.findings.insert(finding.id(), finding.clone());
            }
            if self.new_findings.contains(&finding.id()) {
                self.finding_identity_conflict = true;
            }
        }
        for cycle in state.cycles() {
            if self.uses_index
                && (self.cycle_targets.contains(&cycle.id())
                    || self.new_cycle == Some(cycle.id()))
                && !self.indexed_cycle_ids.contains(&cycle.id())
            {
                self.index_projection_missing = true;
            }
            if self.cycle_targets.contains(&cycle.id()) {
                self.cycles.insert(cycle.id(), cycle.clone());
            }
            if self.new_cycle == Some(cycle.id()) {
                self.cycle_identity_conflict = true;
            }
        }
    }

    pub(crate) fn install(self, state: &mut ReviewRunState) -> Result<(), ReviewError> {
        if self.index_projection_missing {
            return Err(reject(
                ReviewErrorKind::ReplayMismatch,
                "review checkpoint entity is absent from its durable identity index",
            ));
        }
        if self.finding_identity_conflict || self.cycle_identity_conflict {
            return Err(reject(
                ReviewErrorKind::IdentityConflict,
                "review cycle or finding identity already exists in immutable history",
            ));
        }
        if self.submission_cycle_consumed || self.cycle_status_conflict {
            return Err(reject(
                ReviewErrorKind::IllegalTransition,
                "review cycle already has a submission or closed immutable status",
            ));
        }
        if self.waiver_request_conflict || self.waiver_observation_conflict {
            return Err(reject(
                ReviewErrorKind::WaiverInvalid,
                "waiver request or observation was already consumed in immutable history",
            ));
        }
        let findings = self.findings.into_values().collect::<Vec<_>>();
        mutation::detach_hydrated_findings(state, &findings)?;
        mutation::insert_findings(state, findings);
        for cycle in self.cycles.into_values() {
            if state.cycle(cycle.id()).is_none() {
                mutation::push_cycle(state, cycle);
            }
        }
        Ok(())
    }

    fn confirm(
        &mut self,
        event: &ReviewEvent,
        finding_id: FindingId,
        reviewer_cycle: ReviewCycleId,
        evidence: Vec<peritus_evidence::EvidenceId>,
        digest: peritus_types::Sha256Digest,
        kind: DispositionKind,
        related: Option<FindingId>,
    ) {
        let actor = self
            .cycles
            .get(&reviewer_cycle)
            .map(|cycle| cycle.assignment().reviewer().actor_id());
        if let (Some(finding), Some(actor)) = (self.findings.get_mut(&finding_id), actor) {
            set_current_disposition(
                finding,
                DispositionRecord::from_wire(
                    event.id(),
                    kind,
                    Some(actor),
                    Some(reviewer_cycle),
                    event.revision(),
                    evidence,
                    related,
                    None,
                    None,
                    None,
                    digest,
                ),
            );
        }
    }
}

fn set_current_disposition(finding: &mut Finding, record: DispositionRecord) {
    finding.dispositions.clear();
    finding.dispositions.push(record);
}

fn reconciliation_record(
    event: &ReviewEvent,
    kind: DispositionKind,
    related: Option<FindingId>,
    digest: peritus_types::Sha256Digest,
) -> DispositionRecord {
    DispositionRecord::from_wire(
        event.id(),
        kind,
        None,
        None,
        event.revision(),
        Vec::new(),
        related,
        None,
        None,
        None,
        digest,
    )
}
