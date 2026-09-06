use super::*;
use peritus_app_protocol::{
    ProductInteractionMode, ProductModelChoice, ProductModelQuery, ProductRoleModels,
};
use peritus_model_protocol::{ModelRequest, ProviderProfile};
use peritus_provider_core::{
    BoxFuture, CancellationToken, OwnedModelStream, ProviderCoreError, catalog::DiscoveredModel,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

struct CatalogProvider {
    inner: Arc<ScriptedProvider>,
    calls: AtomicU32,
    fail: AtomicBool,
}
impl ModelProvider for CatalogProvider {
    fn profile(&self) -> &ProviderProfile {
        self.inner.profile()
    }
    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        self.inner.start(request, cancellation)
    }
    fn discover_models<'a>(
        &'a self,
        _: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Vec<DiscoveredModel>, ProviderCoreError>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail.load(Ordering::SeqCst) {
                return Err(ProviderCoreError::configuration(
                    "fixture",
                    "provider detail must not escape",
                ));
            }
            Ok(vec![DiscoveredModel::new(
                "new-advertised-model".to_owned(),
                "Advertised".to_owned(),
            )?])
        })
    }
}

#[test]
fn discovery_cache_retains_provenance_and_failed_refresh_never_becomes_fresh() {
    interaction::block_on(cache_scenario());
}
async fn cache_scenario() {
    let repository = repository();
    let state = tempfile::tempdir().expect("state");
    let writer = scripted(0x51, "writer", Vec::new());
    let profile = writer.profile.profile_id();
    let provider = Arc::new(CatalogProvider {
        inner: Arc::clone(&writer),
        calls: AtomicU32::new(0),
        fail: AtomicBool::new(false),
    });
    let mut service = service(
        state.path(),
        repository.path(),
        WorkspaceId::new([0x52; 16]).expect("workspace"),
        [&writer, &writer, &writer],
    );
    Arc::get_mut(&mut service.inner)
        .expect("exclusive fixture")
        .providers
        .insert(profile, provider.clone());
    let fresh = service.query_models(ProductModelQuery::new(profile, false)).await.expect("fresh");
    assert!(!fresh.cached());
    assert_eq!(fresh.models()[0].id(), "new-advertised-model");
    let cached = service.query_models(ProductModelQuery::new(profile, false)).await.expect("cache");
    assert!(cached.cached());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    provider.fail.store(true, Ordering::SeqCst);
    let failed = service
        .query_models(ProductModelQuery::new(profile, true))
        .await
        .expect("failed refresh projection");
    assert!(failed.cached());
    assert_eq!(failed.fetched_unix_seconds(), fresh.fetched_unix_seconds());
    assert_eq!(failed.models(), fresh.models());
    assert!(!failed.error().is_empty());
    assert!(!failed.error().contains("provider detail must not escape"));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    let selected = ProductProviderSelection::new(profile, profile, profile);
    let choice = ProductModelChoice::new("not-advertised".to_owned(), false).expect("choice");
    let options = super::super::interaction::InteractionOptions::new(
        ProductInteractionMode::Chat,
        ProductRoleModels::new(
            choice,
            ProductModelChoice::default(),
            ProductModelChoice::default(),
        ),
    );
    assert!(service.validate_models(selected, &options).await.is_err());
    service.shutdown(Duration::from_secs(1)).await;
}
