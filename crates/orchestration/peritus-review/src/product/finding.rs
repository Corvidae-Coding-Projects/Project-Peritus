//! Typed product findings with policy-derived blocker status.

use peritus_codec::sha256;
use peritus_spec::FindingSeverity;
use peritus_types::Sha256Digest;

use super::{
    ProductFindingBody, ProductFindingBodyPublisher, ProductFindingBodyReference,
    ProductReviewError,
    body::{measure_body, stream_body},
};

/// Product review categories with explicit acceptance semantics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProductFindingCategory {
    /// Code or runtime behavior is incorrect.
    Correctness,
    /// The requested behavior is absent or materially different.
    RequestedBehavior,
    /// The exact changed project was not built or gate coverage is missing.
    BuildCoverage,
    /// Tests are missing, stale, or did not cover the changed behavior.
    TestCoverage,
    /// A production security contract is violated.
    Security,
    /// Maintainability problem that is not itself a functional failure.
    Maintainability,
    /// User-facing or operator documentation problem.
    Documentation,
}

impl ProductFindingCategory {
    /// Parses the stable model-facing category spelling.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "correctness" => Some(Self::Correctness),
            "requested_behavior" => Some(Self::RequestedBehavior),
            "build_coverage" => Some(Self::BuildCoverage),
            "test_coverage" => Some(Self::TestCoverage),
            "security" => Some(Self::Security),
            "maintainability" => Some(Self::Maintainability),
            "documentation" => Some(Self::Documentation),
            _ => None,
        }
    }

    /// Stable model-facing category spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Correctness => "correctness",
            Self::RequestedBehavior => "requested_behavior",
            Self::BuildCoverage => "build_coverage",
            Self::TestCoverage => "test_coverage",
            Self::Security => "security",
            Self::Maintainability => "maintainability",
            Self::Documentation => "documentation",
        }
    }

    /// Categories whose non-advisory findings require remediation below the global high threshold.
    #[must_use]
    pub const fn blocks_when_non_advisory(self) -> bool {
        matches!(
            self,
            Self::Correctness
                | Self::RequestedBehavior
                | Self::BuildCoverage
                | Self::TestCoverage
                | Self::Security
        )
    }
}

/// Current conserved lifecycle of one stable finding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductFindingState {
    /// Admitted by a reviewer and not yet addressed.
    Open,
    /// A fixer ran with this finding, but reviewer confirmation is pending.
    FixProposed {
        /// Fixer cycle that claimed to address the finding.
        cycle: u32,
    },
    /// A later independent review confirmed the finding absent.
    ResolutionConfirmed {
        /// Fresh reviewer cycle that confirmed resolution.
        cycle: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProductFindingBodyOwner {
    Inline(ProductFindingBody),
    Stored(ProductFindingBodyReference),
}

/// One model-originated finding normalized into policy data.
///
/// Fresh submissions own an inline body only until a host publisher synchronizes it. Governed
/// ledgers retain the compact stored variant; legacy embedders may retain inline bodies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductFinding {
    id: Sha256Digest,
    identity_version: u8,
    category: ProductFindingCategory,
    severity: FindingSeverity,
    body: ProductFindingBodyOwner,
    state: ProductFindingState,
    first_cycle: u32,
    last_cycle: u32,
}

impl ProductFinding {
    /// Creates a finding and derives its stable normalized identity.
    ///
    /// # Errors
    /// Rejects missing primary text or fields containing a NUL character.
    #[allow(clippy::too_many_arguments, reason = "typed finding fields remain explicit")]
    pub fn new(
        category: ProductFindingCategory,
        severity: FindingSeverity,
        title: String,
        description: String,
        location: String,
        reproduction: String,
        remediation: String,
        cycle: u32,
    ) -> Result<Self, ProductReviewError> {
        validate_inline(cycle, &title, &description, &location, &reproduction, &remediation)?;
        let provenance = normalize_provenance(&location);
        let id = finding_id(CURRENT_IDENTITY_VERSION, category, &title, &provenance);
        Ok(Self {
            id,
            identity_version: CURRENT_IDENTITY_VERSION,
            category,
            severity,
            body: ProductFindingBodyOwner::Inline(ProductFindingBody::new(
                title,
                description,
                location,
                provenance,
                reproduction,
                remediation,
            )),
            state: ProductFindingState::Open,
            first_cycle: cycle,
            last_cycle: cycle,
        })
    }

    /// Restores a previously validated legacy product finding and lifecycle.
    #[allow(clippy::too_many_arguments, reason = "durable finding fields remain explicit")]
    pub fn restore(
        category: ProductFindingCategory,
        severity: FindingSeverity,
        title: String,
        description: String,
        location: String,
        reproduction: String,
        remediation: String,
        state: ProductFindingState,
        first_cycle: u32,
        last_cycle: u32,
    ) -> Result<Self, ProductReviewError> {
        let provenance = normalize_provenance(&location);
        let id = finding_id(LEGACY_IDENTITY_VERSION, category, &title, &provenance);
        Self::restore_indexed(
            id,
            LEGACY_IDENTITY_VERSION,
            provenance,
            category,
            severity,
            title,
            description,
            location,
            reproduction,
            remediation,
            state,
            first_cycle,
            last_cycle,
        )
    }

    /// Restores one inline entry from a version-three or version-four indexed representation.
    #[allow(clippy::too_many_arguments, reason = "durable finding fields remain explicit")]
    pub fn restore_indexed(
        id: Sha256Digest,
        identity_version: u8,
        provenance: String,
        category: ProductFindingCategory,
        severity: FindingSeverity,
        title: String,
        description: String,
        location: String,
        reproduction: String,
        remediation: String,
        state: ProductFindingState,
        first_cycle: u32,
        last_cycle: u32,
    ) -> Result<Self, ProductReviewError> {
        validate_history(identity_version, state, first_cycle, last_cycle)?;
        validate_inline(
            first_cycle,
            &title,
            &description,
            &location,
            &reproduction,
            &remediation,
        )?;
        if provenance != normalize_provenance(&location)
            || id != finding_id(identity_version, category, &title, &provenance)
        {
            return Err(ProductReviewError::new(
                "restored finding identity does not match its provenance",
            ));
        }
        Ok(Self {
            id,
            identity_version,
            category,
            severity,
            body: ProductFindingBodyOwner::Inline(ProductFindingBody::new(
                title,
                description,
                location,
                provenance,
                reproduction,
                remediation,
            )),
            state,
            first_cycle,
            last_cycle,
        })
    }

    /// Restores one compact version-five indexed entry.
    ///
    /// # Errors
    /// Rejects invalid lifecycle history or a descriptor bound to another finding identity.
    pub fn restore_stored(
        id: Sha256Digest,
        identity_version: u8,
        category: ProductFindingCategory,
        severity: FindingSeverity,
        body: ProductFindingBodyReference,
        state: ProductFindingState,
        first_cycle: u32,
        last_cycle: u32,
    ) -> Result<Self, ProductReviewError> {
        validate_history(identity_version, state, first_cycle, last_cycle)?;
        body.validate_finding(id, identity_version)?;
        Ok(Self {
            id,
            identity_version,
            category,
            severity,
            body: ProductFindingBodyOwner::Stored(body),
            state,
            first_cycle,
            last_cycle,
        })
    }

    /// Stable normalized identity.
    #[must_use]
    pub const fn id(&self) -> Sha256Digest { self.id }
    /// Durable identity derivation version.
    #[must_use]
    pub const fn identity_version(&self) -> u8 { self.identity_version }
    /// Typed category.
    #[must_use]
    pub const fn category(&self) -> ProductFindingCategory { self.category }
    /// Reviewer severity.
    #[must_use]
    pub const fn severity(&self) -> FindingSeverity { self.severity }

    /// Policy-derived blocking status.
    #[must_use]
    pub const fn blocking(&self) -> bool {
        !matches!(self.severity, FindingSeverity::Advisory)
            && (self.category.blocks_when_non_advisory()
                || severity_at_least(self.severity, FindingSeverity::High))
    }

    /// Exact inline body when retained by a fresh submission or legacy embedder.
    #[must_use]
    pub const fn inline_body(&self) -> Option<&ProductFindingBody> {
        match &self.body {
            ProductFindingBodyOwner::Inline(body) => Some(body),
            ProductFindingBodyOwner::Stored(_) => None,
        }
    }

    /// Compact immutable body authority after host publication.
    #[must_use]
    pub const fn body_reference(&self) -> Option<&ProductFindingBodyReference> {
        match &self.body {
            ProductFindingBodyOwner::Inline(_) => None,
            ProductFindingBodyOwner::Stored(reference) => Some(reference),
        }
    }

    /// Exact title for inline compatibility, or the bounded authenticated stored preview.
    #[must_use]
    pub fn title(&self) -> &str {
        match &self.body {
            ProductFindingBodyOwner::Inline(body) => body.title(),
            ProductFindingBodyOwner::Stored(reference) => reference.title_preview().text(),
        }
    }

    /// Exact description when inline; governed callers range-read stored bodies instead.
    #[must_use]
    pub fn description(&self) -> &str {
        self.inline_body().map_or("", ProductFindingBody::description)
    }

    /// Exact location when inline, or bounded normalized provenance when stored.
    #[must_use]
    pub fn location(&self) -> &str {
        match &self.body {
            ProductFindingBodyOwner::Inline(body) => body.location(),
            ProductFindingBodyOwner::Stored(reference) => reference.provenance_preview().text(),
        }
    }

    /// Exact normalized provenance when inline, or its bounded authenticated stored preview.
    #[must_use]
    pub fn provenance(&self) -> &str {
        match &self.body {
            ProductFindingBodyOwner::Inline(body) => body.provenance(),
            ProductFindingBodyOwner::Stored(reference) => reference.provenance_preview().text(),
        }
    }

    /// Exact reproduction when inline; governed callers range-read stored bodies instead.
    #[must_use]
    pub fn reproduction(&self) -> &str {
        self.inline_body().map_or("", ProductFindingBody::reproduction)
    }

    /// Exact remediation when inline; governed callers range-read stored bodies instead.
    #[must_use]
    pub fn remediation(&self) -> &str {
        self.inline_body().map_or("", ProductFindingBody::remediation)
    }

    /// Conserved lifecycle state.
    #[must_use]
    pub const fn state(&self) -> ProductFindingState { self.state }
    /// First review cycle that admitted the finding.
    #[must_use]
    pub const fn first_cycle(&self) -> u32 { self.first_cycle }
    /// Most recent review cycle that observed or resolved it.
    #[must_use]
    pub const fn last_cycle(&self) -> u32 { self.last_cycle }

    /// Computes the compact descriptor for this exact inline body without allocating a second body.
    ///
    /// # Errors
    /// Rejects a finding whose body is already stored or whose canonical size overflows.
    pub fn canonical_body_reference(
        &self,
        source_ordinal: u64,
    ) -> Result<ProductFindingBodyReference, ProductReviewError> {
        let body = self.inline_body().ok_or_else(|| {
            ProductReviewError::new("finding body is already stored out of line")
        })?;
        measure_body(
            self.id,
            self.identity_version,
            self.category,
            self.severity,
            body,
            source_ordinal,
        )
    }

    /// Streams the canonical UTF-8 artifact without constructing another complete body string.
    ///
    /// # Errors
    /// Rejects a stored body, size overflow, or a writer failure.
    pub fn stream_canonical_body(
        &self,
        write: &mut dyn FnMut(&[u8]) -> Result<(), ProductReviewError>,
    ) -> Result<(), ProductReviewError> {
        let body = self.inline_body().ok_or_else(|| {
            ProductReviewError::new("finding body is already stored out of line")
        })?;
        stream_body(
            self.id,
            self.identity_version,
            self.category,
            self.severity,
            body,
            write,
        )
    }

    pub(super) fn publish_body(
        &mut self,
        publisher: &dyn ProductFindingBodyPublisher,
        source_ordinal: u64,
    ) -> Result<(), ProductReviewError> {
        if let Some(reference) = self.body_reference() {
            if reference.source_ordinal() != source_ordinal {
                return Err(ProductReviewError::new(
                    "stored finding body changed its stable source ordinal",
                ));
            }
            return Ok(());
        }
        let reference = publisher.publish(&self, source_ordinal)?;
        self.validate_published_body(&reference, source_ordinal)?;
        self.adopt_published_body(reference);
        Ok(())
    }

    pub(super) fn validate_published_body(
        &self,
        reference: &ProductFindingBodyReference,
        source_ordinal: u64,
    ) -> Result<(), ProductReviewError> {
        reference.validate_finding(self.id, self.identity_version)?;
        if reference.source_ordinal() != source_ordinal {
            return Err(ProductReviewError::new(
                "finding body publisher changed its reserved source ordinal",
            ));
        }
        if reference != &self.canonical_body_reference(source_ordinal)? {
            return Err(ProductReviewError::new(
                "finding body publisher changed its canonical descriptor",
            ));
        }
        Ok(())
    }

    pub(super) fn adopt_published_body(&mut self, reference: ProductFindingBodyReference) {
        self.body = ProductFindingBodyOwner::Stored(reference);
    }

    pub(super) const fn permits_legacy_identity_coalescing(&self) -> bool {
        self.identity_version == LEGACY_IDENTITY_VERSION
    }

    pub(super) fn observe_again(&mut self, finding: &Self, cycle: u32) {
        self.severity = finding.severity;
        self.body.clone_from(&finding.body);
        self.state = ProductFindingState::Open;
        self.last_cycle = cycle;
    }

    pub(super) const fn propose_fix(&mut self, cycle: u32) {
        self.state = ProductFindingState::FixProposed { cycle };
    }

    pub(super) const fn confirm_resolution(&mut self, cycle: u32) {
        self.state = ProductFindingState::ResolutionConfirmed { cycle };
        self.last_cycle = cycle;
    }

    /// Coalesces a legacy duplicate while preserving the newest fail-closed state.
    pub(super) fn merge_restored(&mut self, finding: Self) {
        self.first_cycle = self.first_cycle.min(finding.first_cycle);
        let replace = finding.last_cycle > self.last_cycle
            || (finding.last_cycle == self.last_cycle
                && restored_state_rank(finding.state) > restored_state_rank(self.state));
        if replace {
            self.severity = finding.severity;
            self.body = finding.body;
            self.state = finding.state;
            self.last_cycle = finding.last_cycle;
        }
    }
}

/// One fresh independent reviewer result. It intentionally contains no trusted blocking Boolean.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductReviewSubmission {
    summary: String,
    findings: Vec<ProductFinding>,
    coverage_cursor: Option<String>,
}

impl ProductReviewSubmission {
    /// Creates a reviewer submission conserving all supplied findings.
    pub fn new(
        summary: String,
        findings: Vec<ProductFinding>,
    ) -> Result<Self, ProductReviewError> {
        Self::validate(summary, findings, None)
    }

    /// Creates a reviewer submission that explicitly covers one indexed ledger page.
    pub fn new_page(
        summary: String,
        findings: Vec<ProductFinding>,
        coverage_cursor: String,
    ) -> Result<Self, ProductReviewError> {
        if coverage_cursor.is_empty() || coverage_cursor.contains('\0') {
            return Err(ProductReviewError::new("review coverage cursor is invalid"));
        }
        Self::validate(summary, findings, Some(coverage_cursor))
    }

    fn validate(
        summary: String,
        mut findings: Vec<ProductFinding>,
        coverage_cursor: Option<String>,
    ) -> Result<Self, ProductReviewError> {
        if summary.trim().is_empty() {
            return Err(ProductReviewError::new("review submission summary is empty"));
        }
        findings.sort_by_key(ProductFinding::id);
        if findings.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(ProductReviewError::new("review submission duplicates a finding"));
        }
        Ok(Self { summary, findings, coverage_cursor })
    }

    /// Reviewer summary.
    #[must_use]
    pub fn summary(&self) -> &str { &self.summary }
    /// Typed normalized findings.
    #[must_use]
    pub fn findings(&self) -> &[ProductFinding] { &self.findings }
    /// Explicit subset coverage, or `None` for a legacy full-coverage assertion.
    #[must_use]
    pub fn coverage_cursor(&self) -> Option<&str> { self.coverage_cursor.as_deref() }

    pub(super) fn into_parts(self) -> (String, Vec<ProductFinding>, Option<String>) {
        (self.summary, self.findings, self.coverage_cursor)
    }
}

fn validate_inline(
    cycle: u32,
    title: &str,
    description: &str,
    location: &str,
    reproduction: &str,
    remediation: &str,
) -> Result<(), ProductReviewError> {
    if cycle == 0 || title.trim().is_empty() || description.trim().is_empty() {
        return Err(ProductReviewError::new("review finding is missing required content"));
    }
    if [title, description, location, reproduction, remediation]
        .iter()
        .any(|value| value.contains('\0'))
    {
        return Err(ProductReviewError::new("review finding contains a NUL character"));
    }
    Ok(())
}

fn validate_history(
    identity_version: u8,
    state: ProductFindingState,
    first_cycle: u32,
    last_cycle: u32,
) -> Result<(), ProductReviewError> {
    if first_cycle == 0
        || last_cycle < first_cycle
        || match state {
            ProductFindingState::Open => false,
            // Fix proposals use the independent fixer-cycle coordinate. Review history can
            // advance while a finding remains open, and resume can advance either coordinator
            // without keeping their numeric values equal.
            ProductFindingState::FixProposed { cycle } => cycle == 0,
            ProductFindingState::ResolutionConfirmed { cycle } => {
                cycle < first_cycle || cycle > last_cycle
            }
        }
    {
        return Err(ProductReviewError::new("restored finding cycle history is invalid"));
    }
    if !matches!(identity_version, LEGACY_IDENTITY_VERSION | CURRENT_IDENTITY_VERSION) {
        return Err(ProductReviewError::new(
            "restored finding identity version is unsupported",
        ));
    }
    Ok(())
}

const fn severity_at_least(value: FindingSeverity, threshold: FindingSeverity) -> bool {
    severity_rank(value) >= severity_rank(threshold)
}

const fn severity_rank(value: FindingSeverity) -> u8 {
    match value {
        FindingSeverity::Advisory => 1,
        FindingSeverity::Low => 2,
        FindingSeverity::Medium => 3,
        FindingSeverity::High => 4,
        FindingSeverity::Critical => 5,
    }
}

const fn restored_state_rank(value: ProductFindingState) -> u8 {
    match value {
        ProductFindingState::ResolutionConfirmed { .. } => 1,
        ProductFindingState::Open => 2,
        ProductFindingState::FixProposed { .. } => 3,
    }
}

const LEGACY_IDENTITY_VERSION: u8 = 2;
const CURRENT_IDENTITY_VERSION: u8 = 3;

fn finding_id(
    identity_version: u8,
    category: ProductFindingCategory,
    title: &str,
    provenance: &str,
) -> Sha256Digest {
    let mut bytes = match identity_version {
        LEGACY_IDENTITY_VERSION => b"peritus.product-finding.v2\0".to_vec(),
        _ => b"peritus.product-finding.v3\0".to_vec(),
    };
    bytes.extend_from_slice(category.as_str().as_bytes());
    bytes.push(0);
    bytes.extend(title.trim().chars().flat_map(char::to_lowercase).collect::<String>().as_bytes());
    if identity_version == CURRENT_IDENTITY_VERSION {
        bytes.push(0);
        bytes.extend_from_slice(provenance.as_bytes());
    }
    sha256(&bytes)
}

fn normalize_provenance(location: &str) -> String {
    let normalized = location.trim();
    normalized
        .split_once(';')
        .map_or(normalized, |(primary, _)| primary)
        .trim()
        .to_owned()
}

#[cfg(test)]
#[path = "finding_tests.rs"]
mod tests;
