//! Deterministic repeated-finding, severity-trend, disagreement, and exhaustion reporting.

use peritus_spec::FindingSeverity;
use peritus_types::Sha256Digest;

use crate::{DispositionKind, Finding, ReviewBinding, ReviewCycle, ReviewCyclePhase};

/// Closed non-success review-loop triggers.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OscillationKind {
    /// The canonical finding set repeated across candidate bindings.
    RepeatedFindingSet,
    /// Maximum severity failed to improve across candidate bindings.
    SeverityStagnation,
    /// Maximum severity worsened across candidate bindings.
    SeverityRegression,
    /// A current fixer/reviewer disagreement remains open.
    Disagreement,
    /// The immutable contract cycle cap was consumed before completion.
    ReviewCyclesExhausted,
}

/// Complete deterministic oscillation summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OscillationReport {
    kinds: Vec<OscillationKind>,
    compared_bindings: u16,
    cycles_used: u16,
}

impl OscillationReport {
    /// Evaluates history in cycle order without clocks or arrival-order ambiguity.
    #[must_use]
    pub fn evaluate(
        binding: &ReviewBinding,
        cycles: &[ReviewCycle],
        findings: &[Finding],
        completion_ready: bool,
    ) -> Self {
        let mut groups: Vec<(Sha256Digest, Vec<Sha256Digest>, Option<FindingSeverity>)> =
            Vec::new();
        for cycle in cycles.iter().filter(|cycle| {
            matches!(cycle.phase(), ReviewCyclePhase::Submitted | ReviewCyclePhase::Invalidated)
                && cycle.submission().is_some()
        }) {
            let digest = cycle.assignment().binding_digest();
            let position = groups.iter().position(|group| group.0 == digest);
            let index = position.unwrap_or_else(|| {
                groups.push((digest, Vec::new(), None));
                groups.len() - 1
            });
            if let Some(submission) = cycle.submission() {
                for finding in submission.findings() {
                    groups[index].1.push(finding.normalized_digest());
                    groups[index].2 = Some(groups[index].2.map_or_else(
                        || finding.severity(),
                        |severity| severity.max(finding.severity()),
                    ));
                }
            }
        }
        for (_, fingerprints, _) in &mut groups {
            fingerprints.sort_unstable();
            fingerprints.dedup();
        }

        let mut kinds = Vec::new();
        if let [.., previous, current] = groups.as_slice() {
            if !current.1.is_empty() && previous.1 == current.1 {
                kinds.push(OscillationKind::RepeatedFindingSet);
            }
            if let (Some(previous), Some(current)) = (previous.2, current.2) {
                if current > previous {
                    kinds.push(OscillationKind::SeverityRegression);
                } else if current == previous {
                    kinds.push(OscillationKind::SeverityStagnation);
                }
            }
        }
        if findings.iter().any(|finding| {
            finding.revision() == binding.revision()
                && finding.current_disposition() == DispositionKind::Disputed
        }) {
            kinds.push(OscillationKind::Disagreement);
        }
        let cycles_used = u16::try_from(cycles.len()).unwrap_or(u16::MAX);
        if !completion_ready && cycles_used >= binding.maximum_cycles() {
            kinds.push(OscillationKind::ReviewCyclesExhausted);
        }
        kinds.sort_unstable();
        kinds.dedup();
        Self::from_wire(kinds, u16::try_from(groups.len()).unwrap_or(u16::MAX), cycles_used)
    }

    pub(super) const fn from_wire(
        kinds: Vec<OscillationKind>,
        compared_bindings: u16,
        cycles_used: u16,
    ) -> Self {
        Self { kinds, compared_bindings, cycles_used }
    }

    /// Returns every triggered condition in canonical kind order.
    #[must_use]
    pub const fn kinds(&self) -> &[OscillationKind] {
        self.kinds.as_slice()
    }
    /// Returns the number of distinct candidate bindings compared.
    #[must_use]
    pub const fn compared_bindings(&self) -> u16 {
        self.compared_bindings
    }
    /// Returns the number of assigned cycles consumed.
    #[must_use]
    pub const fn cycles_used(&self) -> u16 {
        self.cycles_used
    }
    /// Returns whether autonomous review must stop.
    #[must_use]
    pub const fn triggered(&self) -> bool {
        !self.kinds.is_empty()
    }
}
