//! Durable command indexes reconstructed from immutable review events.

use std::collections::BTreeMap;

use peritus_spec::ReviewCategory;
use peritus_types::{
    ApprovalRequestId, EventId, FindingId, ReviewCycleId, RevisionTuple, RunId, Sha256Digest,
};

use crate::error::{ReviewError, ReviewErrorKind, reject};
use crate::{
    DispositionKind, DispositionRecord, Finding, QuorumReport, ReviewAssignment, ReviewBinding,
    ReviewCycle, ReviewCyclePhase, ReviewEvent, ReviewEventKind, ReviewRunState, ReviewSubmission,
};

/// Independently keyed radix index domains.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum ReviewIndexKind {
    Cycles,
    Findings,
    Facts,
}

/// One bounded radix node. Branches never collapse, so descendant state keys are immutable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReviewIndexNode {
    run_id: RunId,
    kind: ReviewIndexKind,
    prefix: Vec<u8>,
    through_sequence: u64,
    through_event: EventId,
    payload: ReviewIndexNodePayload,
}

/// Closed node payload for the three exact index domains.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ReviewIndexNodePayload {
    Branch(Vec<u8>),
    Cycles(Vec<IndexedCycle>),
    Findings(Vec<IndexedFinding>),
    Facts(Vec<IndexedFact>),
}

/// Exact command-derived rows loaded without enumerating aggregate history.
pub(crate) struct ReviewIndexSelection {
    pub(crate) cycles: BTreeMap<ReviewCycleId, IndexedCycle>,
    pub(crate) findings: BTreeMap<FindingId, IndexedFinding>,
    pub(crate) waiver_request_exists: bool,
    pub(crate) waiver_observation_exists: bool,
}

impl ReviewIndexSelection {
    pub(crate) fn empty() -> Self {
        Self {
            cycles: BTreeMap::new(),
            findings: BTreeMap::new(),
            waiver_request_exists: false,
            waiver_observation_exists: false,
        }
    }
}

impl ReviewIndexNode {
    pub(crate) const fn from_parts(
        run_id: RunId,
        kind: ReviewIndexKind,
        prefix: Vec<u8>,
        through_sequence: u64,
        through_event: EventId,
        payload: ReviewIndexNodePayload,
    ) -> Self {
        Self { run_id, kind, prefix, through_sequence, through_event, payload }
    }

    pub(crate) const fn run_id(&self) -> RunId {
        self.run_id
    }

    pub(crate) const fn kind(&self) -> ReviewIndexKind {
        self.kind
    }

    pub(crate) const fn prefix(&self) -> &[u8] {
        self.prefix.as_slice()
    }

    pub(crate) const fn through_sequence(&self) -> u64 {
        self.through_sequence
    }

    pub(crate) const fn through_event(&self) -> EventId {
        self.through_event
    }

    pub(crate) const fn payload(&self) -> &ReviewIndexNodePayload {
        &self.payload
    }

    pub(crate) fn payload_mut(&mut self) -> &mut ReviewIndexNodePayload {
        &mut self.payload
    }

    pub(crate) fn advance(&mut self, sequence: u64, event_id: EventId) {
        self.through_sequence = sequence;
        self.through_event = event_id;
    }
}

/// Closed lifecycle retained for one globally unique cycle identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IndexedCyclePhase {
    Assigned,
    Submitted,
    Cancelled,
    Invalidated,
}

/// Minimal exact cycle fact needed for identity, ownership, and quorum lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IndexedCycle {
    assignment: ReviewAssignment,
    phase: IndexedCyclePhase,
    submitted_categories: Vec<ReviewCategory>,
    review_digest: Option<Sha256Digest>,
}

/// Latest finding snapshot plus the exact candidate binding that created it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IndexedFinding {
    binding_digest: Sha256Digest,
    finding: Finding,
}

impl IndexedFinding {
    pub(crate) const fn new(binding_digest: Sha256Digest, finding: Finding) -> Self {
        Self { binding_digest, finding }
    }

    pub(crate) const fn id(&self) -> FindingId {
        self.finding.id()
    }

    pub(crate) const fn binding_digest(&self) -> Sha256Digest {
        self.binding_digest
    }

    pub(crate) const fn finding(&self) -> &Finding {
        &self.finding
    }

    pub(crate) fn finding_mut(&mut self) -> &mut Finding {
        &mut self.finding
    }

    pub(crate) fn into_finding(self) -> Finding {
        self.finding
    }
}

impl IndexedCycle {
    pub(crate) fn assigned(assignment: ReviewAssignment) -> Self {
        Self {
            assignment,
            phase: IndexedCyclePhase::Assigned,
            submitted_categories: Vec::new(),
            review_digest: None,
        }
    }

    pub(crate) const fn from_parts(
        assignment: ReviewAssignment,
        phase: IndexedCyclePhase,
        submitted_categories: Vec<ReviewCategory>,
        review_digest: Option<Sha256Digest>,
    ) -> Self {
        Self { assignment, phase, submitted_categories, review_digest }
    }

    pub(crate) const fn id(&self) -> ReviewCycleId {
        self.assignment.cycle_id()
    }

    pub(crate) const fn assignment(&self) -> &ReviewAssignment {
        &self.assignment
    }

    pub(crate) const fn phase(&self) -> IndexedCyclePhase {
        self.phase
    }

    pub(crate) const fn submitted_categories(&self) -> &[ReviewCategory] {
        self.submitted_categories.as_slice()
    }

    pub(crate) const fn review_digest(&self) -> Option<Sha256Digest> {
        self.review_digest
    }

    pub(crate) fn submit(&mut self, submission: &ReviewSubmission) {
        self.phase = IndexedCyclePhase::Submitted;
        self.submitted_categories = submission.categories().to_vec();
        self.review_digest = Some(submission.review_digest());
    }

    pub(crate) fn set_phase(&mut self, phase: IndexedCyclePhase) {
        self.phase = phase;
        if !matches!(phase, IndexedCyclePhase::Submitted) {
            self.submitted_categories.clear();
            self.review_digest = None;
        }
    }

    pub(crate) fn hydration_cycle(&self) -> ReviewCycle {
        let phase = match self.phase {
            IndexedCyclePhase::Assigned => ReviewCyclePhase::Assigned,
            IndexedCyclePhase::Submitted => ReviewCyclePhase::Submitted,
            IndexedCyclePhase::Cancelled => ReviewCyclePhase::Cancelled,
            IndexedCyclePhase::Invalidated => ReviewCyclePhase::Invalidated,
        };
        ReviewCycle::from_wire(self.assignment.clone(), phase, self.indexed_submission())
    }

    fn quorum_cycle(&self) -> Option<ReviewCycle> {
        if self.phase != IndexedCyclePhase::Submitted {
            return None;
        }
        Some(ReviewCycle::from_wire(
            self.assignment.clone(),
            ReviewCyclePhase::Submitted,
            self.indexed_submission(),
        ))
    }

    fn indexed_submission(&self) -> Option<ReviewSubmission> {
        if self.phase != IndexedCyclePhase::Submitted {
            return None;
        }
        let submission = ReviewSubmission::from_wire(
            self.id(),
            self.assignment.revision(),
            self.submitted_categories.clone(),
            Vec::new(),
            self.review_digest?,
        );
        Some(submission)
    }
}

/// Immutable fact whose exact key is retained independently of entity snapshots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum IndexedFact {
    WaiverRequest {
        approval_request_id: ApprovalRequestId,
    },
    WaiverObservation {
        finding_id: FindingId,
        revision: RevisionTuple,
    },
}

impl IndexedFact {
    pub(crate) fn waiver_request(approval_request_id: ApprovalRequestId) -> Self {
        Self::WaiverRequest { approval_request_id }
    }

    pub(crate) fn waiver_observation(finding_id: FindingId, revision: RevisionTuple) -> Self {
        Self::WaiverObservation { finding_id, revision }
    }

    /// Collision-free typed radix key.
    pub(crate) fn key_bytes(&self) -> Vec<u8> {
        match self {
            Self::WaiverRequest { approval_request_id } => {
                let mut key = Vec::with_capacity(17);
                key.push(1);
                key.extend_from_slice(approval_request_id.as_bytes());
                key
            }
            Self::WaiverObservation { finding_id, revision } => {
                let mut key = Vec::with_capacity(113);
                key.push(2);
                key.extend_from_slice(finding_id.as_bytes());
                write_revision_key(&mut key, *revision);
                key
            }
        }
    }
}

/// Current bounded quorum facts, independent from canceled assignment history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReviewQuorumIndex {
    binding_digest: Sha256Digest,
    members: BTreeMap<ReviewCycleId, IndexedCycle>,
    report: QuorumReport,
}

impl ReviewQuorumIndex {
    pub(crate) fn empty(binding: &ReviewBinding) -> Self {
        Self {
            binding_digest: binding.digest(),
            members: BTreeMap::new(),
            report: QuorumReport::evaluate(binding, &[]),
        }
    }

    pub(crate) fn from_parts(
        binding_digest: Sha256Digest,
        members: Vec<IndexedCycle>,
        report: QuorumReport,
    ) -> Self {
        Self {
            binding_digest,
            members: members.into_iter().map(|member| (member.id(), member)).collect(),
            report,
        }
    }

    pub(crate) const fn binding_digest(&self) -> Sha256Digest {
        self.binding_digest
    }

    pub(crate) fn members(&self) -> impl Iterator<Item = &IndexedCycle> {
        self.members.values()
    }

    pub(crate) const fn report(&self) -> &QuorumReport {
        &self.report
    }

    pub(crate) fn reset(&mut self, binding: &ReviewBinding) {
        *self = Self::empty(binding);
    }

    pub(crate) fn observe_submission(
        &mut self,
        binding: &ReviewBinding,
        cycle: IndexedCycle,
    ) {
        if self.report.complete() || cycle.assignment().binding_digest() != binding.digest() {
            return;
        }
        self.members.insert(cycle.id(), cycle);
        self.refresh(binding);
    }

    pub(crate) fn observe_cancellation(
        &mut self,
        binding: &ReviewBinding,
        cycle_id: ReviewCycleId,
    ) {
        if self.members.remove(&cycle_id).is_some() {
            self.refresh(binding);
        }
    }

    pub(crate) fn validate(
        &self,
        binding: &ReviewBinding,
        limits: crate::ReviewLimits,
    ) -> Result<(), ReviewError> {
        if self.binding_digest != binding.digest() {
            return Err(reject(
                ReviewErrorKind::ReplayMismatch,
                "review quorum index belongs to another binding",
            ));
        }
        if self.report.complete() {
            if !self.members.is_empty() {
                return Err(reject(
                    ReviewErrorKind::ReplayMismatch,
                    "complete review quorum index retains mutable member state",
                ));
            }
            return Ok(());
        }
        if self
            .members
            .values()
            .any(|member| {
                member.phase() != IndexedCyclePhase::Submitted
                    || member.assignment().validate(binding, limits).is_err()
                    || member.submitted_categories().iter().any(|category| {
                        member.assignment().categories().binary_search(category).is_err()
                    })
            })
        {
            return Err(reject(
                ReviewErrorKind::ReplayMismatch,
                "incomplete review quorum index contains a non-submitted member",
            ));
        }
        let expected = evaluate_members(binding, self.members.values());
        if expected != self.report {
            return Err(reject(
                ReviewErrorKind::ReplayMismatch,
                "review quorum index differs from its submitted members",
            ));
        }
        Ok(())
    }

    fn refresh(&mut self, binding: &ReviewBinding) {
        self.report = evaluate_members(binding, self.members.values());
        if self.report.complete() {
            self.members.clear();
        }
    }
}

/// Exact index frontier published atomically with the matching checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReviewIndexManifest {
    run_id: RunId,
    sequence: u64,
    event_id: EventId,
    state_digest: Sha256Digest,
    binding_digest: Sha256Digest,
}

impl ReviewIndexManifest {
    pub(crate) fn from_state(state: &ReviewRunState) -> Self {
        Self {
            run_id: state.run_id(),
            sequence: state.sequence().get(),
            event_id: state.last_event_id(),
            state_digest: state.state_digest(),
            binding_digest: state.binding().digest(),
        }
    }

    pub(crate) const fn from_parts(
        run_id: RunId,
        sequence: u64,
        event_id: EventId,
        state_digest: Sha256Digest,
        binding_digest: Sha256Digest,
    ) -> Self {
        Self { run_id, sequence, event_id, state_digest, binding_digest }
    }

    pub(crate) const fn run_id(self) -> RunId {
        self.run_id
    }

    pub(crate) const fn sequence(self) -> u64 {
        self.sequence
    }

    pub(crate) const fn event_id(self) -> EventId {
        self.event_id
    }

    pub(crate) const fn state_digest(self) -> Sha256Digest {
        self.state_digest
    }

    pub(crate) const fn binding_digest(self) -> Sha256Digest {
        self.binding_digest
    }

    pub(crate) fn validate(self, state: &ReviewRunState) -> Result<(), ReviewError> {
        if self != Self::from_state(state) {
            return Err(reject(
                ReviewErrorKind::ReplayMismatch,
                "review index frontier differs from its exact checkpoint",
            ));
        }
        Ok(())
    }
}

/// Full one-time reconstruction result for an index-less V2 aggregate.
pub(crate) struct ReconstructedReviewIndex {
    pub(crate) cycles: BTreeMap<ReviewCycleId, IndexedCycle>,
    pub(crate) findings: BTreeMap<FindingId, IndexedFinding>,
    pub(crate) facts: BTreeMap<Vec<u8>, IndexedFact>,
    quorum: Option<ReviewQuorumIndex>,
    binding: Option<ReviewBinding>,
}

impl ReconstructedReviewIndex {
    pub(crate) fn new() -> Self {
        Self {
            cycles: BTreeMap::new(),
            findings: BTreeMap::new(),
            facts: BTreeMap::new(),
            quorum: None,
            binding: None,
        }
    }

    pub(crate) fn observe(&mut self, event: &ReviewEvent) -> Result<(), ReviewError> {
        match event.kind() {
            ReviewEventKind::RunStarted { binding, .. } => {
                if self.binding.is_some() {
                    return Err(reject(
                        ReviewErrorKind::ReplayMismatch,
                        "review index history contains another genesis event",
                    ));
                }
                self.binding = Some(binding.clone());
                self.quorum = Some(ReviewQuorumIndex::empty(binding));
            }
            ReviewEventKind::RevisionAdvanced { binding } => {
                if self.binding.is_none() {
                    return Err(missing_binding());
                }
                self.binding = Some(binding.clone());
                self.quorum = Some(ReviewQuorumIndex::empty(binding));
            }
            ReviewEventKind::ReviewerAssigned { assignment } => {
                let binding = self.binding.as_ref().ok_or_else(missing_binding)?;
                if assignment.binding_digest() != binding.digest()
                    || assignment.revision() != binding.revision()
                {
                    return Err(reject(
                        ReviewErrorKind::ReplayMismatch,
                        "review assignment index differs from its current binding",
                    ));
                }
                if self
                    .cycles
                    .insert(assignment.cycle_id(), IndexedCycle::assigned(assignment.clone()))
                    .is_some()
                {
                    return Err(identity_conflict());
                }
            }
            ReviewEventKind::ReviewSubmitted { submission } => {
                let binding = self.binding.as_ref().ok_or_else(missing_binding)?;
                let cycle = self.cycles.get_mut(&submission.cycle_id()).ok_or_else(|| {
                    reject(
                        ReviewErrorKind::ReplayMismatch,
                        "review submission index has no assignment fact",
                    )
                })?;
                if cycle.phase() != IndexedCyclePhase::Assigned
                    || submission.revision() != cycle.assignment().revision()
                    || submission.revision() != binding.revision()
                    || cycle.assignment().binding_digest() != binding.digest()
                {
                    return Err(reject(
                        ReviewErrorKind::ReplayMismatch,
                        "review submission index repeats or differs from its assignment",
                    ));
                }
                cycle.submit(submission);
                let submitted = cycle.clone();
                self.quorum
                    .as_mut()
                    .ok_or_else(missing_binding)?
                    .observe_submission(binding, submitted);
                for finding in submission.findings() {
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
                    let binding_digest = binding.digest();
                    if self
                        .findings
                        .insert(finding.id(), IndexedFinding::new(binding_digest, finding))
                        .is_some()
                    {
                        return Err(identity_conflict());
                    }
                }
            }
            ReviewEventKind::DuplicatesReconciled {
                canonical,
                duplicates,
                reconciliation_digest,
            } => {
                if !self.findings.contains_key(canonical)
                    || duplicates.iter().any(|finding| !self.findings.contains_key(finding))
                {
                    return Err(missing_entity(
                        "review reconciliation index references an absent finding",
                    ));
                }
                if let Some(finding) = self.findings.get_mut(canonical) {
                    set_current_disposition(
                        finding.finding_mut(),
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
                        finding.finding_mut().superseded_by = Some(*canonical);
                        set_current_disposition(
                            finding.finding_mut(),
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
                if !self.findings.contains_key(finding_id)
                    || response
                        .superseding()
                        .is_some_and(|target| !self.findings.contains_key(&target))
                {
                    return Err(missing_entity(
                        "review response index references an absent finding",
                    ));
                }
                if let Some(approval_request_id) = response.approval_request_id() {
                    self.insert_fact(IndexedFact::waiver_request(approval_request_id))?;
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
                        finding.finding_mut(),
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
            } => {
                self.confirm(
                    event,
                    *finding_id,
                    *reviewer_cycle,
                    evidence.clone(),
                    *confirmation_digest,
                    DispositionKind::ResolutionConfirmed,
                    None,
                )?;
            }
            ReviewEventKind::InvalidationConfirmed {
                finding_id,
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
                    DispositionKind::InvalidationConfirmed,
                    None,
                )?;
            }
            ReviewEventKind::SupersessionConfirmed {
                finding_id,
                superseding,
                reviewer_cycle,
                evidence,
                confirmation_digest,
                ..
            } => {
                if !self.findings.contains_key(superseding) {
                    return Err(missing_entity(
                        "review supersession index references an absent target",
                    ));
                }
                self.confirm(
                    event,
                    *finding_id,
                    *reviewer_cycle,
                    evidence.clone(),
                    *confirmation_digest,
                    DispositionKind::Superseded,
                    Some(*superseding),
                )?;
                if let Some(finding) = self.findings.get_mut(finding_id) {
                    finding.finding_mut().superseded_by = Some(*superseding);
                }
                if let Some(target) = self.findings.get_mut(superseding) {
                    set_current_disposition(
                        target.finding_mut(),
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
                if !self.findings.contains_key(&waiver.finding_id()) {
                    return Err(missing_entity(
                        "review waiver index references an absent finding",
                    ));
                }
                self.insert_fact(IndexedFact::waiver_observation(
                    waiver.finding_id(),
                    waiver.revision(),
                ))?;
                if let Some(finding) = self.findings.get_mut(&waiver.finding_id()) {
                    set_current_disposition(
                        finding.finding_mut(),
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
                let binding = self.binding.as_ref().ok_or_else(missing_binding)?;
                let cycle = self.cycles.get_mut(cycle_id).ok_or_else(|| {
                    reject(
                        ReviewErrorKind::ReplayMismatch,
                        "cancelled review cycle has no indexed assignment",
                    )
                })?;
                if cycle.phase() != IndexedCyclePhase::Assigned
                    || cycle.assignment().binding_digest() != binding.digest()
                {
                    return Err(reject(
                        ReviewErrorKind::ReplayMismatch,
                        "cancelled review cycle is stale or already closed",
                    ));
                }
                cycle.set_phase(IndexedCyclePhase::Cancelled);
                self.quorum
                    .as_mut()
                    .ok_or_else(missing_binding)?
                    .observe_cancellation(binding, *cycle_id);
            }
            ReviewEventKind::RunCancelled
            | ReviewEventKind::RunPaused
            | ReviewEventKind::RunResumed
            | ReviewEventKind::BudgetExhausted { .. }
            | ReviewEventKind::RunFailed { .. }
            | ReviewEventKind::RunFinalized => {}
        }
        Ok(())
    }

    pub(crate) fn validate_checkpoint(&self, state: &ReviewRunState) -> Result<(), ReviewError> {
        let binding = self.binding.as_ref().ok_or_else(missing_binding)?;
        let quorum = self.quorum.as_ref().ok_or_else(missing_binding)?;
        quorum.validate(binding, state.limits())?;
        if binding != state.binding() || quorum.report() != state.quorum() {
            return Err(reject(
                ReviewErrorKind::ReplayMismatch,
                "reconstructed review indexes differ from the bounded checkpoint",
            ));
        }
        Ok(())
    }

    pub(crate) fn into_parts(
        self,
    ) -> Result<
        (
            BTreeMap<ReviewCycleId, IndexedCycle>,
            BTreeMap<FindingId, IndexedFinding>,
            BTreeMap<Vec<u8>, IndexedFact>,
            ReviewQuorumIndex,
        ),
        ReviewError,
    > {
        Ok((
            self.cycles,
            self.findings,
            self.facts,
            self.quorum.ok_or_else(missing_binding)?,
        ))
    }

    fn insert_fact(&mut self, fact: IndexedFact) -> Result<(), ReviewError> {
        if self.facts.insert(fact.key_bytes(), fact).is_some() {
            return Err(reject(
                ReviewErrorKind::ReplayMismatch,
                "immutable review fact identity is duplicated",
            ));
        }
        Ok(())
    }

    fn confirm(
        &mut self,
        event: &ReviewEvent,
        finding_id: FindingId,
        reviewer_cycle: ReviewCycleId,
        evidence: Vec<peritus_evidence::EvidenceId>,
        digest: Sha256Digest,
        kind: DispositionKind,
        related: Option<FindingId>,
    ) -> Result<(), ReviewError> {
        let actor = self
            .cycles
            .get(&reviewer_cycle)
            .map(|cycle| cycle.assignment().reviewer().actor_id());
        let actor = actor.ok_or_else(|| {
            missing_entity("review confirmation index references an absent reviewer cycle")
        })?;
        let finding = self.findings.get_mut(&finding_id).ok_or_else(|| {
            missing_entity("review confirmation index references an absent finding")
        })?;
        set_current_disposition(
            finding.finding_mut(),
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
        Ok(())
    }
}

fn evaluate_members<'a>(
    binding: &ReviewBinding,
    members: impl Iterator<Item = &'a IndexedCycle>,
) -> QuorumReport {
    let cycles = members.filter_map(IndexedCycle::quorum_cycle).collect::<Vec<_>>();
    QuorumReport::evaluate(binding, &cycles)
}

fn write_revision_key(bytes: &mut Vec<u8>, revision: RevisionTuple) {
    bytes.extend_from_slice(revision.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(revision.harness_id().as_bytes());
    bytes.extend_from_slice(revision.workspace_id().as_bytes());
    bytes.extend_from_slice(&revision.workspace_generation().get().to_be_bytes());
    bytes.extend_from_slice(&revision.workspace_revision().get().to_be_bytes());
    bytes.extend_from_slice(revision.policy_id().as_bytes());
    bytes.extend_from_slice(revision.provider_profile_id().as_bytes());
}

fn set_current_disposition(finding: &mut Finding, record: DispositionRecord) {
    finding.dispositions.clear();
    finding.dispositions.push(record);
}

fn reconciliation_record(
    event: &ReviewEvent,
    kind: DispositionKind,
    related: Option<FindingId>,
    digest: Sha256Digest,
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

fn identity_conflict() -> ReviewError {
    reject(
        ReviewErrorKind::ReplayMismatch,
        "review entity identity is duplicated in immutable history",
    )
}

fn missing_binding() -> ReviewError {
    reject(
        ReviewErrorKind::ReplayMismatch,
        "review index history has no current binding",
    )
}

fn missing_entity(detail: &'static str) -> ReviewError {
    reject(ReviewErrorKind::ReplayMismatch, detail)
}
