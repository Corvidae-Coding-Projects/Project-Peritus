//! Canonical evidence observations and checked evidence aggregate.

use crate::{
    ApprovalObservation, ApprovalSubject, CanonicalEvidenceCollection, EvidenceError,
    EvidenceErrorKind, GateObservation, ReviewObservation, WaiverObservation,
};
use peritus_spec::EvidenceRequirementId;
use peritus_types::{RevisionTuple, Sha256Digest};
use vstd::prelude::*;

verus! {

/// One required evidence artifact bound to the complete revision tuple.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceObservation {
    requirement_id: EvidenceRequirementId,
    revision: RevisionTuple,
    artifact_digest: Sha256Digest,
}

impl EvidenceObservation {
    /// Specification view of the exact producing revision.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }

    /// Creates an exact-revision evidence observation.
    #[must_use]
    pub const fn new(
        requirement_id: EvidenceRequirementId,
        revision: RevisionTuple,
        artifact_digest: Sha256Digest,
    ) -> Self {
        Self { requirement_id, revision, artifact_digest }
    }

    /// Returns the contract evidence requirement identity.
    #[must_use]
    pub const fn requirement_id(&self) -> EvidenceRequirementId { self.requirement_id }

    /// Returns the exact revision from which the evidence was produced.
    #[must_use]
    pub const fn revision(&self) -> (revision: RevisionTuple)
        ensures revision == self.spec_revision()
    { self.revision }

    /// Returns the content digest of the evidence artifact.
    #[must_use]
    pub const fn artifact_digest(&self) -> Sha256Digest { self.artifact_digest }
}

/// Checked canonical input to the acceptance evaluator.
#[derive(Debug, Eq, PartialEq)]
pub struct AcceptanceEvidence {
    gates: Vec<GateObservation>,
    reviews: Vec<ReviewObservation>,
    evidence: Vec<EvidenceObservation>,
    approvals: Vec<ApprovalObservation>,
    waivers: Vec<WaiverObservation>,
}

impl AcceptanceEvidence {
    /// Specification view of gate observations.
    pub closed spec fn spec_gates(&self) -> Seq<GateObservation> { self.gates@ }

    /// Specification view of review observations.
    pub closed spec fn spec_reviews(&self) -> Seq<ReviewObservation> { self.reviews@ }

    /// Specification view of required-evidence observations.
    pub closed spec fn spec_evidence(&self) -> Seq<EvidenceObservation> { self.evidence@ }

    /// Specification view of approval observations.
    pub closed spec fn spec_approvals(&self) -> Seq<ApprovalObservation> { self.approvals@ }

    /// Specification view of waiver observations.
    pub closed spec fn spec_waivers(&self) -> Seq<WaiverObservation> { self.waivers@ }

    /// INV-003 predicate over every supplied observation collection.
    pub open spec fn spec_all_current(&self, requested: RevisionTuple) -> bool {
        (forall |index: int| 0 <= index < self.spec_gates().len() ==>
            #[trigger] crate::model::revision_fresh(
                self.spec_gates()[index].spec_revision(), requested))
        && (forall |index: int| 0 <= index < self.spec_reviews().len() ==>
            #[trigger] crate::model::revision_fresh(
                self.spec_reviews()[index].spec_revision(), requested))
        && (forall |index: int| 0 <= index < self.spec_evidence().len() ==>
            #[trigger] crate::model::revision_fresh(
                self.spec_evidence()[index].spec_revision(), requested))
        && (forall |index: int| 0 <= index < self.spec_approvals().len() ==>
            #[trigger] crate::model::revision_fresh(
                self.spec_approvals()[index].spec_revision(), requested))
        && (forall |index: int| 0 <= index < self.spec_waivers().len() ==>
            #[trigger] crate::model::revision_fresh(
                self.spec_waivers()[index].spec_revision(), requested))
    }
    /// Validates and stores observations in canonical order.
    ///
    /// Gate, evidence, approval-request, review-cycle, and waiver-finding identities must be
    /// strictly ascending. Approval subjects must additionally be unique. Reviewer actor reuse is
    /// retained as evidence and evaluated according to the contract's independence policy.
    ///
    /// # Errors
    ///
    /// Returns the first duplicate, contradiction, or ordering failure in deterministic
    /// collection order.
    pub fn new(
        gates: Vec<GateObservation>,
        reviews: Vec<ReviewObservation>,
        evidence: Vec<EvidenceObservation>,
        approvals: Vec<ApprovalObservation>,
        waivers: Vec<WaiverObservation>,
    ) -> Result<Self, EvidenceError> {
        validate_gates(gates.as_slice())?;
        validate_reviews(reviews.as_slice())?;
        validate_evidence(evidence.as_slice())?;
        validate_approvals(approvals.as_slice())?;
        validate_waivers(waivers.as_slice(), approvals.as_slice())?;
        Ok(Self { gates, reviews, evidence, approvals, waivers })
    }

    /// Returns canonical gate observations.
    #[must_use]
    pub const fn gates(&self) -> (gates: &[GateObservation])
        ensures gates@ == self.spec_gates()
    { self.gates.as_slice() }

    /// Returns canonical review observations.
    #[must_use]
    pub const fn reviews(&self) -> (reviews: &[ReviewObservation])
        ensures reviews@ == self.spec_reviews()
    { self.reviews.as_slice() }

    /// Returns canonical required-evidence observations.
    #[must_use]
    pub const fn evidence(&self) -> (evidence: &[EvidenceObservation])
        ensures evidence@ == self.spec_evidence()
    { self.evidence.as_slice() }

    /// Returns canonical human approval observations.
    #[must_use]
    pub const fn approvals(&self) -> (approvals: &[ApprovalObservation])
        ensures approvals@ == self.spec_approvals()
    { self.approvals.as_slice() }

    /// Returns canonical waiver observations.
    #[must_use]
    pub const fn waivers(&self) -> (waivers: &[WaiverObservation])
        ensures waivers@ == self.spec_waivers()
    { self.waivers.as_slice() }
}

fn validate_gates(values: &[GateObservation]) -> Result<(), EvidenceError> {
    let mut index = 1;
    while index < values.len()
        invariant (values.len() == 0 && index == 1) || 1 <= index <= values.len(),
        decreases values.len() - index,
    {
        let previous = values[index - 1].gate_id();
        let current = values[index].gate_id();
        if previous == current {
            return Err(EvidenceError::new(
                EvidenceErrorKind::DuplicateObservation,
                CanonicalEvidenceCollection::Gates,
                index,
            ));
        }
        if previous > current {
            return Err(EvidenceError::new(
                EvidenceErrorKind::NonCanonicalOrder,
                CanonicalEvidenceCollection::Gates,
                index,
            ));
        }
        index += 1;
    }
    Ok(())
}

fn validate_reviews(values: &[ReviewObservation]) -> Result<(), EvidenceError> {
    let mut index = 0;
    while index < values.len()
        invariant 0 <= index <= values.len(),
        decreases values.len() - index,
    {
        if index > 0 {
            let previous = values[index - 1].cycle_id();
            let current = values[index].cycle_id();
            if previous == current {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::DuplicateObservation,
                    CanonicalEvidenceCollection::Reviews,
                    index,
                ));
            }
            if previous > current {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::NonCanonicalOrder,
                    CanonicalEvidenceCollection::Reviews,
                    index,
                ));
            }
            let previous_ordinal = values[index - 1].cycle_ordinal();
            let current_ordinal = values[index].cycle_ordinal();
            if previous_ordinal == current_ordinal {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::DuplicateObservation,
                    CanonicalEvidenceCollection::Reviews,
                    index,
                ));
            }
            if previous_ordinal > current_ordinal {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::NonCanonicalOrder,
                    CanonicalEvidenceCollection::Reviews,
                    index,
                ));
            }
        }
        let current_findings = values[index].findings();
        assert(index < values.len());
        let mut finding_index = 0;
        while finding_index < current_findings.len()
            invariant
                0 <= finding_index <= current_findings.len(),
                index < values.len(),
            decreases current_findings.len() - finding_index,
        {
            let current_finding_id = current_findings[finding_index].finding_id();
            let mut earlier_review = 0;
            while earlier_review < index
                invariant
                    0 <= earlier_review,
                    earlier_review <= index,
                    index <= values.len(),
                decreases index - earlier_review,
            {
                let earlier_findings = values[earlier_review].findings();
                let mut earlier_finding = 0;
                while earlier_finding < earlier_findings.len()
                    invariant 0 <= earlier_finding <= earlier_findings.len(),
                    decreases earlier_findings.len() - earlier_finding,
                {
                    if earlier_findings[earlier_finding].finding_id() == current_finding_id {
                        return Err(EvidenceError::new(
                            EvidenceErrorKind::DuplicateObservation,
                            CanonicalEvidenceCollection::Findings,
                            finding_index,
                        ));
                    }
                    earlier_finding += 1;
                }
                earlier_review += 1;
            }
            finding_index += 1;
        }
        index += 1;
    }
    Ok(())
}

fn validate_evidence(values: &[EvidenceObservation]) -> Result<(), EvidenceError> {
    let mut index = 1;
    while index < values.len()
        invariant (values.len() == 0 && index == 1) || 1 <= index <= values.len(),
        decreases values.len() - index,
    {
        let previous = values[index - 1].requirement_id();
        let current = values[index].requirement_id();
        if previous == current {
            return Err(EvidenceError::new(
                EvidenceErrorKind::DuplicateObservation,
                CanonicalEvidenceCollection::Evidence,
                index,
            ));
        }
        if previous > current {
            return Err(EvidenceError::new(
                EvidenceErrorKind::NonCanonicalOrder,
                CanonicalEvidenceCollection::Evidence,
                index,
            ));
        }
        index += 1;
    }
    Ok(())
}

fn validate_approvals(values: &[ApprovalObservation]) -> Result<(), EvidenceError> {
    let mut index = 0;
    while index < values.len()
        invariant 0 <= index <= values.len(),
        decreases values.len() - index,
    {
        if index > 0 {
            let previous = values[index - 1].request_id();
            let current = values[index].request_id();
            if previous == current {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::DuplicateObservation,
                    CanonicalEvidenceCollection::Approvals,
                    index,
                ));
            }
            if previous > current {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::NonCanonicalOrder,
                    CanonicalEvidenceCollection::Approvals,
                    index,
                ));
            }
        }
        let subject = values[index].subject();
        let mut previous_index = 0;
        while previous_index < index
            invariant 0 <= previous_index <= index <= values.len(),
            decreases index - previous_index,
        {
            if values[previous_index].subject() == subject {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::DuplicateApprovalSubject,
                    CanonicalEvidenceCollection::Approvals,
                    index,
                ));
            }
            previous_index += 1;
        }
        index += 1;
    }
    Ok(())
}

fn validate_waivers(
    values: &[WaiverObservation],
    approvals: &[ApprovalObservation],
) -> Result<(), EvidenceError> {
    let mut index = 0;
    while index < values.len()
        invariant 0 <= index <= values.len(),
        decreases values.len() - index,
    {
        if index > 0 {
            let previous = values[index - 1].finding_id();
            let current = values[index].finding_id();
            if previous == current {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::DuplicateObservation,
                    CanonicalEvidenceCollection::Waivers,
                    index,
                ));
            }
            if previous > current {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::NonCanonicalOrder,
                    CanonicalEvidenceCollection::Waivers,
                    index,
                ));
            }
        }
        let mut approval_index = 0;
        while approval_index < approvals.len()
            invariant
                0 <= approval_index <= approvals.len(),
                index < values.len(),
            decreases approvals.len() - approval_index,
        {
            if approvals[approval_index].request_id() == values[index].approval_request_id()
                && approvals[approval_index].subject()
                    != ApprovalSubject::FindingWaiver(values[index].finding_id())
            {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::WaiverApprovalSubjectMismatch,
                    CanonicalEvidenceCollection::Waivers,
                    index,
                ));
            }
            approval_index += 1;
        }
        index += 1;
    }
    Ok(())
}

} // verus!
