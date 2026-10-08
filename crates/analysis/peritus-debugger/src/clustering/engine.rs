//! Input-order-invariant clustering with frozen integer similarity rules.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{
    AnalysisFinding, AnalyzerSignature, DebuggerError, DebuggerLimit, DebuggerLimits,
    DebuggerOperation, EvidenceCitation, FailureCategory, OutcomeClass, PatternId, SubjectId,
    TraceSelectionManifest, AnalysisContext, AnalysisControl, AnalysisStage,
};
use peritus_harness::domain::{ComponentKind, HarnessRevisionIdentity};
use peritus_types::{EnvironmentId, ProviderProfileId, RevisionNumber};

use super::PatternFingerprint;

const PATTERN_ID_DOMAIN: &[u8] = b"peritus-e2-pattern-id-v1\0";

/// Success, task-failure, or infrastructure pattern class.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum PatternKind {
    /// Successful task path.
    Success,
    /// Task-semantic failure or blockage.
    TaskFailure,
    /// Delivery infrastructure failure.
    InfrastructureFailure,
}

/// One provenance-complete pattern member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatternMember {
    subject_id: SubjectId,
    outcome: OutcomeClass,
    category: Option<FailureCategory>,
    analyzer: AnalyzerSignature,
    environment_id: EnvironmentId,
    harness_revision: HarnessRevisionIdentity,
    workspace_revision: RevisionNumber,
    provider_profile_id: ProviderProfileId,
    component_kind: Option<ComponentKind>,
    citations: Arc<[EvidenceCitation]>,
    fingerprint: PatternFingerprint,
}

impl PatternMember {
    /// Returns the exact source subject.
    #[must_use]
    pub const fn subject_id(&self) -> SubjectId {
        self.subject_id
    }
    /// Returns the normalized outcome.
    #[must_use]
    pub const fn outcome(&self) -> OutcomeClass {
        self.outcome
    }
    /// Returns the optional failure category.
    #[must_use]
    pub const fn category(&self) -> Option<FailureCategory> {
        self.category
    }
    /// Returns the deterministic analyzer signature.
    #[must_use]
    pub const fn analyzer(&self) -> AnalyzerSignature {
        self.analyzer
    }
    /// Returns the exact environment identity.
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }
    /// Returns the full E1 revision.
    #[must_use]
    pub const fn harness_revision(&self) -> HarnessRevisionIdentity {
        self.harness_revision
    }
    /// Returns the workspace revision.
    #[must_use]
    pub const fn workspace_revision(&self) -> RevisionNumber {
        self.workspace_revision
    }
    /// Returns the provider profile.
    #[must_use]
    pub const fn provider_profile_id(&self) -> ProviderProfileId {
        self.provider_profile_id
    }
    /// Returns the likely component kind included in the fingerprint.
    #[must_use]
    pub const fn component_kind(&self) -> Option<ComponentKind> {
        self.component_kind
    }
    /// Borrows source citations.
    #[must_use]
    pub fn citations(&self) -> &[EvidenceCitation] {
        &self.citations
    }
    /// Returns the exact typed fingerprint.
    #[must_use]
    pub const fn fingerprint(&self) -> PatternFingerprint {
        self.fingerprint
    }
}

/// One exact or deterministically agglomerated pattern cluster.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatternCluster {
    id: PatternId,
    kind: PatternKind,
    fingerprint: PatternFingerprint,
    source_fingerprints: Vec<PatternFingerprint>,
    members: Vec<PatternMember>,
}

impl PatternCluster {
    /// Returns the stable cluster identity.
    #[must_use]
    pub const fn id(&self) -> PatternId {
        self.id
    }
    /// Returns success/task/infrastructure class.
    #[must_use]
    pub const fn kind(&self) -> PatternKind {
        self.kind
    }
    /// Returns the final exact or combined fingerprint.
    #[must_use]
    pub const fn fingerprint(&self) -> PatternFingerprint {
        self.fingerprint
    }
    /// Borrows constituent exact fingerprints.
    #[must_use]
    pub fn source_fingerprints(&self) -> &[PatternFingerprint] {
        &self.source_fingerprints
    }
    /// Borrows provenance-complete members in canonical order.
    #[must_use]
    pub fn members(&self) -> &[PatternMember] {
        &self.members
    }
}

/// Clusters deterministic findings independently of caller iteration order.
///
/// # Errors
///
/// Rejects missing subject bindings. Oversized clusters are losslessly partitioned at the active
/// member-page bound; no finding is sampled or discarded.
pub fn cluster_findings(
    findings: &[AnalysisFinding],
    manifest: &TraceSelectionManifest,
    limits: DebuggerLimits,
) -> Result<Vec<PatternCluster>, DebuggerError> {
    let context = AnalysisContext::for_manifest(manifest);
    cluster_findings_controlled(
        findings,
        manifest,
        limits,
        context,
        &mut crate::work::RunToCompletion,
    )
}

/// Clusters findings with manifest-bound progress and cooperative control.
///
/// # Errors
/// Rejects context drift, cancellation, or missing subject bindings.
pub fn cluster_findings_controlled(
    findings: &[AnalysisFinding],
    manifest: &TraceSelectionManifest,
    limits: DebuggerLimits,
    context: AnalysisContext,
    control: &mut impl AnalysisControl,
) -> Result<Vec<PatternCluster>, DebuggerError> {
    context.validate(manifest, DebuggerOperation::ClusterPatterns)?;
    type SimilarityKey = (
        PatternKind,
        Option<FailureCategory>,
        AnalyzerSignature,
        EnvironmentId,
        HarnessRevisionIdentity,
        RevisionNumber,
        ProviderProfileId,
        Option<ComponentKind>,
    );
    let mut grouped: BTreeMap<
        SimilarityKey,
        BTreeMap<PatternFingerprint, Vec<PatternMember>>,
    > = BTreeMap::new();
    crate::work::checkpoint(
        control, context, AnalysisStage::ClusterPatterns, 0, findings.len(),
        DebuggerOperation::ClusterPatterns,
    )?;
    for (index, finding) in findings.iter().enumerate() {
        let subject = manifest
            .subject(finding.subject_id())
            .ok_or_else(|| cluster_error("finding subject is absent from the manifest"))?;
        let component_kind = finding.category().and_then(|category| {
            crate::component::component_kinds_for_category(category).first().copied()
        });
        let fingerprint = PatternFingerprint::for_finding(finding, manifest, component_kind)?;
        let key = (
            pattern_kind(finding.outcome()),
            finding.category(),
            finding.signature(),
            subject.environment_id(),
            subject.harness_revision(),
            subject.revision().workspace_revision(),
            subject.revision().provider_profile_id(),
            component_kind,
        );
        grouped.entry(key).or_default().entry(fingerprint).or_default().push(PatternMember {
            subject_id: finding.subject_id(),
            outcome: finding.outcome(),
            category: finding.category(),
            analyzer: finding.signature(),
            environment_id: subject.environment_id(),
            harness_revision: subject.harness_revision(),
            workspace_revision: subject.revision().workspace_revision(),
            provider_profile_id: subject.revision().provider_profile_id(),
            component_kind,
            citations: finding.shared_citations(),
            fingerprint,
        });
        crate::work::checkpoint(
            control,
            context,
            AnalysisStage::ClusterPatterns,
            index.saturating_add(1),
            findings.len(),
            DebuggerOperation::ClusterPatterns,
        )?;
    }
    let mut groups = Vec::with_capacity(grouped.len());
    for exact in grouped.into_values() {
        let mut combined = Vec::new();
        for mut members in exact.into_values() {
            members.sort_by(|left, right| {
                (left.subject_id, &left.citations).cmp(&(right.subject_id, &right.citations))
            });
            combined.extend(members);
        }
        groups.push(combined);
    }
    let member_page_size = usize::try_from(limits.get(DebuggerLimit::PatternMembers))
        .unwrap_or(usize::MAX)
        .max(1);
    let mut partitions = Vec::new();
    for members in groups {
        let partition_count = members.len().div_ceil(member_page_size);
        for (partition, chunk) in members.chunks(member_page_size).enumerate() {
            let partition = (partition_count > 1)
                .then(|| u64::try_from(partition).unwrap_or(u64::MAX));
            partitions.push((chunk.to_vec(), partition));
        }
    }
    let mut clusters = Vec::with_capacity(partitions.len());
    for (mut members, partition) in partitions {
        members.sort_by(|left, right| {
            (left.subject_id, left.fingerprint, &left.citations).cmp(&(
                right.subject_id,
                right.fingerprint,
                &right.citations,
            ))
        });
        let mut source_fingerprints: Vec<_> =
            members.iter().map(|member| member.fingerprint).collect();
        source_fingerprints.sort();
        source_fingerprints.dedup();
        let fingerprint = if source_fingerprints.len() == 1 {
            source_fingerprints[0]
        } else {
            PatternFingerprint::combined(&source_fingerprints)
        };
        let kind = pattern_kind(members[0].outcome);
        let mut identity = Vec::new();
        identity.push(pattern_kind_tag(kind));
        identity.extend_from_slice(fingerprint.digest().as_bytes());
        for member in &members {
            identity.extend_from_slice(member.subject_id.as_bytes());
            identity.extend_from_slice(member.fingerprint.digest().as_bytes());
        }
        if let Some(partition) = partition {
            identity.extend_from_slice(b"partition\0");
            identity.extend_from_slice(&partition.to_be_bytes());
        }
        clusters.push(PatternCluster {
            id: PatternId::derive(PATTERN_ID_DOMAIN, &identity)?,
            kind,
            fingerprint,
            source_fingerprints,
            members,
        });
    }
    clusters.sort_by_key(|cluster| {
        (cluster.kind, cluster.fingerprint, cluster.members[0].subject_id, cluster.id)
    });
    Ok(clusters)
}

const fn pattern_kind(outcome: OutcomeClass) -> PatternKind {
    match outcome {
        OutcomeClass::Task(crate::TaskOutcome::Success) => PatternKind::Success,
        OutcomeClass::Task(_) => PatternKind::TaskFailure,
        OutcomeClass::Infrastructure(_) => PatternKind::InfrastructureFailure,
    }
}

const fn pattern_kind_tag(kind: PatternKind) -> u8 {
    match kind {
        PatternKind::Success => 1,
        PatternKind::TaskFailure => 2,
        PatternKind::InfrastructureFailure => 3,
    }
}

fn cluster_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        crate::DebuggerErrorKind::Report,
        DebuggerOperation::ClusterPatterns,
        crate::DebuggerRecovery::CorrectInput,
        detail,
    )
}
