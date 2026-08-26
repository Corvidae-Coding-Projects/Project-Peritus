//! Selection, deterministic-analysis, report, and publication records.

use peritus_types::{EvidenceId, Sha256Digest};

use crate::{DebuggerError, ReportId, SelectionManifestId};

/// Durable counters from one validated deterministic analysis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisCounts {
    claims: u64,
    causes: u64,
    patterns: u64,
}

impl AnalysisCounts {
    /// Creates exact counters.
    #[must_use]
    pub const fn new(claims: u64, causes: u64, patterns: u64) -> Self {
        Self { claims, causes, patterns }
    }
    /// Claim count.
    #[must_use]
    pub const fn claims(self) -> u64 {
        self.claims
    }
    /// Cause count.
    #[must_use]
    pub const fn causes(self) -> u64 {
        self.causes
    }
    /// Pattern count.
    #[must_use]
    pub const fn patterns(self) -> u64 {
        self.patterns
    }
}

/// Durable exact selection identity and accounting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionRecord {
    id: SelectionManifestId,
    digest: Sha256Digest,
    subjects: u64,
    events: u64,
}

impl SelectionRecord {
    /// Creates a nonempty exact selection observation.
    ///
    /// # Errors
    ///
    /// Rejects a selection with zero subjects or zero events.
    pub fn new(
        id: SelectionManifestId,
        digest: Sha256Digest,
        subjects: u64,
        events: u64,
    ) -> Result<Self, DebuggerError> {
        if subjects == 0 || events == 0 {
            return Err(super::invalid("selection record must contain subjects and events"));
        }
        Ok(Self { id, digest, subjects, events })
    }
    /// Manifest identity.
    #[must_use]
    pub const fn id(self) -> SelectionManifestId {
        self.id
    }
    /// Manifest digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    /// Selected subjects.
    #[must_use]
    pub const fn subjects(self) -> u64 {
        self.subjects
    }
    /// Selected events.
    #[must_use]
    pub const fn events(self) -> u64 {
        self.events
    }
}

/// Validated report identity retained after artifact staging and before evidence publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportRecord {
    id: ReportId,
    digest: Sha256Digest,
    size: u64,
}

impl ReportRecord {
    /// Creates a nonempty report record.
    ///
    /// # Errors
    ///
    /// Rejects a zero-length report artifact.
    pub fn new(id: ReportId, digest: Sha256Digest, size: u64) -> Result<Self, DebuggerError> {
        if size == 0 {
            return Err(super::invalid("report byte length is zero"));
        }
        Ok(Self { id, digest, size })
    }
    /// Report identity.
    #[must_use]
    pub const fn id(self) -> ReportId {
        self.id
    }
    /// Canonical report digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    /// Canonical report bytes.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }
}

/// Exact artifact/evidence publication result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationRecord {
    report_id: ReportId,
    artifact_digest: Sha256Digest,
    artifact_size: u64,
    evidence_id: EvidenceId,
    journal_position: u64,
}

impl PublicationRecord {
    /// Creates a complete publication observation.
    ///
    /// # Errors
    ///
    /// Rejects zero artifact length or a zero report-commit journal position.
    pub fn new(
        report_id: ReportId,
        artifact_digest: Sha256Digest,
        artifact_size: u64,
        evidence_id: EvidenceId,
        journal_position: u64,
    ) -> Result<Self, DebuggerError> {
        if artifact_size == 0 || journal_position == 0 {
            return Err(super::invalid("publication size or journal position is zero"));
        }
        Ok(Self { report_id, artifact_digest, artifact_size, evidence_id, journal_position })
    }
    /// Report identity.
    #[must_use]
    pub const fn report_id(self) -> ReportId {
        self.report_id
    }
    /// Finalized artifact digest.
    #[must_use]
    pub const fn artifact_digest(self) -> Sha256Digest {
        self.artifact_digest
    }
    /// Finalized artifact size.
    #[must_use]
    pub const fn artifact_size(self) -> u64 {
        self.artifact_size
    }
    /// Admitted evidence identity.
    #[must_use]
    pub const fn evidence_id(self) -> EvidenceId {
        self.evidence_id
    }
    /// Report-commit journal position used by evidence provenance.
    #[must_use]
    pub const fn journal_position(self) -> u64 {
        self.journal_position
    }
}
