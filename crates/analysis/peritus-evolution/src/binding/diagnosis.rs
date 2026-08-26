//! Published E2 diagnosis capture with exact cited identities.

use crate::{
    EvolutionError, EvolutionErrorKind, EvolutionLimits, EvolutionOperation, EvolutionRecovery,
    identity::{digest_parts, push_bytes},
};
use peritus_debugger::{
    ClaimId, DebuggerPhase, DebuggerState, PatternId, ReportId, SelectionManifestId,
    ValidatedReport,
};
use peritus_harness::domain::ComponentId;
use peritus_types::{EvidenceId, RevisionTuple, Sha256Digest};

/// One exact citation retained from a validated E2 report.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DiagnosisCitation {
    /// One immutable typed E2 claim.
    Claim(ClaimId),
    /// One provenance-complete E2 pattern.
    Pattern(PatternId),
    /// One exact non-class-only component correlation.
    Component {
        /// Source pattern identity.
        pattern_id: PatternId,
        /// Exact E1 component declaration identity.
        component_id: ComponentId,
    },
}

/// Checked published E2 report and the exact citations admitted into F0.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedDebuggerEvidence {
    revision: RevisionTuple,
    job_id: peritus_debugger::DebuggerJobId,
    report_id: ReportId,
    report_digest: Sha256Digest,
    manifest_id: SelectionManifestId,
    manifest_digest: Sha256Digest,
    query_digest: Sha256Digest,
    artifact_digest: Sha256Digest,
    artifact_size: u64,
    evidence_id: EvidenceId,
    journal_position: u64,
    citations: Vec<DiagnosisCitation>,
    digest: Sha256Digest,
}

impl PublishedDebuggerEvidence {
    /// Captures one completed E2 publication and validates every cited report member.
    ///
    /// # Errors
    /// Rejects non-published state, report/publication drift, empty or noncanonical citations,
    /// absent report members, and class-only component correlations.
    pub fn capture(
        state: &DebuggerState,
        validated: &ValidatedReport,
        citations: Vec<DiagnosisCitation>,
        limits: EvolutionLimits,
    ) -> Result<Self, EvolutionError> {
        let report = validated.report();
        let record = state.report().ok_or_else(incomplete)?;
        let publication = state.publication().ok_or_else(incomplete)?;
        let canonical = !citations.is_empty()
            && citations.len() <= usize::from(limits.citations_per_manifest())
            && citations.windows(2).all(|pair| pair[0] < pair[1]);
        if state.phase() != DebuggerPhase::Published
            || record.id() != validated.id()
            || record.digest() != validated.digest()
            || record.size() != u64::try_from(validated.canonical_bytes().len()).unwrap_or(u64::MAX)
            || publication.report_id() != validated.id()
            || publication.artifact_digest() != validated.digest()
            || publication.artifact_size() != record.size()
            || !canonical
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::BindingDrift,
                EvolutionOperation::BindDiagnosis,
                EvolutionRecovery::CorrectInput,
                "debugger report, publication, or citation binding differs",
            ));
        }
        for citation in &citations {
            let present = match citation {
                DiagnosisCitation::Claim(id) => {
                    report.claims().iter().any(|value| value.id() == *id)
                }
                DiagnosisCitation::Pattern(id) => {
                    report.patterns().iter().any(|value| value.id() == *id)
                }
                DiagnosisCitation::Component { pattern_id, component_id } => {
                    report.correlations().iter().any(|value| {
                        value.pattern_id() == *pattern_id
                            && !value.class_only()
                            && value.component_id() == Some(component_id)
                    })
                }
            };
            if !present {
                return Err(EvolutionError::new(
                    EvolutionErrorKind::IncompleteEvidence,
                    EvolutionOperation::BindDiagnosis,
                    EvolutionRecovery::ObtainEvidence,
                    "diagnosis citation is absent or class-only",
                ));
            }
        }
        let digest = evidence_digest(
            *state.revision(),
            state.job_id(),
            validated.id(),
            validated.digest(),
            report.manifest_id(),
            report.manifest_digest(),
            report.query_digest(),
            publication.artifact_digest(),
            publication.artifact_size(),
            publication.evidence_id(),
            publication.journal_position(),
            &citations,
        );
        Ok(Self {
            revision: *state.revision(),
            job_id: state.job_id(),
            report_id: validated.id(),
            report_digest: validated.digest(),
            manifest_id: report.manifest_id(),
            manifest_digest: report.manifest_digest(),
            query_digest: report.query_digest(),
            artifact_digest: publication.artifact_digest(),
            artifact_size: publication.artifact_size(),
            evidence_id: publication.evidence_id(),
            journal_position: publication.journal_position(),
            citations,
            digest,
        })
    }

    #[allow(clippy::too_many_arguments, reason = "every persisted E2 bridge fact stays explicit")]
    pub(crate) fn from_exact_parts(
        revision: RevisionTuple,
        job_id: peritus_debugger::DebuggerJobId,
        report_id: ReportId,
        report_digest: Sha256Digest,
        manifest_id: SelectionManifestId,
        manifest_digest: Sha256Digest,
        query_digest: Sha256Digest,
        artifact_digest: Sha256Digest,
        artifact_size: u64,
        evidence_id: EvidenceId,
        journal_position: u64,
        citations: Vec<DiagnosisCitation>,
        limits: EvolutionLimits,
    ) -> Result<Self, EvolutionError> {
        if artifact_size == 0
            || journal_position == 0
            || citations.is_empty()
            || citations.len() > usize::from(limits.citations_per_manifest())
            || citations.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::NonCanonical,
                EvolutionOperation::BindDiagnosis,
                EvolutionRecovery::Quarantine,
                "persisted debugger evidence is empty, noncanonical, or over limit",
            ));
        }
        let digest = evidence_digest(
            revision,
            job_id,
            report_id,
            report_digest,
            manifest_id,
            manifest_digest,
            query_digest,
            artifact_digest,
            artifact_size,
            evidence_id,
            journal_position,
            &citations,
        );
        Ok(Self {
            revision,
            job_id,
            report_id,
            report_digest,
            manifest_id,
            manifest_digest,
            query_digest,
            artifact_digest,
            artifact_size,
            evidence_id,
            journal_position,
            citations,
            digest,
        })
    }

    /// Returns the cross-slice revision under which E2 ran.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the E2 debugger job identity.
    #[must_use]
    pub const fn job_id(&self) -> peritus_debugger::DebuggerJobId {
        self.job_id
    }
    /// Returns the validated report identity.
    #[must_use]
    pub const fn report_id(&self) -> ReportId {
        self.report_id
    }
    /// Returns the canonical report digest.
    #[must_use]
    pub const fn report_digest(&self) -> Sha256Digest {
        self.report_digest
    }
    /// Returns the cited trace-selection manifest identity.
    #[must_use]
    pub const fn manifest_id(&self) -> SelectionManifestId {
        self.manifest_id
    }
    /// Returns the complete selection-manifest digest.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest_digest
    }
    /// Returns the frozen debugger query digest.
    #[must_use]
    pub const fn query_digest(&self) -> Sha256Digest {
        self.query_digest
    }
    /// Returns the finalized report artifact digest.
    #[must_use]
    pub const fn artifact_digest(&self) -> Sha256Digest {
        self.artifact_digest
    }
    /// Returns the finalized report artifact byte length.
    #[must_use]
    pub const fn artifact_size(&self) -> u64 {
        self.artifact_size
    }
    /// Returns the admitted C0 evidence identity.
    #[must_use]
    pub const fn evidence_id(&self) -> EvidenceId {
        self.evidence_id
    }
    /// Returns the report event position cited by provenance.
    #[must_use]
    pub const fn journal_position(&self) -> u64 {
        self.journal_position
    }
    /// Borrows canonical exact report citations.
    #[must_use]
    pub fn citations(&self) -> &[DiagnosisCitation] {
        &self.citations
    }
    /// Returns the digest of every retained evidence field.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

#[allow(clippy::too_many_arguments)]
fn evidence_digest(
    revision: RevisionTuple,
    job_id: peritus_debugger::DebuggerJobId,
    report_id: ReportId,
    report_digest: Sha256Digest,
    manifest_id: SelectionManifestId,
    manifest_digest: Sha256Digest,
    query_digest: Sha256Digest,
    artifact_digest: Sha256Digest,
    artifact_size: u64,
    evidence_id: EvidenceId,
    journal_position: u64,
    citations: &[DiagnosisCitation],
) -> Sha256Digest {
    let mut citation_bytes = Vec::new();
    for citation in citations {
        match citation {
            DiagnosisCitation::Claim(id) => {
                citation_bytes.push(1);
                citation_bytes.extend_from_slice(id.as_bytes());
            }
            DiagnosisCitation::Pattern(id) => {
                citation_bytes.push(2);
                citation_bytes.extend_from_slice(id.as_bytes());
            }
            DiagnosisCitation::Component { pattern_id, component_id } => {
                citation_bytes.push(3);
                citation_bytes.extend_from_slice(pattern_id.as_bytes());
                push_bytes(&mut citation_bytes, component_id.as_str().as_bytes());
            }
        }
    }
    digest_parts(
        b"peritus.f0.published-debugger-evidence.v1\0",
        &[
            peritus_evidence::revision_digest(&revision).as_bytes(),
            job_id.as_bytes(),
            report_id.as_bytes(),
            report_digest.as_bytes(),
            manifest_id.as_bytes(),
            manifest_digest.as_bytes(),
            query_digest.as_bytes(),
            artifact_digest.as_bytes(),
            &artifact_size.to_be_bytes(),
            evidence_id.as_bytes(),
            &journal_position.to_be_bytes(),
            &citation_bytes,
        ],
    )
}

const fn incomplete() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::IncompleteEvidence,
        EvolutionOperation::BindDiagnosis,
        EvolutionRecovery::ObtainEvidence,
        "debugger report is not published",
    )
}
