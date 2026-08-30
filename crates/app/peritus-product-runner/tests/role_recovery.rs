//! Product-role recovery after a provider exhausts one invocation's in-turn retries.

#[allow(dead_code, reason = "shared integration support exposes helpers used by sibling tests")]
#[path = "production_composition/support.rs"]
mod support;

use std::{
    collections::VecDeque,
    fs,
    path::Path,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use peritus_product_runner::{
    ProductDeliveryScope, ProductRunInput, ProductRunOutcome, ProductRunner, RoleProviders,
};
use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_types::RunId;

use support::{
    FixedConversation, ScriptedProvider, cargo, design_response, empty_response, git,
    interrupted_response, list_arguments, named_tool_response, profile, read_arguments,
    text_response, tool_response, write_arguments,
};

#[test]
fn roles_restart_after_exhausted_empty_and_interrupted_responses() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let repository = tempfile::tempdir().expect("repository");
            let state = tempfile::tempdir().expect("state directory");
            prepare_repository(repository.path());

            let implemented = r"/// Returns the requested fixture answer.
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
            let mut writer_responses = VecDeque::new();
            writer_responses.push_back(interrupted_response());
            writer_responses.extend([
                named_tool_response("workspace_list", list_arguments("", 3)),
                named_tool_response("workspace_read", read_arguments("Cargo.toml")),
                design_response(),
            ]);
            writer_responses.extend((0..4).map(|_| empty_response()));
            writer_responses.extend([
                named_tool_response("workspace_list", list_arguments("", 3)),
                named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                tool_response(write_arguments("src/lib.rs", implemented)),
                text_response(
                    br#"{"kind":"complete","run_instructions":"cargo test","summary":"Recovered and implemented the requested answer."}"#,
                ),
            ]);
            let writer_provider = Arc::new(ScriptedProvider {
                profile: profile([0xa1; 16], "recovering-writer"),
                responses: Mutex::new(writer_responses),
            });

            let mut reviewer_responses = VecDeque::new();
            reviewer_responses.push_back(interrupted_response());
            reviewer_responses.extend([
                named_tool_response("workspace_list", list_arguments("", 3)),
                named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                text_response(
                    br#"{"findings":[],"summary":"The recovered implementation and exact-target gates satisfy the request."}"#,
                ),
            ]);
            let reviewer_provider = Arc::new(ScriptedProvider {
                profile: profile([0xa2; 16], "recovering-reviewer"),
                responses: Mutex::new(reviewer_responses),
            });
            let writer: Arc<dyn ModelProvider> = writer_provider.clone();
            let reviewer: Arc<dyn ModelProvider> = reviewer_provider.clone();
            let task = "Add a tested answer function that returns 42.".to_owned();

            let outcome = ProductRunner::run(
                ProductRunInput {
                    run_id: RunId::new([0xa3; 16]).expect("run ID"),
                    workspace_root: repository.path().to_owned(),
                    trace_path: state.path().join("product.trace"),
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
                Arc::new(|_| {}),
            )
            .await
            .expect("fresh role invocations recover");
            let ProductRunOutcome::Complete(output) = outcome else {
                panic!("run asked for unexpected user input");
            };

            assert_eq!(output.changed_paths, vec![Path::new("src/lib.rs").to_owned()]);
            assert!(output.summary.contains("Recovered and implemented"));
            assert!(output.review.contains("No findings"));
            assert!(writer_provider.responses.lock().expect("writer responses").is_empty());
            assert!(reviewer_provider.responses.lock().expect("reviewer responses").is_empty());
        });
}

fn prepare_repository(root: &Path) {
    fs::create_dir_all(root.join("src")).expect("source directory");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"role-recovery-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("manifest");
    fs::write(root.join("src/lib.rs"), "pub const fn before() -> u32 { 1 }\n").expect("source");
    git(root, &["init", "--quiet"]);
    git(root, &["config", "user.name", "Peritus Test"]);
    git(root, &["config", "user.email", "peritus@example.invalid"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    cargo(root, &["generate-lockfile"]);
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "initial"]);
}
