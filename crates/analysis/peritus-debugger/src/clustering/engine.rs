//! Input-order-invariant clustering with frozen integer similarity rules.

use std::collections::BTreeMap;

use crate::{
    AnalysisFinding, AnalyzerSignature, DebuggerError, DebuggerLimit, DebuggerLimits,
    DebuggerOperation, EvidenceCitation, FailureCategory, OutcomeClass, PatternId, SubjectId,
    TraceSelectionManifest,
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
    citations: Vec<EvidenceCitation>,
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
/// Rejects missing subject bindings or pattern/member bound excess; it never samples members.
pub fn cluster_findings(
    findings: &[AnalysisFinding],
    manifest: &TraceSelectionManifest,
    limits: DebuggerLimits,
) -> Result<Vec<PatternCluster>, DebuggerError> {
    let mut exact: BTreeMap<PatternFingerprint, Vec<PatternMember>> = BTreeMap::new();
    for finding in findings {
        let subject = manifest
            .subjects()
            .iter()
            .find(|subject| subject.id() == finding.subject_id())
            .ok_or_else(|| cluster_error("finding subject is absent from the manifest"))?;
        let component_kind = finding.category().and_then(|category| {
            crate::component::component_kinds_for_category(category).first().copied()
        });
        let fingerprint = PatternFingerprint::for_finding(finding, manifest, component_kind)?;
        exact.entry(fingerprint).or_default().push(PatternMember {
            subject_id: finding.subject_id(),
            outcome: finding.outcome(),
            category: finding.category(),
            analyzer: finding.signature(),
            environment_id: subject.environment_id(),
            harness_revision: subject.harness_revision(),
            workspace_revision: subject.revision().workspace_revision(),
            provider_profile_id: subject.revision().provider_profile_id(),
            component_kind,
            citations: finding.citations().to_vec(),
            fingerprint,
        });
    }
    let mut groups: Vec<Vec<PatternMember>> = exact.into_values().collect();
    for members in &mut groups {
        members.sort_by(|left, right| {
            (left.subject_id, &left.citations).cmp(&(right.subject_id, &right.citations))
        });
    }
    agglomerate(&mut groups);
    let mut clusters = Vec::with_capacity(groups.len());
    for mut members in groups {
        members.sort_by(|left, right| {
            (left.subject_id, left.fingerprint, &left.citations).cmp(&(
                right.subject_id,
                right.fingerprint,
                &right.citations,
            ))
        });
        limits.check(
            DebuggerLimit::PatternMembers,
            members.len(),
            DebuggerOperation::ClusterPatterns,
        )?;
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
        clusters.push(PatternCluster {
            id: PatternId::derive(PATTERN_ID_DOMAIN, &identity)?,
            kind,
            fingerprint,
            source_fingerprints,
            members,
        });
    }
    clusters
        .sort_by_key(|cluster| (cluster.kind, cluster.fingerprint, cluster.members[0].subject_id));
    limits.check(DebuggerLimit::Patterns, clusters.len(), DebuggerOperation::ClusterPatterns)?;
    Ok(clusters)
}

fn agglomerate(groups: &mut Vec<Vec<PatternMember>>) {
    let mut index = 0;
    while index < groups.len() {
        let mut candidate = index + 1;
        while candidate < groups.len() {
            if similar(&groups[index][0], &groups[candidate][0]) {
                let merged = groups.remove(candidate);
                groups[index].extend(merged);
            } else {
                candidate += 1;
            }
        }
        index += 1;
    }
}

fn similar(left: &PatternMember, right: &PatternMember) -> bool {
    pattern_kind(left.outcome) == pattern_kind(right.outcome)
        && left.category == right.category
        && left.analyzer == right.analyzer
        && left.component_kind == right.component_kind
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
