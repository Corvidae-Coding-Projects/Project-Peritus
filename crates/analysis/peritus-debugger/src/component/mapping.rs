//! Taxonomy and trace-pattern mapping to immutable E1 declarations.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::{
    DebuggerError, DebuggerLimits, DebuggerOperation, FailureCategory, PatternCluster, PatternId,
    PatternKind, SubjectId, TraceSelectionManifest, AnalysisContext, AnalysisControl, AnalysisStage,
};
use peritus_harness::{
    HarnessProjection,
    domain::{ComponentId, ComponentKind, ProtectionClass},
};
use peritus_types::Sha256Digest;

/// Strength of a diagnostic correlation, never an evaluation or promotion decision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ConstraintLevel {
    /// One weak or isolated association.
    Advisory,
    /// Recurrent evidence contributes to the likely explanation.
    Contributing,
    /// Recurrent evidence with no retained contrary subjects dominates this diagnostic cluster.
    Dominant,
    /// Evidence cannot isolate a declaration.
    Unknown,
}

/// Typed reason for a component association.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum CorrelationBasis {
    /// Closed taxonomy-to-component-kind rule.
    Taxonomy,
    /// C7 provider/tool/gate binding sharpened the rule.
    TraceBinding,
    /// One declaration of the expected kind exists in the exact revision.
    UniqueRevisionDeclaration,
    /// Several declarations share the likely class and exact attribution is ambiguous.
    AmbiguousRevisionDeclarations,
}

/// Immutable non-authoritative association between one pattern and one E1 declaration or class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentCorrelation {
    pattern_id: PatternId,
    component_id: Option<ComponentId>,
    component_kind: ComponentKind,
    content_digest: Option<Sha256Digest>,
    protection_class: ProtectionClass,
    basis: CorrelationBasis,
    supporting_subjects: Arc<[SubjectId]>,
    contrary_subjects: Arc<[SubjectId]>,
    constraint: ConstraintLevel,
    class_only: bool,
}

impl ComponentCorrelation {
    /// Returns the source pattern.
    #[must_use]
    pub const fn pattern_id(&self) -> PatternId {
        self.pattern_id
    }
    /// Borrows an exact declaration identity when uniquely supported.
    #[must_use]
    pub const fn component_id(&self) -> Option<&ComponentId> {
        self.component_id.as_ref()
    }
    /// Returns the likely component class.
    #[must_use]
    pub const fn component_kind(&self) -> ComponentKind {
        self.component_kind
    }
    /// Returns exact declaration content when an exact ID is present.
    #[must_use]
    pub const fn content_digest(&self) -> Option<Sha256Digest> {
        self.content_digest
    }
    /// Returns the compiled E1 protection class.
    #[must_use]
    pub const fn protection_class(&self) -> ProtectionClass {
        self.protection_class
    }
    /// Returns the correlation basis.
    #[must_use]
    pub const fn basis(&self) -> CorrelationBasis {
        self.basis
    }
    /// Borrows supporting subjects.
    #[must_use]
    pub fn supporting_subjects(&self) -> &[SubjectId] {
        &self.supporting_subjects
    }
    /// Borrows contrary successful subjects.
    #[must_use]
    pub fn contrary_subjects(&self) -> &[SubjectId] {
        &self.contrary_subjects
    }
    /// Returns bounded diagnostic strength.
    #[must_use]
    pub const fn constraint(&self) -> ConstraintLevel {
        self.constraint
    }
    /// Returns whether no exact E1 declaration could be identified.
    #[must_use]
    pub const fn class_only(&self) -> bool {
        self.class_only
    }
}

/// Maps patterns to only declarations present in each subject's exact E1 revision.
///
/// # Errors
///
/// Rejects revision drift, absent exact revisions, or inconsistent declarations.
#[allow(
    clippy::too_many_lines,
    reason = "the bounded E1 correlation pass keeps revision checks and emitted evidence together"
)]
pub fn map_components(
    patterns: &[PatternCluster],
    manifest: &TraceSelectionManifest,
    harness: &HarnessProjection,
    limits: DebuggerLimits,
) -> Result<Vec<ComponentCorrelation>, DebuggerError> {
    let context = AnalysisContext::for_manifest(manifest);
    map_components_controlled(
        patterns,
        manifest,
        harness,
        limits,
        context,
        &mut crate::work::RunToCompletion,
    )
}

/// Maps patterns through a once-verified revision index with cooperative control.
///
/// # Errors
/// Rejects context drift, cancellation, revision drift, or inconsistent declarations.
#[allow(
    clippy::too_many_lines,
    reason = "the indexed E1 correlation pass keeps revision checks and emitted evidence together"
)]
pub fn map_components_controlled(
    patterns: &[PatternCluster],
    manifest: &TraceSelectionManifest,
    harness: &HarnessProjection,
    _limits: DebuggerLimits,
    context: AnalysisContext,
    control: &mut impl AnalysisControl,
) -> Result<Vec<ComponentCorrelation>, DebuggerError> {
    context.validate(manifest, DebuggerOperation::MapComponents)?;
    let successful: BTreeSet<SubjectId> = patterns
        .iter()
        .filter(|pattern| pattern.kind() == PatternKind::Success)
        .flat_map(|pattern| pattern.members().iter().map(crate::PatternMember::subject_id))
        .collect();
    let required_kinds: BTreeSet<ComponentKind> = patterns
        .iter()
        .filter_map(|pattern| pattern.members().iter().find_map(crate::PatternMember::category))
        .flat_map(component_kinds_for_category)
        .copied()
        .collect();
    let referenced_subjects: BTreeSet<SubjectId> = patterns
        .iter()
        .flat_map(|pattern| pattern.members().iter().map(crate::PatternMember::subject_id))
        .collect();
    let mut declarations = BTreeMap::<
        SubjectId,
        BTreeMap<ComponentKind, Vec<(ComponentId, Sha256Digest)>>,
    >::new();
    for subject_id in referenced_subjects {
        let subject = manifest
            .subject(subject_id)
            .ok_or_else(|| mapping_error("pattern subject is absent from the manifest"))?;
        if harness.harness_id() != subject.harness_revision().harness_id() {
            return Err(mapping_error("harness projection lineage differs from the subject"));
        }
        let revision = harness
            .revision(subject.harness_revision().digest())
            .ok_or_else(|| mapping_error("subject E1 revision is absent"))?;
        if revision.identity() != subject.harness_revision() {
            return Err(mapping_error("subject E1 revision identity drifted"));
        }
        let mut by_kind = BTreeMap::new();
        for declaration in revision.graph().declarations() {
            if required_kinds.contains(&declaration.kind()) {
                by_kind
                    .entry(declaration.kind())
                    .or_insert_with(Vec::new)
                    .push((declaration.id().clone(), declaration.content_digest()));
            }
        }
        declarations.insert(subject.id(), by_kind);
    }
    let contrary_by_kind: BTreeMap<ComponentKind, Arc<[SubjectId]>> = required_kinds
        .iter()
        .map(|&kind| {
            let subjects: Vec<_> = successful
                .iter()
                .copied()
                .filter(|id| {
                    declarations
                        .get(id)
                        .and_then(|by_kind| by_kind.get(&kind))
                        .is_some_and(|items| !items.is_empty())
                })
                .collect();
            (kind, Arc::from(subjects))
        })
        .collect();
    let mut correlations = Vec::new();
    crate::work::checkpoint(
        control, context, AnalysisStage::MapComponents, 0, patterns.len(),
        DebuggerOperation::MapComponents,
    )?;
    for (pattern_index, pattern) in patterns.iter().enumerate() {
        if pattern.kind() == PatternKind::Success {
            crate::work::checkpoint(
                control,
                context,
                AnalysisStage::MapComponents,
                pattern_index.saturating_add(1),
                patterns.len(),
                DebuggerOperation::MapComponents,
            )?;
            continue;
        }
        let category = pattern.members().iter().find_map(crate::PatternMember::category);
        let Some(category) = category else {
            crate::work::checkpoint(
                control,
                context,
                AnalysisStage::MapComponents,
                pattern_index.saturating_add(1),
                patterns.len(),
                DebuggerOperation::MapComponents,
            )?;
            continue;
        };
        for &kind in component_kinds_for_category(category) {
            let mut by_declaration: BTreeMap<Option<(ComponentId, Sha256Digest)>, Vec<SubjectId>> =
                BTreeMap::new();
            for member in pattern.members() {
                let matching = declarations
                    .get(&member.subject_id())
                    .ok_or_else(|| mapping_error("pattern subject is absent from the manifest"))?;
                let matching = matching.get(&kind).map_or(&[][..], Vec::as_slice);
                let key = if matching.len() == 1 {
                    Some(matching[0].clone())
                } else {
                    None
                };
                by_declaration.entry(key).or_default().push(member.subject_id());
            }
            for (declaration, mut support) in by_declaration {
                support.sort();
                support.dedup();
                let contrary = contrary_by_kind.get(&kind).cloned().unwrap_or_default();
                let class_only = declaration.is_none();
                let basis = if class_only {
                    CorrelationBasis::AmbiguousRevisionDeclarations
                } else if trace_sharpens(kind, pattern) {
                    CorrelationBasis::TraceBinding
                } else {
                    CorrelationBasis::UniqueRevisionDeclaration
                };
                let constraint = constraint_level(support.len(), contrary.len(), class_only);
                correlations.push(ComponentCorrelation {
                    pattern_id: pattern.id(),
                    component_id: declaration.as_ref().map(|(id, _)| id.clone()),
                    component_kind: kind,
                    content_digest: declaration.map(|(_, digest)| digest),
                    protection_class: kind.protection_class(),
                    basis,
                    supporting_subjects: Arc::from(support),
                    contrary_subjects: contrary,
                    constraint,
                    class_only,
                });
            }
        }
        crate::work::checkpoint(
            control,
            context,
            AnalysisStage::MapComponents,
            pattern_index.saturating_add(1),
            patterns.len(),
            DebuggerOperation::MapComponents,
        )?;
    }
    correlations.sort_by(|left, right| {
        (left.pattern_id, left.component_kind, &left.component_id, &left.supporting_subjects).cmp(
            &(
                right.pattern_id,
                right.component_kind,
                &right.component_id,
                &right.supporting_subjects,
            ),
        )
    });
    Ok(correlations)
}

const fn constraint_level(support: usize, contrary: usize, class_only: bool) -> ConstraintLevel {
    if class_only {
        ConstraintLevel::Unknown
    } else if support >= 3 && contrary == 0 {
        ConstraintLevel::Dominant
    } else if support >= 2 {
        ConstraintLevel::Contributing
    } else {
        ConstraintLevel::Advisory
    }
}

fn trace_sharpens(kind: ComponentKind, pattern: &PatternCluster) -> bool {
    matches!(
        kind,
        ComponentKind::ProviderProfile
            | ComponentKind::ToolDescriptor
            | ComponentKind::GateDefinition
    ) && pattern.members().iter().all(|member| member.component_kind() == Some(kind))
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "the analyzer and mapper share this frozen internal table without exposing it publicly"
)]
pub(crate) const fn component_kinds_for_category(
    category: FailureCategory,
) -> &'static [ComponentKind] {
    use ComponentKind as C;
    use FailureCategory as F;
    match category {
        F::SpecificationAmbiguity | F::SpecificationConflict | F::SpecificationUnachievable => {
            &[C::BaseInstructionFragment, C::SystemInstructionFragment]
        }
        F::ContextSelection | F::ContextCompaction | F::ContextProvenance => {
            &[C::ContextTransform, C::MemorySelector, C::MemoryInjectionPolicy]
        }
        F::ModelReasoning | F::ModelMalformedOutput | F::ModelRefusal | F::ModelCompletion => {
            &[C::RolePrompt, C::ProviderProfile]
        }
        F::ProviderAuthentication
        | F::ProviderQuota
        | F::ProviderRateLimit
        | F::ProviderTransport
        | F::ProviderProtocol
        | F::ProviderAccounting => &[C::ProviderCapability, C::ProviderProfile],
        F::ToolSchema | F::ToolResultNormalization => &[C::ToolSchema, C::ToolDescriptor],
        F::ToolRouting | F::ToolAuthorization => &[C::ToolExposurePolicy, C::OrchestrationPolicy],
        F::ToolExecution => &[C::ToolImplementation, C::ToolDescriptor],
        F::Workspace
        | F::Patch
        | F::Git
        | F::PathConflict
        | F::Sandbox
        | F::Process
        | F::Network
        | F::Resource => &[C::ToolImplementation, C::Middleware],
        F::DeterministicGateFailure | F::GateInfrastructureFailure => {
            &[C::GateDefinition, C::GateParser]
        }
        F::ReviewDisagreement
        | F::ReviewInvalidFinding
        | F::ReviewUnresolvedBlocker
        | F::ReviewOscillation => {
            &[C::RoleDefinition, C::OrchestrationPolicy, C::TerminationPolicy]
        }
        F::Journal | F::Artifact | F::Projection | F::Migration | F::Recovery => {
            &[C::Middleware, C::ObservabilityPolicy]
        }
        F::AuthorityTimeout | F::AuthorityDenied => &[C::OrchestrationPolicy, C::TerminationPolicy],
        F::SchedulerStarvation | F::SchedulerCancellation | F::SchedulerDependencyFailure => {
            &[C::CollaborationDefinition, C::OrchestrationPolicy]
        }
        F::EvolutionContamination | F::EvolutionAttributionUncertainty => {
            &[C::AnalysisPolicy, C::EvolutionStrategy]
        }
        F::EvolutionStatisticalRejection | F::EvolutionPromotionDenial => {
            &[C::MetricDefinition, C::EvolutionStrategy]
        }
    }
}

fn mapping_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        crate::DebuggerErrorKind::Binding,
        DebuggerOperation::MapComponents,
        crate::DebuggerRecovery::RepairDependency,
        detail,
    )
}
