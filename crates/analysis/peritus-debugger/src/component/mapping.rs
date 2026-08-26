//! Taxonomy and trace-pattern mapping to immutable E1 declarations.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    DebuggerError, DebuggerLimit, DebuggerLimits, DebuggerOperation, FailureCategory,
    PatternCluster, PatternId, PatternKind, SubjectId, TraceSelectionManifest,
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
    supporting_subjects: Vec<SubjectId>,
    contrary_subjects: Vec<SubjectId>,
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
/// Rejects revision drift, absent exact revisions, inconsistent declarations, or link bounds.
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
    let successful: BTreeSet<SubjectId> = patterns
        .iter()
        .filter(|pattern| pattern.kind() == PatternKind::Success)
        .flat_map(|pattern| pattern.members().iter().map(crate::PatternMember::subject_id))
        .collect();
    let subjects: BTreeMap<_, _> =
        manifest.subjects().iter().map(|subject| (subject.id(), subject)).collect();
    let mut correlations = Vec::new();
    for pattern in patterns.iter().filter(|pattern| pattern.kind() != PatternKind::Success) {
        let category = pattern.members().iter().find_map(crate::PatternMember::category);
        let Some(category) = category else { continue };
        for &kind in component_kinds_for_category(category) {
            let mut by_declaration: BTreeMap<Option<(ComponentId, Sha256Digest)>, Vec<SubjectId>> =
                BTreeMap::new();
            for member in pattern.members() {
                let subject = subjects
                    .get(&member.subject_id())
                    .ok_or_else(|| mapping_error("pattern subject is absent from the manifest"))?;
                if harness.harness_id() != subject.harness_revision().harness_id() {
                    return Err(mapping_error(
                        "harness projection lineage differs from the subject",
                    ));
                }
                let revision = harness
                    .revision(subject.harness_revision().digest())
                    .ok_or_else(|| mapping_error("subject E1 revision is absent"))?;
                if revision.identity() != subject.harness_revision() {
                    return Err(mapping_error("subject E1 revision identity drifted"));
                }
                let matching: Vec<_> = revision
                    .graph()
                    .declarations()
                    .iter()
                    .filter(|item| item.kind() == kind)
                    .collect();
                let key = if matching.len() == 1 {
                    Some((matching[0].id().clone(), matching[0].content_digest()))
                } else {
                    None
                };
                by_declaration.entry(key).or_default().push(member.subject_id());
            }
            for (declaration, mut support) in by_declaration {
                support.sort();
                support.dedup();
                let mut contrary: Vec<_> = successful
                    .iter()
                    .copied()
                    .filter(|id| {
                        subjects.get(id).is_some_and(|subject| {
                            harness.revision(subject.harness_revision().digest()).is_some_and(
                                |revision| {
                                    revision
                                        .graph()
                                        .declarations()
                                        .iter()
                                        .any(|item| item.kind() == kind)
                                },
                            )
                        })
                    })
                    .collect();
                contrary.sort();
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
                    supporting_subjects: support,
                    contrary_subjects: contrary,
                    constraint,
                    class_only,
                });
            }
        }
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
    limits.check(
        DebuggerLimit::ComponentLinks,
        correlations.len(),
        DebuggerOperation::MapComponents,
    )?;
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
