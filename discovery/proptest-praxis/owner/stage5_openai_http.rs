//! Private test child of `OpenAiConfig`, using its existing private endpoint constructor only.

#[path = "../src/stage5/providers/common.rs"]
mod common;
#[path = "../src/fingerprint.rs"]
mod praxis_fingerprint;
#[path = "../support/stage5_owner.rs"]
mod runner;

use common::{Case, Metrics, Mode};
use peritus_provider_core::ModelProvider;
use proptest::strategy::BoxedStrategy;
use std::sync::Arc;

fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    common::strategy(&[Mode::OpenAi], max_steps)
}
fn examples() -> Vec<Case> {
    common::examples(&[Mode::OpenAi])
}
fn calibration() -> Case {
    common::example(Mode::OpenAi)
}
fn check(case: &Case) -> Result<Metrics, String> {
    if case.mode != Mode::OpenAi {
        return Err("foreign owner-provider mode".into());
    }
    let mut metrics = if let Some(control) = common::positive_control(case) {
        let mut metrics = check(&control)?;
        common::bump(&mut metrics, "paired_valid_transport_controls");
        metrics
    } else {
        Metrics::new()
    };
    let server = common::Server::new(case)?;
    let config = super::OpenAiConfig::from_endpoint(server.endpoint.clone(), common::credential()?)
        .map_err(|e| e.to_string())?
        .with_retry_policy(common::retry_policy()?);
    let provider: Arc<dyn ModelProvider> = Arc::new(
        crate::OpenAiProvider::new(
            config,
            common::profile(Mode::OpenAi)?,
            Arc::new(common::SyntheticCredential),
        )
        .map_err(|e| e.to_string())?,
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(common::drive(provider, case, &mut metrics))?;
    common::inspect_http(case, &server.finish()?, &mut metrics)?;
    Ok(metrics)
}

#[test]
fn praxis_stage5_openai_http() {
    runner::run("openai_http", strategy, check, examples, calibration);
}
