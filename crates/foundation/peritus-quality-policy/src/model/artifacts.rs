//! Contract-derived artifact membership and exact-revision completeness.

#[cfg(verus_only)]
use crate::{AcceptanceEvidence, EvidenceObservation};
#[cfg(verus_only)]
use peritus_spec::{AcceptanceContract, EvidenceRequirement, EvidenceRequirementId};
#[cfg(verus_only)]
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

/// Exact byte equality of content-addressed evidence requirement identities.
pub open spec fn evidence_requirement_matches(
    left: EvidenceRequirementId,
    right: EvidenceRequirementId,
) -> bool {
    forall |index: int| 0 <= index < 32 ==>
        left.spec_digest().spec_bytes()[index] == right.spec_digest().spec_bytes()[index]
}

/// The supplied requirement identity occurs in the immutable contract declarations.
pub open spec fn artifact_declared(
    requirements: Seq<EvidenceRequirement>,
    target: EvidenceRequirementId,
) -> bool {
    exists |index: int| 0 <= index < requirements.len()
        && evidence_requirement_matches(#[trigger] requirements[index].spec_id(), target)
}

/// At least one observation names the exact requirement and requested revision.
pub open spec fn current_artifact_present(
    observations: Seq<EvidenceObservation>,
    target: EvidenceRequirementId,
    requested: RevisionTuple,
) -> bool {
    exists |index: int| 0 <= index < observations.len()
        && evidence_requirement_matches(#[trigger] observations[index].spec_requirement_id(), target)
        && crate::model::revision_fresh(observations[index].spec_revision(), requested)
}

/// Every current artifact observation names a requirement declared by this contract.
pub open spec fn current_artifacts_declared(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
) -> bool {
    forall |index: int| 0 <= index < evidence.spec_evidence().len()
        && crate::model::revision_fresh(
            #[trigger] evidence.spec_evidence()[index].spec_revision(), requested)
        ==> artifact_declared(
            contract.spec_evidence_requirements(),
            evidence.spec_evidence()[index].spec_requirement_id(),
        )
}

/// Every contract-declared artifact has an observation for the exact requested revision.
pub open spec fn required_artifacts_present(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
) -> bool {
    forall |index: int| 0 <= index < contract.spec_evidence_requirements().len()
        ==> current_artifact_present(
            evidence.spec_evidence(),
            #[trigger] contract.spec_evidence_requirements()[index].spec_id(),
            requested,
        )
}

/// Exact artifact-family completeness, independent of evaluator flags and diagnostics.
pub open spec fn required_artifacts_complete(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
) -> bool {
    current_artifacts_declared(contract, requested, evidence)
        && required_artifacts_present(contract, requested, evidence)
}

} // verus!
