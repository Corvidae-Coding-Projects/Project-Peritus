use super::*;
use crate::tests::runtime_support::{codex_profile, codex_tool_request_with_effort};
use peritus_model_protocol::ReasoningEffort as Effort;
use peritus_provider_core::{ProcessExit, ProcessOutput};
use std::sync::Mutex;

struct Capture(Mutex<Vec<String>>);
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
            drop(args);
        }
    });
}
