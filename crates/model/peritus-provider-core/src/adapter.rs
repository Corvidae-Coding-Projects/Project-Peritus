//! Provider-neutral adapter ownership contract.

use peritus_model_protocol::{ModelRequest, ProviderProfile, ResponseId};

use crate::{
    BoxFuture, CancellationToken, OwnedModelStream, ProviderCoreError, ProviderCoreErrorKind,
};

/// Provider-side result of requesting cancellation for one known response identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseCancellationOutcome {
    /// This provider/profile exposes only local transport cancellation.
    Unsupported,
    /// The provider acknowledged cancellation of the stored response.
    Confirmed {
        /// The provider reported that the response was already terminal/cancelled.
        already_terminal: bool,
    },
}

/// One configured model provider bound to an immutable capability-profile revision.
///
/// Implementations own provider-specific encoding, response parsing, credentials, and transport.
/// Their public boundary remains entirely Peritus-owned.
pub trait ModelProvider: Send + Sync {
    /// Returns the exact profile implemented by this provider instance.
    fn profile(&self) -> &ProviderProfile;

    /// Starts one already-negotiated request and returns its owned normalized event stream.
    ///
    /// Implementations must call [`validate_request_profile`] before encoding or transport. The
    /// supplied cancellation token is owned by the returned stream and may also be cloned by the
    /// transport while an operation is in flight.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe request, configuration, credential, or transport failure. Once
    /// application events are observable, provider failures belong in the normalized event stream.
    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>>;

    /// Requests provider-confirmed cancellation for one stored/background response.
    ///
    /// The default performs no effect and reports unsupported. Implementations may return
    /// `Confirmed` only when the bound profile and provider contract document acknowledgement.
    fn cancel_response<'a>(
        &'a self,
        _response_id: &'a ResponseId,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<ResponseCancellationOutcome, ProviderCoreError>> {
        Box::pin(async { Ok(ResponseCancellationOutcome::Unsupported) })
    }
}

/// Checks that a request still matches the exact immutable profile exposed by an adapter.
///
/// # Errors
///
/// Rejects identity, revision, provider, model, protocol, or wire-dialect drift.
pub fn validate_request_profile(
    profile: &ProviderProfile,
    request: &ModelRequest,
) -> Result<(), ProviderCoreError> {
    let bindings = [
        request.profile_id() == profile.profile_id(),
        request.profile_revision() == profile.revision(),
        request.provider() == profile.provider(),
        request.model() == profile.model(),
        request.protocol() == profile.protocol(),
        request.dialect() == profile.dialect(),
    ];
    if !bindings.into_iter().all(core::convert::identity) {
        return Err(ProviderCoreError::new(
            ProviderCoreErrorKind::InvalidRequest,
            "provider_start",
            "model request does not match the configured provider profile",
        ));
    }
    Ok(())
}
