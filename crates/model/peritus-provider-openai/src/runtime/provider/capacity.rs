//! Route/model-bound numeric capacities, refreshed independently of inference.
use peritus_model_protocol::{ModelName, ProviderProfile};
use peritus_provider_core::{
    CancellationToken, ProviderCoreError,
    catalog::{self, DiscoveredModel},
};
use std::{
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};
#[derive(Default)]
pub(super) struct Cache {
    snapshot: Mutex<Option<Snapshot>>,
}
struct Snapshot {
    fetched: Instant,
    models: Vec<DiscoveredModel>,
}
impl Cache {
    pub async fn prepare(
        &self,
        executable: &Path,
        selected: &ModelName,
        cancellation: &CancellationToken,
    ) -> Result<(), ProviderCoreError> {
        let fresh = self
            .snapshot
            .lock()
            .map_err(|_| catalog::unavailable("context capacity cache is unavailable"))?
            .as_ref()
            .is_some_and(|s| s.fetched.elapsed() < Duration::from_mins(5));
        if !fresh {
            let models = catalog::discover_account_models(
                executable,
                catalog::AccountCatalog::Codex,
                cancellation,
            )
            .await?;
            *self
                .snapshot
                .lock()
                .map_err(|_| catalog::unavailable("context capacity cache is unavailable"))? =
                Some(Snapshot { fetched: Instant::now(), models });
        }
        let model = self.selected(selected)?.ok_or_else(|| {
            catalog::unavailable("selected model context metadata has not been prepared")
        })?;
        if model.usable_context_tokens()?.is_none() {
            return Err(catalog::unavailable(
                "selected model has no fresh advertised context capacity; refresh the official runtime model metadata rather than guessing a window",
            ));
        }
        Ok(())
    }
    pub fn selected(
        &self,
        selected: &ModelName,
    ) -> Result<Option<DiscoveredModel>, ProviderCoreError> {
        let snapshot = self
            .snapshot
            .lock()
            .map_err(|_| catalog::unavailable("context capacity cache is unavailable"))?;
        snapshot
            .as_ref()
            .map(|s| {
                s.models.iter().find(|m| &m.id == selected).cloned().ok_or_else(|| {
                    catalog::unavailable(
                        "the selected model is absent from the current account catalog",
                    )
                })
            })
            .transpose()
    }
    pub fn profile(
        &self,
        profile: &ProviderProfile,
        output: u64,
    ) -> Result<ProviderProfile, ProviderCoreError> {
        self.selected(profile.model())?.map_or_else(
            || Ok(profile.clone()),
            |m| catalog::profile_with_model_limits(profile, &m, output),
        )
    }
    pub fn output(
        &self,
        profile: &ProviderProfile,
        input: u64,
        requested: u64,
    ) -> Result<u64, ProviderCoreError> {
        self.selected(profile.model())?.map_or_else(
            || Ok(requested.min(profile.limits().max_output_tokens())),
            |m| catalog::output_tokens_for_input(profile, &m, input, requested),
        )
    }
}
