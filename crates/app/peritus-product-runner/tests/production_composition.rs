//! Composed D0/D1/D2/E0 product-run regression.

#[path = "production_composition/support.rs"]
mod support;

use std::{
    collections::VecDeque,
    fs,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use peritus_agent::DeveloperReviewRetryReason::InvalidSubmission;
use peritus_product_runner::{
    PRODUCT_RUN_MAX_ELAPSED, ProductDeliveryScope, ProductRunInput, ProductRunPhase, ProductRunner,
    RoleProviders, RunObserver,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, OwnedModelStream, ProviderCoreError,
};
use peritus_run_settlement::{CandidateStage, RunDisposition, SettlementCause};
use peritus_types::{RunId, WorkspaceId};

use support::{
    FixedConversation, ScriptedProvider, cargo, command_arguments, design_response, git,
    list_arguments, named_tool_response, patch_arguments, profile, read_arguments, text_response,
    tool_response, write_arguments,
};

struct CorrectionRecordingProvider {
    inner: ScriptedProvider,
    observed: Arc<AtomicBool>,
}

impl ModelProvider for CorrectionRecordingProvider {
    fn profile(&self) -> &peritus_model_protocol::ProviderProfile {
        self.inner.profile()
    }

    fn start(
        &self,
        request: peritus_model_protocol::ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        let bytes = request.canonical_bytes().expect("fixture request encoding");
        if String::from_utf8_lossy(&bytes).contains(
            "The harness rejected the previous terminal response during validate developer terminal",
        ) {
            self.observed.store(true, Ordering::SeqCst);
        }
        self.inner.start(request, cancellation)
    }
}

#[path = "production_composition/composition_size.rs"]
mod composition_size;

#[path = "production_composition/malformed_terminal.rs"]
mod malformed_terminal;

#[path = "production_composition/provider_failure.rs"]
mod provider_failure;
#[path = "production_composition/recovery_observer.rs"]
mod recovery_observer;

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
            let correction_observed = Arc::new(AtomicBool::new(false));
            let writer: Arc<dyn ModelProvider> = Arc::new(CorrectionRecordingProvider {
                observed: Arc::clone(&correction_observed),
                inner: ScriptedProvider {
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
                    text_response(b"Added the tested answer API."),
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    text_response(
                        br#"{"kind":"complete","run_instructions":"cargo test","summary":"Added the tested answer API."}"#,
                    ),
                ])),
                },
            });
            let reviewer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0x82; 16], "reviewer"),
                responses: Mutex::new(VecDeque::from([
                    text_response(b"not a review object"),
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("Cargo.toml")),
                    text_response(b"still not a review object"),
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("Cargo.toml")),
                    text_response(b"another malformed review"),
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("Cargo.toml")),
                    text_response(
                        br#"{"findings":[],"summary":"The requested API and test are present and exact-target gates passed."}"#,
                    ),
                ])),
            });
            let phases = Arc::new(Mutex::new(Vec::new()));
            let phase_log = Arc::clone(&phases);
            let retry_counts = Arc::new(Mutex::new(Vec::new()));
            let counts = Arc::clone(&retry_counts);
            let observer: RunObserver = Arc::new(move |update| {
                phase_log.lock().expect("phases").push(update.phase);
                counts.lock().expect("counts").push((update.phase, update.progress.retries()));
            });
            let task = "Add a tested answer function that returns 42.".to_owned();
            let conversation = Arc::new(recovery_observer::ObservedConversation::new(task.clone()));
            let run_id = RunId::new([0x83; 16]).expect("run ID");
            let command_runtime = support::command_runtime(state.path(), repository.path(), run_id);

            let outcome = ProductRunner::run(
                ProductRunInput {
                    workspace_kind: peritus_product_runner::ProductWorkspaceKind::Managed,
                    run_id,
                    workspace_id: WorkspaceId::new([0x84; 16]).expect("workspace ID"),
                    workspace_root: repository.path().to_owned(),
                    trace_path: trace_path.clone(),
                    command_runtime,
                    finding_state: String::new(),
                    task: task.clone(),
                    max_elapsed: PRODUCT_RUN_MAX_ELAPSED,
                    delivery_scope: ProductDeliveryScope::WorkspaceChanges,
                    conversation: conversation.clone(),
                    providers: RoleProviders {
                        writer: Arc::clone(&writer),
                        reviewer,
                        fixer: writer,
                        fallbacks: Vec::new(),
                    },
                    cancelled: Arc::new(AtomicBool::new(false)),
                    provider_cancellation: CancellationToken::new(),
                    resume: None,
                },
                observer,
            )
            .await
            .expect("production run");
            assert!(outcome.settlement().is_accepted());
            assert_eq!(conversation.retries.lock().expect("notices").as_slice(), &[
                (2, 3, InvalidSubmission),
                (3, 3, InvalidSubmission),
            ]);
            let counts = retry_counts.lock().expect("counts");
            let before_review = counts.iter().find(|(phase, _)| *phase == ProductRunPhase::Reviewing).expect("review begins").1;
            assert_eq!(counts.last().expect("final progress").1 - before_review, 2, "both rejected reviews are counted");
            drop(counts);
            assert!(correction_observed.load(Ordering::SeqCst), "workspace progress must preserve the malformed-terminal correction");
            let output = outcome.candidate().expect("accepted candidate");

            assert_eq!(output.changed_paths, vec![Path::new("src/lib.rs").to_owned()]);
            assert_eq!(output.successful_commands.len(), 8);
            assert!(
                output
                    .successful_commands
                    .iter()
                    .any(|command| command == "peritus-internal explicit-output-paths")
            );
            assert!(
                output
                    .successful_commands
                    .iter()
                    .any(|command| command == "peritus-internal deliverable-inventory")
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
                 ProductRunPhase::Reviewing, ProductRunPhase::Reviewing,
                 ProductRunPhase::Finalizing]
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
                    max_elapsed: PRODUCT_RUN_MAX_ELAPSED,
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
