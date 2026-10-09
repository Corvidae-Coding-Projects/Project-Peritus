//! Versioned bounded proposed tool-call envelope.

use crate::{BoundedJson, IdempotencyKey, ProtocolError, ProtocolErrorKind, SemanticVersion};
use peritus_policy::AuthorityInstant;
use peritus_types::{ActionId, CapabilityName, Generation, RevisionTuple};

/// Per-call ceilings that can only narrow an immutable descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallLimits {
    timeout_millis: Option<u64>,
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
        Self::new_optional(
            Some(timeout_millis),
            output_bytes,
            model_bytes,
            human_bytes,
            progress_events,
            artifacts,
        )
    }

    /// Creates call ceilings with an explicitly optional execution timeout.
    ///
    /// # Errors
    /// Rejects zero supplied limits. Absence of a timeout must also be permitted by the descriptor.
    pub fn new_optional(
        timeout_millis: Option<u64>,
        output_bytes: u64,
        model_bytes: u32,
        human_bytes: u32,
        progress_events: u32,
        artifacts: u16,
    ) -> Result<Self, ProtocolError> {
        if timeout_millis == Some(0)
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
    pub const fn timeout_millis(self) -> Option<u64> {
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
        let timeout_fits = match (self.timeout_millis, descriptor.timeout_millis()) {
            (_, None) => true,
            (Some(call), Some(maximum)) => call <= maximum,
            (None, Some(_)) => false,
        };
        timeout_fits
            && self.output_bytes <= descriptor.output_bytes()
            && self.model_bytes <= descriptor.model_bytes()
            && self.human_bytes <= descriptor.human_bytes()
            && self.progress_events <= descriptor.progress_events()
            && self.artifacts <= descriptor.artifacts()
    }

    pub(crate) fn canonical_bytes(self) -> [u8; 30] {
        let mut bytes = [0; 30];
        bytes[0..8].copy_from_slice(&self.timeout_millis.unwrap_or(0).to_be_bytes());
        bytes[8..16].copy_from_slice(&self.output_bytes.to_be_bytes());
        bytes[16..20].copy_from_slice(&self.model_bytes.to_be_bytes());
        bytes[20..24].copy_from_slice(&self.human_bytes.to_be_bytes());
        bytes[24..28].copy_from_slice(&self.progress_events.to_be_bytes());
        bytes[28..30].copy_from_slice(&self.artifacts.to_be_bytes());
        bytes
    }
}

/// Explicit invocation lifetime within one authority-clock epoch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallLifetime {
    /// A caller-selected absolute deadline.
    Deadline(AuthorityInstant),
    /// Execution continues until completion or explicit cancellation in this epoch.
    UntilCancelled {
        /// Authority-clock epoch binding retained even without elapsed-time expiry.
        epoch: Generation,
    },
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
    lifetime: CallLifetime,
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
        Self::new_with_lifetime(
            action_id,
            name,
            version,
            arguments,
            limits,
            revision,
            CallLifetime::Deadline(deadline),
            idempotency_key,
        )
    }

    /// Creates a call with an explicit epoch-bound execution lifetime.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new_with_lifetime(
        action_id: ActionId,
        name: CapabilityName,
        version: SemanticVersion,
        arguments: BoundedJson,
        limits: CallLimits,
        revision: RevisionTuple,
        lifetime: CallLifetime,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self { action_id, name, version, arguments, limits, revision, lifetime, idempotency_key }
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
    pub const fn deadline(&self) -> Option<AuthorityInstant> {
        match self.lifetime {
            CallLifetime::Deadline(deadline) => Some(deadline),
            CallLifetime::UntilCancelled { .. } => None,
        }
    }
    /// Returns the required authority-clock epoch independently of deadline selection.
    #[must_use]
    pub const fn authority_epoch(&self) -> Generation {
        match self.lifetime {
            CallLifetime::Deadline(deadline) => deadline.epoch(),
            CallLifetime::UntilCancelled { epoch } => epoch,
        }
    }
    /// Returns the explicit execution lifetime without losing its authority epoch.
    #[must_use]
    pub const fn lifetime(&self) -> CallLifetime {
        self.lifetime
    }
    /// Borrows the explicit idempotency identity.
    #[must_use]
    pub const fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }

    /// Returns the stable version-one canonical call envelope bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let version =
            if self.deadline().is_some() && self.limits.timeout_millis().is_some() { 1 } else { 2 };
        let mut bytes = crate::wire::begin_version(2, version);
        bytes.extend_from_slice(self.action_id.as_bytes());
        crate::wire::text(&mut bytes, self.name.as_str());
        crate::wire::u16_value(&mut bytes, self.version.major());
        crate::wire::u16_value(&mut bytes, self.version.minor());
        crate::wire::u16_value(&mut bytes, self.version.patch());
        crate::wire::bytes(&mut bytes, self.arguments.canonical_bytes());
        bytes.extend_from_slice(&self.limits.canonical_bytes());
        crate::wire::revision(&mut bytes, self.revision);
        if let Some(deadline) = self.deadline() {
            if version == 2 {
                bytes.push(1);
            }
            crate::wire::instant(&mut bytes, deadline);
        } else {
            bytes.push(0);
            crate::wire::u64_value(&mut bytes, self.authority_epoch().get());
        }
        crate::wire::text(&mut bytes, self.idempotency_key.as_str());
        bytes
    }
}
