//! Exact integrated-candidate JSON projection.

use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, Sha256Digest, WorkspaceId,
};
use serde::{Deserialize, Serialize};

use crate::{IntegratedCandidate, QualificationError, hex_digest};

use super::interchange;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CandidateDocument {
    acceptance_spec_id: String,
    harness_id: String,
    workspace_id: String,
    workspace_generation: u64,
    workspace_revision: u64,
    policy_id: String,
    provider_profile_id: String,
    source_sha256: String,
    release_manifest_sha256: String,
    qualification_plan_sha256: String,
}

impl CandidateDocument {
    pub(super) fn from_candidate(candidate: IntegratedCandidate) -> Self {
        let revision = candidate.revision();
        Self {
            acceptance_spec_id: hex(revision.acceptance_spec_id().as_bytes()),
            harness_id: hex(revision.harness_id().as_bytes()),
            workspace_id: hex(revision.workspace_id().as_bytes()),
            workspace_generation: revision.workspace_generation().get(),
            workspace_revision: revision.workspace_revision().get(),
            policy_id: hex(revision.policy_id().as_bytes()),
            provider_profile_id: hex(revision.provider_profile_id().as_bytes()),
            source_sha256: hex_digest(candidate.source_digest()),
            release_manifest_sha256: hex_digest(candidate.release_manifest_digest()),
            qualification_plan_sha256: hex_digest(candidate.qualification_plan_digest()),
        }
    }

    pub(super) fn into_candidate(self) -> Result<IntegratedCandidate, QualificationError> {
        let revision = RevisionTuple::new(
            AcceptanceSpecId::new(parse_hex(&self.acceptance_spec_id)?)
                .map_err(|_| interchange("candidate acceptance-spec identity is zero"))?,
            HarnessId::new(parse_hex(&self.harness_id)?)
                .map_err(|_| interchange("candidate harness identity is zero"))?,
            WorkspaceId::new(parse_hex(&self.workspace_id)?)
                .map_err(|_| interchange("candidate workspace identity is zero"))?,
            Generation::new(self.workspace_generation)
                .map_err(|_| interchange("candidate workspace generation is zero"))?,
            RevisionNumber::new(self.workspace_revision)
                .map_err(|_| interchange("candidate workspace revision is zero"))?,
            PolicyId::new(parse_hex(&self.policy_id)?)
                .map_err(|_| interchange("candidate policy identity is zero"))?,
            ProviderProfileId::new(parse_hex(&self.provider_profile_id)?)
                .map_err(|_| interchange("candidate provider-profile identity is zero"))?,
        );
        Ok(IntegratedCandidate::new(
            revision,
            Sha256Digest::new(parse_hex(&self.source_sha256)?),
            Sha256Digest::new(parse_hex(&self.release_manifest_sha256)?),
            Sha256Digest::new(parse_hex(&self.qualification_plan_sha256)?),
        ))
    }
}

fn hex<const N: usize>(bytes: &[u8; N]) -> String {
    use core::fmt::Write as _;

    let mut encoded = String::with_capacity(N * 2);
    for byte in bytes {
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

pub(super) fn parse_hex<const N: usize>(value: &str) -> Result<[u8; N], QualificationError> {
    if value.len() != N * 2 {
        return Err(interchange("candidate or evidence digest has the wrong hexadecimal length"));
    }
    let mut bytes = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = nibble(pair[0])? << 4 | nibble(pair[1])?;
    }
    Ok(bytes)
}

fn nibble(value: u8) -> Result<u8, QualificationError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(interchange("candidate or evidence digest is not lowercase hexadecimal")),
    }
}
