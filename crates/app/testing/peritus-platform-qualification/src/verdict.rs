//! Evidence reduction into a bounded ready/not-ready verdict.

use crate::{ObservationOutcome, QualificationRun, ScenarioId, Sha256Digest, digest_bytes};

/// Stable reason that blocks a release target from readiness.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NotReadyReason {
    /// The scenario contradicted a required contract.
    ScenarioFailed(ScenarioId),
    /// A required native facility was absent.
    ScenarioUnsupported(ScenarioId),
    /// The scenario subject or its owned resources were not completely removed.
    CleanupIncomplete(ScenarioId),
}

/// Compact acceptance evidence for one ready package target.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReadyEvidence {
    manifest_digest: Sha256Digest,
    scenario_count: usize,
    evidence_digest: Sha256Digest,
}

impl ReadyEvidence {
    /// Returns the exact package manifest digest.
    #[must_use]
    pub const fn manifest_digest(self) -> Sha256Digest {
        self.manifest_digest
    }

    /// Returns the number of passing fresh-subject scenarios.
    #[must_use]
    pub const fn scenario_count(self) -> usize {
        self.scenario_count
    }

    /// Returns a deterministic digest over scenario outcomes and cleanup evidence.
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
}

/// Final H2 release-target disposition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadinessVerdict {
    /// Every required scenario passed on a distinct subject with complete cleanup.
    Ready(ReadyEvidence),
    /// At least one required assertion, facility, or cleanup contract failed.
    NotReady(Vec<NotReadyReason>),
}

/// Pure reducer over one complete qualification run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualificationReport {
    run: QualificationRun,
    verdict: ReadinessVerdict,
}

impl QualificationReport {
    /// Evaluates all required scenarios without consulting ambient host state.
    #[must_use]
    pub fn evaluate(run: QualificationRun) -> Self {
        let mut reasons = Vec::new();
        let mut canonical = format!(
            "peritus/h2-evidence/v1\nmanifest={}\nplatform={}\narchitecture={}\n",
            run.manifest_digest(),
            run.target().platform().as_str(),
            run.target().architecture().as_str(),
        );
        for observation in run.observations() {
            use core::fmt::Write as _;
            match observation.outcome() {
                ObservationOutcome::Passed => {}
                ObservationOutcome::Failed => {
                    reasons.push(NotReadyReason::ScenarioFailed(observation.scenario()));
                }
                ObservationOutcome::Unsupported => {
                    reasons.push(NotReadyReason::ScenarioUnsupported(observation.scenario()));
                }
            }
            let Some(cleanup) = observation.cleanup() else {
                reasons.push(NotReadyReason::CleanupIncomplete(observation.scenario()));
                continue;
            };
            if !cleanup.complete() {
                reasons.push(NotReadyReason::CleanupIncomplete(observation.scenario()));
            }
            let _ = writeln!(
                &mut canonical,
                "{:?}|{:?}|{}|{}|{}",
                observation.scenario(),
                observation.outcome(),
                observation.evidence().digest(),
                cleanup.digest(),
                cleanup.remaining_resources(),
            );
        }
        let verdict = if reasons.is_empty() {
            ReadinessVerdict::Ready(ReadyEvidence {
                manifest_digest: run.manifest_digest(),
                scenario_count: run.observations().len(),
                evidence_digest: digest_bytes(canonical.as_bytes()).sha256(),
            })
        } else {
            reasons.sort();
            reasons.dedup();
            ReadinessVerdict::NotReady(reasons)
        };
        Self { run, verdict }
    }

    /// Borrows the complete bounded run.
    #[must_use]
    pub const fn run(&self) -> &QualificationRun {
        &self.run
    }

    /// Borrows the final disposition.
    #[must_use]
    pub const fn verdict(&self) -> &ReadinessVerdict {
        &self.verdict
    }
}
