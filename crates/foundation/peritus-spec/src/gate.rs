//! Checked deterministic gate declarations and dependency graph.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{CanonicalCollection, ContentReference, EvidenceRequirementId, LimitKind, SpecError};
use peritus_types::{EnvironmentId, GateId};
use vstd::prelude::*;

verus! {

mod query;

/// Declares which revision dimensions a gate executor must consider when reusing raw results.
///
/// Acceptance evidence is nevertheless emitted against and counted for one exact revision tuple.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateFreshnessScope {
    /// Every change to the complete revision tuple requires a new execution.
    ExactRevisionTuple,
    /// A gate engine may prove workspace content unchanged before reusing execution output.
    WorkspaceContent,
}

/// Frozen rule by which a gate result reports success.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateSuccessRule {
    /// Process completion is successful exactly when the exit code is zero.
    ExitCodeZero,
    /// A separately versioned parser evaluates the referenced structured predicate.
    Predicate(ContentReference),
}

/// Immutable execution inputs declared by a gate.
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
    /// Creates a plan with a nonzero timeout.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError::ZeroLimit`] when `timeout_ms` is zero.
    #[allow(clippy::too_many_arguments, reason = "the frozen gate plan keeps each execution binding explicit")]
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

    /// Returns the immutable structured action plan.
    #[must_use]
    pub const fn action(&self) -> ContentReference { self.action }

    /// Returns the exact execution environment identity.
    #[must_use]
    pub const fn environment(&self) -> EnvironmentId { self.environment }

    /// Returns the immutable input manifest.
    #[must_use]
    pub const fn inputs(&self) -> ContentReference { self.inputs }

    /// Returns the immutable result parser.
    #[must_use]
    pub const fn parser(&self) -> ContentReference { self.parser }

    /// Returns the frozen success rule.
    #[must_use]
    pub const fn success_rule(&self) -> GateSuccessRule { self.success }

    /// Returns the gate timeout in milliseconds.
    #[must_use]
    pub const fn timeout_ms(&self) -> u64 { self.timeout_ms }

    /// Returns the immutable resource-limit declaration.
    #[must_use]
    pub const fn resources(&self) -> ContentReference { self.resources }

    /// Returns the declared freshness scope.
    #[must_use]
    pub const fn freshness(&self) -> GateFreshnessScope { self.freshness }
}

/// One checked gate node in the acceptance dependency graph.
#[derive(Debug, Eq, PartialEq)]
pub struct GateDefinition {
    id: GateId,
    plan: GateExecutionPlan,
    dependencies: Vec<GateId>,
    required_evidence: Vec<EvidenceRequirementId>,
}

impl GateDefinition {
    /// Specification view of the exact gate identity.
    pub closed spec fn spec_id(&self) -> GateId { self.id }

    /// Specification view of dependencies in canonical order.
    pub closed spec fn spec_dependencies(&self) -> Seq<GateId> { self.dependencies@ }

    /// Creates a gate whose dependency and evidence identifiers are strictly ordered.
    ///
    /// # Errors
    ///
    /// Returns a typed duplicate, ordering, or self-dependency failure.
    pub fn new(
        id: GateId,
        plan: GateExecutionPlan,
        dependencies: Vec<GateId>,
        required_evidence: Vec<EvidenceRequirementId>,
    ) -> Result<Self, SpecError> {
        let mut index = 0;
        while index < dependencies.len()
            invariant index <= dependencies.len(),
            decreases dependencies.len() - index,
        {
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
        while index < required_evidence.len()
            invariant index <= required_evidence.len(),
            decreases required_evidence.len() - index,
        {
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

    /// Returns the gate identifier.
    #[must_use]
    pub const fn id(&self) -> (id: GateId)
        ensures id == self.spec_id(),
    { self.id }

    /// Returns the immutable execution plan.
    #[must_use]
    pub const fn plan(&self) -> GateExecutionPlan { self.plan }

    /// Returns dependency identifiers in canonical order.
    #[must_use]
    pub const fn dependencies(&self) -> (dependencies: &[GateId])
        ensures dependencies@ == self.spec_dependencies(),
    { self.dependencies.as_slice() }

    /// Returns gate-specific evidence identifiers in canonical order.
    #[must_use]
    pub const fn required_evidence(&self) -> &[EvidenceRequirementId] {
        self.required_evidence.as_slice()
    }
}

/// Validated acyclic graph of every deterministic acceptance gate.
#[derive(Debug, Eq, PartialEq)]
pub struct GateGraph {
    definitions: Vec<GateDefinition>,
    execution_order: Vec<GateId>,
}

impl GateGraph {
    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool {
        crate::gate_execution_order_is_valid(self.definitions@, self.execution_order@)
            && self.definitions@.len() == self.execution_order@.len()
    }
}

fn contains_gate(values: &[GateId], target: GateId) -> bool {
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

fn find_definition(
    definitions: &[GateDefinition],
    target: GateId,
) -> Option<&GateDefinition> {
    let mut index = 0;
    while index < definitions.len()
        invariant index <= definitions.len(),
        decreases definitions.len() - index,
    {
        if definitions[index].id() == target { return Some(&definitions[index]); }
        index += 1;
    }
    None
}

fn dependencies_resolved(definition: &GateDefinition, resolved: &[GateId]) -> bool {
    let mut index = 0;
    let dependencies = definition.dependencies();
    while index < dependencies.len()
        invariant index <= dependencies@.len(),
        decreases dependencies.len() - index,
    {
        if !contains_gate(resolved, dependencies[index]) { return false; }
        index += 1;
    }
    true
}

impl GateGraph {
    /// Specification view of gate definitions in canonical identifier order.
    pub closed spec fn spec_definitions(&self) -> Seq<GateDefinition> { self.definitions@ }

    /// Specification view of the declared gate count.
    pub closed spec fn spec_gate_count(&self) -> nat { self.definitions@.len() }

    /// Specification view of the deterministic execution-order sequence.
    pub closed spec fn spec_execution_order(&self) -> Seq<GateId> { self.execution_order@ }

    /// Validates a nonempty canonical gate set, declared dependencies, and acyclicity.
    ///
    /// The stored topological order is deterministic: the lowest canonical eligible gate is
    /// selected at each step.
    ///
    /// # Errors
    ///
    /// Returns a typed empty, duplicate, ordering, unknown-dependency, or cycle failure.
    pub fn new(definitions: Vec<GateDefinition>) -> (result: Result<Self, SpecError>)
        ensures
            match result {
                Ok(graph) => crate::gate_execution_order_is_valid(
                    graph.spec_definitions(),
                    graph.spec_execution_order(),
                ),
                Err(_) => true,
            },
    {
        if definitions.is_empty() {
            return Err(SpecError::EmptyCollection(CanonicalCollection::Gates));
        }
        let mut index = 0;
        while index < definitions.len()
            invariant index <= definitions.len(),
            decreases definitions.len() - index,
        {
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
        while index < definitions.len()
            invariant index <= definitions.len(),
            decreases definitions.len() - index,
        {
            let mut dependency = 0;
            let dependencies = definitions[index].dependencies();
            while dependency < dependencies.len()
                invariant
                    dependency <= dependencies@.len(),
                    index < definitions@.len(),
                decreases dependencies.len() - dependency,
            {
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
        while execution_order.len() < definitions.len()
            invariant execution_order.len() <= definitions.len(),
            decreases definitions.len() - execution_order.len(),
        {
            let mut candidate = 0;
            let mut selected = None;
            while candidate < definitions.len()
                invariant candidate <= definitions.len(),
                decreases definitions.len() - candidate,
            {
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

    /// Returns gate definitions in canonical identifier order.
    #[must_use]
    pub const fn definitions(&self) -> (definitions: &[GateDefinition])
        ensures definitions@ == self.spec_definitions(),
    { self.definitions.as_slice() }

    /// Returns the deterministic dependency-respecting execution order.
    #[must_use]
    pub const fn execution_order(&self) -> (order: &[GateId])
        ensures
            order@ == self.spec_execution_order(),
            order@.len() == self.spec_gate_count(),
    {
        proof { use_type_invariant(self); }
        self.execution_order.as_slice()
    }

    /// Returns the declared definition for `id`.
    #[must_use]
    #[allow(clippy::option_if_let_else, reason = "explicit branches keep the Verus model direct")]
    pub fn get(&self, id: GateId) -> Option<&GateDefinition> {
        find_definition(self.definitions.as_slice(), id)
    }
}

} // verus!
