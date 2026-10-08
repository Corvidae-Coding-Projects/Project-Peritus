//! Immutable effort selection without changing credentials, routing, or in-flight requests.

use std::sync::Arc;

use peritus_model_protocol::{
    Capability, ModelName, ModelRequest, NegotiatedCapabilities, ProviderProfile, ReasoningEffort,
    ReasoningPolicy, ResponseId, SummaryPolicy,
};

use crate::{
    BoxFuture, CancellationToken, ContinuationRestoreOutcome, ModelProvider, OwnedModelStream,
    PersistedContinuation, ProviderAvailability, ProviderCoreError, ProviderCoreErrorKind,
    ProviderRoute, ResponseCancellationOutcome, catalog::DiscoveredModel,
};

/// Selects an exact effort on a separate adapter owned by one logical model turn.
///
/// # Errors
/// Rejects efforts the adapter cannot encode; never maps them to a nearby level.
pub fn select_reasoning_effort(
    provider: Arc<dyn ModelProvider>,
    effort: ReasoningEffort,
) -> Result<Arc<dyn ModelProvider>, ProviderCoreError> {
    if !provider.supports_reasoning_effort(effort) {
        return Err(ProviderCoreError::new(
            ProviderCoreErrorKind::InvalidRequest,
            "select_effort",
            "the selected provider cannot encode this reasoning effort",
        ));
    }
    Ok(Arc::new(EffortSelection { provider, effort }))
}

pub(crate) fn qualification_reasoning_policy(
    provider: &dyn ModelProvider,
    negotiated: NegotiatedCapabilities,
    unselected: ReasoningPolicy,
) -> Result<ReasoningPolicy, ProviderCoreError> {
    let Some(effort) = provider.reasoning_effort() else { return Ok(unselected) };
    if !negotiated.includes(Capability::ReasoningControls)
        || !provider.supports_reasoning_effort(effort)
    {
        return Err(ProviderCoreError::unsupported_capability(
            "the selected reasoning effort is not supported by the qualified provider profile",
        ));
    }
    Ok(ReasoningPolicy::Effort { effort, summary: SummaryPolicy::None })
}

struct EffortSelection {
    provider: Arc<dyn ModelProvider>,
    effort: ReasoningEffort,
}

impl ModelProvider for EffortSelection {
    fn profile(&self) -> &ProviderProfile {
        self.provider.profile()
    }

    fn validate_request(&self, request: &ModelRequest) -> Result<(), ProviderCoreError> {
        self.provider.validate_request(request)?;
        if !matches!(request.options().reasoning(),
            ReasoningPolicy::Effort { effort, .. } if effort == self.effort)
        {
            return Err(ProviderCoreError::new(
                ProviderCoreErrorKind::InvalidRequest,
                "selected_effort",
                "request did not retain the explicitly selected reasoning effort",
            ));
        }
        Ok(())
    }

    fn reasoning_effort(&self) -> Option<ReasoningEffort> {
        Some(self.effort)
    }

    fn supports_reasoning_effort(&self, effort: ReasoningEffort) -> bool {
        self.provider.supports_reasoning_effort(effort)
    }

    fn discover_models<'a>(
        &'a self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Vec<DiscoveredModel>, ProviderCoreError>> {
        self.provider.discover_models(cancellation)
    }

    fn select_model(&self, model: ModelName) -> Result<Arc<dyn ModelProvider>, ProviderCoreError> {
        select_reasoning_effort(self.provider.select_model(model)?, self.effort)
    }

    fn route(&self) -> ProviderRoute {
        self.provider.route()
    }

    fn availability(&self) -> ProviderAvailability {
        self.provider.availability()
    }

    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        if let Err(error) = self.validate_request(&request) {
            return Box::pin(async move { Err(error) });
        }
        self.provider.start(request, cancellation)
    }

    fn cancel_response<'a>(
        &'a self,
        response: &'a ResponseId,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<ResponseCancellationOutcome, ProviderCoreError>> {
        self.provider.cancel_response(response, cancellation)
    }

    fn restore_continuation<'a>(
        &'a self,
        persisted: &'a PersistedContinuation,
    ) -> BoxFuture<'a, Result<ContinuationRestoreOutcome, ProviderCoreError>> {
        self.provider.restore_continuation(persisted)
    }
}
