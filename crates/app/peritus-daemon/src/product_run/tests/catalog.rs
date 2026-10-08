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
    pause: AtomicBool,
    started: tokio::sync::Notify,
    resume: tokio::sync::Notify,
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
            if self.pause.load(Ordering::SeqCst) {
                self.started.notify_one();
                self.resume.notified().await;
            }
            if self.fail.load(Ordering::SeqCst) {
                return Err(ProviderCoreError::configuration(
                    "fixture",
                    "provider detail must not escape",
                ));
            }
            let mut model =
                DiscoveredModel::new("new-advertised-model".to_owned(), "Advertised".to_owned())?;
            model.tools = Some(false);
            Ok(vec![model])
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
        pause: AtomicBool::new(false),
        started: tokio::sync::Notify::new(),
        resume: tokio::sync::Notify::new(),
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
    let advertised = ProductModelChoice::new("new-advertised-model".to_owned(), false)
        .expect("advertised choice");
    let advertised_options = super::super::interaction::InteractionOptions::test(
        ProductInteractionMode::Chat,
        ProductRoleModels::new(advertised.clone(), advertised.clone(), advertised),
    );
    assert!(service.validate_models(selected, &advertised_options.models).await.is_ok());
    let choice = ProductModelChoice::new("not-advertised".to_owned(), false).expect("choice");
    let options = super::super::interaction::InteractionOptions::test(
        ProductInteractionMode::Chat,
        ProductRoleModels::new(
            choice,
            ProductModelChoice::default(),
            ProductModelChoice::default(),
        ),
    );
    assert!(service.validate_models(selected, &options.models).await.is_err());
    service.shutdown().await.expect("shutdown product runs");
}

#[test]
fn slow_discovery_does_not_block_cached_models_or_another_provider() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().unwrap();
        let writer = scripted(0x61, "writer", Vec::new());
        let reviewer = scripted(0x62, "reviewer", Vec::new());
        let provider = |inner: &Arc<ScriptedProvider>| {
            Arc::new(CatalogProvider {
                inner: Arc::clone(inner),
                calls: AtomicU32::new(0),
                fail: AtomicBool::new(false),
                pause: AtomicBool::new(false),
                started: tokio::sync::Notify::new(),
                resume: tokio::sync::Notify::new(),
            })
        };
        let slow = provider(&writer);
        let fast = provider(&reviewer);
        let slow_id = slow.profile().profile_id();
        let fast_id = fast.profile().profile_id();
        let mut service = service(
            state.path(),
            repository.path(),
            WorkspaceId::new([0x63; 16]).unwrap(),
            [&writer, &reviewer, &writer],
        );
        let inner = Arc::get_mut(&mut service.inner).unwrap();
        inner.providers.insert(slow_id, slow.clone());
        inner.providers.insert(fast_id, fast.clone());
        service.query_models(ProductModelQuery::new(slow_id, false)).await.unwrap();
        slow.pause.store(true, Ordering::SeqCst);
        let background = service.clone();
        let refresh = tokio::spawn(async move {
            background.query_models(ProductModelQuery::new(slow_id, true)).await
        });
        tokio::time::timeout(Duration::from_secs(1), slow.started.notified()).await.unwrap();
        let available = tokio::time::timeout(Duration::from_secs(1), async {
            let cached =
                service.query_models(ProductModelQuery::new(slow_id, false)).await.unwrap();
            let fresh = service.query_models(ProductModelQuery::new(fast_id, false)).await.unwrap();
            let busy = service.query_models(ProductModelQuery::new(slow_id, true)).await.unwrap();
            (cached, fresh, busy)
        })
        .await;
        slow.resume.notify_one();
        refresh.await.unwrap().unwrap();
        service.shutdown().await.expect("shutdown product runs");
        let (cached, fresh, busy) = available.expect("independent model catalogs remain available");
        assert!(cached.cached());
        assert!(!fresh.cached());
        assert!(busy.cached());
        assert!(!busy.error().is_empty());
        assert_eq!(slow.calls.load(Ordering::SeqCst), 2, "refreshes cannot pile up");
        assert_eq!(fast.calls.load(Ordering::SeqCst), 1);
    });
}
