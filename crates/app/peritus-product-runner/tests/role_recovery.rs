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

use peritus_product_runner::{ProductDeliveryScope, ProductRunInput, ProductRunner, RoleProviders};
use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_types::{RunId, WorkspaceId};

use support::{
    FixedConversation, ScriptedProvider, cargo, design_response, empty_response, git,
    interrupted_response, list_arguments, named_tool_response, profile, read_arguments,
    text_response, tool_response, write_arguments,
};

struct CapturingProvider {
    inner: Arc<ScriptedProvider>,
    requests: Arc<Mutex<Vec<peritus_model_protocol::ModelRequest>>>,
}
impl ModelProvider for CapturingProvider {
    fn profile(&self) -> &peritus_model_protocol::ProviderProfile {
        self.inner.profile()
    }
    fn start(
        &self,
        request: peritus_model_protocol::ModelRequest,
        cancellation: CancellationToken,
    ) -> peritus_provider_core::BoxFuture<
        '_,
        Result<peritus_provider_core::OwnedModelStream, peritus_provider_core::ProviderCoreError>,
    > {
        self.requests.lock().unwrap().push(request.clone());
        self.inner.start(request, cancellation)
    }
}

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
            let writer_responses = recovering_writer_responses(implemented);
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
            let requests = Arc::new(Mutex::new(Vec::new()));
            let writer: Arc<dyn ModelProvider> = Arc::new(CapturingProvider {
                inner: writer_provider.clone(), requests: Arc::clone(&requests),
            });
            let reviewer: Arc<dyn ModelProvider> = reviewer_provider.clone();
            let task = "Add a tested answer function that returns 42.".to_owned();
            let run_id = RunId::new([0xa3; 16]).expect("run ID");
            let command_runtime = support::command_runtime(state.path(), repository.path(), run_id);

            let outcome = ProductRunner::run(
                ProductRunInput {
                    workspace_kind: peritus_product_runner::ProductWorkspaceKind::Managed,
                    run_id,
                    workspace_id: WorkspaceId::new([0xa4; 16]).expect("workspace ID"),
                    workspace_root: repository.path().to_owned(),
                    trace_path: state.path().join("product.trace"),
                    command_runtime,
                    finding_state: String::new(),
                    task: task.clone(),
                    max_elapsed: Some(std::time::Duration::from_hours(8)),
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
                    resume: None,
                },
                Arc::new(|_| {}),
            )
            .await
            .expect("fresh role invocations recover");
            assert!(outcome.settlement().is_accepted());
            let output = outcome.candidate().expect("accepted candidate");

            assert_eq!(output.changed_paths, vec![Path::new("src/lib.rs").to_owned()]);
            assert!(output.summary.contains("Recovered and implemented"));
            assert!(output.review.contains("No findings"));
            assert!(writer_provider.responses.lock().expect("writer responses").is_empty());
            assert!(reviewer_provider.responses.lock().expect("reviewer responses").is_empty());
            assert_retained_designer(&requests.lock().unwrap());
        });
}

fn recovering_writer_responses(
    implemented: &str,
) -> VecDeque<VecDeque<peritus_model_protocol::EventEnvelope>> {
    let mut writer_responses = VecDeque::new();
    writer_responses.push_back(interrupted_response());
    writer_responses.extend([
        named_tool_response("workspace_list", list_arguments("", 3)),
        named_tool_response("workspace_read", read_arguments("Cargo.toml")),
        interrupted_response(),
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
    writer_responses
}

fn assert_retained_designer(requests: &[peritus_model_protocol::ModelRequest]) {
    let designer = requests
        .iter()
        .filter(|request| {
            request
                .local_session_directory()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .is_some_and(|name| name == "designer")
        })
        .collect::<Vec<_>>();
    assert!(designer.len() >= 7, "design recovery must retain its task namespace");
    assert!(
        designer
            .windows(2)
            .all(|pair| pair[0].local_session_directory() == pair[1].local_session_directory())
    );
    let recovered = designer
        .iter()
        .find(|request| request.request_id().expose_for_wire().contains("invocation-3"))
        .expect("third design invocation");
    assert!(
        recovered.messages().iter().flat_map(peritus_model_protocol::Message::content).any(
            |block| matches!(block, peritus_model_protocol::ContentBlock::Text(text)
            if text.expose_for_wire().contains("UNTRUSTED TOOL EVIDENCE")
                && text.expose_for_wire().contains("role-recovery-fixture"))
        ),
        "completed repository reads must survive as scoped historical evidence"
    );
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
