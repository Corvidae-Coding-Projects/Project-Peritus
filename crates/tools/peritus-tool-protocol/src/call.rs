//! Versioned bounded proposed tool-call envelope.

use crate::{BoundedJson, IdempotencyKey, ProtocolError, ProtocolErrorKind, SemanticVersion};
use peritus_policy::AuthorityInstant;
use peritus_types::{ActionId, CapabilityName, RevisionTuple};

/// Per-call ceilings that can only narrow an immutable descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallLimits {
    timeout_millis: u64,
    output_bytes: u64,
    model_bytes: u32,
    human_bytes: u32,
    progress_events: u32,
    artifacts: u16,
}

impl CallLimits {
    /// Creates complete nonzero call ceilings.
    ///
    /// # Errors
    ///
    /// Rejects any zero ceiling.
    pub fn new(
        timeout_millis: u64,
        output_bytes: u64,
        model_bytes: u32,
        human_bytes: u32,
        progress_events: u32,
        artifacts: u16,
    ) -> Result<Self, ProtocolError> {
        if timeout_millis == 0
            || output_bytes == 0
            || model_bytes == 0
            || human_bytes == 0
            || progress_events == 0
            || artifacts == 0
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::CallLimit,
                "call.limits",
                "every call ceiling must be nonzero",
            ));
        }
        Ok(Self {
            timeout_millis,
            output_bytes,
            model_bytes,
            human_bytes,
            progress_events,
            artifacts,
        })
    }

    /// Returns the wall-time ceiling.
    #[must_use]
    pub const fn timeout_millis(self) -> u64 {
        self.timeout_millis
    }
    /// Returns the complete output ceiling.
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        self.output_bytes
    }
    /// Returns the model rendering ceiling.
    #[must_use]
    pub const fn model_bytes(self) -> u32 {
        self.model_bytes
    }
    /// Returns the human rendering ceiling.
    #[must_use]
    pub const fn human_bytes(self) -> u32 {
        self.human_bytes
    }
    /// Returns the progress-event ceiling.
    #[must_use]
    pub const fn progress_events(self) -> u32 {
        self.progress_events
    }
    /// Returns the artifact-reference ceiling.
    #[must_use]
    pub const fn artifacts(self) -> u16 {
        self.artifacts
    }

    pub(crate) const fn fits(self, descriptor: crate::ToolLimits) -> bool {
        self.timeout_millis <= descriptor.timeout_millis()
            && self.output_bytes <= descriptor.output_bytes()
            && self.model_bytes <= descriptor.model_bytes()
            && self.human_bytes <= descriptor.human_bytes()
            && self.progress_events <= descriptor.progress_events()
            && self.artifacts <= descriptor.artifacts()
    }

    pub(crate) fn canonical_bytes(self) -> [u8; 30] {
        let mut bytes = [0; 30];
        bytes[0..8].copy_from_slice(&self.timeout_millis.to_be_bytes());
        bytes[8..16].copy_from_slice(&self.output_bytes.to_be_bytes());
        bytes[16..20].copy_from_slice(&self.model_bytes.to_be_bytes());
        bytes[20..24].copy_from_slice(&self.human_bytes.to_be_bytes());
        bytes[24..28].copy_from_slice(&self.progress_events.to_be_bytes());
        bytes[28..30].copy_from_slice(&self.artifacts.to_be_bytes());
        bytes
    }
}

/// One untrusted model-proposed call, validated for structural bounds only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolCall {
    action_id: ActionId,
    name: CapabilityName,
    version: SemanticVersion,
    arguments: BoundedJson,
    limits: CallLimits,
    revision: RevisionTuple,
    deadline: AuthorityInstant,
    idempotency_key: IdempotencyKey,
}

impl ToolCall {
    /// Creates a complete versioned call envelope.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        action_id: ActionId,
        name: CapabilityName,
        version: SemanticVersion,
        arguments: BoundedJson,
        limits: CallLimits,
        revision: RevisionTuple,
        deadline: AuthorityInstant,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self { action_id, name, version, arguments, limits, revision, deadline, idempotency_key }
    }

    /// Returns the B0 action identity.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    /// Borrows the exact tool/capability name.
    #[must_use]
    pub const fn name(&self) -> &CapabilityName {
        &self.name
    }
    /// Returns the requested semantic version.
    #[must_use]
    pub const fn version(&self) -> SemanticVersion {
        self.version
    }
    /// Borrows the complete bounded arguments.
    #[must_use]
    pub const fn arguments(&self) -> &BoundedJson {
        &self.arguments
    }
    /// Returns narrowed call limits.
    #[must_use]
    pub const fn limits(&self) -> CallLimits {
        self.limits
    }
    /// Returns the exact authority revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the immutable authority-clock deadline.
    #[must_use]
    pub const fn deadline(&self) -> AuthorityInstant {
        self.deadline
    }
    /// Borrows the explicit idempotency identity.
    #[must_use]
    pub const fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }

    /// Returns the stable version-one canonical call envelope bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = crate::wire::begin(2);
        bytes.extend_from_slice(self.action_id.as_bytes());
        crate::wire::text(&mut bytes, self.name.as_str());
        crate::wire::u16_value(&mut bytes, self.version.major());
        crate::wire::u16_value(&mut bytes, self.version.minor());
        crate::wire::u16_value(&mut bytes, self.version.patch());
        crate::wire::bytes(&mut bytes, self.arguments.canonical_bytes());
        bytes.extend_from_slice(&self.limits.canonical_bytes());
        crate::wire::revision(&mut bytes, self.revision);
        crate::wire::instant(&mut bytes, self.deadline);
        crate::wire::text(&mut bytes, self.idempotency_key.as_str());
        bytes
    }
}
