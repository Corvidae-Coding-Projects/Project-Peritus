//! Cross-component acceptance-contract validation.

use crate::{
    Assumption, CanonicalCollection, CompletionPolicy, EvidenceRequirement, EvidenceRequirementId,
    EvidenceSource, Exclusion, GateGraph, HumanApprovalPolicy, Requirement, ReviewCategory,
    ReviewPolicy, SpecError, WaiverPolicy,
};
use vstd::prelude::*;

verus! {

fn has_evidence(values: &[EvidenceRequirement], target: EvidenceRequirementId) -> bool {
    let mut index = 0;
    while index < values.len()
        invariant index <= values.len(),
        decreases values.len() - index,
    {
        if values[index].id() == target { return true; }
        index += 1;
    }
    false
}

fn evidence_source(
    values: &[EvidenceRequirement],
    target: EvidenceRequirementId,
) -> Option<EvidenceSource> {
    let mut index = 0;
    while index < values.len()
        invariant index <= values.len(),
        decreases values.len() - index,
    {
        if values[index].id() == target { return Some(values[index].source()); }
        index += 1;
    }
    None
}

fn has_category(values: &[ReviewCategory], target: ReviewCategory) -> bool {
    let mut index = 0;
    while index < values.len()
        invariant index <= values.len(),
        decreases values.len() - index,
    {
        if values[index] == target { return true; }
        index += 1;
    }
    false
}

fn validate_requirements(values: &[Requirement]) -> Result<(), SpecError> {
    if values.is_empty() {
        return Err(SpecError::EmptyCollection(CanonicalCollection::Requirements));
    }
    let mut index = 0;
    while index < values.len()
        invariant index <= values.len(),
        decreases values.len() - index,
    {
        if index > 0 {
            if values[index - 1].id() == values[index].id() {
                return Err(SpecError::DuplicateCanonicalValue(CanonicalCollection::Requirements));
            }
            if values[index - 1].id() > values[index].id() {
                return Err(SpecError::NonCanonicalOrder(CanonicalCollection::Requirements));
            }
        }
        index += 1;
    }
    Ok(())
}

fn validate_exclusions(values: &[Exclusion]) -> Result<(), SpecError> {
    let mut index = 0;
    while index < values.len()
        invariant index <= values.len(),
        decreases values.len() - index,
    {
        if index > 0 {
            if values[index - 1].content() == values[index].content() {
                return Err(SpecError::DuplicateCanonicalValue(CanonicalCollection::Exclusions));
            }
            if values[index - 1].content() > values[index].content() {
                return Err(SpecError::NonCanonicalOrder(CanonicalCollection::Exclusions));
            }
        }
        index += 1;
    }
    Ok(())
}

fn validate_assumptions(values: &[Assumption]) -> Result<(), SpecError> {
    let mut index = 0;
    while index < values.len()
        invariant index <= values.len(),
        decreases values.len() - index,
    {
        if index > 0 {
            if values[index - 1].content() == values[index].content() {
                return Err(SpecError::DuplicateCanonicalValue(CanonicalCollection::Assumptions));
            }
            if values[index - 1].content() > values[index].content() {
                return Err(SpecError::NonCanonicalOrder(CanonicalCollection::Assumptions));
            }
        }
        index += 1;
    }
    Ok(())
}

fn validate_evidence(
    values: &[EvidenceRequirement],
    gates: &GateGraph,
    review: &ReviewPolicy,
) -> Result<(), SpecError> {
    if values.is_empty() {
        return Err(SpecError::EmptyCollection(CanonicalCollection::EvidenceRequirements));
    }
    let mut index = 0;
    while index < values.len()
        invariant index <= values.len(),
        decreases values.len() - index,
    {
        if index > 0 {
            if values[index - 1].id() == values[index].id() {
                return Err(SpecError::DuplicateCanonicalValue(
                    CanonicalCollection::EvidenceRequirements,
                ));
            }
            if values[index - 1].id() > values[index].id() {
                return Err(SpecError::NonCanonicalOrder(
                    CanonicalCollection::EvidenceRequirements,
                ));
            }
        }
        match values[index].source() {
            EvidenceSource::Gate(id) if gates.get(id).is_none() => {
                return Err(SpecError::InvalidEvidenceSource(values[index].id()));
            }
            EvidenceSource::Review(category)
                if !has_category(review.required_categories(), category) =>
            {
                return Err(SpecError::InvalidEvidenceSource(values[index].id()));
            }
            _ => {}
        }
        index += 1;
    }
    Ok(())
}

fn validate_gate_evidence(
    gates: &GateGraph,
    evidence: &[EvidenceRequirement],
) -> Result<(), SpecError> {
    let mut gate_index = 0;
    let gate_definitions = gates.definitions();
    while gate_index < gate_definitions.len()
        invariant gate_index <= gate_definitions@.len(),
        decreases gate_definitions.len() - gate_index,
    {
        let gate = &gate_definitions[gate_index];
        let mut evidence_index = 0;
        let required_evidence = gate.required_evidence();
        while evidence_index < required_evidence.len()
            invariant
                evidence_index <= required_evidence@.len(),
                gate_index < gate_definitions@.len(),
            decreases required_evidence.len() - evidence_index,
        {
            let required = required_evidence[evidence_index];
            if !has_evidence(evidence, required) {
                return Err(SpecError::UnknownEvidenceRequirement(gate.id()));
            }
            match evidence_source(evidence, required) {
                Some(EvidenceSource::Gate(source)) if source == gate.id() => {}
                _ => return Err(SpecError::InvalidEvidenceSource(required)),
            }
            evidence_index += 1;
        }
        gate_index += 1;
    }
    Ok(())
}

fn validate_authority_evidence(
    evidence: &[EvidenceRequirement],
    approval: HumanApprovalPolicy,
    waiver: WaiverPolicy,
) -> Result<(), SpecError> {
    if approval.is_required() {
        let mut index = 0;
        let mut found = false;
        while index < evidence.len()
            invariant index <= evidence.len(),
            decreases evidence.len() - index,
        {
            if matches!(evidence[index].source(), EvidenceSource::HumanApproval) {
                found = true;
                break;
            }
            index += 1;
        }
        if !found { return Err(SpecError::MissingApprovalEvidence); }
    }
    #[allow(clippy::collapsible_if, reason = "keeps the Verus-supported control flow explicit")]
    if let Some(required) = waiver.evidence_requirement() {
        if evidence_source(evidence, required) != Some(EvidenceSource::WaiverAuthorization) {
            return Err(SpecError::InvalidWaiverEvidence);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, reason = "validation receives each contract component explicitly")]
pub(super) fn validate_contract(
    requirements: &[Requirement],
    exclusions: &[Exclusion],
    assumptions: &[Assumption],
    gates: &GateGraph,
    review: &ReviewPolicy,
    completion: CompletionPolicy,
    evidence: &[EvidenceRequirement],
    approval: HumanApprovalPolicy,
    waiver: WaiverPolicy,
) -> Result<(), SpecError> {
    if review.reviewer_quorum() > completion.max_review_cycles() {
        return Err(SpecError::ReviewQuorumExceedsCycleLimit {
            reviewer_quorum: review.reviewer_quorum(),
            max_review_cycles: completion.max_review_cycles(),
        });
    }
    validate_requirements(requirements)?;
    validate_exclusions(exclusions)?;
    validate_assumptions(assumptions)?;
    validate_evidence(evidence, gates, review)?;
    validate_gate_evidence(gates, evidence)?;
    validate_authority_evidence(evidence, approval, waiver)
}

} // verus!
