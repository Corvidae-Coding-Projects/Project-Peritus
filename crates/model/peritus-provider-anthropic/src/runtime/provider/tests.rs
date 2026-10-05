//! Live native persistence qualification, explicitly selected by the operator.

use super::*;

#[test]
#[ignore = "requires an authenticated official Claude runtime selected by PERITUS_TEST_CLAUDE"]
fn live_native_session_survives_provider_reconstruction() {
    let executable = crate::ClaudeExecutable::pin(std::path::PathBuf::from(
        std::env::var_os("PERITUS_TEST_CLAUDE").expect("official runtime path"),
    ))
    .expect("pin");
    let model = peritus_model_protocol::ModelName::new(
        std::env::var("PERITUS_TEST_CLAUDE_MODEL").expect("live model"),
    )
    .expect("model");
    let profile = peritus_provider_core::catalog::selected_profile(
        &crate::test_support::runtime_profile(),
        model,
    )
    .expect("live profile");
    let root = tempfile::tempdir().expect("isolated host task");
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        let request = crate::test_support::runtime_request_with_effort(
            &profile,
            false,
            peritus_model_protocol::ReasoningEffort::Low,
        )
        .with_local_session_directory(root.path().to_path_buf());
        let encoded = super::super::request::encode(&request).unwrap();
        let mut identity = None;
        for _ in 0..2 {
            let provider = ClaudeRuntimeProvider::new(
                ClaudeRuntimeConfig::new(
                    executable.clone(),
                    profile.clone(),
                    ProcessLimits::PRODUCTION,
                )
                .unwrap(),
            );
            let output = provider
                .run_turn(&request, &encoded, &CancellationToken::new())
                .await
                .expect("live native turn");
            assert!(output.exit().success(), "native turn failed");
            let response: serde_json::Value = serde_json::from_slice(output.stdout()).unwrap();
            assert_eq!(response["is_error"], false);
            let current = response["session_id"].as_str().expect("native identity").to_owned();
            if let Some(previous) = &identity {
                assert_eq!(&current, previous);
            } else {
                identity = Some(current);
            }
        }
        println!(
            "verified exact Claude session across reconstructed providers: {}",
            identity.unwrap()
        );
    });
}
