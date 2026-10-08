//! Persisted continuation bindings restored into provider-owned runtime state.

use peritus_model_protocol::{Continuation, EventEnvelope};
use peritus_types::ProviderProfileId;

/// Durable exact-profile binding for a continuation recovered from local state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedContinuation {
    profile_id: ProviderProfileId,
    profile_revision: u64,
    continuation: Continuation,
    prefix: Vec<EventEnvelope>,
}

impl PersistedContinuation {
    /// Creates a profile-bound persisted continuation.
    ///
    /// # Errors
    ///
    /// Rejects revision zero. The continuation itself is already structurally checked by C5.
    pub fn new(
        profile_id: ProviderProfileId,
        profile_revision: u64,
        continuation: Continuation,
    ) -> Result<Self, crate::ProviderCoreError> {
        if profile_revision == 0 {
            return Err(crate::ProviderCoreError::invalid_request(
                "continuation_restore",
                "persisted continuation profile revision must be nonzero",
            ));
        }
        Ok(Self { profile_id, profile_revision, continuation, prefix: Vec::new() })
    }

    /// Creates a profile-bound continuation with its exact durable normalized prefix.
    ///
    /// Providers that restore private decoder state use the same envelopes that reconstruct the
    /// provider-neutral reducer, keeping both state machines on one acknowledged boundary.
    ///
    /// # Errors
    ///
    /// Rejects revision zero or an empty prefix.
    pub fn with_prefix(
        profile_id: ProviderProfileId,
        profile_revision: u64,
        continuation: Continuation,
        prefix: Vec<EventEnvelope>,
    ) -> Result<Self, crate::ProviderCoreError> {
        if prefix.is_empty() {
            return Err(crate::ProviderCoreError::invalid_request(
                "continuation_restore",
                "persisted exact continuation prefix must be nonempty",
            ));
        }
        let mut persisted = Self::new(profile_id, profile_revision, continuation)?;
        persisted.prefix = prefix;
        Ok(persisted)
    }

    /// Returns the immutable provider-profile identity.
    #[must_use]
    pub const fn profile_id(&self) -> ProviderProfileId {
        self.profile_id
    }

    /// Returns the immutable provider-profile revision.
    #[must_use]
    pub const fn profile_revision(&self) -> u64 {
        self.profile_revision
    }

    /// Borrows the provider-neutral continuation cursor.
    #[must_use]
    pub const fn continuation(&self) -> &Continuation {
        &self.continuation
    }

    /// Borrows the exact durable normalized prefix at the continuation boundary.
    #[must_use]
    pub fn prefix(&self) -> &[EventEnvelope] {
        &self.prefix
    }
}

/// Provider-side result of restoring a persisted continuation into runtime state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContinuationRestoreOutcome {
    /// This adapter cannot prove exact continuation after process-local state was lost.
    Unsupported,
    /// The adapter restored the exact continuation and will accept it on a later request.
    Restored(Continuation),
}
