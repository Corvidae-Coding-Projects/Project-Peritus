use std::{
    collections::VecDeque,
    fs,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, EventEnvelope,
    FinishReason, ItemId, ItemKind, ModelEvent, ModelLimits, ModelName, ModelRequest,
    OutputLimitEnforcement, ProtocolLimits, ProviderName, ProviderProfile, ResumeKind, StateMode,
    StreamFragment, ToolCallId, ToolName, WireDialect,
};
use peritus_product_runner::{
    CommandRuntime, ConversationView, ProductDeliveryScope, ProductRunInput, ProductRunResume,
    RoleProviders,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, ModelStream, OwnedModelStream, ProviderCoreError,
};
use peritus_types::{ProviderProfileId, RunId, Sha256Digest, WorkspaceId};
use serde_json::{Map, Value};

pub(super) const TASK: &str = "Add a tested answer function that returns 42.";
const SOURCE: &str = "pub const fn answer() -> u32 {\n    42\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn answer_is_42() {\n        assert_eq!(super::answer(), 42);\n    }\n}\n";

struct ScriptedStream {
    events: VecDeque<EventEnvelope>,
}

impl ModelStream for ScriptedStream {
    fn next<'a>(
        &'a mut self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move { Ok(self.events.pop_front()) })
    }
}

pub(super) struct ScriptedProvider {
    profile: ProviderProfile,
    responses: Mutex<VecDeque<VecDeque<EventEnvelope>>>,
    starts: AtomicUsize,
}

impl ScriptedProvider {
    pub(super) fn starts(&self) -> usize {
        self.starts.load(Ordering::Acquire)
    }
}

impl ModelProvider for ScriptedProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn start(
        &self,
        _request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        self.starts.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move {
            let events = self
                .responses
                .lock()
                .map_err(|_| ProviderCoreError::configuration("fixture", "lock failed"))?
                .pop_front()
                .ok_or_else(|| ProviderCoreError::configuration("fixture", "script exhausted"))?;
            Ok(OwnedModelStream::new(ScriptedStream { events }, cancellation))
        })
    }
}

struct FixedConversation(String);

impl ConversationView for FixedConversation {
    fn revision(&self) -> u64 {
        1
    }

    fn render(&self) -> String {
        self.0.clone()
    }
}

pub(super) fn scripted(
    id: u8,
    name: &str,
    responses: Vec<VecDeque<EventEnvelope>>,
) -> Arc<ScriptedProvider> {
    Arc::new(ScriptedProvider {
        profile: profile([id; 16], name),
        responses: Mutex::new(responses.into()),
        starts: AtomicUsize::new(0),
    })
}

pub(super) fn roles(
    writer: Arc<ScriptedProvider>,
    reviewer: Arc<ScriptedProvider>,
    fixer: Arc<ScriptedProvider>,
) -> RoleProviders {
    let writer: Arc<dyn ModelProvider> = writer;
    let reviewer: Arc<dyn ModelProvider> = reviewer;
    let fixer: Arc<dyn ModelProvider> = fixer;
    RoleProviders { writer, reviewer, fixer, fallbacks: Vec::new() }
}

#[allow(clippy::too_many_arguments, reason = "fixture keeps authority values visible")]
pub(super) fn input(
    repository: &tempfile::TempDir,
    state: &tempfile::TempDir,
    run: u8,
    workspace: u8,
    providers: RoleProviders,
    resume: Option<ProductRunResume>,
) -> ProductRunInput {
    let run_id = RunId::new([run; 16]).expect("run id");
    ProductRunInput {
        run_id,
        workspace_id: WorkspaceId::new([workspace; 16]).expect("workspace id"),
        workspace_root: repository.path().to_owned(),
        trace_path: state.path().join(format!("{run:02x}.trace")),
        command_runtime: command_runtime(state.path(), repository.path(), run_id),
        finding_state: String::new(),
        task: TASK.to_owned(),
        max_elapsed: Duration::from_mins(1),
        delivery_scope: ProductDeliveryScope::WorkspaceChanges,
        conversation: Arc::new(FixedConversation(format!("User:\n{TASK}"))),
        providers,
        cancelled: Arc::new(AtomicBool::new(false)),
        provider_cancellation: CancellationToken::new(),
        resume,
    }
}

pub(super) fn complete_writer() -> Vec<VecDeque<EventEnvelope>> {
    let mut responses = vec![
        named_tool_response("workspace_list", object([("path", json("")), ("depth", 3.into())])),
        named_tool_response("workspace_read", read_arguments("Cargo.toml")),
        named_tool_response("workspace_read", read_arguments("src/lib.rs")),
        text_response(DESIGN.as_bytes()),
        named_tool_response("workspace_list", object([("path", json("")), ("depth", 3.into())])),
        named_tool_response("workspace_read", read_arguments("src/lib.rs")),
        named_tool_response(
            "workspace_write",
            object([("path", json("src/lib.rs")), ("content", json(SOURCE))]),
        ),
    ];
    responses.push(text_response(
        br#"{"kind":"complete","run_instructions":"cargo test","summary":"Implemented and tested the answer."}"#,
    ));
    responses
}

pub(super) fn clean_review() -> Vec<VecDeque<EventEnvelope>> {
    vec![
        named_tool_response("workspace_list", object([("path", json("")), ("depth", 3.into())])),
        named_tool_response("workspace_read", read_arguments("src/lib.rs")),
        text_response(
            br#"{"findings":[],"summary":"The exact request and independent gates are satisfied."}"#,
        ),
    ]
}

pub(super) fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("repository");
    fs::create_dir_all(root.path().join("src")).expect("source directory");
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"generic-resume-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
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

pub(super) fn profile(id: [u8; 16], name: &str) -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new(id).expect("profile id"),
        1,
        ProviderName::new(format!("scripted-{name}")).expect("provider"),
        ModelName::new(format!("scripted-{name}")).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&[Capability::ToolCalls], &[]).expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(32_768, 4_096, 16, 1, 512 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

fn named_tool_response(name: &str, arguments: Vec<u8>) -> VecDeque<EventEnvelope> {
    let item = ItemId::new(format!("{name}-item")).expect("item");
    let call = ToolCallId::new(format!("{name}-call")).expect("call");
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::ToolCall },
        ModelEvent::ToolCallStarted {
            item_id: item.clone(),
            call_id: call.clone(),
            name: ToolName::new(name.to_owned()).expect("tool"),
        },
        ModelEvent::ToolArgumentDelta {
            call_id: call,
            fragment: StreamFragment::new(arguments, ProtocolLimits::PRODUCTION)
                .expect("arguments"),
        },
        ModelEvent::ItemCompleted(item),
        ModelEvent::Finish(FinishReason::ToolCalls),
        ModelEvent::ResponseCompleted,
    ])
}

fn text_response(text: &[u8]) -> VecDeque<EventEnvelope> {
    let item = ItemId::new(format!("text-{}", text.len())).expect("item");
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::Message },
        ModelEvent::TextDelta {
            item_id: item.clone(),
            fragment: StreamFragment::new(text.to_vec(), ProtocolLimits::PRODUCTION).expect("text"),
        },
        ModelEvent::ItemCompleted(item),
        ModelEvent::Finish(FinishReason::Stop),
        ModelEvent::ResponseCompleted,
    ])
}

fn response<const N: usize>(events: [ModelEvent; N]) -> VecDeque<EventEnvelope> {
    events
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            let sequence = u64::try_from(index + 1).expect("sequence");
            EventEnvelope::new(
                sequence,
                None,
                None,
                Sha256Digest::new([u8::try_from(index + 1).expect("digest"); 32]),
                event,
            )
            .expect("event")
        })
        .collect()
}

fn object<const N: usize>(entries: [(&str, Value); N]) -> Vec<u8> {
    let object =
        entries.into_iter().map(|(key, value)| (key.to_owned(), value)).collect::<Map<_, _>>();
    serde_json::to_vec(&Value::Object(object)).expect("arguments")
}

fn json(value: &str) -> Value {
    Value::String(value.to_owned())
}

fn read_arguments(path: &str) -> Vec<u8> {
    object([("path", json(path)), ("start_line", 1.into()), ("end_line", 500.into())])
}

fn git(root: &Path, arguments: &[&str]) {
    let output =
        std::process::Command::new("git").args(arguments).current_dir(root).output().expect("git");
    assert!(output.status.success(), "git failed: {}", String::from_utf8_lossy(&output.stderr));
}

fn cargo(root: &Path, arguments: &[&str]) {
    let output = std::process::Command::new("cargo")
        .args(arguments)
        .current_dir(root)
        .output()
        .expect("cargo");
    assert!(output.status.success(), "cargo failed: {}", String::from_utf8_lossy(&output.stderr));
}

fn command_runtime(state: &Path, workspace: &Path, run_id: RunId) -> CommandRuntime {
    let root = state.join("command-runtime");
    let processes = peritus_process::ProcessStore::open(root.join("processes"), workspace)
        .expect("process store");
    CommandRuntime::open(root.join("router"), workspace, run_id, processes).expect("runtime")
}

const DESIGN: &str = "# Generic answer design\n\n## Objective and acceptance criteria\nAdd a public Rust answer function returning exactly 42 with a focused test.\n\n## Repository findings\nThe repository is one Cargo library and the target is src/lib.rs.\n\n## Architecture and interfaces\nKeep the function in the existing library API without new dependencies.\n\n## Data and control flow\nThe caller invokes a constant function and receives 42.\n\n## File and module plan\nEdit src/lib.rs only.\n\n## Implementation slices\nImplement the function and its adjacent unit test, then run gates.\n\n## Verification\nRun formatting, compilation, Clippy, and tests.\n\n## Risks and non-goals\nDo not change unrelated files or add runtime state.\n";
