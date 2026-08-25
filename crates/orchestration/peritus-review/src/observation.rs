//! Canonical current B2 quality observations derived without authority widening.

use peritus_quality_policy::{
    FindingDisposition, FindingObservation, ReviewObservation, WaiverObservation,
};

use crate::error::{ReviewError, ReviewErrorKind};
use crate::{DispositionKind, Finding, ReviewCycle, ReviewCyclePhase, ReviewRunState};

/// Current fully checked B2 projections for downstream acceptance evaluation.
#[derive(Debug, Eq, PartialEq)]
pub struct QualityProjection {
    reviews: Vec<ReviewObservation>,
    findings: Vec<FindingObservation>,
    waivers: Vec<WaiverObservation>,
}

impl QualityProjection {
    /// Projects only exact-current complete cycles, canonical findings, and consumed external
    /// waivers through B2 constructors.
    ///
    /// # Errors
    /// Returns a typed D2 error if retained state cannot form canonical B2 observations.
    pub fn from_state(state: &ReviewRunState) -> Result<Self, ReviewError> {
        let findings = state
            .findings()
            .iter()
            .filter(|finding| state.finding_is_current(finding))
            .filter_map(project_finding)
            .collect::<Vec<_>>();
        let mut reviews = Vec::new();
        for cycle in state.cycles().iter().filter(|cycle| {
            state.cycle_is_current(cycle)
                && cycle.phase() == ReviewCyclePhase::Submitted
                && cycle.submission().is_some()
        }) {
            reviews.push(project_review(state, cycle)?);
        }
        let waivers = state
            .waivers()
            .iter()
            .filter(|waiver| {
                waiver.revision() == state.binding().revision()
                    && state.finding(waiver.finding_id()).is_some_and(|finding| {
                        state.finding_is_current(finding)
                            && finding.current_disposition() == DispositionKind::Waived
                    })
            })
            .map(crate::ObservedWaiver::observation)
            .collect();
        Ok(Self { reviews, findings, waivers })
    }

    /// Returns current complete review observations in cycle ordinal order.
    #[must_use]
    pub const fn reviews(&self) -> &[ReviewObservation] {
        self.reviews.as_slice()
    }
    /// Returns current canonical finding observations in finding identity order.
    #[must_use]
    pub const fn findings(&self) -> &[FindingObservation] {
        self.findings.as_slice()
    }
    /// Returns current externally authorized waiver observations in event order.
    #[must_use]
    pub const fn waivers(&self) -> &[WaiverObservation] {
        self.waivers.as_slice()
    }
}

fn project_review(
    state: &ReviewRunState,
    cycle: &ReviewCycle,
) -> Result<ReviewObservation, ReviewError> {
    let submission = cycle.submission().ok_or_else(|| {
        crate::error::reject(
            ReviewErrorKind::InvalidInput,
            "submitted cycle lacks its structured submission",
        )
    })?;
    let findings = state
        .findings()
        .iter()
        .filter(|finding| {
            state.finding_is_current(finding) && finding.origin().cycle_id() == cycle.id()
        })
        .filter_map(project_finding)
        .collect();
    ReviewObservation::new(
        cycle.id(),
        cycle.ordinal(),
        state.binding().revision(),
        *cycle.assignment().reviewer(),
        submission.categories().to_vec(),
        findings,
        submission.review_digest(),
    )
    .map_err(|_| {
        ReviewError::new(
            ReviewErrorKind::InvalidInput,
            crate::ReviewRecoveryAction::Quarantine,
            "retained review cannot form a canonical B2 observation",
        )
    })
}

fn project_finding(finding: &Finding) -> Option<FindingObservation> {
    let disposition = match finding.current_disposition() {
        DispositionKind::InvalidationConfirmed | DispositionKind::Superseded => return None,
        DispositionKind::ResolutionConfirmed => {
            let record = finding.dispositions().last()?;
            FindingDisposition::Resolved {
                revision: record.revision(),
                evidence_digest: record.record_digest(),
            }
        }
        DispositionKind::WaiverRequested | DispositionKind::Waived => {
            FindingDisposition::WaiverRequested
        }
        DispositionKind::Open
        | DispositionKind::Fixed
        | DispositionKind::Disputed
        | DispositionKind::SupersessionProposed => FindingDisposition::Open,
    };
    Some(FindingObservation::new(
        finding.id(),
        finding.severity(),
        disposition,
        finding.normalized_digest(),
    ))
}
