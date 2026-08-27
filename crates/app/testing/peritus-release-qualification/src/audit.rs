//! Independent final audit and finding-closure evidence.

use serde::Serialize;

use peritus_release_artifacts::{ReleaseBinding, ReleasePath, Sha256Digest, digest_bytes};

use crate::{
    EvidenceDisposition, EvidenceKind, EvidenceReference, EvidenceSignature, ParticipantId,
    QualificationError, QualificationErrorCode, SignedEvidenceRecord,
};

/// Stable final-audit finding identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct FindingId(String);

impl FindingId {
    /// Validates a bounded finding identifier.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] for nonportable input.
    pub fn new(value: impl Into<String>) -> Result<Self, QualificationError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 96
            || !value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            })
        {
            return Err(QualificationError::new(
                QualificationErrorCode::InvalidValue,
                "validate audit finding ID",
                "finding ID violates the 1 through 96 byte portable grammar",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the finding identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Audit finding severity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FindingSeverity {
    /// Informational observation.
    Informational,
    /// Low-impact defect.
    Low,
    /// Moderate defect requiring tracked closure.
    Medium,
    /// High-impact release blocker.
    High,
    /// Critical release blocker.
    Critical,
}

impl FindingSeverity {
    /// Returns whether the severity blocks release until actually closed.
    #[must_use]
    pub const fn is_release_blocking(self) -> bool {
        matches!(self, Self::High | Self::Critical)
    }
}

/// Finding disposition and its externally signed evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FindingDisposition {
    /// The finding remains unresolved.
    Open,
    /// The finding was corrected and independently rechecked.
    Closed {
        /// Signed evidence of correction and recheck.
        closure_evidence: EvidenceReference,
    },
    /// A nonblocking residual risk was explicitly accepted.
    RiskAccepted {
        /// Signed approval evidence retained outside this report.
        approval_evidence: EvidenceReference,
    },
}

/// One independently authored final-audit finding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AuditFinding {
    id: FindingId,
    severity: FindingSeverity,
    summary: String,
    disposition: FindingDisposition,
}

impl AuditFinding {
    /// Creates a bounded audit finding.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] for empty, oversized, or control-bearing summary text.
    pub fn new(
        id: FindingId,
        severity: FindingSeverity,
        summary: impl Into<String>,
        disposition: FindingDisposition,
    ) -> Result<Self, QualificationError> {
        let summary = summary.into();
        if summary.is_empty()
            || summary.len() > 2_048
            || summary.bytes().any(|byte| {
                byte == 0 || (byte.is_ascii_control() && byte != b'\n' && byte != b'\t')
            })
        {
            return Err(QualificationError::new(
                QualificationErrorCode::InvalidValue,
                "validate audit finding",
                "finding summary is empty, unsafe, or exceeds 2048 bytes",
            ));
        }
        Ok(Self { id, severity, summary, disposition })
    }

    /// Returns the finding identity.
    #[must_use]
    pub const fn id(&self) -> &FindingId {
        &self.id
    }

    /// Returns the finding severity.
    #[must_use]
    pub const fn severity(&self) -> FindingSeverity {
        self.severity
    }

    /// Returns the finding disposition.
    #[must_use]
    pub const fn disposition(&self) -> &FindingDisposition {
        &self.disposition
    }

    /// Returns whether a release-blocking finding is actually closed.
    #[must_use]
    pub const fn blocking_finding_is_closed(&self) -> bool {
        !self.severity.is_release_blocking()
            || matches!(&self.disposition, FindingDisposition::Closed { .. })
    }
}

/// Unsigned final-audit content prepared by an external independent auditor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AuditDraft {
    schema_version: u32,
    binding: ReleaseBinding,
    auditor: ParticipantId,
    contributors: Vec<ParticipantId>,
    reviewed_evidence_set_digest: Sha256Digest,
    findings: Vec<AuditFinding>,
}

impl AuditDraft {
    /// Creates a canonical final-audit draft.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] for more than 1024 findings or duplicate finding IDs.
    pub fn new(
        binding: ReleaseBinding,
        auditor: ParticipantId,
        mut contributors: Vec<ParticipantId>,
        reviewed_evidence_set_digest: Sha256Digest,
        mut findings: Vec<AuditFinding>,
    ) -> Result<Self, QualificationError> {
        if contributors.len() > 4_096 {
            return Err(QualificationError::new(
                QualificationErrorCode::BoundExceeded,
                "create final audit draft",
                "final audit exceeds 4096 contributor identities",
            ));
        }
        contributors.sort();
        contributors.dedup();
        if findings.len() > 1_024 {
            return Err(QualificationError::new(
                QualificationErrorCode::BoundExceeded,
                "create final audit draft",
                "final audit exceeds 1024 findings",
            ));
        }
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        if let Some(pair) = findings.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(QualificationError::new(
                QualificationErrorCode::Duplicate,
                "create final audit draft",
                format!("duplicate audit finding {}", pair[0].id.as_str()),
            ));
        }
        Ok(Self {
            schema_version: 1,
            binding,
            auditor,
            contributors,
            reviewed_evidence_set_digest,
            findings,
        })
    }

    /// Serializes the exact bytes an external auditor must sign.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] if serialization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, QualificationError> {
        serde_json::to_vec(self)
            .map_err(|source| QualificationError::serialization("serialize final audit", source))
    }
}

/// Signature-verified final audit with an explicit independence observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FinalAudit {
    draft: AuditDraft,
    signed_record: SignedEvidenceRecord,
    independent: bool,
    audit_digest: Sha256Digest,
}

impl FinalAudit {
    /// Verifies a final audit signature and checks the auditor against known contributors.
    ///
    /// Independence failure is retained as `false` so final reduction fails closed without
    /// discarding the signed audit.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] if serialization or signature verification fails.
    pub fn verify(
        draft: AuditDraft,
        retained_path: ReleasePath,
        signature: EvidenceSignature,
    ) -> Result<Self, QualificationError> {
        let bytes = draft.canonical_json()?;
        let signed_record = SignedEvidenceRecord::verify(
            draft.binding.clone(),
            EvidenceKind::FinalAudit,
            EvidenceDisposition::Satisfied,
            retained_path,
            &bytes,
            signature,
        )?;
        let independent = !draft.contributors.contains(&draft.auditor);
        Ok(Self { draft, signed_record, independent, audit_digest: digest_bytes(&bytes) })
    }

    /// Returns the exact release binding.
    #[must_use]
    pub const fn binding(&self) -> &ReleaseBinding {
        &self.draft.binding
    }

    /// Returns the external auditor identity.
    #[must_use]
    pub const fn auditor(&self) -> &ParticipantId {
        &self.draft.auditor
    }

    /// Returns whether the auditor was absent from the supplied contributor set.
    #[must_use]
    pub const fn is_independent(&self) -> bool {
        self.independent
    }

    /// Returns the pre-audit evidence-set digest reviewed by the auditor.
    #[must_use]
    pub const fn reviewed_evidence_set_digest(&self) -> Sha256Digest {
        self.draft.reviewed_evidence_set_digest
    }

    /// Returns canonical findings.
    #[must_use]
    pub fn findings(&self) -> &[AuditFinding] {
        &self.draft.findings
    }

    /// Returns whether every release-blocking finding is actually closed.
    #[must_use]
    pub fn blocking_findings_closed(&self) -> bool {
        self.draft.findings.iter().all(AuditFinding::blocking_finding_is_closed)
    }

    /// Returns the signed final-audit evidence reference.
    #[must_use]
    pub const fn evidence_reference(&self) -> &EvidenceReference {
        self.signed_record.evidence_reference()
    }

    /// Returns the final-audit content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.audit_digest
    }
}
