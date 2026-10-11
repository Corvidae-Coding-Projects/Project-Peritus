//! Append-preserving product finding ledger.

use std::collections::{BTreeMap, BTreeSet};

use peritus_types::Sha256Digest;

use super::{ProductFinding, ProductFindingState, ProductReviewError, ProductReviewSubmission};

/// Concrete iterator over complete product finding history.
pub type ProductFindingValues<'a> =
    std::collections::btree_map::Values<'a, Sha256Digest, ProductFinding>;

/// Concrete iterator over unresolved product findings.
pub type OpenProductFindings<'a> =
    std::iter::Filter<ProductFindingValues<'a>, fn(&&ProductFinding) -> bool>;

/// Conserves every finding until a fixer proposal and a fresh review confirm resolution.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProductFindingLedger {
    cycle: u32,
    summary: String,
    findings: BTreeMap<Sha256Digest, ProductFinding>,
}

impl ProductFindingLedger {
    /// Creates an empty ledger before the first review.
    #[must_use]
    pub const fn new() -> Self {
        Self { cycle: 0, summary: String::new(), findings: BTreeMap::new() }
    }

    /// Restores a durable product finding ledger without dropping open history.
    ///
    /// # Errors
    /// Rejects cycles newer than the ledger head. Pre-v2 location-derived duplicate identities are
    /// coalesced by stable title while preserving their newest fail-closed state.
    pub fn restore(
        cycle: u32,
        summary: String,
        findings: Vec<ProductFinding>,
    ) -> Result<Self, ProductReviewError> {
        if (cycle == 0 && !findings.is_empty())
            || findings.iter().any(|finding| finding.last_cycle() > cycle)
        {
            return Err(ProductReviewError::new("restored review ledger cycle is invalid"));
        }
        let mut restored = BTreeMap::<Sha256Digest, ProductFinding>::new();
        for finding in findings {
            if let Some(existing) = restored.get_mut(&finding.id()) {
                existing.merge_restored(finding);
            } else {
                restored.insert(finding.id(), finding);
            }
        }
        if cycle > 0 && summary.trim().is_empty() {
            return Err(ProductReviewError::new("restored review ledger summary is empty"));
        }
        Ok(Self { cycle, summary, findings: restored })
    }

    /// Admits one fresh reviewer submission and reconciles it against conserved history.
    ///
    /// A missing old finding closes only when a fixer previously proposed a fix. A reviewer cannot
    /// make an untouched finding disappear by omitting it from a later response.
    ///
    /// # Errors
    /// Rejects nonmonotonic cycles or findings constructed for another cycle.
    pub fn admit_review(
        &mut self,
        cycle: u32,
        submission: ProductReviewSubmission,
    ) -> Result<(), ProductReviewError> {
        if cycle != self.cycle.saturating_add(1)
            || submission.findings().iter().any(|finding| finding.first_cycle() != cycle)
        {
            return Err(ProductReviewError::new("review cycle is stale or nonmonotonic"));
        }
        self.summary.clone_from(&submission.summary().to_owned());
        let submitted = submission.into_findings();
        let observed = submitted.iter().map(ProductFinding::id).collect::<BTreeSet<_>>();
        for finding in self.findings.values_mut() {
            if !observed.contains(&finding.id())
                && matches!(finding.state(), ProductFindingState::FixProposed { .. })
            {
                finding.confirm_resolution(cycle);
            }
        }
        for finding in submitted {
            if let Some(existing) = self.findings.get_mut(&finding.id()) {
                existing.observe_again(&finding, cycle);
            } else {
                self.findings.insert(finding.id(), finding);
            }
        }
        self.cycle = cycle;
        Ok(())
    }

    /// Records that the fixer received every currently open finding. This never closes one.
    pub fn record_fixer_proposal(&mut self, cycle: u32) {
        for finding in self.findings.values_mut().filter(|finding| {
            !matches!(finding.state(), ProductFindingState::ResolutionConfirmed { .. })
        }) {
            finding.propose_fix(cycle);
        }
    }

    /// Latest admitted review cycle.
    #[must_use]
    pub const fn cycle(&self) -> u32 {
        self.cycle
    }

    /// Latest reviewer summary without replacing task-level completion text.
    #[must_use]
    pub fn review_summary(&self) -> &str {
        &self.summary
    }

    /// Complete finding history.
    pub fn findings(&self) -> ProductFindingValues<'_> {
        self.findings.values()
    }

    /// Current unresolved findings, including fixer proposals awaiting review.
    pub fn open_findings(&self) -> OpenProductFindings<'_> {
        self.findings.values().filter(unresolved)
    }

    /// Whether policy derives at least one unresolved blocker.
    #[must_use]
    pub fn has_blockers(&self) -> bool {
        self.open_findings().any(ProductFinding::blocking)
    }
}

#[allow(clippy::trivially_copy_pass_by_ref, reason = "Iterator::filter requires a reference")]
const fn unresolved(finding: &&ProductFinding) -> bool {
    !matches!(finding.state(), ProductFindingState::ResolutionConfirmed { .. })
}
