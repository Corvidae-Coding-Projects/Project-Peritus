//! Canonical durable one-use process consumption claims.

use peritus_types::{ActionId, ProcessId, Sha256Digest};
use sha2::{Digest, Sha256};

use crate::{
    ErrorCode, ExecutionIdentity, ProcessError, ProcessOperation, RecoveryClass,
    recovery::manifest::ExecutionManifest,
};

const MAGIC_V2: &[u8] = b"PERITUS-PROCESS-CONSUMED-V2\0";
const MAGIC_V3: &[u8] = b"PERITUS-PROCESS-CONSUMED-V3\0";
const PAYLOAD_BYTES: usize = 16 + 16 + Sha256Digest::LENGTH + Sha256Digest::LENGTH;
const OWNER_BINDING_BYTES: usize = Sha256Digest::LENGTH + Sha256Digest::LENGTH;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RetainedClaimBinding {
    operation_digest: Sha256Digest,
    request_digest: Sha256Digest,
}

impl RetainedClaimBinding {
    pub(crate) const fn new(
        operation_digest: Sha256Digest,
        request_digest: Sha256Digest,
    ) -> Self {
        Self { operation_digest, request_digest }
    }

    pub(crate) const fn operation_digest(self) -> Sha256Digest { self.operation_digest }
    pub(crate) const fn request_digest(self) -> Sha256Digest { self.request_digest }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ConsumptionClaim {
    action_id: ActionId,
    process_id: ProcessId,
    action_digest: Sha256Digest,
    plan_digest: Sha256Digest,
    retained_owner: Option<RetainedClaimBinding>,
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
            retained_owner: None,
        }
    }

    pub(crate) const fn new_retained(
        identity: &ExecutionIdentity,
        action_digest: Sha256Digest,
        plan_digest: Sha256Digest,
        retained_owner: RetainedClaimBinding,
    ) -> Self {
        Self {
            action_id: identity.action_id(),
            process_id: identity.process_id(),
            action_digest,
            plan_digest,
            retained_owner: Some(retained_owner),
        }
    }

    pub(crate) const fn process_id(self) -> ProcessId {
        self.process_id
    }

    pub(crate) const fn action_id(self) -> ActionId {
        self.action_id
    }

    pub(crate) const fn action_digest(self) -> Sha256Digest { self.action_digest }
    pub(crate) const fn plan_digest(self) -> Sha256Digest { self.plan_digest }
    pub(crate) const fn retained_owner(self) -> Option<RetainedClaimBinding> {
        self.retained_owner
    }

    pub(crate) fn matches_manifest(self, manifest: &ExecutionManifest) -> bool {
        self.process_id == manifest.identity.process_id()
            && self.action_id == manifest.identity.action_id()
            && self.action_digest == manifest.action_digest
            && self.plan_digest == manifest.plan_digest
    }

    pub(crate) fn encode(self) -> Vec<u8> {
        let magic = if self.retained_owner.is_some() { MAGIC_V3 } else { MAGIC_V2 };
        let mut bytes = Vec::with_capacity(
            magic.len()
                + PAYLOAD_BYTES
                + self.retained_owner.map_or(0, |_| OWNER_BINDING_BYTES)
                + Sha256Digest::LENGTH,
        );
        bytes.extend_from_slice(magic);
        bytes.extend_from_slice(self.action_id.as_bytes());
        bytes.extend_from_slice(self.process_id.as_bytes());
        bytes.extend_from_slice(self.action_digest.as_bytes());
        bytes.extend_from_slice(self.plan_digest.as_bytes());
        if let Some(owner) = self.retained_owner {
            bytes.extend_from_slice(owner.operation_digest.as_bytes());
            bytes.extend_from_slice(owner.request_digest.as_bytes());
        }
        let checksum: [u8; 32] = Sha256::digest(&bytes).into();
        bytes.extend_from_slice(&checksum);
        bytes
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, ProcessError> {
        let (magic, retained) = if bytes.starts_with(MAGIC_V3) {
            (MAGIC_V3, true)
        } else if bytes.starts_with(MAGIC_V2) {
            (MAGIC_V2, false)
        } else {
            return Err(corrupt("process consumption claim has invalid framing"));
        };
        let expected_length = magic.len()
            + PAYLOAD_BYTES
            + if retained { OWNER_BINDING_BYTES } else { 0 }
            + Sha256Digest::LENGTH;
        if bytes.len() != expected_length {
            return Err(corrupt("process consumption claim has invalid framing"));
        }
        let checksum_at = bytes.len() - Sha256Digest::LENGTH;
        let expected: [u8; 32] = Sha256::digest(&bytes[..checksum_at]).into();
        if bytes[checksum_at..] != expected {
            return Err(corrupt("process consumption claim checksum differs"));
        }
        let mut offset = magic.len();
        let action_id = ActionId::new(take(bytes, &mut offset)?)
            .map_err(|_| corrupt("process consumption claim has a zero action identifier"))?;
        let process_id = ProcessId::new(take(bytes, &mut offset)?)
            .map_err(|_| corrupt("process consumption claim has a zero process identifier"))?;
        let action_digest = Sha256Digest::new(take(bytes, &mut offset)?);
        let plan_digest = Sha256Digest::new(take(bytes, &mut offset)?);
        let retained_owner = if retained {
            Some(RetainedClaimBinding::new(
                Sha256Digest::new(take(bytes, &mut offset)?),
                Sha256Digest::new(take(bytes, &mut offset)?),
            ))
        } else {
            None
        };
        if offset != checksum_at {
            return Err(corrupt("process consumption claim has noncanonical fields"));
        }
        Ok(Self { action_id, process_id, action_digest, plan_digest, retained_owner })
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
