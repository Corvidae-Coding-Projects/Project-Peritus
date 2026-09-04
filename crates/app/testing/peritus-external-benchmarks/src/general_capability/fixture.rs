use serde::Deserialize;

use peritus_obligations::{EvidenceBinding, ObligationLimits};
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

#[derive(Deserialize)]
pub(super) struct FixtureSet<T> {
    pub(super) cases: Vec<T>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Expected {
    Success,
    Partial,
    Failure,
}

pub(super) const fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::new([byte; 32])
}

pub(super) fn binding(requirement: u8, evidence: u8) -> EvidenceBinding {
    let candidate = CandidateIdentity::new(
        RunId::new([31; 16]).expect("run id"),
        WorkspaceId::new([32; 16]).expect("workspace id"),
        digest(33),
        1,
        1,
    )
    .expect("candidate");
    EvidenceBinding::new(
        RequirementId::new(digest(requirement)),
        digest(34),
        candidate,
        digest(evidence),
        Vec::new(),
        ObligationLimits::production(),
    )
    .expect("evidence binding")
}
