//! Canonical primitive encoders and bounded readers.

#![allow(
    clippy::redundant_pub_crate,
    reason = "the private module shares format helpers across sibling evidence modules"
)]

use crate::{EvidenceError, EvidenceErrorKind, RecoveryAction};
use peritus_types::{
    AcceptanceSpecId, EventId, EvidenceId, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

pub(super) fn put_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}
pub(super) fn put_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}
pub(super) fn put_digest(bytes: &mut Vec<u8>, value: Sha256Digest) {
    bytes.extend_from_slice(value.as_bytes());
}
pub(super) fn put_bytes(bytes: &mut Vec<u8>, value: &[u8]) {
    put_u64(bytes, value.len() as u64);
    bytes.extend_from_slice(value);
}
pub(super) fn put_text(bytes: &mut Vec<u8>, value: &str) {
    put_bytes(bytes, value.as_bytes());
}
pub(super) fn put_revision(bytes: &mut Vec<u8>, revision: &RevisionTuple) {
    bytes.extend_from_slice(revision.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(revision.harness_id().as_bytes());
    bytes.extend_from_slice(revision.workspace_id().as_bytes());
    put_u64(bytes, revision.workspace_generation().get());
    put_u64(bytes, revision.workspace_revision().get());
    bytes.extend_from_slice(revision.policy_id().as_bytes());
    bytes.extend_from_slice(revision.provider_profile_id().as_bytes());
}

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(super) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    pub(super) fn finish(self) -> Result<(), EvidenceError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(invalid("trailing canonical bytes"))
        }
    }
    pub(super) fn take(&mut self, length: usize) -> Result<&'a [u8], EvidenceError> {
        let end =
            self.offset.checked_add(length).ok_or_else(|| invalid("canonical length overflow"))?;
        let value =
            self.bytes.get(self.offset..end).ok_or_else(|| invalid("truncated canonical bytes"))?;
        self.offset = end;
        Ok(value)
    }
    pub(super) fn u16(&mut self) -> Result<u16, EvidenceError> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().map_err(|_| invalid("invalid u16"))?))
    }
    pub(super) fn u64(&mut self) -> Result<u64, EvidenceError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().map_err(|_| invalid("invalid u64"))?))
    }
    pub(super) fn digest(&mut self) -> Result<Sha256Digest, EvidenceError> {
        Ok(Sha256Digest::new(self.take(32)?.try_into().map_err(|_| invalid("invalid digest"))?))
    }
    pub(super) fn evidence_id(&mut self) -> Result<EvidenceId, EvidenceError> {
        EvidenceId::new(self.take(16)?.try_into().map_err(|_| invalid("invalid evidence id"))?)
            .map_err(|_| invalid("reserved evidence id"))
    }
    pub(super) fn event_id(&mut self) -> Result<EventId, EvidenceError> {
        EventId::new(self.take(16)?.try_into().map_err(|_| invalid("invalid event id"))?)
            .map_err(|_| invalid("reserved event id"))
    }
    pub(super) fn bytes(&mut self, limit: usize) -> Result<&'a [u8], EvidenceError> {
        let length = usize::try_from(self.u64()?).map_err(|_| invalid("length exceeds usize"))?;
        if length > limit {
            return Err(invalid("canonical value exceeds limit"));
        }
        self.take(length)
    }
    pub(super) fn text(&mut self, limit: usize) -> Result<String, EvidenceError> {
        let value = self.bytes(limit)?;
        std::str::from_utf8(value).map(str::to_owned).map_err(|_| invalid("text is not UTF-8"))
    }
    pub(super) fn revision(&mut self) -> Result<RevisionTuple, EvidenceError> {
        let acceptance = AcceptanceSpecId::new(array16(self.take(16)?)?)
            .map_err(|_| invalid("acceptance id"))?;
        let harness =
            HarnessId::new(array16(self.take(16)?)?).map_err(|_| invalid("harness id"))?;
        let workspace =
            WorkspaceId::new(array16(self.take(16)?)?).map_err(|_| invalid("workspace id"))?;
        let generation =
            Generation::new(self.u64()?).map_err(|_| invalid("workspace generation"))?;
        let revision =
            RevisionNumber::new(self.u64()?).map_err(|_| invalid("workspace revision"))?;
        let policy = PolicyId::new(array16(self.take(16)?)?).map_err(|_| invalid("policy id"))?;
        let provider =
            ProviderProfileId::new(array16(self.take(16)?)?).map_err(|_| invalid("provider id"))?;
        Ok(RevisionTuple::new(
            acceptance, harness, workspace, generation, revision, policy, provider,
        ))
    }
}

fn array16(bytes: &[u8]) -> Result<[u8; 16], EvidenceError> {
    bytes.try_into().map_err(|_| invalid("identity must be 16 bytes"))
}

pub(super) fn invalid(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::InvalidBundle,
        RecoveryAction::CorrectInput,
        "decode canonical evidence",
        detail,
    )
}
