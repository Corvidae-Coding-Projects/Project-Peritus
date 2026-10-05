use super::*;
use crate::tests::runtime_support::{codex_profile, codex_tool_request_with_effort};
use peritus_model_protocol::ReasoningEffort as Effort;
use peritus_provider_core::{ProcessExit, ProcessOutput};
use std::sync::Mutex;

struct Capture(Mutex<Vec<String>>);

#[test]
#[ignore = "requires an authenticated official Codex runtime selected by PERITUS_TEST_CODEX"]
fn live_native_thread_survives_provider_reconstruction() {
    let executable = crate::CodexExecutable::pin(std::path::PathBuf::from(
        std::env::var_os("PERITUS_TEST_CODEX").expect("official runtime path"),
    ))
    .expect("pin");
    let model = std::env::var("PERITUS_TEST_CODEX_MODEL").expect("live model");
    let profile = codex_profile(&model, true);
    let root = tempfile::tempdir().expect("isolated host task");
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        let request = codex_tool_request_with_effort(&profile, "live-native-1", Effort::Low)
            .with_local_session_directory(root.path().to_path_buf());
        let config =
            CodexRuntimeConfig::new(executable.clone(), profile.clone(), ProcessLimits::PRODUCTION)
                .unwrap();
        let encoded = super::super::super::request::encode(&request).unwrap();
        let first = run_turn(
            &config,
            &peritus_provider_core::TokioProcessTransport,
            &request,
            &encoded,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert!(first.process.exit().success(), "first runtime failed");
        assert!(first.final_message.is_ok());
        let thread = sessions::Session::open(&request)
            .unwrap()
            .thread()
            .expect("retained thread")
            .to_owned();
        // Recreate both configuration and transport, as after daemon restart.
        let next_config =
            CodexRuntimeConfig::new(executable, profile.clone(), ProcessLimits::PRODUCTION)
                .unwrap();
        let next_request = codex_tool_request_with_effort(&profile, "live-native-2", Effort::Low)
            .with_local_session_directory(root.path().to_path_buf());
        let encoded = super::super::super::request::encode(&next_request).unwrap();
        let second = run_turn(
            &next_config,
            &peritus_provider_core::TokioProcessTransport,
            &next_request,
            &encoded,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert!(second.process.exit().success(), "resumed runtime failed");
        assert!(second.final_message.is_ok());
        assert_eq!(sessions::Session::open(&next_request).unwrap().thread(), Some(thread.as_str()));
        println!("verified exact native thread across reconstructed providers: {thread}");
    });
}
impl ProcessTransport for Capture {
    fn run<'a>(
        &'a self,
        request: ProcessRequest,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<ProcessOutput, ProviderCoreError>> {
        *self.0.lock().expect("capture") = request.arguments().to_vec();
        Box::pin(async move {
            ProcessOutput::new(
                ProcessExit::new(true, Some(0)),
                Vec::new(),
                Vec::new(),
                request.limits(),
            )
        })
    }
}

#[test]
fn selected_effort_reaches_the_owned_codex_process_unchanged() {
    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    runtime.block_on(async {
        for effort in [
            Effort::Minimal,
            Effort::Low,
            Effort::Medium,
            Effort::High,
            Effort::XHigh,
            Effort::Max,
            Effort::Ultra,
        ] {
            let profile = codex_profile("fixture-model", true);
            let executable =
                crate::CodexExecutable::pin(std::env::current_exe().expect("executable"))
                    .expect("pin");
            let config =
                CodexRuntimeConfig::new(executable, profile.clone(), ProcessLimits::PRODUCTION)
                    .expect("config");
            let request = codex_tool_request_with_effort(&profile, "effort-request", effort);
            let encoded = super::super::super::request::encode(&request).expect("encode");
            assert_eq!(encoded.reasoning_effort(), effort.as_str());
            let capture = Capture(Mutex::new(Vec::new()));
            run_turn(&config, &capture, &request, &encoded, &CancellationToken::new())
                .await
                .expect("invocation");
            let args = capture.0.lock().expect("capture");
            assert!(
                args.iter()
                    .any(|arg| arg == &format!("model_reasoning_effort=\"{}\"", effort.as_str()))
            );
            assert!(args.iter().any(|arg| arg == "--skip-git-repo-check"));
            assert!(!args.iter().any(|arg| arg == "--ephemeral"));
            for feature in DISABLED_NATIVE_FEATURES {
                assert!(args.windows(2).any(|pair| pair == ["--disable", feature]));
            }
            for setting in [
                "tools.update_plan.enabled=false",
                "tools.experimental_request_user_input.enabled=false",
                "web_search=\"disabled\"",
            ] {
                assert!(args.windows(2).any(|pair| pair == ["--config", setting]));
            }
            drop(args);
        }
    });
}
