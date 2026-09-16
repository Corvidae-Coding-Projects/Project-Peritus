//! Failure before a first provider response still publishes the exact empty trace.

use super::*;

#[test]
fn provider_failure_before_first_response_retains_an_empty_trace() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime").block_on(
        async {
            let repository = tempfile::tempdir().expect("repository");
            let state = tempfile::tempdir().expect("state directory");
            let trace_path = state.path().join("nested/product.trace");
            fs::write(repository.path().join("README.md"), "# Fixture\n").expect("repository file");
            git(repository.path(), &["init", "--quiet"]);
            git(repository.path(), &["config", "user.name", "Peritus Test"]);
            git(repository.path(), &["config", "user.email", "peritus@example.invalid"]);
            git(repository.path(), &["config", "commit.gpgsign", "false"]);
            git(repository.path(), &["add", "."]);
            git(repository.path(), &["commit", "--quiet", "-m", "initial"]);

            let provider: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0x71; 16], "fails-before-response"),
                responses: Mutex::new(VecDeque::new()),
            });
            let task = "Document the fixture.".to_owned();
            let run_id = RunId::new([0x72; 16]).expect("run ID");
            let command_runtime = support::command_runtime(state.path(), repository.path(), run_id);
            let outcome = ProductRunner::run(
                ProductRunInput {
                    workspace_kind: peritus_product_runner::ProductWorkspaceKind::Managed,
                    run_id,
                    workspace_id: WorkspaceId::new([0x73; 16]).expect("workspace ID"),
                    workspace_root: repository.path().to_owned(),
                    trace_path: trace_path.clone(),
                    command_runtime,
                    finding_state: String::new(),
                    task: task.clone(),
                    max_elapsed: PRODUCT_RUN_MAX_ELAPSED,
                    delivery_scope: ProductDeliveryScope::WorkspaceChanges,
                    conversation: Arc::new(FixedConversation(task)),
                    providers: RoleProviders {
                        writer: Arc::clone(&provider),
                        reviewer: Arc::clone(&provider),
                        fixer: provider,
                        fallbacks: Vec::new(),
                    },
                    cancelled: Arc::new(AtomicBool::new(false)),
                    provider_cancellation: CancellationToken::new(),
                    resume: None,
                },
                Arc::new(|_| {}),
            )
            .await
            .expect("provider exhaustion is an ordinary terminal settlement");

            assert_eq!(outcome.settlement().disposition(), RunDisposition::FailedNoCandidate);
            assert_eq!(outcome.settlement().cause(), SettlementCause::Provider);
            assert!(outcome.candidate().is_none());
            assert!(trace_path.is_file());
            assert_eq!(fs::metadata(trace_path).expect("trace metadata").len(), 0);
        },
    );
}
