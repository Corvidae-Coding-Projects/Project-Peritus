//! Checked deterministic gate declarations and dependency graph.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{CanonicalCollection, ContentReference, EvidenceRequirementId, LimitKind, SpecError};
use peritus_types::{EnvironmentId, GateId};

mod query;
#[doc = " Declares which revision dimensions a gate executor must consider when reusing raw results."]
#[doc = ""]
#[doc = " Acceptance evidence is nevertheless emitted against and counted for one exact revision tuple."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateFreshnessScope {
    #[doc = " Every change to the complete revision tuple requires a new execution."]
    ExactRevisionTuple,
    #[doc = " A gate engine may prove workspace content unchanged before reusing execution output."]
    WorkspaceContent,
}
#[doc = " Frozen rule by which a gate result reports success."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateSuccessRule {
    #[doc = " Process completion is successful exactly when the exit code is zero."]
    ExitCodeZero,
    #[doc = " A separately versioned parser evaluates the referenced structured predicate."]
    Predicate(ContentReference),
}
#[doc = " Immutable execution inputs declared by a gate."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GateExecutionPlan {
    action: ContentReference,
    environment: EnvironmentId,
    inputs: ContentReference,
    parser: ContentReference,
    success: GateSuccessRule,
    timeout_ms: u64,
    resources: ContentReference,
    freshness: GateFreshnessScope,
}
impl GateExecutionPlan {
    #[doc = " Creates a plan with a nonzero timeout."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`SpecError::ZeroLimit`] when `timeout_ms` is zero."]
    #[allow(
        clippy::too_many_arguments,
        reason = "the frozen gate plan keeps each execution binding explicit"
    )]
    pub const fn new(
        action: ContentReference,
        environment: EnvironmentId,
        inputs: ContentReference,
        parser: ContentReference,
        success: GateSuccessRule,
        timeout_ms: u64,
        resources: ContentReference,
        freshness: GateFreshnessScope,
    ) -> Result<Self, SpecError> {
        if timeout_ms == 0 {
            return Err(SpecError::ZeroLimit(LimitKind::GateTimeout));
        }
        Ok(Self { action, environment, inputs, parser, success, timeout_ms, resources, freshness })
    }
    #[doc = " Returns the immutable structured action plan."]
    #[must_use]
    pub const fn action(&self) -> ContentReference {
        self.action
    }
    #[doc = " Returns the exact execution environment identity."]
    #[must_use]
    pub const fn environment(&self) -> EnvironmentId {
        self.environment
    }
    #[doc = " Returns the immutable input manifest."]
    #[must_use]
    pub const fn inputs(&self) -> ContentReference {
        self.inputs
    }
    #[doc = " Returns the immutable result parser."]
    #[must_use]
    pub const fn parser(&self) -> ContentReference {
        self.parser
    }
    #[doc = " Returns the frozen success rule."]
    #[must_use]
    pub const fn success_rule(&self) -> GateSuccessRule {
        self.success
    }
    #[doc = " Returns the gate timeout in milliseconds."]
    #[must_use]
    pub const fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }
    #[doc = " Returns the immutable resource-limit declaration."]
    #[must_use]
    pub const fn resources(&self) -> ContentReference {
        self.resources
    }
    #[doc = " Returns the declared freshness scope."]
    #[must_use]
    pub const fn freshness(&self) -> GateFreshnessScope {
        self.freshness
    }
}
#[doc = " One checked gate node in the acceptance dependency graph."]
#[derive(Debug, Eq, PartialEq)]
pub struct GateDefinition {
    id: GateId,
    plan: GateExecutionPlan,
    dependencies: Vec<GateId>,
    required_evidence: Vec<EvidenceRequirementId>,
}
impl GateDefinition {
    #[doc = " Creates a gate whose dependency and evidence identifiers are strictly ordered."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed duplicate, ordering, or self-dependency failure."]
    pub fn new(
        id: GateId,
        plan: GateExecutionPlan,
        dependencies: Vec<GateId>,
        required_evidence: Vec<EvidenceRequirementId>,
    ) -> Result<Self, SpecError> {
        let mut index = 0;
        while index < dependencies.len() {
            if dependencies[index] == id {
                return Err(SpecError::SelfDependency(id));
            }
            if index > 0 {
                if dependencies[index - 1] == dependencies[index] {
                    return Err(SpecError::DuplicateCanonicalValue(
                        CanonicalCollection::GateDependencies,
                    ));
                }
                if dependencies[index - 1] > dependencies[index] {
                    return Err(SpecError::NonCanonicalOrder(
                        CanonicalCollection::GateDependencies,
                    ));
                }
            }
            index += 1;
        }
        index = 0;
        while index < required_evidence.len() {
            if index > 0 {
                if required_evidence[index - 1] == required_evidence[index] {
                    return Err(SpecError::DuplicateCanonicalValue(
                        CanonicalCollection::GateEvidence,
                    ));
                }
                if required_evidence[index - 1] > required_evidence[index] {
                    return Err(SpecError::NonCanonicalOrder(CanonicalCollection::GateEvidence));
                }
            }
            index += 1;
        }
        Ok(Self { id, plan, dependencies, required_evidence })
    }
    #[doc = " Returns the gate identifier."]
    #[must_use]
    pub const fn id(&self) -> GateId {
        self.id
    }
    #[doc = " Returns the immutable execution plan."]
    #[must_use]
    pub const fn plan(&self) -> GateExecutionPlan {
        self.plan
    }
    #[doc = " Returns dependency identifiers in canonical order."]
    #[must_use]
    pub const fn dependencies(&self) -> &[GateId] {
        self.dependencies.as_slice()
    }
    #[doc = " Returns gate-specific evidence identifiers in canonical order."]
    #[must_use]
    pub const fn required_evidence(&self) -> &[EvidenceRequirementId] {
        self.required_evidence.as_slice()
    }
}
#[doc = " Validated acyclic graph of every deterministic acceptance gate."]
#[derive(Debug, Eq, PartialEq)]
pub struct GateGraph {
    definitions: Vec<GateDefinition>,
    execution_order: Vec<GateId>,
}
impl GateGraph {}
fn contains_gate(values: &[GateId], target: GateId) -> bool {
    let mut index = 0;
    while index < values.len() {
        if values[index] == target {
            return true;
        }
        index += 1;
    }
    false
}
fn find_definition(definitions: &[GateDefinition], target: GateId) -> Option<&GateDefinition> {
    let mut index = 0;
    while index < definitions.len() {
        if definitions[index].id() == target {
            return Some(&definitions[index]);
        }
        index += 1;
    }
    None
}
fn dependencies_resolved(definition: &GateDefinition, resolved: &[GateId]) -> bool {
    let mut index = 0;
    let dependencies = definition.dependencies();
    while index < dependencies.len() {
        if !contains_gate(resolved, dependencies[index]) {
            return false;
        }
        index += 1;
    }
    true
}
impl GateGraph {
    #[doc = " Validates a nonempty canonical gate set, declared dependencies, and acyclicity."]
    #[doc = ""]
    #[doc = " The stored topological order is deterministic: the lowest canonical eligible gate is"]
    #[doc = " selected at each step."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed empty, duplicate, ordering, unknown-dependency, or cycle failure."]
    pub fn new(definitions: Vec<GateDefinition>) -> Result<Self, SpecError> {
        if definitions.is_empty() {
            return Err(SpecError::EmptyCollection(CanonicalCollection::Gates));
        }
        let mut index = 0;
        while index < definitions.len() {
            if index > 0 {
                if definitions[index - 1].id() == definitions[index].id() {
                    return Err(SpecError::DuplicateCanonicalValue(CanonicalCollection::Gates));
                }
                if definitions[index - 1].id() > definitions[index].id() {
                    return Err(SpecError::NonCanonicalOrder(CanonicalCollection::Gates));
                }
            }
            index += 1;
        }
        index = 0;
        while index < definitions.len() {
            let mut dependency = 0;
            let dependencies = definitions[index].dependencies();
            while dependency < dependencies.len() {
                let target = dependencies[dependency];
                if find_definition(definitions.as_slice(), target).is_none() {
                    return Err(SpecError::UnknownGateDependency {
                        gate: definitions[index].id(),
                        dependency: target,
                    });
                }
                dependency += 1;
            }
            index += 1;
        }
        let mut execution_order = Vec::new();
        while execution_order.len() < definitions.len() {
            let mut candidate = 0;
            let mut selected = None;
            while candidate < definitions.len() {
                let id = definitions[candidate].id();
                if !contains_gate(execution_order.as_slice(), id)
                    && dependencies_resolved(&definitions[candidate], execution_order.as_slice())
                {
                    selected = Some(id);
                    break;
                }
                candidate += 1;
            }
            match selected {
                Some(id) => execution_order.push(id),
                None => return Err(SpecError::GateCycle),
            }
        }
        if !crate::gate_model::execution_order_is_valid(
            definitions.as_slice(),
            execution_order.as_slice(),
        ) {
            return Err(SpecError::GateCycle);
        }
        Ok(Self { definitions, execution_order })
    }
    #[doc = " Returns gate definitions in canonical identifier order."]
    #[must_use]
    pub const fn definitions(&self) -> &[GateDefinition] {
        self.definitions.as_slice()
    }
    #[doc = " Returns the deterministic dependency-respecting execution order."]
    #[must_use]
    pub const fn execution_order(&self) -> &[GateId] {
        self.execution_order.as_slice()
    }
    #[doc = " Returns the declared definition for `id`."]
    #[must_use]
    #[allow(clippy::option_if_let_else, reason = "explicit branches keep the Verus model direct")]
    pub fn get(&self, id: GateId) -> Option<&GateDefinition> {
        find_definition(self.definitions.as_slice(), id)
    }
}
