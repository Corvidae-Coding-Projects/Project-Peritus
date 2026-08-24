//! Canonical durable one-use process consumption claims.

use peritus_types::{ActionId, ProcessId, Sha256Digest};
use sha2::{Digest, Sha256};

use crate::{
    ErrorCode, ExecutionIdentity, ProcessError, ProcessOperation, RecoveryClass,
    recovery::manifest::ExecutionManifest,
};

const MAGIC: &[u8] = b"PERITUS-PROCESS-CONSUMED-V2\0";
const PAYLOAD_BYTES: usize = 16 + 16 + Sha256Digest::LENGTH + Sha256Digest::LENGTH;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ConsumptionClaim {
    action_id: ActionId,
    process_id: ProcessId,
    action_digest: Sha256Digest,
    plan_digest: Sha256Digest,
}

impl ConsumptionClaim {
    pub(crate) const fn new(
        identity: &ExecutionIdentity,
        action_digest: Sha256Digest,
        plan_digest: Sha256Digest,
    ) -> Self {
        Self {
            action_id: identity.action_id(),
            process_id: identity.process_id(),
            action_digest,
            plan_digest,
        }
    }

    pub(crate) const fn process_id(self) -> ProcessId {
        self.process_id
    }

    pub(crate) fn matches_manifest(self, manifest: &ExecutionManifest) -> bool {
        self.process_id == manifest.identity.process_id()
            && self.action_id == manifest.identity.action_id()
            && self.action_digest == manifest.action_digest
            && self.plan_digest == manifest.plan_digest
    }

    pub(crate) fn encode(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(MAGIC.len() + PAYLOAD_BYTES + Sha256Digest::LENGTH);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(self.action_id.as_bytes());
        bytes.extend_from_slice(self.process_id.as_bytes());
        bytes.extend_from_slice(self.action_digest.as_bytes());
        bytes.extend_from_slice(self.plan_digest.as_bytes());
        let checksum: [u8; 32] = Sha256::digest(&bytes).into();
        bytes.extend_from_slice(&checksum);
        bytes
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, ProcessError> {
        let expected_length = MAGIC.len() + PAYLOAD_BYTES + Sha256Digest::LENGTH;
        if bytes.len() != expected_length || !bytes.starts_with(MAGIC) {
            return Err(corrupt("process consumption claim has invalid framing"));
        }
        let checksum_at = bytes.len() - Sha256Digest::LENGTH;
        let expected: [u8; 32] = Sha256::digest(&bytes[..checksum_at]).into();
        if bytes[checksum_at..] != expected {
            return Err(corrupt("process consumption claim checksum differs"));
        }
        let mut offset = MAGIC.len();
        let action_id = ActionId::new(take(bytes, &mut offset)?)
            .map_err(|_| corrupt("process consumption claim has a zero action identifier"))?;
        let process_id = ProcessId::new(take(bytes, &mut offset)?)
            .map_err(|_| corrupt("process consumption claim has a zero process identifier"))?;
        let action_digest = Sha256Digest::new(take(bytes, &mut offset)?);
        let plan_digest = Sha256Digest::new(take(bytes, &mut offset)?);
        if offset != checksum_at {
            return Err(corrupt("process consumption claim has noncanonical fields"));
        }
        Ok(Self { action_id, process_id, action_digest, plan_digest })
    }
}

fn take<const N: usize>(bytes: &[u8], offset: &mut usize) -> Result<[u8; N], ProcessError> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| corrupt("process consumption claim offset overflowed"))?;
    let source =
        bytes.get(*offset..end).ok_or_else(|| corrupt("process consumption claim is truncated"))?;
    let mut value = [0_u8; N];
    value.copy_from_slice(source);
    *offset = end;
    Ok(value)
}

const fn corrupt(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::CorruptRecovery,
        ProcessOperation::Reconcile,
        RecoveryClass::Quarantine,
        detail,
    )
}
