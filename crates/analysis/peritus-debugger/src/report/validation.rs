//! Complete report validation and validated wrapper.

use crate::{
    AnalysisContext, AnalysisControl, AnalysisStage, ComponentCorrelation, DebuggerError,
    DebuggerErrorKind, DebuggerLimit, DebuggerLimits, DebuggerOperation, DebuggerRecovery,
    DiagnosticStatus, EvidenceCitation, HarnessHealthSummary, PatternCluster, ReportClaim,
    ReportId, RootCauseCandidate, Timeline, TraceSelectionManifest,
};
use peritus_types::{EvidenceId, Sha256Digest};
use std::collections::BTreeSet;
use std::sync::OnceLock;

const REPORT_ARTIFACT_FANOUT: usize = 256;
const REPORT_PAGE_ENVELOPE_ALLOWANCE: usize = 128;

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
    /// Rejects binding drift, noncanonical sections, bad citations, unsupported recommendation
    /// parents, taxonomy disagreement, or any authority-bearing representation.
    pub fn validate(
        self,
        manifest: &TraceSelectionManifest,
        limits: DebuggerLimits,
    ) -> Result<ValidatedReport, DebuggerError> {
        validate_report(self, manifest, limits)
    }

    /// Validates with exact progress and cooperative suspension or cancellation.
    ///
    /// # Errors
    /// Rejects context drift, cancellation, or any complete report-contract violation.
    pub fn validate_controlled(
        self,
        manifest: &TraceSelectionManifest,
        limits: DebuggerLimits,
        context: AnalysisContext,
        control: &mut impl AnalysisControl,
    ) -> Result<ValidatedReport, DebuggerError> {
        validate_report_controlled(self, manifest, limits, context, control)
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

/// Exact cursor for one lossless canonical report page.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReportContinuationCursor {
    report_id: ReportId,
    payload_digest: Sha256Digest,
    page_ordinal: u64,
    byte_offset: u64,
}

impl ReportContinuationCursor {
    /// Returns the semantic report identity bound to this cursor.
    #[must_use]
    pub const fn report_id(self) -> ReportId {
        self.report_id
    }
    /// Returns the complete canonical payload digest bound to this cursor.
    #[must_use]
    pub const fn payload_digest(self) -> Sha256Digest {
        self.payload_digest
    }
    /// Returns the zero-based page ordinal.
    #[must_use]
    pub const fn page_ordinal(self) -> u64 {
        self.page_ordinal
    }
    /// Returns the exact byte offset in the complete schema-v1 payload.
    #[must_use]
    pub const fn byte_offset(self) -> u64 {
        self.byte_offset
    }
}

/// One durable page in the lossless canonical report stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportArtifactPage {
    cursor: ReportContinuationCursor,
    next_cursor: Option<ReportContinuationCursor>,
    artifact_digest: Sha256Digest,
    artifact_size: u64,
    payload_end: u64,
    artifact_index: usize,
    artifact_payload_start: usize,
    artifact_payload_end: usize,
}

impl ReportArtifactPage {
    /// Returns the cursor that reads this page.
    #[must_use]
    pub const fn cursor(self) -> ReportContinuationCursor {
        self.cursor
    }
    /// Returns the exact next cursor, or `None` after the final page.
    #[must_use]
    pub const fn next_cursor(self) -> Option<ReportContinuationCursor> {
        self.next_cursor
    }
    /// Returns the content digest of the durable page artifact.
    #[must_use]
    pub const fn artifact_digest(self) -> Sha256Digest {
        self.artifact_digest
    }
    /// Returns the exact durable page artifact size.
    #[must_use]
    pub const fn artifact_size(self) -> u64 {
        self.artifact_size
    }
    /// Returns the exclusive canonical payload byte offset covered by this page.
    #[must_use]
    pub const fn payload_end(self) -> u64 {
        self.payload_end
    }
}

/// One bottom-up durable artifact object and its exact direct dependencies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReportArtifactObject {
    digest: Sha256Digest,
    bytes: Vec<u8>,
    dependencies: Vec<(Sha256Digest, u64)>,
}

impl ReportArtifactObject {
    /// Returns the raw content digest.
    pub(crate) const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Borrows the exact artifact bytes.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Returns the exact artifact size.
    pub(crate) fn size(&self) -> u64 {
        u64::try_from(self.bytes.len()).unwrap_or(u64::MAX)
    }
    /// Borrows ordered direct child digests and sizes.
    pub(crate) fn dependencies(&self) -> &[(Sha256Digest, u64)] {
        &self.dependencies
    }
}

/// Report proven manifest-contained, with a lossless bounded artifact layout.
#[derive(Clone, Debug)]
pub struct ValidatedReport {
    id: ReportId,
    digest: Sha256Digest,
    canonical_size: u64,
    canonical_cache: OnceLock<Vec<u8>>,
    artifacts: Vec<ReportArtifactObject>,
    root_artifact: usize,
    pages: Vec<ReportArtifactPage>,
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
    /// Materializes the complete canonical schema-v1 payload on explicit request.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        self.canonical_cache
            .get_or_init(|| {
                let mut bytes = Vec::with_capacity(
                    usize::try_from(self.canonical_size).unwrap_or(usize::MAX),
                );
                for page in &self.pages {
                    if let Some((chunk, _)) = self.read_page(page.cursor) {
                        bytes.extend_from_slice(chunk);
                    }
                }
                bytes
            })
            .as_slice()
    }
    /// Returns the exact complete schema-v1 payload size without materializing it.
    #[must_use]
    pub const fn canonical_size(&self) -> u64 {
        self.canonical_size
    }
    /// Borrows the root artifact bytes referenced by durable report and evidence records.
    ///
    /// Reports within the active page bound retain their exact schema-v1 bytes here. Larger
    /// reports expose a schema-v2 index whose dependency tree covers every schema-v1 byte.
    #[must_use]
    pub fn artifact_bytes(&self) -> &[u8] {
        self.artifacts
            .get(self.root_artifact)
            .map_or(&[], ReportArtifactObject::bytes)
    }
    /// Borrows canonical report-page metadata in exact continuation order.
    #[must_use]
    pub fn artifact_pages(&self) -> &[ReportArtifactPage] {
        &self.pages
    }
    /// Returns the first exact continuation cursor.
    #[must_use]
    pub fn first_cursor(&self) -> Option<ReportContinuationCursor> {
        self.pages.first().map(|page| page.cursor)
    }
    /// Reads one canonical payload page and returns its exact continuation cursor.
    #[must_use]
    pub fn read_page(
        &self,
        cursor: ReportContinuationCursor,
    ) -> Option<(&[u8], Option<ReportContinuationCursor>)> {
        let page = self.pages.get(usize::try_from(cursor.page_ordinal).ok()?)?;
        if page.cursor != cursor {
            return None;
        }
        self.artifacts
            .get(page.artifact_index)?
            .bytes
            .get(page.artifact_payload_start..page.artifact_payload_end)
            .map(|bytes| (bytes, page.next_cursor))
    }
    /// Borrows the completely validated semantic report.
    #[must_use]
    pub const fn report(&self) -> &DebuggerReport {
        &self.report
    }
    /// Borrows bottom-up artifact objects; the final object is the durable root.
    pub(crate) fn artifact_objects(&self) -> &[ReportArtifactObject] {
        &self.artifacts
    }
}

impl PartialEq for ValidatedReport {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.digest == other.digest
            && self.canonical_size == other.canonical_size
            && self.artifacts == other.artifacts
            && self.root_artifact == other.root_artifact
            && self.pages == other.pages
            && self.report == other.report
    }
}

impl Eq for ValidatedReport {}

/// Validates every report section and derives canonical identity.
///
/// # Errors
///
/// Returns a typed report, citation, or binding error without partial output.
pub fn validate_report(
    report: DebuggerReport,
    manifest: &TraceSelectionManifest,
    limits: DebuggerLimits,
) -> Result<ValidatedReport, DebuggerError> {
    let context = AnalysisContext::for_manifest(manifest);
    validate_report_controlled(
        report,
        manifest,
        limits,
        context,
        &mut crate::work::RunToCompletion,
    )
}

/// Validates and incrementally encodes a report with exact cooperative progress.
///
/// # Errors
/// Returns a typed binding, report, citation, or cancellation error without partial output.
pub fn validate_report_controlled(
    report: DebuggerReport,
    manifest: &TraceSelectionManifest,
    limits: DebuggerLimits,
    context: AnalysisContext,
    control: &mut impl AnalysisControl,
) -> Result<ValidatedReport, DebuggerError> {
    context.validate(manifest, DebuggerOperation::ValidateReport)?;
    if report.manifest_id != manifest.id()
        || report.manifest_digest != manifest.digest()
        || report.query_digest != manifest.query_digest()
    {
        return Err(report_error("report selection or query binding differs"));
    }
    let mut validated_citations = BTreeSet::new();
    crate::work::checkpoint(
        control, context, AnalysisStage::ValidateReport, 0, 5,
        DebuggerOperation::ValidateReport,
    )?;
    validate_timelines(&report, manifest, &mut validated_citations)?;
    crate::work::checkpoint(
        control, context, AnalysisStage::ValidateReport, 1, 5,
        DebuggerOperation::ValidateReport,
    )?;
    validate_causes(&report, manifest, &mut validated_citations)?;
    crate::work::checkpoint(
        control, context, AnalysisStage::ValidateReport, 2, 5,
        DebuggerOperation::ValidateReport,
    )?;
    validate_patterns(&report, manifest, limits, &mut validated_citations)?;
    crate::work::checkpoint(
        control, context, AnalysisStage::ValidateReport, 3, 5,
        DebuggerOperation::ValidateReport,
    )?;
    validate_correlations(&report)?;
    crate::work::checkpoint(
        control, context, AnalysisStage::ValidateReport, 4, 5,
        DebuggerOperation::ValidateReport,
    )?;
    validate_claims(&report, manifest, &mut validated_citations)?;
    crate::work::checkpoint(
        control, context, AnalysisStage::ValidateReport, 5, 5,
        DebuggerOperation::ValidateReport,
    )?;
    drop(validated_citations);
    if report.health.status() != DiagnosticStatus::DiagnosticOnly {
        return Err(report_error("health summary is not diagnostic-only"));
    }
    let page_bound = usize::try_from(limits.get(DebuggerLimit::ReportBytes))
        .unwrap_or(usize::MAX)
        .max(1);
    let page_payload_bound =
        page_bound.saturating_sub(REPORT_PAGE_ENVELOPE_ALLOWANCE).max(1);
    let encoded = super::streaming::encode_report(
        &report,
        page_payload_bound,
        context,
        control,
    )?;
    let digest = encoded.digest;
    let id = ReportId::derive(b"peritus-e2-debugger-report-id-v1\0", digest.as_bytes())?;
    let (artifacts, root_artifact, pages) = build_artifact_layout(
        id,
        digest,
        encoded.chunks,
        encoded.payload_size,
        page_bound,
    );
    Ok(ValidatedReport {
        id,
        digest,
        canonical_size: encoded.payload_size,
        canonical_cache: OnceLock::new(),
        artifacts,
        root_artifact,
        pages,
        report,
    })
}

#[derive(Clone, Copy)]
struct ArtifactReference {
    digest: Sha256Digest,
    size: u64,
    start: u64,
    end: u64,
}

fn build_artifact_layout(
    report_id: ReportId,
    payload_digest: Sha256Digest,
    chunks: Vec<Vec<u8>>,
    payload_size: u64,
    page_bound: usize,
) -> (Vec<ReportArtifactObject>, usize, Vec<ReportArtifactPage>) {
    if payload_size <= u64::try_from(page_bound).unwrap_or(u64::MAX) {
        let mut payload = Vec::with_capacity(usize::try_from(payload_size).unwrap_or(page_bound));
        for chunk in chunks {
            payload.extend_from_slice(&chunk);
        }
        let digest = peritus_codec::sha256(&payload);
        let cursor = ReportContinuationCursor {
            report_id,
            payload_digest,
            page_ordinal: 0,
            byte_offset: 0,
        };
        return (
            vec![ReportArtifactObject {
                digest,
                bytes: payload,
                dependencies: Vec::new(),
            }],
            0,
            vec![ReportArtifactPage {
                cursor,
                next_cursor: None,
                artifact_digest: digest,
                artifact_size: payload_size,
                payload_end: payload_size,
                artifact_index: 0,
                artifact_payload_start: 0,
                artifact_payload_end: usize::try_from(payload_size).unwrap_or(page_bound),
            }],
        );
    }

    let mut artifacts = Vec::new();
    let mut pages = Vec::with_capacity(chunks.len());
    let mut references = Vec::with_capacity(chunks.len());
    let mut start = 0_usize;
    for (ordinal, chunk) in chunks.into_iter().enumerate() {
        let end = start.saturating_add(chunk.len());
        let ordinal = u64::try_from(ordinal).unwrap_or(u64::MAX);
        let start_u64 = u64::try_from(start).unwrap_or(u64::MAX);
        let end_u64 = u64::try_from(end).unwrap_or(u64::MAX);
        let (bytes, payload_range) = super::canonical::encode_report_page(
            report_id,
            payload_digest,
            ordinal,
            start_u64,
            end_u64,
            &chunk,
        );
        let digest = peritus_codec::sha256(&bytes);
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let cursor = ReportContinuationCursor {
            report_id,
            payload_digest,
            page_ordinal: ordinal,
            byte_offset: start_u64,
        };
        let next_cursor = (end_u64 < payload_size).then_some(ReportContinuationCursor {
            report_id,
            payload_digest,
            page_ordinal: ordinal.saturating_add(1),
            byte_offset: end_u64,
        });
        let artifact_index = artifacts.len();
        artifacts.push(ReportArtifactObject { digest, bytes, dependencies: Vec::new() });
        pages.push(ReportArtifactPage {
            cursor,
            next_cursor,
            artifact_digest: digest,
            artifact_size: size,
            payload_end: end_u64,
            artifact_index,
            artifact_payload_start: payload_range.start,
            artifact_payload_end: payload_range.end,
        });
        references.push(ArtifactReference {
            digest,
            size,
            start: start_u64,
            end: end_u64,
        });
        start = end;
    }

    let mut level = 1_u16;
    while references.len() > 1 {
        let mut parents = Vec::with_capacity(references.len().div_ceil(REPORT_ARTIFACT_FANOUT));
        for (ordinal, children) in references.chunks(REPORT_ARTIFACT_FANOUT).enumerate() {
            let start = children.first().map_or(0, |child| child.start);
            let end = children.last().map_or(start, |child| child.end);
            let encoded_children: Vec<_> = children
                .iter()
                .map(|child| (child.digest, child.size, child.start, child.end))
                .collect();
            let bytes = super::canonical::encode_report_index(
                report_id,
                payload_digest,
                payload_size,
                level,
                u64::try_from(ordinal).unwrap_or(u64::MAX),
                (start, end),
                &encoded_children,
            );
            let digest = peritus_codec::sha256(&bytes);
            let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            let dependencies =
                children.iter().map(|child| (child.digest, child.size)).collect();
            artifacts.push(ReportArtifactObject { digest, bytes, dependencies });
            parents.push(ArtifactReference { digest, size, start, end });
        }
        references = parents;
        level = level.saturating_add(1);
    }
    let root_artifact = artifacts.len().saturating_sub(1);
    (artifacts, root_artifact, pages)
}

fn validate_timelines<'a>(
    report: &'a DebuggerReport,
    manifest: &TraceSelectionManifest,
    validated: &mut BTreeSet<&'a EvidenceCitation>,
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
            validate_citation_once(entry.citation(), manifest, validated)?;
        }
    }
    Ok(())
}

fn validate_causes<'a>(
    report: &'a DebuggerReport,
    manifest: &TraceSelectionManifest,
    validated: &mut BTreeSet<&'a EvidenceCitation>,
) -> Result<(), DebuggerError> {
    if report.causes.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
        return Err(report_error("causes must be strictly ordered by stable identity"));
    }
    for cause in &report.causes {
        cause
            .support()
            .iter()
            .chain(cause.contrary())
            .try_for_each(|citation| validate_citation_once(citation, manifest, validated))?;
    }
    Ok(())
}

fn validate_patterns<'a>(
    report: &'a DebuggerReport,
    manifest: &TraceSelectionManifest,
    limits: DebuggerLimits,
    validated: &mut BTreeSet<&'a EvidenceCitation>,
) -> Result<(), DebuggerError> {
    if report.patterns.iter().any(|pattern| pattern.members().is_empty()) {
        return Err(report_error("pattern has no provenance members"));
    }
    if report.patterns.windows(2).any(|pair| {
        (
            pair[0].kind(),
            pair[0].fingerprint(),
            pair[0].members()[0].subject_id(),
            pair[0].id(),
        ) >= (
            pair[1].kind(),
            pair[1].fingerprint(),
            pair[1].members()[0].subject_id(),
            pair[1].id(),
        )
    }) {
        return Err(report_error("patterns are not in canonical order"));
    }
    for pattern in &report.patterns {
        limits.check(
            DebuggerLimit::PatternMembers,
            pattern.members().len(),
            DebuggerOperation::ValidateReport,
        )?;
        if pattern
            .source_fingerprints()
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
            || pattern.members().windows(2).any(|pair| {
                (pair[0].subject_id(), pair[0].fingerprint(), pair[0].citations())
                    >= (pair[1].subject_id(), pair[1].fingerprint(), pair[1].citations())
            })
        {
            return Err(report_error("pattern provenance is not in canonical order"));
        }
        let first = &pattern.members()[0];
        for member in pattern.members() {
            let subject = manifest
                .subject(member.subject_id())
                .ok_or_else(|| report_error("pattern member subject is absent"))?;
            if member.environment_id() != subject.environment_id()
                || member.harness_revision() != subject.harness_revision()
                || member.workspace_revision() != subject.revision().workspace_revision()
                || member.provider_profile_id() != subject.revision().provider_profile_id()
                || member.environment_id() != first.environment_id()
                || member.harness_revision() != first.harness_revision()
                || member.workspace_revision() != first.workspace_revision()
                || member.provider_profile_id() != first.provider_profile_id()
            {
                return Err(report_error(
                    "pattern crosses an environment, harness, workspace, or provider revision",
                ));
            }
            member
                .citations()
                .iter()
                .try_for_each(|citation| validate_citation_once(citation, manifest, validated))?;
        }
    }
    Ok(())
}

fn validate_correlations(report: &DebuggerReport) -> Result<(), DebuggerError> {
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

fn validate_claims<'a>(
    report: &'a DebuggerReport,
    manifest: &TraceSelectionManifest,
    validated: &mut BTreeSet<&'a EvidenceCitation>,
) -> Result<(), DebuggerError> {
    if report.claims.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
        return Err(report_error("claims must be strictly ordered by stable identity"));
    }
    for claim in &report.claims {
        claim
            .support()
            .iter()
            .chain(claim.contrary())
            .try_for_each(|citation| validate_citation_once(citation, manifest, validated))?;
        if let Some(parent) = claim.parent() {
            let parent = report
                .claims
                .binary_search_by_key(&parent, ReportClaim::id)
                .ok()
                .map(|index| &report.claims[index])
                .ok_or_else(|| report_error("recommendation parent is absent"))?;
            if !matches!(parent.kind(), crate::ClaimKind::Observation | crate::ClaimKind::Inference)
            {
                return Err(report_error("recommendation parent is not a supported claim"));
            }
        }
    }
    Ok(())
}

fn validate_citation_once<'a>(
    citation: &'a EvidenceCitation,
    manifest: &TraceSelectionManifest,
    validated: &mut BTreeSet<&'a EvidenceCitation>,
) -> Result<(), DebuggerError> {
    if validated.insert(citation) {
        citation.validate_against(manifest)?;
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
