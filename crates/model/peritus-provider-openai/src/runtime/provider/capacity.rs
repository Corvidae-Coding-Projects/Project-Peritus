//! Route/model-bound numeric capacities, refreshed independently of inference.
use peritus_model_protocol::{ModelName, ProviderProfile};
use peritus_provider_core::{
    CancellationToken, ProviderCoreError, ProviderCoreErrorKind,
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
        profile: &ProviderProfile,
        output: u64,
        cancellation: &CancellationToken,
    ) -> Result<peritus_provider_core::PreparedModelContext, ProviderCoreError> {
        let fresh = self.fresh()?;
        if !fresh {
            let discovered = catalog::discover_account_models(
                executable,
                catalog::AccountCatalog::Codex,
                cancellation,
            )
            .await;
            match discovered {
                Ok(models) => {
                    self.observe(profile.model(), models)?;
                }
                Err(error) if error.kind() == ProviderCoreErrorKind::Cancelled => {
                    return Err(error);
                }
                Err(_) => {
                    // Discovery is advisory for an explicit route and model. Keep a previous
                    // exact-model observation, or the conservative configured envelope below.
                }
            }
        }
        self.prepared(profile, output)
    }

    fn fresh(&self) -> Result<bool, ProviderCoreError> {
        Ok(self
            .snapshot
            .lock()
            .map_err(|_| catalog::unavailable("context capacity cache is unavailable"))?
            .as_ref()
            .is_some_and(|snapshot| snapshot.fetched.elapsed() < Duration::from_mins(5)))
    }

    fn observe(
        &self,
        selected: &ModelName,
        models: Vec<DiscoveredModel>,
    ) -> Result<bool, ProviderCoreError> {
        let mut matching = models.iter().filter(|model| &model.id == selected);
        let usable = matching
            .next()
            .is_some_and(|model| model.usable_context_tokens().is_ok_and(|value| value.is_some()))
            && matching.next().is_none();
        if !usable {
            return Ok(false);
        }
        *self
            .snapshot
            .lock()
            .map_err(|_| catalog::unavailable("context capacity cache is unavailable"))? =
            Some(Snapshot { fetched: Instant::now(), models });
        Ok(true)
    }

    fn prepared(
        &self,
        profile: &ProviderProfile,
        output: u64,
    ) -> Result<peritus_provider_core::PreparedModelContext, ProviderCoreError> {
        self.selected(profile.model())?.map_or_else(
            || Ok(peritus_provider_core::PreparedModelContext::configured(profile.clone())),
            |model| {
                peritus_provider_core::PreparedModelContext::advertised(profile, &model, output)
            },
        )
    }

    pub fn selected(
        &self,
        selected: &ModelName,
    ) -> Result<Option<DiscoveredModel>, ProviderCoreError> {
        let snapshot = self
            .snapshot
            .lock()
            .map_err(|_| catalog::unavailable("context capacity cache is unavailable"))?;
        Ok(snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.models.iter().find(|model| &model.id == selected))
            .cloned())
    }
}

#[cfg(test)]
#[path = "../../../../../../discovery/proptest-praxis/owner/openai_capacity.rs"]
mod replay;
