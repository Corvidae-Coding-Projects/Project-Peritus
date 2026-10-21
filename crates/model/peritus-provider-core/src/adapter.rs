//! Provider-neutral adapter ownership contract.

use peritus_model_protocol::{Message, ModelRequest, ProviderProfile, ResponseId, ToolDefinition};

use crate::{
    BoxFuture, CancellationToken, ContinuationRestoreOutcome, OwnedModelStream,
    PersistedContinuation, ProviderAvailability, ProviderCoreError, ProviderCoreErrorKind,
    ProviderRoute,
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
    /// Returns the configured route/capability contract and fallback numeric limits.
    /// Use `request_profile` after metadata preparation for execution token capacities.
    fn profile(&self) -> &ProviderProfile;

    /// Resolves numeric limits for this exact route/model and requested output allowance.
    ///
    /// Call `prepare_context` first. Discovery may replace fallback token capacities, but must
    /// preserve identity, capabilities, lifecycle guarantees, and non-token resource limits.
    /// Providers without model-specific discovery retain their explicitly configured profile.
    ///
    /// # Errors
    /// Rejects contradictory model metadata rather than sending an oversized request.
    fn request_profile(&self, _output_tokens: u64) -> Result<ProviderProfile, ProviderCoreError> {
        Ok(self.profile().clone())
    }

    /// Fits the requested output to model limits and any shared context remaining after input.
    /// The input estimate must include message and tool framing. An uncapped caller uses
    /// `u64::MAX`; this is not a reservation of the model's entire output maximum.
    ///
    /// # Errors
    /// Rejects an input that leaves no output space in an advertised shared context window.
    fn output_tokens_for_input(
        &self,
        _input_tokens: u64,
        requested: u64,
    ) -> Result<u64, ProviderCoreError> {
        Ok(requested.min(self.request_profile(requested)?.limits().max_output_tokens()))
    }

    /// Soft selection target accounting for a transport-specific input envelope.
    /// This does not replace the model's token capacity or impose a global token ceiling.
    fn context_input_target(&self) -> u64 {
        self.request_profile(u64::MAX).map_or_else(
            |_| self.profile().limits().max_input_tokens(),
            |p| p.limits().max_input_tokens(),
        )
    }

    /// Loads required runtime metadata before an initial role context is projected.
    /// The default performs no work. Executable routers may need their advertised model input
    /// allowance before a reviewer chooses which immutable evidence to include inline.
    ///
    /// # Errors
    /// Returns the same cancellation or metadata failure that request startup would report.
    fn prepare_context<'a>(
        &'a self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<(), ProviderCoreError>> {
        Box::pin(async { Ok(()) })
    }

    /// Whether a prospective context fits this adapter's known transport envelope.
    /// The default leaves transport enforcement to adapters without a separate message cap.
    ///
    /// # Errors
    /// Returns an encoding failure without starting a request or invoking credentials.
    fn context_input_fits(
        &self,
        _messages: &[Message],
        _tools: &[ToolDefinition],
    ) -> Result<bool, ProviderCoreError> {
        Ok(true)
    }

    /// Explicit effort bound to this immutable selection; absent retains the caller's default.
    fn reasoning_effort(&self) -> Option<peritus_model_protocol::ReasoningEffort> {
        None
    }

    /// Whether the adapter can encode this effort without substitution.
    ///
    /// This is a transport capability, not a guarantee that every model accepts the value.
    fn supports_reasoning_effort(&self, effort: peritus_model_protocol::ReasoningEffort) -> bool {
        self.profile()
            .capabilities()
            .supports(peritus_model_protocol::Capability::ReasoningControls)
            && matches!(
                effort,
                peritus_model_protocol::ReasoningEffort::Low
                    | peritus_model_protocol::ReasoningEffort::Medium
                    | peritus_model_protocol::ReasoningEffort::High
            )
    }

    /// Queries the configured endpoint or credential-owning runtime, without inference.
    ///
    /// # Errors
    /// Returns an explicit unsupported, authentication, transport, or bounded-data failure.
    fn discover_models<'a>(
        &'a self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Vec<crate::catalog::DiscoveredModel>, ProviderCoreError>> {
        Box::pin(async {
            Err(crate::catalog::unavailable("this provider does not expose model discovery"))
        })
    }

    /// Creates a separate immutable adapter for an explicitly selected model.
    ///
    /// Discovery does not prove capabilities. The configured transport contract is revalidated;
    /// a provider rejection must remain visible rather than triggering a substitute model.
    ///
    /// # Errors
    /// Returns an unsupported or invalid-profile failure.
    fn select_model(
        &self,
        _model: peritus_model_protocol::ModelName,
    ) -> Result<std::sync::Arc<dyn ModelProvider>, ProviderCoreError> {
        Err(crate::catalog::unavailable("this provider does not support model selection"))
    }

    /// Declares whether this provider uses a first-party API, compatible API, or account runtime.
    fn route(&self) -> ProviderRoute {
        ProviderRoute::from_dialect(self.profile().dialect())
    }

    /// Returns the strongest current credential/readiness observation held by this instance.
    ///
    /// Account runtimes remain `Unchecked` until a real canary or successful model turn proves the
    /// session. API adapters may report `CredentialPresent` after resolving their configured
    /// credential source without exposing the credential.
    fn availability(&self) -> ProviderAvailability {
        ProviderAvailability::Unchecked
    }

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

    /// Restores a locally persisted continuation into provider-owned runtime state.
    ///
    /// The default performs no effect and explicitly reports unsupported. An adapter may report
    /// `Restored` only when the persisted binding matches its immutable profile and its provider
    /// contract supports exact cursor retrieval after local process loss.
    fn restore_continuation<'a>(
        &'a self,
        _persisted: &'a PersistedContinuation,
    ) -> BoxFuture<'a, Result<ContinuationRestoreOutcome, ProviderCoreError>> {
        Box::pin(async { Ok(ContinuationRestoreOutcome::Unsupported) })
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
