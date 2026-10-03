//! Persistent findings and unchanged fixes remain actionable until fresh acceptance.

use super::*;

#[test]
#[allow(clippy::too_many_lines, reason = "one complete finding-conservation fixture")]
fn unchanged_fixes_and_repeated_findings_continue_until_fresh_review_confirms_resolution() {
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
                "[package]\nname = \"fixer-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            )
            .expect("manifest");
            fs::write(
                repository.path().join("src/lib.rs"),
                "pub const fn initial() -> bool { true }\n",
            )
            .expect("initial source");
            git(repository.path(), &["init", "--quiet"]);
            git(repository.path(), &["config", "user.name", "Peritus Test"]);
            git(
                repository.path(),
                &["config", "user.email", "peritus@example.invalid"],
            );
            git(repository.path(), &["config", "commit.gpgsign", "false"]);
            cargo(repository.path(), &["generate-lockfile"]);
            git(repository.path(), &["add", "."]);
            git(repository.path(), &["commit", "--quiet", "-m", "initial"]);

            let initial = r"/// Returns the fixture answer.
#[must_use]
pub const fn answer() -> u32 {
    41
}

#[cfg(test)]
mod tests {
    #[test]
    fn answer_matches_implementation() {
        assert_eq!(super::answer(), 41);
    }
}
";
            let writer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0x91; 16], "writer-with-finding"),
                responses: Mutex::new(VecDeque::from([
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("Cargo.toml")),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    design_response(),
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    tool_response(write_arguments("src/lib.rs", initial)),
                    text_response(
                        br#"{"kind":"complete","run_instructions":"cargo test","summary":"Added an answer API and test."}"#,
                    ),
                ])),
            });
            let mut fixer_responses = VecDeque::new();
            for _ in 0..3 {
                fixer_responses.extend([
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    text_response(
                        br#"{"kind":"complete","run_instructions":"cargo test","summary":"Rechecked the current answer without changing it."}"#,
                    ),
                ]);
            }
            fixer_responses.extend([
                named_tool_response("workspace_list", list_arguments("", 3)),
                named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                named_tool_response(
                    "workspace_patch",
                    patch_arguments("src/lib.rs", "41", "42", true),
                ),
                text_response(
                    br#"{"kind":"complete","run_instructions":"cargo test","summary":"Corrected the answer and its regression test to 42."}"#,
                ),
            ]);
            let fixer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0x92; 16], "fixer"),
                responses: Mutex::new(fixer_responses),
            });
            let mut reviewer_responses = VecDeque::new();
            for _ in 0..4 {
                reviewer_responses.extend([
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    text_response(
                        br#"{"findings":[{"category":"requested_behavior","description":"The implementation returns 41 although the task requires 42.","location":"src/lib.rs","remediation":"Return and test 42.","reproduction":"Inspect answer and its test.","severity":"low","title":"Answer is not 42"}],"summary":"The requested result is incorrect."}"#,
                    ),
                ]);
            }
            reviewer_responses.extend([
                named_tool_response("workspace_list", list_arguments("", 3)),
                named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                text_response(
                    br#"{"findings":[],"summary":"The answer and regression test now require 42."}"#,
                ),
            ]);
            let reviewer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0x93; 16], "reviewer-finding"),
                responses: Mutex::new(reviewer_responses),
            });
            let task = "Add a tested answer function that returns 42.".to_owned();
            let run_id = RunId::new([0x94; 16]).expect("run ID");
            let command_runtime = support::command_runtime(state.path(), repository.path(), run_id);

            let outcome = ProductRunner::run(
                ProductRunInput {
                    workspace_kind: peritus_product_runner::ProductWorkspaceKind::Managed,
                    run_id,
                    workspace_id: WorkspaceId::new([0x95; 16]).expect("workspace ID"),
                    workspace_root: repository.path().to_owned(),
                    trace_path: state.path().join("product.trace"),
                    command_runtime,
                    finding_state: String::new(),
                    task: task.clone(),
                    max_elapsed: Some(std::time::Duration::from_hours(8)),
                    delivery_scope: ProductDeliveryScope::WorkspaceChanges,
                    conversation: Arc::new(FixedConversation(task)),
                    providers: RoleProviders {
                        writer,
                        reviewer,
                        fixer,
                        fallbacks: Vec::new(),
                    },
                    cancelled: Arc::new(AtomicBool::new(false)),
                    provider_cancellation: CancellationToken::new(),
                    resume: None,
                },
                Arc::new(|_| {}),
            )
            .await
            .expect("production run");
            assert!(outcome.settlement().is_accepted());
            let output = outcome.candidate().expect("accepted candidate");

            assert_eq!(output.fixer_cycles, 4);
            assert!(output.summary.contains("Added an answer API and test"));
            assert!(output.summary.contains("Corrected the answer"));
            assert!(output.review.contains("resolution confirmed"));
            assert!(!output.review.contains("/ open]"));
            assert!(
                fs::read_to_string(repository.path().join("src/lib.rs"))
                    .expect("source")
                    .contains("42")
            );
        });
}
