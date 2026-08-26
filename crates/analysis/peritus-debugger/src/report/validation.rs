//! Complete report validation and validated wrapper.

use crate::{
    ComponentCorrelation, DebuggerError, DebuggerErrorKind, DebuggerLimit, DebuggerLimits,
    DebuggerOperation, DebuggerRecovery, DiagnosticStatus, HarnessHealthSummary, PatternCluster,
    ReportClaim, ReportId, RootCauseCandidate, Timeline, TraceSelectionManifest,
};
use peritus_types::{EvidenceId, Sha256Digest};

/// Complete diagnostic report prior to final validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebuggerReport {
    pub(super) manifest_id: crate::SelectionManifestId,
    pub(super) manifest_digest: Sha256Digest,
    pub(super) query_digest: Sha256Digest,
    pub(super) supersedes: Option<EvidenceId>,
    pub(super) timelines: Vec<Timeline>,
    pub(super) causes: Vec<RootCauseCandidate>,
    pub(super) patterns: Vec<PatternCluster>,
    pub(super) correlations: Vec<ComponentCorrelation>,
    pub(super) health: HarnessHealthSummary,
    pub(super) claims: Vec<ReportClaim>,
}

impl DebuggerReport {
    /// Assembles a report. [`Self::validate`] reruns every complete invariant.
    #[allow(
        clippy::too_many_arguments,
        reason = "complete immutable report sections remain explicit"
    )]
    #[must_use]
    pub const fn new(
        manifest: &TraceSelectionManifest,
        supersedes: Option<EvidenceId>,
        timelines: Vec<Timeline>,
        causes: Vec<RootCauseCandidate>,
        patterns: Vec<PatternCluster>,
        correlations: Vec<ComponentCorrelation>,
        health: HarnessHealthSummary,
        claims: Vec<ReportClaim>,
    ) -> Self {
        Self {
            manifest_id: manifest.id(),
            manifest_digest: manifest.digest(),
            query_digest: manifest.query_digest(),
            supersedes,
            timelines,
            causes,
            patterns,
            correlations,
            health,
            claims,
        }
    }

    /// Reruns the complete report contract and produces the only canonically encodable wrapper.
    ///
    /// # Errors
    ///
    /// Rejects binding drift, noncanonical sections, bound excess, bad citations, unsupported
    /// recommendation parents, taxonomy disagreement, or any authority-bearing representation.
    pub fn validate(
        self,
        manifest: &TraceSelectionManifest,
        limits: DebuggerLimits,
    ) -> Result<ValidatedReport, DebuggerError> {
        validate_report(self, manifest, limits)
    }

    /// Returns the selection manifest identity.
    #[must_use]
    pub const fn manifest_id(&self) -> crate::SelectionManifestId {
        self.manifest_id
    }
    /// Returns the complete selection manifest digest.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest_digest
    }
    /// Returns the frozen query digest.
    #[must_use]
    pub const fn query_digest(&self) -> Sha256Digest {
        self.query_digest
    }
    /// Returns a prior report evidence identity corrected by this new report.
    #[must_use]
    pub const fn supersedes(&self) -> Option<EvidenceId> {
        self.supersedes
    }
    /// Borrows per-subject timelines.
    #[must_use]
    pub fn timelines(&self) -> &[Timeline] {
        &self.timelines
    }
    /// Borrows root-cause candidates.
    #[must_use]
    pub fn causes(&self) -> &[RootCauseCandidate] {
        &self.causes
    }
    /// Borrows cross-run patterns.
    #[must_use]
    pub fn patterns(&self) -> &[PatternCluster] {
        &self.patterns
    }
    /// Borrows E1 component correlations.
    #[must_use]
    pub fn correlations(&self) -> &[ComponentCorrelation] {
        &self.correlations
    }
    /// Borrows diagnostic-only health.
    #[must_use]
    pub const fn health(&self) -> &HarnessHealthSummary {
        &self.health
    }
    /// Borrows typed claims.
    #[must_use]
    pub fn claims(&self) -> &[ReportClaim] {
        &self.claims
    }
}

/// Report proven bounded and manifest-contained, with stable canonical bytes and digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedReport {
    id: ReportId,
    digest: Sha256Digest,
    canonical_bytes: Vec<u8>,
    report: DebuggerReport,
}

impl ValidatedReport {
    /// Returns the content-derived report identity.
    #[must_use]
    pub const fn id(&self) -> ReportId {
        self.id
    }
    /// Returns SHA-256 over complete canonical report bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Borrows canonical schema-v1 bytes eligible for artifact finalization.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
    /// Borrows the completely validated semantic report.
    #[must_use]
    pub const fn report(&self) -> &DebuggerReport {
        &self.report
    }
}

/// Validates every report section and derives canonical identity.
///
/// # Errors
///
/// Returns a typed report, citation, binding, or bound error without partial output.
pub fn validate_report(
    report: DebuggerReport,
    manifest: &TraceSelectionManifest,
    limits: DebuggerLimits,
) -> Result<ValidatedReport, DebuggerError> {
    if report.manifest_id != manifest.id()
        || report.manifest_digest != manifest.digest()
        || report.query_digest != manifest.query_digest()
    {
        return Err(report_error("report selection or query binding differs"));
    }
    validate_timelines(&report, manifest)?;
    validate_causes(&report, manifest, limits)?;
    validate_patterns(&report, manifest, limits)?;
    validate_correlations(&report, limits)?;
    validate_claims(&report, manifest, limits)?;
    if report.health.status() != DiagnosticStatus::DiagnosticOnly {
        return Err(report_error("health summary is not diagnostic-only"));
    }
    let canonical_bytes = super::canonical::encode_report(&report);
    limits.check(
        DebuggerLimit::ReportBytes,
        canonical_bytes.len(),
        DebuggerOperation::ValidateReport,
    )?;
    let digest =
        crate::identity::domain_digest(b"peritus-e2-debugger-report-digest-v1\0", &canonical_bytes);
    let id = ReportId::derive(b"peritus-e2-debugger-report-id-v1\0", digest.as_bytes())?;
    Ok(ValidatedReport { id, digest, canonical_bytes, report })
}

fn validate_timelines(
    report: &DebuggerReport,
    manifest: &TraceSelectionManifest,
) -> Result<(), DebuggerError> {
    if report.timelines.len() != manifest.subjects().len()
        || report.timelines.windows(2).any(|pair| pair[0].subject_id() >= pair[1].subject_id())
    {
        return Err(report_error("timelines do not cover subjects exactly in canonical order"));
    }
    for (timeline, subject) in report.timelines.iter().zip(manifest.subjects()) {
        if timeline.subject_id() != subject.id() {
            return Err(report_error("timeline subject binding differs"));
        }
        for entry in timeline.entries() {
            entry.citation().validate_against(manifest)?;
        }
    }
    Ok(())
}

fn validate_causes(
    report: &DebuggerReport,
    manifest: &TraceSelectionManifest,
    limits: DebuggerLimits,
) -> Result<(), DebuggerError> {
    if report.causes.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
        return Err(report_error("causes must be strictly ordered by stable identity"));
    }
    limits.check(DebuggerLimit::Claims, report.causes.len(), DebuggerOperation::ValidateReport)?;
    for cause in &report.causes {
        cause
            .support()
            .iter()
            .chain(cause.contrary())
            .try_for_each(|citation| citation.validate_against(manifest))?;
        limits.check(
            DebuggerLimit::ContraryCitations,
            cause.contrary().len(),
            DebuggerOperation::ValidateReport,
        )?;
    }
    Ok(())
}

fn validate_patterns(
    report: &DebuggerReport,
    manifest: &TraceSelectionManifest,
    limits: DebuggerLimits,
) -> Result<(), DebuggerError> {
    if report.patterns.iter().any(|pattern| pattern.members().is_empty()) {
        return Err(report_error("pattern has no provenance members"));
    }
    if report.patterns.windows(2).any(|pair| {
        (pair[0].kind(), pair[0].fingerprint(), pair[0].members()[0].subject_id())
            >= (pair[1].kind(), pair[1].fingerprint(), pair[1].members()[0].subject_id())
    }) {
        return Err(report_error("patterns are not in canonical order"));
    }
    limits.check(
        DebuggerLimit::Patterns,
        report.patterns.len(),
        DebuggerOperation::ValidateReport,
    )?;
    for pattern in &report.patterns {
        limits.check(
            DebuggerLimit::PatternMembers,
            pattern.members().len(),
            DebuggerOperation::ValidateReport,
        )?;
        for member in pattern.members() {
            if manifest
                .subjects()
                .binary_search_by_key(&member.subject_id(), crate::AnalysisSubject::id)
                .is_err()
            {
                return Err(report_error("pattern member subject is absent"));
            }
            member
                .citations()
                .iter()
                .try_for_each(|citation| citation.validate_against(manifest))?;
        }
    }
    Ok(())
}

fn validate_correlations(
    report: &DebuggerReport,
    limits: DebuggerLimits,
) -> Result<(), DebuggerError> {
    limits.check(
        DebuggerLimit::ComponentLinks,
        report.correlations.len(),
        DebuggerOperation::ValidateReport,
    )?;
    for correlation in &report.correlations {
        if correlation.class_only() != correlation.component_id().is_none()
            || correlation.class_only() != correlation.content_digest().is_none()
            || correlation.protection_class() != correlation.component_kind().protection_class()
            || correlation.supporting_subjects().is_empty()
        {
            return Err(report_error(
                "component correlation identity or protection invariant differs",
            ));
        }
    }
    Ok(())
}

fn validate_claims(
    report: &DebuggerReport,
    manifest: &TraceSelectionManifest,
    limits: DebuggerLimits,
) -> Result<(), DebuggerError> {
    if report.claims.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
        return Err(report_error("claims must be strictly ordered by stable identity"));
    }
    limits.check(DebuggerLimit::Claims, report.claims.len(), DebuggerOperation::ValidateReport)?;
    for claim in &report.claims {
        claim
            .support()
            .iter()
            .chain(claim.contrary())
            .try_for_each(|citation| citation.validate_against(manifest))?;
        if let Some(parent) = claim.parent() {
            let parent = report
                .claims
                .iter()
                .find(|claim| claim.id() == parent)
                .ok_or_else(|| report_error("recommendation parent is absent"))?;
            if !matches!(parent.kind(), crate::ClaimKind::Observation | crate::ClaimKind::Inference)
            {
                return Err(report_error("recommendation parent is not a supported claim"));
            }
        }
    }
    Ok(())
}

fn report_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Report,
        DebuggerOperation::ValidateReport,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}
