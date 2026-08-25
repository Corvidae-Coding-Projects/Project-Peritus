//! Exact acceptance-contract to C4 quality-definition planning.

use std::collections::{BTreeMap, BTreeSet};

use peritus_spec::{
    AcceptanceContract, ContractBinding, EvidenceRequirementId, GateExecutionPlan, GateSuccessRule,
};
use peritus_tools_quality::{
    CheckCatalog, CheckDefinition, CheckRequirement, QualityAcceptanceBinding,
};
use peritus_types::{AcceptanceSpecId, EnvironmentId, GateId, RevisionTuple, RunId, Sha256Digest};
use sha2::{Digest, Sha256};

use crate::error::{GateError, GateRejection, reject};

/// Hard D1 gate-count bound independent from codec collection ceilings.
pub const MAX_GATES_PER_RUN: usize = 1_024;
/// Hard direct dependency bound on one planned gate.
pub const MAX_GATE_DEPENDENCIES: usize = 1_024;
/// Hard required-evidence declaration bound on one planned gate.
pub const MAX_GATE_EVIDENCE: usize = 1_024;
/// Hard total attempt-accounting bound for one run.
pub const MAX_TOTAL_GATE_ATTEMPTS: usize =
    peritus_codec::CodecLimits::PRODUCTION.max_collection_items;

/// One contract gate bound to its exact C4 quality implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedGate {
    id: GateId,
    execution: GateExecutionPlan,
    dependencies: Vec<GateId>,
    required_evidence: Vec<EvidenceRequirementId>,
    quality: CheckDefinition,
    quality_binding: QualityAcceptanceBinding,
}

impl PlannedGate {
    /// Returns the stable B2 gate identity.
    #[must_use]
    pub const fn id(&self) -> GateId {
        self.id
    }

    /// Returns the frozen B2 execution declaration.
    #[must_use]
    pub const fn execution(&self) -> GateExecutionPlan {
        self.execution
    }

    /// Borrows dependency identifiers in canonical order.
    #[must_use]
    pub fn dependencies(&self) -> &[GateId] {
        &self.dependencies
    }

    /// Borrows required evidence identifiers in canonical order.
    #[must_use]
    pub fn required_evidence(&self) -> &[EvidenceRequirementId] {
        &self.required_evidence
    }

    /// Borrows the exact authorized C4 definition.
    #[must_use]
    pub const fn quality_definition(&self) -> &CheckDefinition {
        &self.quality
    }

    /// Returns the component-by-component B2/C4 binding.
    #[must_use]
    pub const fn quality_binding(&self) -> QualityAcceptanceBinding {
        self.quality_binding
    }
}

/// Complete deterministic D1 plan for one run and exact revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GatePlan {
    run_id: RunId,
    contract_id: AcceptanceSpecId,
    contract_digest: Sha256Digest,
    revision: RevisionTuple,
    maximum_attempts: u16,
    gates: Vec<PlannedGate>,
    execution_order: Vec<GateId>,
    digest: Sha256Digest,
}

impl GatePlan {
    /// Binds every declared acceptance gate to an exact explicit C4 quality definition.
    ///
    /// # Errors
    /// Rejects revision drift, bounds, absent/duplicate implementations, discovered-only or
    /// optional definitions, and any B2/C4 execution-field mismatch.
    pub fn new(
        run_id: RunId,
        contract: &AcceptanceContract,
        revision: RevisionTuple,
        catalog: &CheckCatalog,
        environment: EnvironmentId,
    ) -> Result<Self, GateError> {
        let binding = contract.bind(revision).map_err(|_| {
            reject(
                GateRejection::BindingMismatch,
                "acceptance contract does not bind the requested exact revision",
            )
        })?;
        let graph = contract.gates();
        if graph.definitions().len() > MAX_GATES_PER_RUN {
            return Err(reject(
                GateRejection::LimitExceeded,
                "acceptance contract exceeds the D1 gate-count bound",
            ));
        }
        let maximum_attempts = contract.completion_policy().max_gate_attempts();
        if !total_attempts_within_bound(graph.definitions().len(), maximum_attempts) {
            return Err(reject(
                GateRejection::LimitExceeded,
                "acceptance contract exceeds the D1 total-attempt accounting bound",
            ));
        }
        let mut quality_by_id = BTreeMap::new();
        for discovered in catalog.checks() {
            let definition = discovered.definition();
            if quality_by_id.insert(definition.gate_id(), definition).is_some() {
                return Err(reject(
                    GateRejection::BindingMismatch,
                    "quality catalog contains duplicate gate identities",
                ));
            }
        }
        let mut gates = Vec::with_capacity(graph.definitions().len());
        for definition in graph.definitions() {
            if definition.dependencies().len() > MAX_GATE_DEPENDENCIES
                || definition.required_evidence().len() > MAX_GATE_EVIDENCE
            {
                return Err(reject(
                    GateRejection::LimitExceeded,
                    "gate dependencies or evidence declarations exceed D1 bounds",
                ));
            }
            let quality = quality_by_id.get(&definition.id()).ok_or_else(|| {
                reject(
                    GateRejection::BindingMismatch,
                    "acceptance gate has no exact quality definition",
                )
            })?;
            if quality.requirement() != CheckRequirement::Required {
                return Err(reject(
                    GateRejection::BindingMismatch,
                    "acceptance gate must bind an explicit required quality definition",
                ));
            }
            let quality_binding = quality.acceptance_binding(environment).map_err(|error| {
                GateError::sourced(
                    crate::GateErrorKind::Quality,
                    crate::GateRecoveryAction::CorrectInput,
                    "quality definition cannot be represented by the acceptance contract",
                    error,
                )
            })?;
            if definition.plan().timeout_ms() != quality.timeout_millis()
                || !quality_binding.matches(definition.plan())
            {
                return Err(reject(
                    GateRejection::BindingMismatch,
                    "B2 gate plan differs from the exact C4 quality definition",
                ));
            }
            gates.push(PlannedGate {
                id: definition.id(),
                execution: definition.plan(),
                dependencies: definition.dependencies().to_vec(),
                required_evidence: definition.required_evidence().to_vec(),
                quality: (*quality).clone(),
                quality_binding,
            });
        }
        let execution_order = graph.execution_order().to_vec();
        validate_order(&gates, &execution_order)?;
        let digest = plan_digest(run_id, binding, maximum_attempts, &gates, &execution_order);
        Ok(Self {
            run_id,
            contract_id: binding.contract_id(),
            contract_digest: binding.contract_digest(),
            revision,
            maximum_attempts,
            gates,
            execution_order,
            digest,
        })
    }

    /// Returns the stable run identity used by C0.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }

    /// Returns the immutable acceptance specification identity.
    #[must_use]
    pub const fn contract_id(&self) -> AcceptanceSpecId {
        self.contract_id
    }

    /// Returns the digest of the complete immutable acceptance contract.
    #[must_use]
    pub const fn contract_digest(&self) -> Sha256Digest {
        self.contract_digest
    }

    /// Returns the exact complete revision tuple.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }

    /// Returns the per-gate attempt cap.
    #[must_use]
    pub const fn maximum_attempts(&self) -> u16 {
        self.maximum_attempts
    }

    /// Borrows planned gates in canonical gate-identifier order.
    #[must_use]
    pub fn gates(&self) -> &[PlannedGate] {
        &self.gates
    }

    /// Borrows the contract-proven deterministic topological order.
    #[must_use]
    pub fn execution_order(&self) -> &[GateId] {
        &self.execution_order
    }

    /// Looks up one exact declared gate.
    #[must_use]
    pub fn gate(&self, id: GateId) -> Option<&PlannedGate> {
        self.gates.binary_search_by_key(&id, PlannedGate::id).ok().map(|index| &self.gates[index])
    }

    /// Returns the canonical digest of the complete execution plan.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn total_attempts_within_bound(gate_count: usize, maximum_attempts: u16) -> bool {
    matches!(
        gate_count.checked_mul(usize::from(maximum_attempts)),
        Some(total) if total <= MAX_TOTAL_GATE_ATTEMPTS
    )
}

fn validate_order(gates: &[PlannedGate], order: &[GateId]) -> Result<(), GateError> {
    if gates.len() != order.len() {
        return Err(reject(
            GateRejection::BindingMismatch,
            "gate execution order does not cover the exact gate set",
        ));
    }
    let mut resolved = BTreeSet::new();
    for id in order {
        let gate = gates
            .binary_search_by_key(id, PlannedGate::id)
            .ok()
            .map(|index| &gates[index])
            .ok_or_else(|| {
                reject(
                    GateRejection::BindingMismatch,
                    "gate execution order contains an unknown identity",
                )
            })?;
        if !gate.dependencies.iter().all(|dependency| resolved.contains(dependency))
            || !resolved.insert(*id)
        {
            return Err(reject(
                GateRejection::BindingMismatch,
                "gate execution order violates dependency or uniqueness invariants",
            ));
        }
    }
    Ok(())
}

fn plan_digest(
    run_id: RunId,
    binding: ContractBinding,
    maximum_attempts: u16,
    gates: &[PlannedGate],
    execution_order: &[GateId],
) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(b"peritus-d1-gate-plan-v1\0");
    hash.update(run_id.as_bytes());
    hash.update(binding.contract_id().as_bytes());
    hash.update(binding.contract_digest().as_bytes());
    append_revision(&mut hash, binding.revision());
    hash.update(maximum_attempts.to_be_bytes());
    hash.update(u64::try_from(gates.len()).unwrap_or(u64::MAX).to_be_bytes());
    for gate in gates {
        hash.update(gate.id.as_bytes());
        hash.update(gate.quality_binding.definition_digest().as_bytes());
        append_execution(&mut hash, gate.execution);
        append_ids(&mut hash, &gate.dependencies);
        hash.update(u64::try_from(gate.required_evidence.len()).unwrap_or(u64::MAX).to_be_bytes());
        for evidence in &gate.required_evidence {
            hash.update(evidence.digest().as_bytes());
        }
    }
    append_ids(&mut hash, execution_order);
    Sha256Digest::new(hash.finalize().into())
}

fn append_execution(hash: &mut Sha256, plan: GateExecutionPlan) {
    hash.update(plan.action().digest().as_bytes());
    hash.update(plan.environment().as_bytes());
    hash.update(plan.inputs().digest().as_bytes());
    hash.update(plan.parser().digest().as_bytes());
    match plan.success_rule() {
        GateSuccessRule::ExitCodeZero => hash.update([1]),
        GateSuccessRule::Predicate(reference) => {
            hash.update([2]);
            hash.update(reference.digest().as_bytes());
        }
    }
    hash.update(plan.timeout_ms().to_be_bytes());
    hash.update(plan.resources().digest().as_bytes());
    hash.update([plan.freshness() as u8]);
}

fn append_ids(hash: &mut Sha256, ids: &[GateId]) {
    hash.update(u64::try_from(ids.len()).unwrap_or(u64::MAX).to_be_bytes());
    for id in ids {
        hash.update(id.as_bytes());
    }
}

fn append_revision(hash: &mut Sha256, revision: RevisionTuple) {
    hash.update(revision.acceptance_spec_id().as_bytes());
    hash.update(revision.harness_id().as_bytes());
    hash.update(revision.workspace_id().as_bytes());
    hash.update(revision.workspace_generation().get().to_be_bytes());
    hash.update(revision.workspace_revision().get().to_be_bytes());
    hash.update(revision.policy_id().as_bytes());
    hash.update(revision.provider_profile_id().as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_attempt_bound_matches_the_production_codec_collection_ceiling() {
        assert_eq!(MAX_TOTAL_GATE_ATTEMPTS, 65_535);
        assert!(total_attempts_within_bound(1, u16::MAX));
        assert!(!total_attempts_within_bound(2, 32_768));
    }
}
