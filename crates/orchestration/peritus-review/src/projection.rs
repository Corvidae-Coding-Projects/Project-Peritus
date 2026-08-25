//! Rebuildable read-only query projections over authoritative D2 state.

use peritus_quality_policy::ReviewCycleOrdinal;
use peritus_spec::{FindingSeverity, ReviewCategory};
use peritus_types::{ActorId, FindingId, ReviewCycleId, RevisionTuple, RunId, Sha256Digest};

use crate::{
    DispositionKind, ReviewCyclePhase, ReviewRunPhase, ReviewRunState, ReviewTerminalKind,
};

/// One retained reviewer-cycle query row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedCycle {
    cycle_id: ReviewCycleId,
    ordinal: ReviewCycleOrdinal,
    phase: ReviewCyclePhase,
    reviewer: ActorId,
    categories: Vec<ReviewCategory>,
    current: bool,
    review_digest: Option<Sha256Digest>,
}

impl ProjectedCycle {
    /// Returns the stable cycle identity.
    #[must_use]
    pub const fn cycle_id(&self) -> ReviewCycleId {
        self.cycle_id
    }
    /// Returns the one-based cycle ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> ReviewCycleOrdinal {
        self.ordinal
    }
    /// Returns the current retained phase.
    #[must_use]
    pub const fn phase(&self) -> ReviewCyclePhase {
        self.phase
    }
    /// Returns the assigned reviewer actor.
    #[must_use]
    pub const fn reviewer(&self) -> ActorId {
        self.reviewer
    }
    /// Borrows the assigned canonical categories.
    #[must_use]
    pub fn categories(&self) -> &[ReviewCategory] {
        &self.categories
    }
    /// Returns whether this row belongs to the exact current candidate binding.
    #[must_use]
    pub const fn current(&self) -> bool {
        self.current
    }
    /// Returns the normalized accepted review digest, when submitted.
    #[must_use]
    pub const fn review_digest(&self) -> Option<Sha256Digest> {
        self.review_digest
    }
}

/// One retained finding query row with current/history classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedFinding {
    finding_id: FindingId,
    severity: FindingSeverity,
    blocking: bool,
    disposition: DispositionKind,
    current: bool,
    superseded_by: Option<FindingId>,
    normalized_digest: Sha256Digest,
    source_count: usize,
    disposition_count: usize,
}

impl ProjectedFinding {
    /// Returns the stable finding identity.
    #[must_use]
    pub const fn finding_id(&self) -> FindingId {
        self.finding_id
    }
    /// Returns the finding severity.
    #[must_use]
    pub const fn severity(&self) -> FindingSeverity {
        self.severity
    }
    /// Returns the contract-derived blocking fact.
    #[must_use]
    pub const fn blocking(&self) -> bool {
        self.blocking
    }
    /// Returns the latest derived disposition.
    #[must_use]
    pub const fn disposition(&self) -> DispositionKind {
        self.disposition
    }
    /// Returns whether the finding belongs to the exact current candidate binding.
    #[must_use]
    pub const fn current(&self) -> bool {
        self.current
    }
    /// Returns the retained canonical replacement, when superseded.
    #[must_use]
    pub const fn superseded_by(&self) -> Option<FindingId> {
        self.superseded_by
    }
    /// Returns the semantic defect fingerprint.
    #[must_use]
    pub const fn normalized_digest(&self) -> Sha256Digest {
        self.normalized_digest
    }
    /// Returns the number of retained reviewer/cycle provenance sources.
    #[must_use]
    pub const fn source_count(&self) -> usize {
        self.source_count
    }
    /// Returns the number of retained append-only lifecycle facts.
    #[must_use]
    pub const fn disposition_count(&self) -> usize {
        self.disposition_count
    }
}

/// Complete query projection with no review, waiver, or acceptance authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewProjection {
    run_id: RunId,
    revision: RevisionTuple,
    phase: ReviewRunPhase,
    terminal: Option<ReviewTerminalKind>,
    sequence: u64,
    state_digest: Sha256Digest,
    submitted_reviews: u16,
    quorum_complete: bool,
    unconserved_findings: Vec<FindingId>,
    cycles: Vec<ProjectedCycle>,
    findings: Vec<ProjectedFinding>,
}

impl ReviewProjection {
    /// Projects one checked authoritative state deterministically.
    #[must_use]
    pub fn from_state(state: &ReviewRunState) -> Self {
        let cycles = state
            .cycles()
            .iter()
            .map(|cycle| ProjectedCycle {
                cycle_id: cycle.id(),
                ordinal: cycle.ordinal(),
                phase: cycle.phase(),
                reviewer: cycle.assignment().reviewer().actor_id(),
                categories: cycle.assignment().categories().to_vec(),
                current: state.cycle_is_current(cycle),
                review_digest: cycle.submission().map(crate::ReviewSubmission::review_digest),
            })
            .collect();
        let findings = state
            .findings()
            .iter()
            .map(|finding| ProjectedFinding {
                finding_id: finding.id(),
                severity: finding.severity(),
                blocking: finding.blocking(),
                disposition: finding.current_disposition(),
                current: state.finding_is_current(finding),
                superseded_by: finding.superseded_by(),
                normalized_digest: finding.normalized_digest(),
                source_count: finding.sources().len(),
                disposition_count: finding.dispositions().len(),
            })
            .collect();
        Self {
            run_id: state.run_id(),
            revision: state.binding().revision(),
            phase: state.phase(),
            terminal: state.terminal().map(crate::ReviewTerminal::kind),
            sequence: state.sequence().get(),
            state_digest: state.state_digest(),
            submitted_reviews: state.quorum().submitted_reviews(),
            quorum_complete: state.quorum().complete(),
            unconserved_findings: state.unconserved_current_findings(),
            cycles,
            findings,
        }
    }

    /// Returns the run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns the exact current revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the run phase.
    #[must_use]
    pub const fn phase(&self) -> ReviewRunPhase {
        self.phase
    }
    /// Returns the truthful D2 terminal, when present.
    #[must_use]
    pub const fn terminal(&self) -> Option<ReviewTerminalKind> {
        self.terminal
    }
    /// Returns the latest aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Returns the authoritative complete-state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    /// Returns the current submitted review count.
    #[must_use]
    pub const fn submitted_reviews(&self) -> u16 {
        self.submitted_reviews
    }
    /// Returns whether every independent current quorum dimension passes.
    #[must_use]
    pub const fn quorum_complete(&self) -> bool {
        self.quorum_complete
    }
    /// Borrows canonical unconserved current finding identities.
    #[must_use]
    pub fn unconserved_findings(&self) -> &[FindingId] {
        &self.unconserved_findings
    }
    /// Borrows every retained cycle row in ordinal order.
    #[must_use]
    pub fn cycles(&self) -> &[ProjectedCycle] {
        &self.cycles
    }
    /// Borrows every retained finding row in identity order.
    #[must_use]
    pub fn findings(&self) -> &[ProjectedFinding] {
        &self.findings
    }
}
