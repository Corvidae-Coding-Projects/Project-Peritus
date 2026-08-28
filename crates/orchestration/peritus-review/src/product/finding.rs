//! Typed product findings with policy-derived blocker status.

use peritus_codec::sha256;
use peritus_spec::FindingSeverity;
use peritus_types::Sha256Digest;

use super::ProductReviewError;

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

    /// Categories whose presence blocks acceptance regardless of reviewer severity wording.
    #[must_use]
    pub const fn always_blocks(self) -> bool {
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

/// One model-originated finding normalized into policy data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductFinding {
    id: Sha256Digest,
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
}

impl ProductFinding {
    /// Creates a bounded finding and derives its stable normalized identity.
    ///
    /// # Errors
    /// Rejects missing primary text or oversized model fields.
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
        if cycle == 0 || title.trim().is_empty() || description.trim().is_empty() {
            return Err(ProductReviewError::new("review finding is missing required content"));
        }
        if [&title, &description, &location, &reproduction, &remediation]
            .iter()
            .any(|value| value.len() > 64 * 1024 || value.contains('\0'))
        {
            return Err(ProductReviewError::new("review finding exceeds its text bound"));
        }
        let id = finding_id(category, &title, &location);
        Ok(Self {
            id,
            category,
            severity,
            title,
            description,
            location,
            reproduction,
            remediation,
            state: ProductFindingState::Open,
            first_cycle: cycle,
            last_cycle: cycle,
        })
    }

    /// Restores a previously validated product finding and its conserved lifecycle.
    ///
    /// # Errors
    /// Rejects inconsistent review-cycle history or invalid finding content.
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
        if last_cycle < first_cycle
            || match state {
                ProductFindingState::Open => false,
                ProductFindingState::FixProposed { cycle } => cycle == 0,
                ProductFindingState::ResolutionConfirmed { cycle } => {
                    cycle < first_cycle || cycle > last_cycle
                }
            }
        {
            return Err(ProductReviewError::new("restored finding cycle history is invalid"));
        }
        let mut finding = Self::new(
            category,
            severity,
            title,
            description,
            location,
            reproduction,
            remediation,
            first_cycle,
        )?;
        finding.state = state;
        finding.last_cycle = last_cycle;
        Ok(finding)
    }

    /// Stable normalized identity.
    #[must_use]
    pub const fn id(&self) -> Sha256Digest {
        self.id
    }

    /// Typed category.
    #[must_use]
    pub const fn category(&self) -> ProductFindingCategory {
        self.category
    }

    /// Reviewer severity.
    #[must_use]
    pub const fn severity(&self) -> FindingSeverity {
        self.severity
    }

    /// Policy-derived blocking status.
    #[must_use]
    pub const fn blocking(&self) -> bool {
        self.category.always_blocks() || severity_at_least(self.severity, FindingSeverity::High)
    }

    /// Short finding name.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Specific finding detail.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Optional source location.
    #[must_use]
    pub fn location(&self) -> &str {
        &self.location
    }

    /// Reproduction or evidence instructions.
    #[must_use]
    pub fn reproduction(&self) -> &str {
        &self.reproduction
    }

    /// Expected fix.
    #[must_use]
    pub fn remediation(&self) -> &str {
        &self.remediation
    }

    /// Conserved lifecycle state.
    #[must_use]
    pub const fn state(&self) -> ProductFindingState {
        self.state
    }

    /// First review cycle that admitted the finding.
    #[must_use]
    pub const fn first_cycle(&self) -> u32 {
        self.first_cycle
    }

    /// Most recent review cycle that observed or resolved it.
    #[must_use]
    pub const fn last_cycle(&self) -> u32 {
        self.last_cycle
    }

    pub(super) fn observe_again(&mut self, finding: &Self, cycle: u32) {
        self.severity = finding.severity;
        self.description.clone_from(&finding.description);
        self.reproduction.clone_from(&finding.reproduction);
        self.remediation.clone_from(&finding.remediation);
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
}

/// One fresh independent reviewer result. It intentionally contains no trusted blocking Boolean.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductReviewSubmission {
    summary: String,
    findings: Vec<ProductFinding>,
}

impl ProductReviewSubmission {
    /// Creates a bounded reviewer submission.
    ///
    /// # Errors
    /// Rejects an empty summary, too many findings, or duplicate normalized identities.
    pub fn new(
        summary: String,
        mut findings: Vec<ProductFinding>,
    ) -> Result<Self, ProductReviewError> {
        if summary.trim().is_empty() || summary.len() > 128 * 1024 || findings.len() > 128 {
            return Err(ProductReviewError::new("review submission exceeds its bounds"));
        }
        findings.sort_by_key(ProductFinding::id);
        if findings.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(ProductReviewError::new("review submission duplicates a finding"));
        }
        Ok(Self { summary, findings })
    }

    /// Reviewer summary.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Typed normalized findings.
    #[must_use]
    pub fn findings(&self) -> &[ProductFinding] {
        &self.findings
    }

    pub(super) fn into_findings(self) -> Vec<ProductFinding> {
        self.findings
    }
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

fn finding_id(category: ProductFindingCategory, title: &str, location: &str) -> Sha256Digest {
    let mut bytes = b"peritus.product-finding.v1\0".to_vec();
    bytes.extend_from_slice(category.as_str().as_bytes());
    bytes.push(0);
    bytes.extend(title.trim().chars().flat_map(char::to_lowercase).collect::<String>().as_bytes());
    bytes.push(0);
    bytes.extend(location.trim().as_bytes());
    sha256(&bytes)
}
