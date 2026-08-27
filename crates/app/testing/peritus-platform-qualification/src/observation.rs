//! Scenario, cleanup, and complete run observations.

use std::collections::BTreeSet;

use crate::{
    EvidenceSet, QualificationError, QualificationErrorCode, QualificationRecovery,
    QualificationTarget, ScenarioId, Sha256Digest,
};

/// Terminal scenario outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObservationOutcome {
    /// Every scenario assertion was observed directly.
    Passed,
    /// A required assertion was contradicted.
    Failed,
    /// A required native facility was unavailable.
    Unsupported,
}

/// Cleanup observation returned after every fresh subject is closed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupObservation {
    subject_id: String,
    complete: bool,
    remaining_resources: u32,
    digest: Sha256Digest,
}

impl CleanupObservation {
    /// Creates a bounded cleanup observation.
    ///
    /// # Errors
    ///
    /// Rejects a malformed subject identity or logically inconsistent completion facts.
    pub fn new(
        subject_id: impl Into<String>,
        complete: bool,
        remaining_resources: u32,
        digest: Sha256Digest,
    ) -> Result<Self, QualificationError> {
        let subject_id = checked_subject_id(subject_id.into())?;
        if complete != (remaining_resources == 0) {
            return Err(protocol_error("cleanup completion differs from remaining resource count"));
        }
        Ok(Self { subject_id, complete, remaining_resources, digest })
    }

    /// Borrows the fresh-subject identity.
    #[must_use]
    pub fn subject_id(&self) -> &str {
        &self.subject_id
    }

    /// Reports complete subject and platform-resource cleanup.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.complete
    }

    /// Returns remaining owned resources.
    #[must_use]
    pub const fn remaining_resources(&self) -> u32 {
        self.remaining_resources
    }

    /// Returns the cleanup-evidence digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Bounded result for one scenario on one fresh subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioObservation {
    scenario: ScenarioId,
    subject_id: String,
    outcome: ObservationOutcome,
    evidence: EvidenceSet,
    cleanup: Option<CleanupObservation>,
}

impl ScenarioObservation {
    /// Creates a pre-cleanup scenario observation.
    ///
    /// # Errors
    ///
    /// Rejects malformed subject identity or empty evidence.
    pub fn new(
        scenario: ScenarioId,
        subject_id: impl Into<String>,
        outcome: ObservationOutcome,
        evidence: EvidenceSet,
    ) -> Result<Self, QualificationError> {
        let subject_id = checked_subject_id(subject_id.into())?;
        if evidence.is_empty() {
            return Err(protocol_error("scenario observation must contain direct evidence"));
        }
        Ok(Self { scenario, subject_id, outcome, evidence, cleanup: None })
    }

    /// Attaches the terminal cleanup observation from the same subject.
    ///
    /// # Errors
    ///
    /// Rejects a subject mismatch or repeated cleanup attachment.
    pub fn attach_cleanup(
        &mut self,
        cleanup: CleanupObservation,
    ) -> Result<(), QualificationError> {
        if cleanup.subject_id != self.subject_id || self.cleanup.is_some() {
            return Err(protocol_error("scenario cleanup does not bind the exact fresh subject"));
        }
        self.cleanup = Some(cleanup);
        Ok(())
    }

    /// Returns the scenario identity.
    #[must_use]
    pub const fn scenario(&self) -> ScenarioId {
        self.scenario
    }

    /// Borrows the fresh-subject identity.
    #[must_use]
    pub fn subject_id(&self) -> &str {
        &self.subject_id
    }

    /// Returns the scenario outcome.
    #[must_use]
    pub const fn outcome(&self) -> ObservationOutcome {
        self.outcome
    }

    /// Borrows bounded evidence.
    #[must_use]
    pub const fn evidence(&self) -> &EvidenceSet {
        &self.evidence
    }

    /// Borrows cleanup evidence, when the runner has closed the subject.
    #[must_use]
    pub const fn cleanup(&self) -> Option<&CleanupObservation> {
        self.cleanup.as_ref()
    }
}

/// Complete canonical set of fresh-subject scenario observations for one package target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualificationRun {
    target: QualificationTarget,
    manifest_digest: Sha256Digest,
    observations: Vec<ScenarioObservation>,
}

impl QualificationRun {
    /// Validates a complete run.
    ///
    /// # Errors
    ///
    /// Rejects missing, repeated, out-of-order scenarios, reused subjects, or absent cleanup.
    pub fn new(
        target: QualificationTarget,
        manifest_digest: Sha256Digest,
        observations: Vec<ScenarioObservation>,
    ) -> Result<Self, QualificationError> {
        let required = ScenarioId::all();
        if observations.len() != required.len()
            || observations
                .iter()
                .zip(required)
                .any(|(observation, spec)| observation.scenario != spec.id())
            || observations.iter().any(|observation| observation.cleanup.is_none())
        {
            return Err(protocol_error(
                "qualification run does not contain one canonical result per scenario",
            ));
        }
        let subjects = observations
            .iter()
            .map(|observation| observation.subject_id.as_str())
            .collect::<BTreeSet<_>>();
        if subjects.len() != observations.len() {
            return Err(protocol_error("a qualification subject was reused across scenarios"));
        }
        Ok(Self { target, manifest_digest, observations })
    }

    /// Returns the exact target.
    #[must_use]
    pub const fn target(&self) -> QualificationTarget {
        self.target
    }

    /// Returns the qualified manifest digest.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest_digest
    }

    /// Borrows scenario observations in canonical order.
    #[must_use]
    pub fn observations(&self) -> &[ScenarioObservation] {
        &self.observations
    }
}

fn checked_subject_id(value: String) -> Result<String, QualificationError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(protocol_error("fresh-subject identity is not canonical"));
    }
    Ok(value)
}

fn protocol_error(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::SubjectProtocol,
        QualificationRecovery::ReplaceSubject,
        "validate fresh-subject observation",
        detail,
    )
}
