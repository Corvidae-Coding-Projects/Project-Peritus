//! Append-preserving product finding ledger.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    ops::Bound,
};

use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use super::{
    PRODUCT_FINDING_SOURCE_ORDINAL_BASE, ProductFinding, ProductFindingBodyPublisher,
    ProductFindingBodyReference, ProductFindingState, ProductReviewError,
    ProductReviewSubmission, ProductReviewSummaryReference,
};

/// Concrete iterator over complete product finding history.
pub type ProductFindingValues<'a> =
    std::collections::btree_map::Values<'a, Sha256Digest, ProductFinding>;

/// Concrete iterator over unresolved product findings.
pub type OpenProductFindings<'a> =
    std::iter::Filter<ProductFindingValues<'a>, fn(&&ProductFinding) -> bool>;

const REVIEW_PAGE_FINDINGS: usize = 32;
const REVIEW_PREVIEW_BYTES: usize = 160;

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProductReviewSummaryOwner {
    Inline(String),
    Stored(ProductReviewSummaryReference),
}

impl Default for ProductReviewSummaryOwner {
    fn default() -> Self { Self::Inline(String::new()) }
}

/// Bounded identity projection for one finding in a physical reconciliation page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductFindingIndex {
    id: Sha256Digest,
    category: super::ProductFindingCategory,
    severity: peritus_spec::FindingSeverity,
    state: ProductFindingState,
    title: String,
    title_truncated: bool,
    provenance: String,
    provenance_truncated: bool,
    body: Option<ProductFindingBodyReference>,
}

impl ProductFindingIndex {
    /// Exact durable finding identity.
    #[must_use]
    pub const fn id(&self) -> Sha256Digest { self.id }

    /// Typed category.
    #[must_use]
    pub const fn category(&self) -> super::ProductFindingCategory { self.category }

    /// Typed severity.
    #[must_use]
    pub const fn severity(&self) -> peritus_spec::FindingSeverity { self.severity }

    /// Conserved lifecycle state at page construction.
    #[must_use]
    pub const fn state(&self) -> ProductFindingState { self.state }

    /// Bounded title preview.
    #[must_use]
    pub fn title(&self) -> &str { &self.title }

    /// Whether the exact title continues in the page's finding source.
    #[must_use]
    pub const fn title_truncated(&self) -> bool { self.title_truncated }

    /// Bounded normalized location-provenance preview.
    #[must_use]
    pub fn provenance(&self) -> &str { &self.provenance }

    /// Whether the exact provenance continues in the page's finding source.
    #[must_use]
    pub const fn provenance_truncated(&self) -> bool { self.provenance_truncated }

    /// Exact immutable body authority, absent only for a legacy inline ledger.
    #[must_use]
    pub const fn body_reference(&self) -> Option<&ProductFindingBodyReference> {
        self.body.as_ref()
    }
}

/// One deterministic physical page of the conserved unresolved-finding index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductReviewPage {
    review_cycle: u32,
    coverage_cursor: String,
    next_after: Option<Sha256Digest>,
    entries: Vec<ProductFindingIndex>,
}

impl ProductReviewPage {
    /// Review cycle to which this page is bound.
    #[must_use]
    pub const fn review_cycle(&self) -> u32 { self.review_cycle }

    /// Exact coverage assertion expected from a subset submission.
    #[must_use]
    pub fn coverage_cursor(&self) -> &str { &self.coverage_cursor }

    /// Bounded ordered identity catalog for this page.
    #[must_use]
    pub fn entries(&self) -> &[ProductFindingIndex] { &self.entries }

    /// Whether the next review begins again at the start of the unresolved index.
    #[must_use]
    pub const fn wraps_after_admission(&self) -> bool { self.next_after.is_none() }
}

/// Conserves every finding until a fixer proposal and a fresh review confirm resolution.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProductFindingLedger {
    cycle: u32,
    summary: ProductReviewSummaryOwner,
    coverage_after: Option<Sha256Digest>,
    pending_fixer: Vec<Sha256Digest>,
    findings: BTreeMap<Sha256Digest, ProductFinding>,
}

impl ProductFindingLedger {
    /// Creates an empty ledger before the first review.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cycle: 0,
            summary: ProductReviewSummaryOwner::Inline(String::new()),
            coverage_after: None,
            pending_fixer: Vec::new(),
            findings: BTreeMap::new(),
        }
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
        Self::restore_indexed(cycle, summary, None, findings)
    }

    /// Restores the durable ordered index and its next subset boundary.
    ///
    /// # Errors
    /// Rejects cycles newer than the ledger head or a cursor outside the retained index.
    pub fn restore_indexed(
        cycle: u32,
        summary: String,
        coverage_after: Option<Sha256Digest>,
        findings: Vec<ProductFinding>,
    ) -> Result<Self, ProductReviewError> {
        Self::restore_durable(cycle, summary, coverage_after, Vec::new(), findings)
    }

    /// Restores every durable index cursor, including a reviewer page awaiting its fixer.
    ///
    /// # Errors
    /// Rejects a pending fixer identity outside unresolved retained history.
    pub fn restore_durable(
        cycle: u32,
        summary: String,
        coverage_after: Option<Sha256Digest>,
        pending_fixer: Vec<Sha256Digest>,
        findings: Vec<ProductFinding>,
    ) -> Result<Self, ProductReviewError> {
        Self::restore_with_summary(
            cycle,
            ProductReviewSummaryOwner::Inline(summary),
            coverage_after,
            pending_fixer,
            findings,
        )
    }

    /// Restores every durable index cursor with an out-of-line exact review summary.
    ///
    /// # Errors
    /// Rejects a summary bound to another review cycle or invalid retained finding history.
    pub fn restore_durable_stored(
        cycle: u32,
        summary: ProductReviewSummaryReference,
        coverage_after: Option<Sha256Digest>,
        pending_fixer: Vec<Sha256Digest>,
        findings: Vec<ProductFinding>,
    ) -> Result<Self, ProductReviewError> {
        Self::restore_with_summary(
            cycle,
            ProductReviewSummaryOwner::Stored(summary),
            coverage_after,
            pending_fixer,
            findings,
        )
    }

    fn restore_with_summary(
        cycle: u32,
        summary: ProductReviewSummaryOwner,
        coverage_after: Option<Sha256Digest>,
        mut pending_fixer: Vec<Sha256Digest>,
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
                if !existing.permits_legacy_identity_coalescing()
                    || !finding.permits_legacy_identity_coalescing()
                {
                    return Err(ProductReviewError::new(
                        "restored indexed findings repeat an identity",
                    ));
                }
                existing.merge_restored(finding);
            } else {
                restored.insert(finding.id(), finding);
            }
        }
        let summary_valid = match &summary {
            ProductReviewSummaryOwner::Inline(summary) => {
                if cycle == 0 { summary.is_empty() } else { !summary.trim().is_empty() }
            }
            ProductReviewSummaryOwner::Stored(reference) => {
                cycle > 0 && reference.validate_cycle(cycle).is_ok()
            }
        };
        if !summary_valid {
            return Err(ProductReviewError::new("restored review ledger summary is empty"));
        }
        let has_inline = restored.values().any(|finding| finding.body_reference().is_none());
        let has_stored = restored.values().any(|finding| finding.body_reference().is_some());
        if has_inline && has_stored {
            return Err(ProductReviewError::new(
                "restored findings mix inline and stored body authority",
            ));
        }
        if has_inline && matches!(&summary, ProductReviewSummaryOwner::Stored(_)) {
            return Err(ProductReviewError::new(
                "restored review artifacts mix inline findings and stored summary authority",
            ));
        }
        if cycle == 0 && coverage_after.is_some()
            || coverage_after.is_some_and(|cursor| !restored.contains_key(&cursor))
        {
            return Err(ProductReviewError::new(
                "restored review coverage cursor is outside the finding index",
            ));
        }
        pending_fixer.sort_unstable();
        if pending_fixer.windows(2).any(|pair| pair[0] == pair[1])
            || pending_fixer.iter().any(|id| {
                restored.get(id).is_none_or(|finding| !unresolved_value(finding))
            })
        {
            return Err(ProductReviewError::new(
                "restored pending fixer page is outside unresolved finding history",
            ));
        }
        validate_body_ordinals(restored.values())?;
        if let ProductReviewSummaryOwner::Stored(summary) = &summary
            && restored.values().filter_map(ProductFinding::body_reference).any(|body| {
                body.source_ordinal() == summary.source_ordinal()
            })
        {
            return Err(ProductReviewError::new(
                "restored review artifacts repeat a source ordinal",
            ));
        }
        Ok(Self { cycle, summary, coverage_after, pending_fixer, findings: restored })
    }

    /// Publishes every legacy inline body one at a time and adopts only compact descriptors.
    ///
    /// Publication may leave an inert extra artifact reference if a later body fails. Callers must
    /// publish their authoritative record head only after this method succeeds.
    ///
    /// # Errors
    /// Rejects ordinal exhaustion, publisher failure, or a noncanonical published descriptor.
    pub fn externalize_bodies(
        &mut self,
        publisher: &dyn ProductFindingBodyPublisher,
    ) -> Result<bool, ProductReviewError> {
        let mut next = self.next_source_ordinal();
        let mut published = Vec::new();
        for (id, finding) in &self.findings {
            if finding.body_reference().is_some() {
                continue;
            }
            let ordinal = next.ok_or_else(|| {
                ProductReviewError::new("finding source ordinal representation is exhausted")
            })?;
            next = ordinal.checked_add(1);
            let reference = publisher.publish(finding, ordinal)?;
            finding.validate_published_body(&reference, ordinal)?;
            published.push((*id, reference));
        }
        let published_summary = match &self.summary {
            ProductReviewSummaryOwner::Inline(summary) if self.cycle > 0 => {
                let ordinal = next.ok_or_else(|| {
                    ProductReviewError::new("review source ordinal representation is exhausted")
                })?;
                let reference = publisher.publish_summary(summary, self.cycle, ordinal)?;
                reference.validate_cycle(self.cycle)?;
                if reference
                    != ProductReviewSummaryReference::measure(self.cycle, summary, ordinal)?
                {
                    return Err(ProductReviewError::new(
                        "review summary publisher changed its canonical descriptor",
                    ));
                }
                Some(reference)
            }
            ProductReviewSummaryOwner::Inline(_) | ProductReviewSummaryOwner::Stored(_) => None,
        };
        if published.iter().any(|(id, _)| !self.findings.contains_key(id)) {
            return Err(ProductReviewError::new(
                "published finding body is outside the retained index",
            ));
        }
        let changed = !published.is_empty() || published_summary.is_some();
        for (id, reference) in published {
            if let Some(finding) = self.findings.get_mut(&id) {
                finding.adopt_published_body(reference);
            }
        }
        if let Some(summary) = published_summary {
            self.summary = ProductReviewSummaryOwner::Stored(summary);
        }
        Ok(changed)
    }

    /// Whether this ledger still owns any legacy inline finding body.
    #[must_use]
    pub fn has_inline_bodies(&self) -> bool {
        self.has_inline_finding_bodies()
            || (self.cycle > 0
                && matches!(&self.summary, ProductReviewSummaryOwner::Inline(_)))
    }

    /// Whether any retained finding still owns its legacy inline body.
    #[must_use]
    pub fn has_inline_finding_bodies(&self) -> bool {
        self.findings.values().any(|finding| finding.body_reference().is_none())
    }

    /// Whether a completed review still owns its exact summary inline.
    #[must_use]
    pub const fn has_inline_summary(&self) -> bool {
        self.cycle > 0 && matches!(&self.summary, ProductReviewSummaryOwner::Inline(_))
    }

    /// Selects one bounded ordered subset for the next independent review.
    ///
    /// # Errors
    /// Rejects exhaustion of the durable review-cycle representation.
    pub fn review_page(&self) -> Result<ProductReviewPage, ProductReviewError> {
        let review_cycle = self.cycle.checked_add(1).ok_or_else(|| {
            ProductReviewError::new("review cycle representation is exhausted")
        })?;
        self.page_for_cycle(review_cycle)
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
        if submission.coverage_cursor().is_some() {
            return Err(ProductReviewError::new(
                "explicit review coverage requires its indexed page",
            ));
        }
        self.validate_inline_admission(&submission)?;
        self.validate_submission(cycle, &submission)?;
        self.admit_validated(cycle, submission, None, None);
        Ok(())
    }

    /// Admits one independently adoptable subset after its exact candidate was revalidated.
    ///
    /// A submission with the exact page cursor reconciles omission only inside that page. Every
    /// finding outside explicit coverage remains unchanged.
    ///
    /// # Errors
    /// Rejects a stale page or cycle, a mismatched coverage assertion, or findings constructed for
    /// another cycle.
    pub fn admit_review_page(
        &mut self,
        cycle: u32,
        page: &ProductReviewPage,
        submission: ProductReviewSubmission,
    ) -> Result<(), ProductReviewError> {
        self.validate_inline_admission(&submission)?;
        self.validate_submission(cycle, &submission)?;
        if &self.review_page()? != page || page.review_cycle != cycle {
            return Err(ProductReviewError::new("review coverage page is stale"));
        }
        match submission.coverage_cursor() {
            Some(cursor) if cursor == page.coverage_cursor => {}
            Some(_) => return Err(ProductReviewError::new("review coverage cursor does not match")),
            None => return Err(ProductReviewError::new("review coverage cursor is missing")),
        }
        self.admit_validated(cycle, submission, Some(page), None);
        Ok(())
    }

    /// Publishes every submitted body before atomically adopting one exact reconciliation page.
    ///
    /// Existing identities retain their stable source ordinal. New identities receive monotonic
    /// ordinals from the collision-free upper namespace. Publication failure leaves this ledger
    /// unchanged; already synchronized content remains an inert run-owned artifact.
    ///
    /// # Errors
    /// Rejects stale coverage, mixed body ownership, ordinal exhaustion, or publication failure.
    pub fn admit_review_page_published(
        &mut self,
        cycle: u32,
        page: &ProductReviewPage,
        submission: ProductReviewSubmission,
        publisher: &dyn ProductFindingBodyPublisher,
    ) -> Result<(), ProductReviewError> {
        if self.has_inline_finding_bodies() {
            return Err(ProductReviewError::new(
                "stored publication requires prior externalization of retained bodies",
            ));
        }
        self.validate_submission(cycle, &submission)?;
        if &self.review_page()? != page || page.review_cycle != cycle {
            return Err(ProductReviewError::new("review coverage page is stale"));
        }
        match submission.coverage_cursor() {
            Some(cursor) if cursor == page.coverage_cursor => {}
            Some(_) => {
                return Err(ProductReviewError::new(
                    "review coverage cursor does not match",
                ));
            }
            None => return Err(ProductReviewError::new("review coverage cursor is missing")),
        }
        let (summary, mut submitted, coverage_cursor) = submission.into_parts();
        let mut next = self.next_source_ordinal();
        for finding in &mut submitted {
            let ordinal = self
                .findings
                .get(&finding.id())
                .and_then(ProductFinding::body_reference)
                .map_or_else(
                    || {
                        let ordinal = next.ok_or_else(|| {
                            ProductReviewError::new(
                                "finding source ordinal representation is exhausted",
                            )
                        })?;
                        next = ordinal.checked_add(1);
                        Ok(ordinal)
                    },
                    |reference| Ok(reference.source_ordinal()),
                )?;
            finding.publish_body(publisher, ordinal)?;
        }
        let summary_ordinal = next.ok_or_else(|| {
            ProductReviewError::new("review source ordinal representation is exhausted")
        })?;
        let summary_reference = publisher.publish_summary(&summary, cycle, summary_ordinal)?;
        summary_reference.validate_cycle(cycle)?;
        if summary_reference
            != ProductReviewSummaryReference::measure(cycle, &summary, summary_ordinal)?
        {
            return Err(ProductReviewError::new(
                "review summary publisher changed its canonical descriptor",
            ));
        }
        let submission = match coverage_cursor {
            Some(cursor) => ProductReviewSubmission::new_page(summary, submitted, cursor),
            None => ProductReviewSubmission::new(summary, submitted),
        }?;
        self.admit_validated(cycle, submission, Some(page), Some(summary_reference));
        Ok(())
    }

    /// Records that the fixer received the current bounded finding page. This never closes one.
    pub fn record_fixer_proposal(&mut self, cycle: u32) {
        let ids = if self.pending_fixer.is_empty() {
            self.page_ids().0
        } else {
            std::mem::take(&mut self.pending_fixer)
        };
        for id in ids {
            if let Some(finding) = self.findings.get_mut(&id) {
                finding.propose_fix(cycle);
            }
        }
    }

    /// Latest admitted review cycle.
    #[must_use]
    pub const fn cycle(&self) -> u32 {
        self.cycle
    }

    /// Latest bounded reviewer-summary preview without replacing task-level completion text.
    #[must_use]
    pub fn review_summary(&self) -> &str {
        match &self.summary {
            ProductReviewSummaryOwner::Inline(summary) => preview_text(summary).0,
            ProductReviewSummaryOwner::Stored(reference) => reference.preview().text(),
        }
    }

    /// Whether the bounded reviewer-summary preview omits exact retained bytes.
    #[must_use]
    pub fn review_summary_truncated(&self) -> bool {
        match &self.summary {
            ProductReviewSummaryOwner::Inline(summary) => preview_text(summary).1,
            ProductReviewSummaryOwner::Stored(reference) => reference.preview().truncated(),
        }
    }

    /// Exact legacy inline summary before governed artifact publication.
    #[must_use]
    pub fn inline_review_summary(&self) -> Option<&str> {
        match &self.summary {
            ProductReviewSummaryOwner::Inline(summary) => Some(summary),
            ProductReviewSummaryOwner::Stored(_) => None,
        }
    }

    /// Compact immutable authority for the exact current review summary.
    #[must_use]
    pub const fn review_summary_reference(&self) -> Option<&ProductReviewSummaryReference> {
        match &self.summary {
            ProductReviewSummaryOwner::Inline(_) => None,
            ProductReviewSummaryOwner::Stored(reference) => Some(reference),
        }
    }

    /// Last finding identity covered before the next bounded projection, if any.
    #[must_use]
    pub const fn coverage_after(&self) -> Option<Sha256Digest> {
        self.coverage_after
    }

    /// Exact identities in the admitted reviewer page awaiting its fixer.
    #[must_use]
    pub fn pending_fixer(&self) -> &[Sha256Digest] {
        &self.pending_fixer
    }

    /// Exact finding body addressed by an index entry.
    #[must_use]
    pub fn finding(&self, id: Sha256Digest) -> Option<&ProductFinding> {
        self.findings.get(&id)
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

    fn validate_submission(
        &self,
        cycle: u32,
        submission: &ProductReviewSubmission,
    ) -> Result<(), ProductReviewError> {
        if self.cycle.checked_add(1) != Some(cycle)
            || submission.findings().iter().any(|finding| finding.first_cycle() != cycle)
        {
            return Err(ProductReviewError::new("review cycle is stale or nonmonotonic"));
        }
        Ok(())
    }

    fn validate_inline_admission(
        &self,
        submission: &ProductReviewSubmission,
    ) -> Result<(), ProductReviewError> {
        if !submission.findings().is_empty()
            && self.findings.values().any(|finding| finding.body_reference().is_some())
        {
            return Err(ProductReviewError::new(
                "stored finding history requires a body publisher",
            ));
        }
        Ok(())
    }

    fn admit_validated(
        &mut self,
        cycle: u32,
        submission: ProductReviewSubmission,
        page: Option<&ProductReviewPage>,
        published_summary: Option<ProductReviewSummaryReference>,
    ) {
        let (summary, submitted, _) = submission.into_parts();
        let mut pending_fixer = submitted.iter().map(ProductFinding::id).collect::<Vec<_>>();
        if let Some(page) = page {
            for entry in &page.entries {
                if !contains_identity(&submitted, entry.id)
                    && let Some(finding) = self.findings.get_mut(&entry.id)
                    && matches!(finding.state(), ProductFindingState::FixProposed { .. })
                {
                    finding.confirm_resolution(cycle);
                }
            }
            pending_fixer.extend(page.entries.iter().map(ProductFindingIndex::id));
            self.coverage_after = page.next_after;
        } else {
            for finding in self.findings.values_mut() {
                if !contains_identity(&submitted, finding.id())
                    && matches!(finding.state(), ProductFindingState::FixProposed { .. })
                {
                    finding.confirm_resolution(cycle);
                }
            }
            self.coverage_after = None;
        }
        for finding in submitted {
            if let Some(existing) = self.findings.get_mut(&finding.id()) {
                existing.observe_again(&finding, cycle);
            } else {
                self.findings.insert(finding.id(), finding);
            }
        }
        if page.is_none() {
            pending_fixer.extend(
                self.findings
                    .iter()
                    .filter_map(|(id, finding)| unresolved_value(finding).then_some(*id)),
            );
        }
        pending_fixer.sort_unstable();
        pending_fixer.dedup();
        pending_fixer.retain(|id| {
            self.findings.get(id).is_some_and(unresolved_value)
        });
        self.pending_fixer = pending_fixer;
        self.summary = published_summary.map_or_else(
            || ProductReviewSummaryOwner::Inline(summary),
            ProductReviewSummaryOwner::Stored,
        );
        self.cycle = cycle;
    }

    fn page_for_cycle(&self, review_cycle: u32) -> Result<ProductReviewPage, ProductReviewError> {
        let (ids, next_after, after) = self.page_ids();
        let coverage_cursor = self.coverage_cursor(review_cycle, after, next_after, &ids)?;
        let entries = ids
            .iter()
            .filter_map(|id| self.findings.get(id))
            .map(ProductFindingIndex::from_finding)
            .collect();
        Ok(ProductReviewPage { review_cycle, coverage_cursor, next_after, entries })
    }

    fn page_ids(
        &self,
    ) -> (Vec<Sha256Digest>, Option<Sha256Digest>, Option<Sha256Digest>) {
        let mut after = self.coverage_after;
        let mut ids = collect_page_ids(&self.findings, after);
        if ids.is_empty() && after.is_some() {
            after = None;
            ids = collect_page_ids(&self.findings, after);
        }
        let has_more = ids.len() > REVIEW_PAGE_FINDINGS;
        ids.truncate(REVIEW_PAGE_FINDINGS);
        let next_after = if has_more { ids.last().copied() } else { None };
        (ids, next_after, after)
    }

    fn next_source_ordinal(&self) -> Option<u64> {
        let finding = self
            .findings
            .values()
            .filter_map(ProductFinding::body_reference)
            .map(ProductFindingBodyReference::source_ordinal)
            .max();
        let summary = self
            .review_summary_reference()
            .map(ProductReviewSummaryReference::source_ordinal);
        finding
            .into_iter()
            .chain(summary)
            .max()
            .map_or(Some(PRODUCT_FINDING_SOURCE_ORDINAL_BASE), |ordinal| ordinal.checked_add(1))
    }

    fn coverage_cursor(
        &self,
        review_cycle: u32,
        after: Option<Sha256Digest>,
        next_after: Option<Sha256Digest>,
        ids: &[Sha256Digest],
    ) -> Result<String, ProductReviewError> {
        let mut digest = Sha256::new();
        digest.update(b"peritus.product-review-page.v1\0");
        digest.update(self.cycle.to_le_bytes());
        digest.update(review_cycle.to_le_bytes());
        update_optional_digest(&mut digest, after);
        update_optional_digest(&mut digest, next_after);
        update_length(&mut digest, ids.len())?;
        for id in ids {
            digest.update(id.as_bytes());
            let finding = self.findings.get(id).ok_or_else(|| {
                ProductReviewError::new("review page identity is outside the finding index")
            })?;
            update_finding(&mut digest, finding)?;
        }
        Ok(hex_digest(&digest.finalize()))
    }
}

impl ProductFindingIndex {
    fn from_finding(finding: &ProductFinding) -> Self {
        let (title, title_truncated, provenance, provenance_truncated, body) =
            if let Some(reference) = finding.body_reference() {
                (
                    reference.title_preview().text().to_owned(),
                    reference.title_preview().truncated(),
                    reference.provenance_preview().text().to_owned(),
                    reference.provenance_preview().truncated(),
                    Some(reference.clone()),
                )
            } else {
                let (title, title_truncated) = preview(finding.title());
                let (provenance, provenance_truncated) = preview(finding.provenance());
                (title, title_truncated, provenance, provenance_truncated, None)
            };
        Self {
            id: finding.id(),
            category: finding.category(),
            severity: finding.severity(),
            state: finding.state(),
            title,
            title_truncated,
            provenance,
            provenance_truncated,
            body,
        }
    }
}

fn collect_page_ids(
    findings: &BTreeMap<Sha256Digest, ProductFinding>,
    after: Option<Sha256Digest>,
) -> Vec<Sha256Digest> {
    let lower = after.map_or(Bound::Unbounded, Bound::Excluded);
    findings
        .range((lower, Bound::Unbounded))
        .filter_map(|(id, finding)| unresolved_value(finding).then_some(*id))
        .take(REVIEW_PAGE_FINDINGS.saturating_add(1))
        .collect()
}

fn contains_identity(findings: &[ProductFinding], id: Sha256Digest) -> bool {
    findings.binary_search_by_key(&id, ProductFinding::id).is_ok()
}

fn validate_body_ordinals<'a>(
    findings: impl Iterator<Item = &'a ProductFinding>,
) -> Result<(), ProductReviewError> {
    let mut ordinals = BTreeSet::new();
    for reference in findings.filter_map(ProductFinding::body_reference) {
        if !ordinals.insert(reference.source_ordinal()) {
            return Err(ProductReviewError::new(
                "restored findings repeat a body source ordinal",
            ));
        }
    }
    Ok(())
}

fn preview(value: &str) -> (String, bool) {
    let (preview, truncated) = preview_text(value);
    (preview.to_owned(), truncated)
}

fn preview_text(value: &str) -> (&str, bool) {
    if value.len() <= REVIEW_PREVIEW_BYTES {
        return (value, false);
    }
    let end = value.floor_char_boundary(REVIEW_PREVIEW_BYTES);
    (&value[..end], true)
}

fn update_finding(digest: &mut Sha256, finding: &ProductFinding) -> Result<(), ProductReviewError> {
    digest.update([finding.identity_version()]);
    update_text(digest, finding.category().as_str())?;
    digest.update([severity_tag(finding.severity())]);
    if let Some(body) = finding.body_reference() {
        digest.update([5]);
        digest.update(body.digest().as_bytes());
        digest.update(body.bytes().to_le_bytes());
        digest.update(body.source_ordinal().to_le_bytes());
        for field in [
            body.fields().title(),
            body.fields().description(),
            body.fields().location(),
            body.fields().provenance(),
            body.fields().reproduction(),
            body.fields().remediation(),
        ] {
            digest.update(field.offset().to_le_bytes());
            digest.update(field.bytes().to_le_bytes());
            digest.update(field.digest().as_bytes());
        }
        update_text(digest, body.title_preview().text())?;
        digest.update([u8::from(body.title_preview().truncated())]);
        digest.update(body.title_preview().binding().as_bytes());
        update_text(digest, body.provenance_preview().text())?;
        digest.update([u8::from(body.provenance_preview().truncated())]);
        digest.update(body.provenance_preview().binding().as_bytes());
    } else {
        // Preserve the exact legacy cursor derivation for inline v2-v4 ledgers.
        update_text(digest, finding.title())?;
        update_text(digest, finding.description())?;
        update_text(digest, finding.location())?;
        update_text(digest, finding.provenance())?;
        update_text(digest, finding.reproduction())?;
        update_text(digest, finding.remediation())?;
    }
    match finding.state() {
        ProductFindingState::Open => digest.update([1]),
        ProductFindingState::FixProposed { cycle } => {
            digest.update([2]);
            digest.update(cycle.to_le_bytes());
        }
        ProductFindingState::ResolutionConfirmed { cycle } => {
            digest.update([3]);
            digest.update(cycle.to_le_bytes());
        }
    }
    digest.update(finding.first_cycle().to_le_bytes());
    digest.update(finding.last_cycle().to_le_bytes());
    Ok(())
}

fn update_text(digest: &mut Sha256, value: &str) -> Result<(), ProductReviewError> {
    update_length(digest, value.len())?;
    digest.update(value.as_bytes());
    Ok(())
}

fn update_length(digest: &mut Sha256, value: usize) -> Result<(), ProductReviewError> {
    let value = u64::try_from(value)
        .map_err(|_| ProductReviewError::new("review page length exceeds its representation"))?;
    digest.update(value.to_le_bytes());
    Ok(())
}

fn update_optional_digest(digest: &mut Sha256, value: Option<Sha256Digest>) {
    if let Some(value) = value {
        digest.update([1]);
        digest.update(value.as_bytes());
    } else {
        digest.update([0]);
    }
}

fn severity_tag(value: peritus_spec::FindingSeverity) -> u8 {
    match value {
        peritus_spec::FindingSeverity::Advisory => 1,
        peritus_spec::FindingSeverity::Low => 2,
        peritus_spec::FindingSeverity::Medium => 3,
        peritus_spec::FindingSeverity::High => 4,
        peritus_spec::FindingSeverity::Critical => 5,
    }
}

fn hex_digest(value: &[u8]) -> String {
    let mut output = String::with_capacity(value.len().saturating_mul(2));
    for byte in value {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[allow(clippy::trivially_copy_pass_by_ref, reason = "Iterator::filter requires a reference")]
const fn unresolved(finding: &&ProductFinding) -> bool {
    unresolved_value(finding)
}

const fn unresolved_value(finding: &ProductFinding) -> bool {
    !matches!(finding.state(), ProductFindingState::ResolutionConfirmed { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product::{FindingSeverity, ProductFindingCategory};

    fn finding(cycle: u32, severity: FindingSeverity) -> ProductFinding {
        ProductFinding::new(
            ProductFindingCategory::BuildCoverage,
            severity,
            "Nested target was not built".to_owned(),
            "Root tests did not include the candidate".to_owned(),
            "game/Cargo.toml".to_owned(),
            "cargo check --manifest-path game/Cargo.toml".to_owned(),
            "Run exact target gates".to_owned(),
            cycle,
        )
        .expect("finding")
    }

    #[test]
    fn clean_reviews_restore_without_permitting_findings_outside_the_ledger_cycle() {
        let clean = ProductFindingLedger::restore(1, "No findings".to_owned(), Vec::new())
            .expect("a completed review may contain no findings");
        assert_eq!(clean.cycle(), 1);
        assert!(!clean.has_blockers());
        assert!(ProductFindingLedger::restore(1, String::new(), Vec::new()).is_err());
        for cycle in [0, 1] {
            assert!(
                ProductFindingLedger::restore(
                    cycle,
                    "Invalid future finding".to_owned(),
                    vec![finding(2, FindingSeverity::High)],
                )
                .is_err()
            );
        }
    }

    #[test]
    fn omission_cannot_close_finding_until_fixer_and_fresh_review() {
        let mut ledger = ProductFindingLedger::new();
        ledger
            .admit_review(
                1,
                ProductReviewSubmission::new(
                    "initial".to_owned(),
                    vec![finding(1, FindingSeverity::Low)],
                )
                .expect("submission"),
            )
            .expect("review");
        ledger
            .admit_review(
                2,
                ProductReviewSubmission::new("omitted".to_owned(), Vec::new()).expect("submission"),
            )
            .expect("review");
        assert!(ledger.has_blockers());

        ledger.record_fixer_proposal(2);
        ledger
            .admit_review(
                3,
                ProductReviewSubmission::new("confirmed".to_owned(), Vec::new())
                    .expect("submission"),
            )
            .expect("review");
        assert!(!ledger.has_blockers());
    }

    #[test]
    fn conserved_advisory_does_not_block_acceptance() {
        let mut ledger = ProductFindingLedger::new();
        ledger
            .admit_review(
                1,
                ProductReviewSubmission::new(
                    "advisory only".to_owned(),
                    vec![finding(1, FindingSeverity::Advisory)],
                )
                .expect("submission"),
            )
            .expect("review");

        assert_eq!(ledger.open_findings().count(), 1);
        assert!(!ledger.has_blockers());
    }

    #[test]
    fn repeated_finding_updates_location_without_forking_identity() {
        let mut ledger = ProductFindingLedger::new();
        let first = ProductFinding::new(
            ProductFindingCategory::RequestedBehavior,
            FindingSeverity::Medium,
            "Wrong category".to_owned(),
            "The value uses an unrelated category".to_owned(),
            "out/report.csv:8".to_owned(),
            "Inspect the source category".to_owned(),
            "Use the declared category".to_owned(),
            1,
        )
        .expect("first finding");
        ledger
            .admit_review(
                1,
                ProductReviewSubmission::new("first review".to_owned(), vec![first])
                    .expect("submission"),
            )
            .expect("review");
        ledger.record_fixer_proposal(1);
        let repeated = ProductFinding::new(
            ProductFindingCategory::RequestedBehavior,
            FindingSeverity::Medium,
            "Wrong category".to_owned(),
            "The value still uses an unrelated category".to_owned(),
            "out/report.csv:8; out/report.json category".to_owned(),
            "Inspect both outputs".to_owned(),
            "Use the declared category in both outputs".to_owned(),
            2,
        )
        .expect("repeated finding");
        ledger
            .admit_review(
                2,
                ProductReviewSubmission::new("second review".to_owned(), vec![repeated])
                    .expect("submission"),
            )
            .expect("review");

        let findings = ledger.findings().collect::<Vec<_>>();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].location(), "out/report.csv:8; out/report.json category");
        assert!(ledger.has_blockers());
    }

    #[test]
    fn restore_coalesces_pre_v2_location_duplicates_fail_closed() {
        let resolved = ProductFinding::restore(
            ProductFindingCategory::RequestedBehavior,
            FindingSeverity::Medium,
            "Wrong category".to_owned(),
            "First location form".to_owned(),
            "out/report.csv:8 (row)".to_owned(),
            "Inspect the CSV".to_owned(),
            "Use the declared category".to_owned(),
            ProductFindingState::ResolutionConfirmed { cycle: 2 },
            1,
            2,
        )
        .expect("resolved finding");
        let open = ProductFinding::restore(
            ProductFindingCategory::RequestedBehavior,
            FindingSeverity::Medium,
            "Wrong category".to_owned(),
            "Updated location form".to_owned(),
            "out/report.csv:8; out/report.json category".to_owned(),
            "Inspect both outputs".to_owned(),
            "Use the declared category in both outputs".to_owned(),
            ProductFindingState::FixProposed { cycle: 3 },
            2,
            3,
        )
        .expect("open finding");

        let ledger =
            ProductFindingLedger::restore(3, "restored review".to_owned(), vec![resolved, open])
                .expect("coalesced ledger");
        let findings = ledger.findings().collect::<Vec<_>>();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].first_cycle(), 1);
        assert_eq!(findings[0].last_cycle(), 3);
        assert_eq!(findings[0].location(), "out/report.csv:8; out/report.json category");
        assert!(ledger.has_blockers());
    }
}
