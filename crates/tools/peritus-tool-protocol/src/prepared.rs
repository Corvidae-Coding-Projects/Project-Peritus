//! Effect-free descriptor-bound call preparation and replay identity.

use std::sync::Arc;

use crate::{ProtocolError, ProtocolErrorKind, SchemaDigest, ToolCall, ToolDescriptor};
use peritus_types::Sha256Digest;

/// Digest identity that distinguishes exact replay from action-ID conflict.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReplayIdentity(Sha256Digest);

impl ReplayIdentity {
    /// Returns the exact replay digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.0
    }
    /// Borrows replay digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

/// Immutable effect-free call plan bound to one exact registered descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedToolCall {
    descriptor: Arc<ToolDescriptor>,
    call: ToolCall,
    arguments_digest: Sha256Digest,
    prepared_digest: Sha256Digest,
    replay_identity: ReplayIdentity,
}

impl PreparedToolCall {
    /// Borrows the exact descriptor selected during preparation.
    #[must_use]
    pub fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }
    /// Borrows the original validated call envelope.
    #[must_use]
    pub const fn call(&self) -> &ToolCall {
        &self.call
    }
    /// Borrows the complete validated structured arguments.
    #[must_use]
    pub const fn arguments(&self) -> &crate::BoundedJson {
        self.call.arguments()
    }
    /// Returns the canonical arguments digest.
    #[must_use]
    pub const fn arguments_digest(&self) -> Sha256Digest {
        self.arguments_digest
    }
    /// Returns the exact full descriptor digest.
    #[must_use]
    pub fn descriptor_digest(&self) -> SchemaDigest {
        self.descriptor.descriptor_digest()
    }
    /// Returns the domain-separated prepared-call digest.
    #[must_use]
    pub const fn prepared_digest(&self) -> Sha256Digest {
        self.prepared_digest
    }
    /// Returns the exact action replay identity.
    #[must_use]
    pub const fn replay_identity(&self) -> ReplayIdentity {
        self.replay_identity
    }

    /// Returns the stable version-one canonical prepared-call envelope bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = crate::wire::begin(3);
        crate::wire::bytes(&mut bytes, &self.call.canonical_bytes());
        bytes.extend_from_slice(self.descriptor.descriptor_digest().as_bytes());
        bytes.extend_from_slice(self.arguments_digest.as_bytes());
        bytes.extend_from_slice(self.prepared_digest.as_bytes());
        bytes.extend_from_slice(self.replay_identity.as_bytes());
        bytes
    }
}

/// Validates a call against an exact descriptor and produces deterministic prepared digests.
///
/// This operation is pure with respect to external systems and consumes no authority.
///
/// # Errors
///
/// Rejects name/version mismatch, widened call limits, or any schema violation.
pub fn prepare_call(
    descriptor: Arc<ToolDescriptor>,
    call: ToolCall,
) -> Result<PreparedToolCall, ProtocolError> {
    if call.name() != descriptor.name() || call.version() != descriptor.version() {
        return Err(ProtocolError::at(
            ProtocolErrorKind::DescriptorMismatch,
            "call.tool",
            "call tool identity/version differs from the selected descriptor",
        ));
    }
    if !call.limits().fits(descriptor.limits()) {
        return Err(ProtocolError::at(
            ProtocolErrorKind::CallLimit,
            "call.limits",
            "call limits widen the immutable descriptor ceiling",
        ));
    }
    descriptor.schema().validate(call.arguments())?;
    let arguments_digest = call.arguments().digest();
    let prepared_digest = prepared_digest(&descriptor, &call, arguments_digest);
    let replay_identity =
        ReplayIdentity(replay_digest(&descriptor, &call, arguments_digest, prepared_digest));
    Ok(PreparedToolCall { descriptor, call, arguments_digest, prepared_digest, replay_identity })
}

fn prepared_digest(
    descriptor: &ToolDescriptor,
    call: &ToolCall,
    arguments: Sha256Digest,
) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(256);
    bytes.extend_from_slice(b"peritus.prepared-tool-call.v1\0");
    bytes.extend_from_slice(call.action_id().as_bytes());
    bytes.extend_from_slice(descriptor.descriptor_digest().as_bytes());
    bytes.extend_from_slice(arguments.as_bytes());
    bytes.extend_from_slice(&call.limits().canonical_bytes());
    append_revision(&mut bytes, call.revision());
    bytes.extend_from_slice(&call.deadline().epoch().get().to_be_bytes());
    bytes.extend_from_slice(&call.deadline().tick_millis().to_be_bytes());
    append_bytes(&mut bytes, call.idempotency_key().as_str().as_bytes());
    peritus_codec::sha256(&bytes)
}

fn replay_digest(
    descriptor: &ToolDescriptor,
    call: &ToolCall,
    arguments: Sha256Digest,
    prepared: Sha256Digest,
) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(256);
    bytes.extend_from_slice(b"peritus.tool-replay.v1\0");
    bytes.extend_from_slice(call.action_id().as_bytes());
    append_bytes(&mut bytes, descriptor.name().as_str().as_bytes());
    bytes.extend_from_slice(&descriptor.version().major().to_be_bytes());
    bytes.extend_from_slice(&descriptor.version().minor().to_be_bytes());
    bytes.extend_from_slice(&descriptor.version().patch().to_be_bytes());
    bytes.extend_from_slice(descriptor.descriptor_digest().as_bytes());
    bytes.extend_from_slice(arguments.as_bytes());
    bytes.extend_from_slice(prepared.as_bytes());
    bytes.extend_from_slice(&call.limits().canonical_bytes());
    append_revision(&mut bytes, call.revision());
    append_bytes(&mut bytes, call.idempotency_key().as_str().as_bytes());
    peritus_codec::sha256(&bytes)
}

fn append_revision(bytes: &mut Vec<u8>, revision: peritus_types::RevisionTuple) {
    bytes.extend_from_slice(revision.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(revision.harness_id().as_bytes());
    bytes.extend_from_slice(revision.workspace_id().as_bytes());
    bytes.extend_from_slice(&revision.workspace_generation().get().to_be_bytes());
    bytes.extend_from_slice(&revision.workspace_revision().get().to_be_bytes());
    bytes.extend_from_slice(revision.policy_id().as_bytes());
    bytes.extend_from_slice(revision.provider_profile_id().as_bytes());
}

fn append_bytes(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value);
}
