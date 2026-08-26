//! Lossless B3 envelope/command parsing and outer request binding.

use crate::{
    AppErrorCode, AppProtocolError, AppProtocolLimits, CorrelationId, IdempotencyKey, RequestId,
};
use peritus_codec::{CanonicalDecode, decode_frame, decode_message, sha256};
use peritus_protocol::{
    CommandEnvelopeDto,
    schema::{FAMILIES, MessageRole},
};
use peritus_types::{ActorId, RevisionTuple, SessionId, Sha256Digest};

/// SHA-256 identity of one complete bound application command request.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestDigest(Sha256Digest);

impl RequestDigest {
    /// Borrows the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
    /// Returns the foundation digest representation.
    #[must_use]
    pub const fn as_sha256(self) -> Sha256Digest {
        self.0
    }
}

/// One checked canonical B3 frame retained byte-for-byte.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExactB3Frame {
    bytes: Vec<u8>,
    family: u16,
    schema_version: u16,
    digest: Sha256Digest,
}

impl ExactB3Frame {
    /// Borrows the exact complete PRTS frame bytes.
    #[must_use]
    pub const fn bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }
    /// Returns the checked B3 family tag.
    #[must_use]
    pub const fn family(&self) -> u16 {
        self.family
    }
    /// Returns the checked B3 schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }
    /// Returns SHA-256 over the exact complete frame bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    fn checked(bytes: Vec<u8>, limits: AppProtocolLimits) -> Result<Self, AppProtocolError> {
        let decoded = decode_frame(&bytes, limits.codec()).map_err(AppProtocolError::from_codec)?;
        let header = decoded.header();
        Ok(Self {
            family: header.family(),
            schema_version: header.schema_version(),
            digest: sha256(&bytes),
            bytes,
        })
    }
}

/// Exact B3 command-envelope and command frames accepted for one application submission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSubmissionFrames {
    envelope: ExactB3Frame,
    decoded_envelope: CommandEnvelopeDto,
    command: ExactB3Frame,
}

impl CommandSubmissionFrames {
    /// Parses two complete canonical B3 frames without reserializing either one.
    ///
    /// The first frame must be the current B0 command envelope. The second must be a current B3
    /// registry family whose semantic role is [`MessageRole::Command`].
    ///
    /// # Errors
    ///
    /// Returns a codec-derived error for malformed framing/envelope payload, an unsupported-schema
    /// error for a stale registered command schema, or an invalid-command-frame error for an
    /// unregistered/non-command family.
    pub fn parse(
        envelope_bytes: Vec<u8>,
        command_bytes: Vec<u8>,
        limits: AppProtocolLimits,
    ) -> Result<Self, AppProtocolError> {
        let envelope = ExactB3Frame::checked(envelope_bytes, limits)?;
        if envelope.family() != <CommandEnvelopeDto as CanonicalDecode>::FAMILY {
            return Err(AppProtocolError::new(AppErrorCode::CommandBindingMismatch, None));
        }
        if envelope.schema_version() != <CommandEnvelopeDto as CanonicalDecode>::SCHEMA_VERSION {
            return Err(AppProtocolError::new(AppErrorCode::UnsupportedSchema, None));
        }
        let decoded_envelope =
            decode_message::<CommandEnvelopeDto>(envelope.bytes(), limits.codec())
                .map_err(AppProtocolError::from_codec)?;

        let command = ExactB3Frame::checked(command_bytes, limits)?;
        let Some(registered) = FAMILIES.iter().find(|family| family.tag == command.family()) else {
            return Err(AppProtocolError::new(AppErrorCode::InvalidCommandFrame, None));
        };
        if registered.schema_version != command.schema_version() {
            return Err(AppProtocolError::new(AppErrorCode::UnsupportedSchema, None));
        }
        if registered.role() != MessageRole::Command {
            return Err(AppProtocolError::new(AppErrorCode::InvalidCommandFrame, None));
        }
        Ok(Self { envelope, decoded_envelope, command })
    }

    /// Borrows the retained exact command-envelope frame.
    #[must_use]
    pub const fn envelope_frame(&self) -> &ExactB3Frame {
        &self.envelope
    }
    /// Borrows the checked B0 command envelope decoded from the exact retained bytes.
    #[must_use]
    pub const fn envelope(&self) -> &CommandEnvelopeDto {
        &self.decoded_envelope
    }
    /// Borrows the retained exact registered command frame.
    #[must_use]
    pub const fn command_frame(&self) -> &ExactB3Frame {
        &self.command
    }
}

/// Complete outer metadata bound to exact B3 submission bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandBinding {
    actor_id: ActorId,
    session_id: SessionId,
    request_id: RequestId,
    correlation_id: CorrelationId,
    idempotency_key: IdempotencyKey,
    expected_revision: Option<RevisionTuple>,
    request_digest: RequestDigest,
    frames: CommandSubmissionFrames,
}

impl CommandBinding {
    /// Binds outer identities, idempotency, and freshness to exact B3 frame bytes.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::CommandBindingMismatch`] when a present outer expected revision
    /// differs from the revision decoded from the exact B0 envelope, or a limit error on impossible
    /// preimage length overflow.
    pub fn new(
        actor_id: ActorId,
        session_id: SessionId,
        request_id: RequestId,
        correlation_id: CorrelationId,
        idempotency_key: IdempotencyKey,
        expected_revision: Option<RevisionTuple>,
        frames: CommandSubmissionFrames,
    ) -> Result<Self, AppProtocolError> {
        if expected_revision
            .is_some_and(|revision| revision != frames.envelope().as_domain().revision())
        {
            return Err(AppProtocolError::new(AppErrorCode::CommandBindingMismatch, None));
        }
        let request_digest = digest_request(
            actor_id,
            session_id,
            request_id,
            correlation_id,
            &idempotency_key,
            expected_revision,
            &frames,
        )?;
        Ok(Self {
            actor_id,
            session_id,
            request_id,
            correlation_id,
            idempotency_key,
            expected_revision,
            request_digest,
            frames,
        })
    }

    /// Returns the authenticated actor identity asserted by the caller boundary.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    /// Returns the durable session in which the actor-scoped key is meaningful.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
    /// Returns the application request identity.
    #[must_use]
    pub const fn request_id(&self) -> RequestId {
        self.request_id
    }
    /// Returns the request/response correlation identity.
    #[must_use]
    pub const fn correlation_id(&self) -> CorrelationId {
        self.correlation_id
    }
    /// Borrows the durable-session-and-actor-scoped idempotency key.
    #[must_use]
    pub const fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }
    /// Returns the optional outer freshness revision.
    #[must_use]
    pub const fn expected_revision(&self) -> Option<RevisionTuple> {
        self.expected_revision
    }
    /// Returns the domain-separated digest over all outer fields and both exact frame byte strings.
    #[must_use]
    pub const fn request_digest(&self) -> RequestDigest {
        self.request_digest
    }
    /// Borrows the checked, losslessly retained B3 submission frames.
    #[must_use]
    pub const fn frames(&self) -> &CommandSubmissionFrames {
        &self.frames
    }
}

fn digest_request(
    actor_id: ActorId,
    session_id: SessionId,
    request_id: RequestId,
    correlation_id: CorrelationId,
    idempotency_key: &IdempotencyKey,
    expected_revision: Option<RevisionTuple>,
    frames: &CommandSubmissionFrames,
) -> Result<RequestDigest, AppProtocolError> {
    const DOMAIN: &[u8] = b"peritus.app.command-request.v1\0";
    let revision_bytes = if expected_revision.is_some() { 96 } else { 0 };
    let capacity = DOMAIN
        .len()
        .checked_add(16 * 4 + 4 + idempotency_key.as_bytes().len() + 1 + revision_bytes + 8 * 2)
        .and_then(|size| size.checked_add(frames.envelope_frame().bytes().len()))
        .and_then(|size| size.checked_add(frames.command_frame().bytes().len()))
        .ok_or_else(|| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?;
    let mut preimage = Vec::with_capacity(capacity);
    preimage.extend_from_slice(DOMAIN);
    preimage.extend_from_slice(actor_id.as_bytes());
    preimage.extend_from_slice(session_id.as_bytes());
    preimage.extend_from_slice(request_id.as_bytes());
    preimage.extend_from_slice(correlation_id.as_bytes());
    push_len_u32(&mut preimage, idempotency_key.as_bytes().len())?;
    preimage.extend_from_slice(idempotency_key.as_bytes());
    preimage.push(u8::from(expected_revision.is_some()));
    if let Some(revision) = expected_revision {
        push_revision(&mut preimage, revision);
    }
    push_len_u64(&mut preimage, frames.envelope_frame().bytes().len())?;
    preimage.extend_from_slice(frames.envelope_frame().bytes());
    push_len_u64(&mut preimage, frames.command_frame().bytes().len())?;
    preimage.extend_from_slice(frames.command_frame().bytes());
    Ok(RequestDigest(sha256(&preimage)))
}

fn push_revision(output: &mut Vec<u8>, revision: RevisionTuple) {
    output.extend_from_slice(revision.acceptance_spec_id().as_bytes());
    output.extend_from_slice(revision.harness_id().as_bytes());
    output.extend_from_slice(revision.workspace_id().as_bytes());
    output.extend_from_slice(&revision.workspace_generation().get().to_be_bytes());
    output.extend_from_slice(&revision.workspace_revision().get().to_be_bytes());
    output.extend_from_slice(revision.policy_id().as_bytes());
    output.extend_from_slice(revision.provider_profile_id().as_bytes());
}

fn push_len_u32(output: &mut Vec<u8>, length: usize) -> Result<(), AppProtocolError> {
    let length = u32::try_from(length)
        .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn push_len_u64(output: &mut Vec<u8>, length: usize) -> Result<(), AppProtocolError> {
    let length = u64::try_from(length)
        .map_err(|_| AppProtocolError::new(AppErrorCode::LimitExceeded, None))?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}
