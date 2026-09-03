#![allow(dead_code, reason = "fixture builders are shared across obligation matrices")]

use peritus_obligations::{
    EvidenceBinding, ObligationLimits, ObligationSpec, PathId, PathMention, PublicTaskSource,
    RequirementDraft, RequirementLedger,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

pub const fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::new([byte; 32])
}

pub const fn requirement_id(byte: u8) -> RequirementId {
    RequirementId::new(digest(byte))
}

pub const fn path_id(byte: u8) -> PathId {
    PathId::new(digest(byte))
}

pub fn candidate(candidate_digest: u8, conversation: u64, sequence: u64) -> CandidateIdentity {
    CandidateIdentity::new(
        RunId::new([91; 16]).expect("run id"),
        WorkspaceId::new([92; 16]).expect("workspace id"),
        digest(candidate_digest),
        conversation,
        sequence,
    )
    .expect("candidate")
}

pub fn ledger(
    entries: Vec<(u8, &'static [u8], ObligationSpec, Vec<PathMention>)>,
) -> RequirementLedger {
    let limits = ObligationLimits::production();
    let mut source_bytes = Vec::new();
    let mut drafts = Vec::new();
    for (id, clause, specification, paths) in entries {
        let start = source_bytes.len();
        source_bytes.extend_from_slice(clause);
        let end = source_bytes.len();
        source_bytes.push(b'\n');
        drafts.push(RequirementDraft::new(requirement_id(id), start, end, specification, paths));
    }
    let source = PublicTaskSource::new(source_bytes, 7, limits).expect("public source");
    RequirementLedger::extract(&source, drafts, limits).expect("requirement ledger")
}

pub fn binding(
    ledger: &RequirementLedger,
    candidate: CandidateIdentity,
    requirement: u8,
    observed_paths: Vec<PathId>,
    evidence_digest: u8,
) -> EvidenceBinding {
    EvidenceBinding::new(
        requirement_id(requirement),
        ledger.digest(),
        candidate,
        digest(evidence_digest),
        observed_paths,
        ledger.limits(),
    )
    .expect("evidence binding")
}
