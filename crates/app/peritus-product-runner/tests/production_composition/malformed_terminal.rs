//! Malformed model-report recovery without a fixed retry ceiling.

use super::*;

#[test]
#[allow(clippy::too_many_lines, reason = "one complete malformed-terminal composition fixture")]
fn malformed_terminal_retries_past_the_old_ceiling_and_reaches_acceptance() {
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

            let implemented = "pub const fn answer() -> u32 {\n    42\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn answer_is_42() {\n        assert_eq!(super::answer(), 42);\n    }\n}\n";
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
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    text_response(
                        br#"{"kind":"complete","run_instructions":"cargo test","summary":"Added and verified the answer function after correcting malformed terminal reports."}"#,
                    ),
                    named_tool_response("workspace_list", list_arguments("", 2)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    text_response(
                        br#"{"findings":[],"summary":"The answer function and focused test satisfy the request."}"#,
                    ),
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
                    max_elapsed: Some(std::time::Duration::from_hours(8)),
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
            .expect("run recovers from malformed terminals");

            assert!(outcome.settlement().is_accepted(), "{:?}: {:?}", outcome.settlement(), outcome.detail());
            assert_eq!(outcome.settlement().cause(), SettlementCause::Completed);
            assert_eq!(outcome.settlement().disposition(), RunDisposition::Accepted);
            assert_eq!(
                outcome.settlement().checkpoint().expect("accepted checkpoint").stage(),
                CandidateStage::Qualified,
            );
            let candidate = outcome.candidate().expect("accepted candidate");
            assert_eq!(candidate.changed_paths, vec![Path::new("src/lib.rs").to_owned()]);
            assert!(candidate.diff.contains("answer"));
            assert!(
                candidate
                    .successful_commands
                    .iter()
                    .any(|command| command.starts_with("cargo test "))
            );
            assert!(candidate.gates.contains("Exact-target acceptance: PASS"));
            assert!(candidate.review.contains("No findings"));
            assert!(candidate.summary.contains("correcting malformed terminal reports"));
            assert_eq!(candidate.run_instructions, "cargo test");
            assert!(outcome.detail().is_none());
            assert!(outcome.remaining_work().is_empty());
        });
}
