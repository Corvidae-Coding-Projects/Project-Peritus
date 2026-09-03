use std::{
    collections::VecDeque,
    fs,
    future::Future,
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Duration,
};

use peritus_model_protocol::{EventEnvelope, ModelRequest, ProviderProfile};
use peritus_product_runner::{
    ProductDeliveryScope, ProductRunInput, ProductRunResume, RoleProviders,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, OwnedModelStream, ProviderCoreError,
};
use peritus_types::{RunId, WorkspaceId};

use super::support::{
    FixedConversation, ScriptedProvider, cargo, design_response, git, list_arguments,
    named_tool_response, patch_arguments, profile, read_arguments, text_response, tool_response,
    write_arguments,
};

pub const TASK: &str = "Add a tested answer function that returns 42.";
pub const CORRECT: &str = "pub const fn answer() -> u32 {\n    42\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn answer_is_42() {\n        assert_eq!(super::answer(), 42);\n    }\n}\n";
pub const INCORRECT: &str = "pub const fn answer() -> u32 {\n    41\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn answer_is_42() {\n        assert_eq!(super::answer(), 41);\n    }\n}\n";
pub const UNFORMATTED: &str = "pub const fn answer() -> u32 { 42 }\n";

#[allow(clippy::too_many_arguments, reason = "fixture keeps each authority input explicit")]
pub fn input(
    repository: &tempfile::TempDir,
    state: &tempfile::TempDir,
    run: u8,
    workspace: u8,
    providers: RoleProviders,
    cancelled: Arc<AtomicBool>,
    max_elapsed: Duration,
    resume: Option<ProductRunResume>,
) -> ProductRunInput {
    let run_id = RunId::new([run; 16]).expect("run ID");
    ProductRunInput {
        run_id,
        workspace_id: WorkspaceId::new([workspace; 16]).expect("workspace ID"),
        workspace_root: repository.path().to_owned(),
        trace_path: state.path().join(format!("{run:02x}.trace")),
        command_runtime: super::support::command_runtime(state.path(), repository.path(), run_id),
        finding_state: String::new(),
        task: TASK.to_owned(),
        max_elapsed,
        delivery_scope: ProductDeliveryScope::WorkspaceChanges,
        conversation: Arc::new(FixedConversation(format!("User:\n{TASK}"))),
        providers,
        cancelled,
        provider_cancellation: CancellationToken::new(),
        resume,
    }
}

pub fn roles(
    writer: Arc<ScriptedProvider>,
    reviewer: Arc<ScriptedProvider>,
    fixer: Arc<ScriptedProvider>,
) -> RoleProviders {
    let writer: Arc<dyn ModelProvider> = writer;
    let reviewer: Arc<dyn ModelProvider> = reviewer;
    let fixer: Arc<dyn ModelProvider> = fixer;
    RoleProviders { writer, reviewer, fixer, fallbacks: Vec::new() }
}

struct StalledProvider {
    profile: ProviderProfile,
}

impl ModelProvider for StalledProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn start(
        &self,
        _request: ModelRequest,
        _cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(std::future::pending())
    }
}

pub fn stalled_roles(id: u8) -> RoleProviders {
    let provider: Arc<dyn ModelProvider> =
        Arc::new(StalledProvider { profile: profile([id; 16], "stalled") });
    RoleProviders {
        writer: Arc::clone(&provider),
        reviewer: Arc::clone(&provider),
        fixer: provider,
        fallbacks: Vec::new(),
    }
}

pub fn scripted(
    id: u8,
    name: &str,
    responses: Vec<VecDeque<EventEnvelope>>,
) -> Arc<ScriptedProvider> {
    Arc::new(ScriptedProvider {
        profile: profile([id; 16], name),
        responses: Mutex::new(responses.into()),
    })
}

pub fn complete_writer(source: &str) -> Vec<VecDeque<EventEnvelope>> {
    let mut responses = partial_writer(source);
    responses.push(text_response(
        br#"{"kind":"complete","run_instructions":"cargo test","summary":"Implemented the requested answer."}"#,
    ));
    responses
}

pub fn partial_writer(source: &str) -> Vec<VecDeque<EventEnvelope>> {
    vec![
        named_tool_response("workspace_list", list_arguments("", 3)),
        named_tool_response("workspace_read", read_arguments("Cargo.toml")),
        named_tool_response("workspace_read", read_arguments("src/lib.rs")),
        design_response(),
        named_tool_response("workspace_list", list_arguments("", 3)),
        named_tool_response("workspace_read", read_arguments("src/lib.rs")),
        tool_response(write_arguments("src/lib.rs", source)),
    ]
}

pub fn clean_review() -> Vec<VecDeque<EventEnvelope>> {
    vec![
        named_tool_response("workspace_list", list_arguments("", 3)),
        named_tool_response("workspace_read", read_arguments("src/lib.rs")),
        text_response(br#"{"findings":[],"summary":"The exact request and gates are satisfied."}"#),
    ]
}

pub fn finding_then_clean_review() -> Vec<VecDeque<EventEnvelope>> {
    let mut responses = vec![
        named_tool_response("workspace_list", list_arguments("", 3)),
        named_tool_response("workspace_read", read_arguments("src/lib.rs")),
        text_response(br#"{"findings":[{"category":"requested_behavior","description":"The implementation returns 41 instead of 42.","location":"src/lib.rs","remediation":"Return and test 42.","reproduction":"Inspect answer and its test.","severity":"low","title":"Wrong answer"}],"summary":"The result is incorrect."}"#),
    ];
    responses.extend(clean_review());
    responses
}

pub fn completed_fixer() -> Vec<VecDeque<EventEnvelope>> {
    vec![
        named_tool_response("workspace_list", list_arguments("", 3)),
        named_tool_response("workspace_read", read_arguments("src/lib.rs")),
        text_response(
            br#"{"kind":"complete","run_instructions":"cargo test","summary":"Confirmed the interrupted correction."}"#,
        ),
    ]
}

pub fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("repository");
    fs::create_dir_all(root.path().join("src")).expect("source directory");
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"resume-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("manifest");
    fs::write(root.path().join("src/lib.rs"), "pub const fn before() -> u32 { 1 }\n")
        .expect("source");
    git(root.path(), &["init", "--quiet"]);
    git(root.path(), &["config", "user.name", "Peritus Test"]);
    git(root.path(), &["config", "user.email", "peritus@example.invalid"]);
    git(root.path(), &["config", "commit.gpgsign", "false"]);
    cargo(root.path(), &["generate-lockfile"]);
    git(root.path(), &["add", "."]);
    git(root.path(), &["commit", "--quiet", "-m", "initial"]);
    root
}

pub fn run_async(future: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future);
}

pub fn fixer_patch() -> Vec<VecDeque<EventEnvelope>> {
    vec![
        named_tool_response("workspace_list", list_arguments("", 3)),
        named_tool_response("workspace_read", read_arguments("src/lib.rs")),
        named_tool_response("workspace_patch", patch_arguments("src/lib.rs", "41", "42", true)),
    ]
}
