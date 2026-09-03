//! Product-path provider failover regression.

#[path = "production_composition/support.rs"]
mod support;

use std::{
    collections::VecDeque,
    fs,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use peritus_model_protocol::{ModelRequest, ProviderProfile};
use peritus_product_runner::{
    PRODUCT_RUN_MAX_ELAPSED, ProductDeliveryScope, ProductRunInput, ProductRunner, RoleProviders,
    RunObserver,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, OwnedModelStream, ProviderCoreError,
};
use peritus_types::{RunId, WorkspaceId};

use support::{
    FixedConversation, ScriptedProvider, cargo, design_response, git, list_arguments,
    named_tool_response, patch_arguments, profile, read_arguments, text_response, tool_response,
    write_arguments,
};

struct UnavailableProvider {
    profile: ProviderProfile,
    starts: Arc<AtomicUsize>,
}

impl ModelProvider for UnavailableProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn start(
        &self,
        _request: ModelRequest,
        _cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Err(ProviderCoreError::credential("configured account is temporarily unavailable"))
        })
    }
}

#[test]
#[allow(clippy::too_many_lines, reason = "one complete provider-switch product fixture")]
fn unavailable_selected_provider_uses_consented_fallbacks_for_each_role() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let repository = tempfile::tempdir().expect("repository");
            let state = tempfile::tempdir().expect("state directory");
            let trace_path = state.path().join("product.trace");
            prepare_repository(repository.path());

            let intermediate = r"/// Returns the fixture answer.
#[must_use]
pub const fn answer() -> u32 {
    41
}

#[cfg(test)]
mod tests {
    #[test]
    fn answer_is_42() {
        assert_eq!(super::answer(), 41);
    }
}
";
            let unavailable_starts = Arc::new(AtomicUsize::new(0));
            let unavailable: Arc<dyn ModelProvider> = Arc::new(UnavailableProvider {
                profile: profile([0x61; 16], "unavailable-primary"),
                starts: Arc::clone(&unavailable_starts),
            });
            let implementer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0x62; 16], "implementation-fallback"),
                responses: Mutex::new(VecDeque::from([
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("Cargo.toml")),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    design_response(),
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("src/lib.rs")),
                    tool_response(write_arguments("src/lib.rs", intermediate)),
                    named_tool_response(
                        "workspace_patch",
                        patch_arguments("src/lib.rs", "41", "42", true),
                    ),
                    text_response(
                        br#"{"kind":"complete","run_instructions":"cargo test","summary":"Added the tested answer API."}"#,
                    ),
                ])),
            });
            let reviewer: Arc<dyn ModelProvider> = Arc::new(ScriptedProvider {
                profile: profile([0x63; 16], "review-fallback"),
                responses: Mutex::new(VecDeque::from([
                    named_tool_response("workspace_list", list_arguments("", 3)),
                    named_tool_response("workspace_read", read_arguments("Cargo.toml")),
                    text_response(
                        br#"{"findings":[],"summary":"The requested API and test are present and exact-target gates passed."}"#,
                    ),
                ])),
            });
            let observed_failovers = Arc::new(Mutex::new(Vec::new()));
            let observations = Arc::clone(&observed_failovers);
            let observer: RunObserver = Arc::new(move |update| {
                observations
                    .lock()
                    .expect("progress observations")
                    .push(update.progress.provider_failovers());
            });
            let task = "Add a tested answer function that returns 42.".to_owned();
            let run_id = RunId::new([0x64; 16]).expect("run ID");
            let command_runtime = support::command_runtime(state.path(), repository.path(), run_id);

            let outcome = ProductRunner::run(
                ProductRunInput {
                    run_id,
                    workspace_id: WorkspaceId::new([0x65; 16]).expect("workspace ID"),
                    workspace_root: repository.path().to_owned(),
                    trace_path: trace_path.clone(),
                    command_runtime,
                    finding_state: String::new(),
                    task: task.clone(),
                    max_elapsed: PRODUCT_RUN_MAX_ELAPSED,
                    delivery_scope: ProductDeliveryScope::WorkspaceChanges,
                    conversation: Arc::new(FixedConversation(task)),
                    providers: RoleProviders {
                        writer: Arc::clone(&unavailable),
                        reviewer: Arc::clone(&unavailable),
                        fixer: unavailable,
                        fallbacks: vec![implementer, reviewer],
                    },
                    cancelled: Arc::new(AtomicBool::new(false)),
                    provider_cancellation: CancellationToken::new(),
                    resume: None,
                },
                observer,
            )
            .await
            .expect("fallback-backed production run");
            assert!(outcome.settlement().is_accepted());
            let output = outcome.candidate().expect("accepted candidate");

            assert_eq!(output.changed_paths, vec![Path::new("src/lib.rs").to_owned()]);
            assert!(output.gates.contains("Exact-target acceptance: PASS"));
            assert!(output.review.contains("No findings"));
            assert_eq!(
                observed_failovers.lock().expect("progress observations").last(),
                Some(&4)
            );
            assert_eq!(trace_tags(&trace_path).into_iter().filter(|tag| *tag == 5).count(), 4);
            assert_eq!(unavailable_starts.load(Ordering::SeqCst), 1);
        });
}

fn prepare_repository(root: &Path) {
    fs::create_dir_all(root.join("src")).expect("source directory");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"failover-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("manifest");
    fs::write(root.join("src/lib.rs"), "pub const fn before() -> u32 { 1 }\n")
        .expect("initial source");
    git(root, &["init", "--quiet"]);
    git(root, &["config", "user.name", "Peritus Test"]);
    git(root, &["config", "user.email", "peritus@example.invalid"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    cargo(root, &["generate-lockfile"]);
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "initial"]);
}

fn trace_tags(path: &Path) -> Vec<u8> {
    let bytes = fs::read(path).expect("trace bytes");
    let mut offset = 0_usize;
    let mut tags = Vec::new();
    while offset < bytes.len() {
        tags.push(bytes[offset]);
        let length_bytes: [u8; 8] = bytes
            .get(offset + 1..offset + 9)
            .expect("complete trace length")
            .try_into()
            .expect("eight-byte trace length");
        let length = usize::try_from(u64::from_le_bytes(length_bytes)).expect("trace length");
        offset = offset.checked_add(9 + length).expect("bounded trace offset");
        assert!(offset <= bytes.len(), "trace payload is truncated");
    }
    tags
}
