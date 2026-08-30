//! Composed D0/D1/D2/E0 product-run regression.

#[path = "production_composition/support.rs"]
mod support;

use std::{
    collections::VecDeque,
    fs,
    path::Path,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use peritus_product_runner::{
    ProductDeliveryScope, ProductRunInput, ProductRunOutcome, ProductRunPhase, ProductRunner,
    ProductRunnerErrorKind, RoleProviders, RunObserver,
};
use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_types::RunId;

use support::{
    FixedConversation, ScriptedProvider, cargo, design_response, git, list_arguments,
    named_tool_response, patch_arguments, profile, read_arguments, text_response, tool_response,
    write_arguments,
};

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
            let error = ProductRunner::run(
                ProductRunInput {
                    run_id: RunId::new([0x72; 16]).expect("run ID"),
                    workspace_root: repository.path().to_owned(),
                    trace_path: trace_path.clone(),
                    finding_state: String::new(),
                    task: task.clone(),
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
                },
                Arc::new(|_| {}),
            )
            .await
            .expect_err("provider must fail before returning a response");

            assert_eq!(error.kind(), ProductRunnerErrorKind::Provider);
            assert!(trace_path.is_file());
            assert_eq!(fs::metadata(trace_path).expect("trace metadata").len(), 0);
        },
    );
}

#[test]
#[allow(clippy::too_many_lines, reason = "one complete production composition fixture")]
fn exact_target_tool_edit_and_typed_review_are_required_for_completion() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let repository = tempfile::tempdir().expect("repository");
            let state = tempfile::tempdir().expect("state directory");
            let trace_path = state.path().join("product.trace");
            fs::create_dir_all(repository.path().join("src")).expect("source directory");
            fs::write(
                repository.path().join("Cargo.toml"),
                "[package]\nname = \"composed-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            )
            .expect("manifest");
            fs::write(
                repository.path().join("src/lib.rs"),
                "pub const fn before() -> u32 { 1 }\n",
            )
            .expect("initial source");
            git(repository.path(), &["init", "--quiet"]);
            git(repository.path(), &["config", "user.name", "Peritus Test"]);
            git(repository.path(), &["config", "user.email", "peritus@example.invalid"]);
            git(repository.path(), &["config", "commit.gpgsign", "false"]);
            cargo(repository.path(), &["generate-lockfile"]);
            git(repository.path(), &["add", "."]);
            git(repository.path(), &["commit", "--quiet", "-m", "initial"]);

            let implemented = r"/// Returns the fixture answer.
#[must_use]
pub const fn answer() -> u32 {
    42
}

#[cfg(test)]
mod tests {
    #[test]
    fn answer_is_42() {
        assert_eq!(super::answer(), 42);
    }
}
";
            let write_arguments = write_arguments("src/lib.rs", implemented);
            let writer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0x81; 16], "writer"),
                responses: Mutex::new(VecDeque::from([
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("Cargo.toml")),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    design_response(),
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    text_response(
                        br#"{"kind":"question","message":"Please provide a writable managed workspace."}"#,
                    ),
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    tool_response(write_arguments),
                    text_response(
                        br#"{"kind":"complete","run_instructions":"cargo test","summary":"Added the tested answer API."}"#,
                    ),
                ])),
            });
            let reviewer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0x82; 16], "reviewer"),
                responses: Mutex::new(VecDeque::from([
                    text_response(b"not a review object"),
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("Cargo.toml")),
                    text_response(
                        br#"{"findings":[],"summary":"The requested API and test are present and exact-target gates passed."}"#,
                    ),
                ])),
            });
            let phases = Arc::new(Mutex::new(Vec::new()));
            let phase_log = Arc::clone(&phases);
            let observer: RunObserver = Arc::new(move |update| {
                phase_log.lock().expect("phases").push(update.phase);
            });
            let task = "Add a tested answer function that returns 42.".to_owned();

            let outcome = ProductRunner::run(
                ProductRunInput {
                    run_id: RunId::new([0x83; 16]).expect("run ID"),
                    workspace_root: repository.path().to_owned(),
                    trace_path: trace_path.clone(),
                    finding_state: String::new(),
                    task: task.clone(),
                    delivery_scope: ProductDeliveryScope::WorkspaceChanges,
                    conversation: Arc::new(FixedConversation(task)),
                    providers: RoleProviders {
                        writer: Arc::clone(&writer),
                        reviewer,
                        fixer: writer,
                        fallbacks: Vec::new(),
                    },
                    cancelled: Arc::new(AtomicBool::new(false)),
                    provider_cancellation: CancellationToken::new(),
                },
                observer,
            )
            .await
            .expect("production run");
            let ProductRunOutcome::Complete(output) = outcome else {
                panic!("run asked for unexpected user input");
            };

            assert_eq!(output.changed_paths, vec![Path::new("src/lib.rs").to_owned()]);
            assert_eq!(output.successful_commands.len(), 7);
            assert!(
                output
                    .successful_commands
                    .iter()
                    .any(|command| command == "peritus-internal explicit-output-paths")
            );
            assert!(output.successful_commands.iter().any(|command| {
                command.contains("peritus-internal source-layout --max-lines 500")
            }));
            assert!(output.successful_commands.iter().any(|command| {
                command == "cargo fmt --manifest-path Cargo.toml --all -- --check"
            }));
            assert!(output.successful_commands.iter().filter(|command| command.starts_with("cargo ") && !command.starts_with("cargo fmt ")).all(|command| {
                command.contains("--manifest-path Cargo.toml")
                    && command.contains("--all-targets")
                    && command.contains("--all-features")
            }));
            assert!(output.gates.contains("Exact-target acceptance: PASS"));
            assert!(output.review.contains("No findings"));
            assert!(!output.diff.contains('\0'));
            assert!(output.summary.contains("Added the tested answer API"));
            assert_eq!(output.run_instructions, "cargo test");
            assert!(output.design_path.is_file());
            assert!(
                fs::read_to_string(&output.design_path)
                    .expect("design document")
                    .contains("## Architecture and interfaces")
            );
            assert!(
                fs::read_to_string(&output.design_path)
                    .expect("design document")
                    .contains("## Repository grounding evidence")
            );
            assert!(trace_path.is_file());
            assert_eq!(
                phases.lock().expect("phases").as_slice(),
                [ProductRunPhase::Designing, ProductRunPhase::Designing,
                 ProductRunPhase::Writing, ProductRunPhase::Checking,
                 ProductRunPhase::Reviewing, ProductRunPhase::Reviewing]
            );
        });
}

#[test]
#[allow(clippy::too_many_lines, reason = "one complete finding-conservation fixture")]
fn fixer_cannot_erase_a_finding_without_fresh_reviewer_confirmation() {
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
            let fixer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0x92; 16], "fixer"),
                responses: Mutex::new(VecDeque::from([
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    named_tool_response(
                        "workspace_patch",
                        patch_arguments("src/lib.rs", "41", "42", true),
                    ),
                    text_response(
                        br#"{"kind":"complete","run_instructions":"cargo test","summary":"Corrected the answer and its regression test to 42."}"#,
                    ),
                ])),
            });
            let reviewer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0x93; 16], "reviewer-finding"),
                responses: Mutex::new(VecDeque::from([
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    text_response(
                        br#"{"findings":[{"category":"requested_behavior","description":"The implementation returns 41 although the task requires 42.","location":"src/lib.rs","remediation":"Return and test 42.","reproduction":"Inspect answer and its test.","severity":"low","title":"Answer is not 42"}],"summary":"The requested result is incorrect."}"#,
                    ),
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    text_response(
                        br#"{"findings":[],"summary":"The answer and regression test now require 42."}"#,
                    ),
                ])),
            });
            let task = "Add a tested answer function that returns 42.".to_owned();

            let outcome = ProductRunner::run(
                ProductRunInput {
                    run_id: RunId::new([0x94; 16]).expect("run ID"),
                    workspace_root: repository.path().to_owned(),
                    trace_path: state.path().join("product.trace"),
                    finding_state: String::new(),
                    task: task.clone(),
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
                },
                Arc::new(|_| {}),
            )
            .await
            .expect("production run");
            let ProductRunOutcome::Complete(output) = outcome else {
                panic!("run asked for unexpected user input");
            };

            assert_eq!(output.fixer_cycles, 1);
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
