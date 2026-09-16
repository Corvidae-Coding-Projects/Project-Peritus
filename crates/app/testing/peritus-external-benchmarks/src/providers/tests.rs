//! Provider-source selection and role capabilities retain their declared contracts.

use peritus_model_protocol::ModelRequest;
use peritus_provider_core::{BoxFuture, OwnedModelStream, ProviderCoreError};

use super::*;

struct StubProvider(ProviderProfile);

impl ModelProvider for StubProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.0
    }

    fn start(
        &self,
        _request: ModelRequest,
        _cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async {
            Err(ProviderCoreError::configuration(
                "benchmark_provider_test",
                "provider invocation is outside this composition test",
            ))
        })
    }
}

#[test]
fn provider_source_requires_an_explicit_supported_value() {
    assert_eq!(ProviderSource::parse(None).unwrap(), ProviderSource::AccountRuntimes);
    assert_eq!(
        ProviderSource::parse(Some("account-runtimes")).unwrap(),
        ProviderSource::AccountRuntimes
    );
    assert_eq!(ProviderSource::parse(Some("configured")).unwrap(), ProviderSource::Configured);
    assert!(ProviderSource::parse(Some("automatic")).is_err());
}

#[test]
fn account_runtime_roles_retain_the_explicit_fallback_candidates() {
    let writer: Arc<dyn ModelProvider> = Arc::new(StubProvider(
        profile([0xC1; 16], "openai", "writer", WireDialect::OpenAiCodexRuntime)
            .expect("writer profile"),
    ));
    let reviewer: Arc<dyn ModelProvider> = Arc::new(StubProvider(
        profile([0xC2; 16], "anthropic", "reviewer", WireDialect::AnthropicClaudeRuntime)
            .expect("reviewer profile"),
    ));
    let writer_id = writer.profile().profile_id();
    let reviewer_id = reviewer.profile().profile_id();
    let plan = ProviderPlan {
        writer: Arc::clone(&writer),
        reviewer: Arc::clone(&reviewer),
        fixer: Arc::clone(&writer),
        fallbacks: vec![writer, reviewer],
    };

    assert_eq!(plan.writer.profile().profile_id(), writer_id);
    assert_eq!(plan.fixer.profile().profile_id(), writer_id);
    assert_eq!(plan.reviewer.profile().profile_id(), reviewer_id);
    assert!(plan.writer.profile().capabilities().supports(Capability::ImageInput));
    assert!(!plan.reviewer.profile().capabilities().supports(Capability::ImageInput));
    assert_eq!(
        plan.fallbacks.iter().map(|provider| provider.profile().profile_id()).collect::<Vec<_>>(),
        vec![writer_id, reviewer_id],
    );
}
