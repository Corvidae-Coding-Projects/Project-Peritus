//! Persisted continuation bindings restored into provider-owned runtime state.

use peritus_model_protocol::Continuation;
use peritus_types::ProviderProfileId;

/// Durable exact-profile binding for a continuation recovered from local state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedContinuation {
    profile_id: ProviderProfileId,
    profile_revision: u64,
    continuation: Continuation,
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
        Ok(Self { profile_id, profile_revision, continuation })
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
}

/// Provider-side result of restoring a persisted continuation into runtime state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContinuationRestoreOutcome {
    /// This adapter cannot prove exact continuation after process-local state was lost.
    Unsupported,
    /// The adapter restored the exact continuation and will accept it on a later request.
    Restored(Continuation),
}
