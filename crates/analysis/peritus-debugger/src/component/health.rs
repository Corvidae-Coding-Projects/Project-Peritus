//! Bounded integer diagnostic health summary without verdict authority.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AnalysisFinding, ComponentCorrelation, DebuggerError, DebuggerLimit, DebuggerLimits,
    DebuggerOperation, FailureCategory, OutcomeClass, PatternCluster, RootCauseCandidate,
    TraceSelectionManifest,
};
use peritus_harness::domain::HarnessRevisionIdentity;

/// Explicit marker that health values are diagnostics only.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticStatus {
    /// Carries no pass/fail, threshold, evaluation, or promotion meaning.
    DiagnosticOnly,
}

/// One exact per-category sample count.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HealthCategoryCount {
    category: FailureCategory,
    count: u64,
}

impl HealthCategoryCount {
    /// Returns the closed category.
    #[must_use]
    pub const fn category(self) -> FailureCategory {
        self.category
    }
    /// Returns exact finding count.
    #[must_use]
    pub const fn count(self) -> u64 {
        self.count
    }
}

/// Complete diagnostic-only harness health summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessHealthSummary {
    status: DiagnosticStatus,
    revisions: Vec<HarnessRevisionIdentity>,
    subject_count: u64,
    subject_coverage_millionths: u32,
    successful_attempts: u64,
    failed_attempts: u64,
    indeterminate_attempts: u64,
    infrastructure_share_millionths: u32,
    repeated_pattern_share_millionths: u32,
    citation_coverage_millionths: u32,
    ambiguity_share_millionths: u32,
    exact_component_correlations: u64,
    class_only_correlations: u64,
    category_counts: Vec<HealthCategoryCount>,
}

impl HarnessHealthSummary {
    /// Returns the explicit non-authoritative marker.
    #[must_use]
    pub const fn status(&self) -> DiagnosticStatus {
        self.status
    }
    /// Borrows full E1 revisions in canonical order.
    #[must_use]
    pub fn revisions(&self) -> &[HarnessRevisionIdentity] {
        &self.revisions
    }
    /// Returns total frozen subjects.
    #[must_use]
    pub const fn subject_count(&self) -> u64 {
        self.subject_count
    }
    /// Returns subjects with selected evidence in integer millionths.
    #[must_use]
    pub const fn subject_coverage_millionths(&self) -> u32 {
        self.subject_coverage_millionths
    }
    /// Returns successful attempt count.
    #[must_use]
    pub const fn successful_attempts(&self) -> u64 {
        self.successful_attempts
    }
    /// Returns failed attempt count.
    #[must_use]
    pub const fn failed_attempts(&self) -> u64 {
        self.failed_attempts
    }
    /// Returns indeterminate attempt count.
    #[must_use]
    pub const fn indeterminate_attempts(&self) -> u64 {
        self.indeterminate_attempts
    }
    /// Returns infrastructure finding share in integer millionths.
    #[must_use]
    pub const fn infrastructure_share_millionths(&self) -> u32 {
        self.infrastructure_share_millionths
    }
    /// Returns membership in recurring patterns in integer millionths.
    #[must_use]
    pub const fn repeated_pattern_share_millionths(&self) -> u32 {
        self.repeated_pattern_share_millionths
    }
    /// Returns citation-bearing finding share in integer millionths.
    #[must_use]
    pub const fn citation_coverage_millionths(&self) -> u32 {
        self.citation_coverage_millionths
    }
    /// Returns ambiguous-cause share in integer millionths.
    #[must_use]
    pub const fn ambiguity_share_millionths(&self) -> u32 {
        self.ambiguity_share_millionths
    }
    /// Returns exact declaration correlation count.
    #[must_use]
    pub const fn exact_component_correlations(&self) -> u64 {
        self.exact_component_correlations
    }
    /// Returns class-only correlation count.
    #[must_use]
    pub const fn class_only_correlations(&self) -> u64 {
        self.class_only_correlations
    }
    /// Borrows nonzero category counts in tag order.
    #[must_use]
    pub fn category_counts(&self) -> &[HealthCategoryCount] {
        &self.category_counts
    }
}

/// Computes a bounded diagnostic-only summary from fully provenance-retaining inputs.
///
/// # Errors
///
/// Rejects unrepresentable counts or diagnostic bound excess.
pub fn summarize_health(
    manifest: &TraceSelectionManifest,
    findings: &[AnalysisFinding],
    causes: &[RootCauseCandidate],
    patterns: &[PatternCluster],
    correlations: &[ComponentCorrelation],
    limits: DebuggerLimits,
) -> Result<HarnessHealthSummary, DebuggerError> {
    let subject_count = as_u64(manifest.subjects().len())?;
    let covered: BTreeSet<_> =
        manifest.entries().iter().map(|entry| entry.subject().id()).collect();
    let mut outcomes: BTreeMap<_, Vec<OutcomeClass>> = BTreeMap::new();
    for finding in findings {
        outcomes.entry(finding.subject_id()).or_default().push(finding.outcome());
    }
    let mut successful_attempts = 0_usize;
    let mut failed_attempts = 0_usize;
    let mut indeterminate_attempts = 0_usize;
    for subject in manifest.subjects() {
        match outcomes.get(&subject.id()) {
            Some(values) if values.iter().any(|outcome| !outcome.is_task_success()) => {
                failed_attempts = failed_attempts.saturating_add(1);
            }
            Some(values) if values.iter().any(|outcome| outcome.is_task_success()) => {
                successful_attempts = successful_attempts.saturating_add(1);
            }
            _ => indeterminate_attempts = indeterminate_attempts.saturating_add(1),
        }
    }
    let infrastructure = findings
        .iter()
        .filter(|finding| matches!(finding.outcome(), OutcomeClass::Infrastructure(_)))
        .count();
    let repeated_members: usize = patterns
        .iter()
        .filter(|pattern| pattern.members().len() > 1)
        .map(|pattern| pattern.members().len())
        .sum();
    let all_members: usize = patterns.iter().map(|pattern| pattern.members().len()).sum();
    let cited = findings.iter().filter(|finding| !finding.citations().is_empty()).count();
    let ambiguous = causes.iter().filter(|cause| !cause.ambiguities().is_empty()).count();
    let exact_component_correlations =
        correlations.iter().filter(|item| !item.class_only()).count();
    let class_only_correlations = correlations.len().saturating_sub(exact_component_correlations);
    let mut counts = BTreeMap::<FailureCategory, u64>::new();
    for category in findings.iter().filter_map(AnalysisFinding::category) {
        *counts.entry(category).or_default() =
            counts.get(&category).copied().unwrap_or(0).saturating_add(1);
    }
    let category_counts: Vec<_> = counts
        .into_iter()
        .map(|(category, count)| HealthCategoryCount { category, count })
        .collect();
    limits.check(
        DebuggerLimit::Diagnostics,
        category_counts.len(),
        DebuggerOperation::MapComponents,
    )?;
    let mut revisions: Vec<_> =
        manifest.subjects().iter().map(crate::AnalysisSubject::harness_revision).collect();
    revisions.sort();
    revisions.dedup();
    Ok(HarnessHealthSummary {
        status: DiagnosticStatus::DiagnosticOnly,
        revisions,
        subject_count,
        subject_coverage_millionths: ratio(covered.len(), manifest.subjects().len()),
        successful_attempts: as_u64(successful_attempts)?,
        failed_attempts: as_u64(failed_attempts)?,
        indeterminate_attempts: as_u64(indeterminate_attempts)?,
        infrastructure_share_millionths: ratio(infrastructure, findings.len()),
        repeated_pattern_share_millionths: ratio(repeated_members, all_members),
        citation_coverage_millionths: ratio(cited, findings.len()),
        ambiguity_share_millionths: ratio(ambiguous, causes.len()),
        exact_component_correlations: as_u64(exact_component_correlations)?,
        class_only_correlations: as_u64(class_only_correlations)?,
        category_counts,
    })
}

fn ratio(numerator: usize, denominator: usize) -> u32 {
    if denominator == 0 {
        return 0;
    }
    let scaled = (numerator as u128).saturating_mul(1_000_000) / denominator as u128;
    u32::try_from(scaled.min(1_000_000)).unwrap_or(1_000_000)
}

fn as_u64(value: usize) -> Result<u64, DebuggerError> {
    u64::try_from(value).map_err(|_| {
        DebuggerError::new(
            crate::DebuggerErrorKind::Budget,
            DebuggerOperation::MapComponents,
            crate::DebuggerRecovery::CorrectInput,
            "health count cannot be represented",
        )
    })
}
