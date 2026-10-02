//! Malformed model-report recovery with independently retained host facts.

use super::*;

#[test]
#[allow(clippy::too_many_lines, reason = "one complete malformed-terminal composition fixture")]
fn malformed_terminal_retains_host_changes_and_verification_without_claiming_acceptance() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let repository = tempfile::tempdir().expect("repository");
            let state = tempfile::tempdir().expect("state directory");
            fs::create_dir_all(repository.path().join("src")).expect("source directory");
            fs::write(
                repository.path().join("Cargo.toml"),
                "[package]\nname = \"malformed-terminal-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            )
            .expect("manifest");
            fs::write(repository.path().join("src/lib.rs"), "pub fn before() -> u32 { 1 }\n")
                .expect("initial source");
            git(repository.path(), &["init", "--quiet"]);
            git(repository.path(), &["config", "user.name", "Peritus Test"]);
            git(repository.path(), &["config", "user.email", "peritus@example.invalid"]);
            git(repository.path(), &["config", "commit.gpgsign", "false"]);
            cargo(repository.path(), &["generate-lockfile"]);
            git(repository.path(), &["add", "."]);
            git(repository.path(), &["commit", "--quiet", "-m", "initial"]);

            let implemented = "pub fn answer() -> u32 { 42 }\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn answer_is_42() { assert_eq!(super::answer(), 42); }\n}\n";
            let writer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0x91; 16], "malformed-terminal-writer"),
                responses: Mutex::new(VecDeque::from([
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    design_response(),
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    tool_response(write_arguments("src/lib.rs", implemented)),
                    named_tool_response(
                        "run_command",
                        command_arguments("cargo", &["test", "--quiet"], "verification"),
                    ),
                    text_response(b"implemented and verified, but omitted the terminal object"),
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    text_response(b"still malformed"),
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    text_response(b"still malformed again"),
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    text_response(b"final malformed report"),
                ])),
            });
            let run = RunId::new([0x92; 16]).expect("run");
            let task = "Add and verify an answer function that returns 42.".to_owned();
            let outcome = ProductRunner::run(
                ProductRunInput {
                    workspace_kind: peritus_product_runner::ProductWorkspaceKind::Managed,
                    run_id: run,
                    workspace_id: WorkspaceId::new([0x93; 16]).expect("workspace"),
                    workspace_root: repository.path().to_owned(),
                    trace_path: state.path().join("malformed-terminal.trace"),
                    command_runtime: support::command_runtime(
                        state.path(),
                        repository.path(),
                        run,
                    ),
                    finding_state: String::new(),
                    task: task.clone(),
                    max_elapsed: Some(PRODUCT_RUN_MAX_ELAPSED),
                    delivery_scope: ProductDeliveryScope::WorkspaceChanges,
                    conversation: Arc::new(FixedConversation(task)),
                    providers: RoleProviders {
                        writer: Arc::clone(&writer),
                        reviewer: Arc::clone(&writer),
                        fixer: writer,
                        fallbacks: Vec::new(),
                    },
                    cancelled: Arc::new(AtomicBool::new(false)),
                    provider_cancellation: CancellationToken::new(),
                    resume: None,
                },
                Arc::new(|_| {}),
            )
            .await
            .expect("settled malformed terminal");

            assert_eq!(outcome.settlement().cause(), SettlementCause::Adapter);
            assert_eq!(outcome.settlement().disposition(), RunDisposition::CandidateAvailable);
            assert_eq!(
                outcome.settlement().checkpoint().expect("candidate checkpoint").stage(),
                CandidateStage::SelfChecked,
            );
            assert!(!outcome.settlement().is_accepted());
            let candidate = outcome.candidate().expect("retained candidate");
            assert_eq!(candidate.changed_paths, vec![Path::new("src/lib.rs").to_owned()]);
            assert!(candidate.diff.contains("answer"));
            assert!(
                candidate
                    .successful_commands
                    .iter()
                    .any(|command| command.contains("cargo") && command.contains("verification"))
            );
            assert!(candidate.gates.is_empty());
            assert!(candidate.review.is_empty());
            assert!(candidate.summary.contains("retained host-observed work"));
            assert!(outcome.detail().is_some_and(|detail| detail.contains("developer terminal")));
            assert!(
                outcome
                    .remaining_work()
                    .iter()
                    .any(|work| work.contains("valid terminal report"))
            );
            assert!(outcome.resume().is_some());
        });
}
