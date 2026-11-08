//! One immutable capacity decision for a complete model turn.
use crate::{
    ProviderCoreError,
    catalog::{self, DiscoveredModel},
};
use peritus_model_protocol::ProviderProfile;

/// Prepared numeric limits for one exact provider/model selection.
///
/// Metadata loading and profile resolution finish before this value is returned. Output
/// fitting uses the same snapshot, even if a shared model catalog is refreshed later.
#[derive(Clone, Debug)]
pub struct PreparedModelContext {
    profile: ProviderProfile,
    shared_tokens: Option<u64>,
}

impl PreparedModelContext {
    /// Retains an explicitly configured profile when the route has no numeric discovery.
    #[must_use]
    pub const fn configured(profile: ProviderProfile) -> Self {
        Self { profile, shared_tokens: None }
    }

    /// Resolves advertised input, reply, and shared-window bounds together.
    ///
    /// # Errors
    /// Rejects metadata for another model or invalid/contradictory numeric limits.
    pub fn advertised(
        profile: &ProviderProfile,
        model: &DiscoveredModel,
        requested_output: u64,
    ) -> Result<Self, ProviderCoreError> {
        if profile.model() != &model.id {
            return Err(catalog::unavailable("context metadata belongs to another selected model"));
        }
        Ok(Self {
            profile: catalog::profile_with_model_limits(profile, model, requested_output)?,
            shared_tokens: model.usable_context_tokens()?,
        })
    }

    /// Exact resolved profile; identity, capabilities and lifecycle policy are preserved.
    #[must_use]
    pub const fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    /// Fits reply space using this turn's snapshot, without consulting mutable metadata.
    ///
    /// # Errors
    /// Rejects a zero allowance or an input that leaves no shared-window reply space.
    pub fn output_tokens_for_input(
        &self,
        input: u64,
        requested: u64,
    ) -> Result<u64, ProviderCoreError> {
        let mut output = requested.min(self.profile.limits().max_output_tokens());
        if let Some(shared) = self.shared_tokens {
            output = output.min(shared.saturating_sub(input));
        }
        if output == 0 {
            return Err(catalog::unavailable("model context leaves no output capacity"));
        }
        Ok(output)
    }
}
