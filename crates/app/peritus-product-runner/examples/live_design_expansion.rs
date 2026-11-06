//! Credentialed Stage 6/7 design-expansion qualification on disposable repositories.

use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, ModelLimits, ModelName,
    OutputLimitEnforcement, ProviderName, ProviderProfile, ResumeKind, StateMode, WireDialect,
};
use peritus_product_runner::{
    ConversationView, ProductDeliveryScope, ProductRunInput, ProductRunOutcome, ProductRunPhase,
    ProductRunner, RoleProviders, RunObserver,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, OwnedModelStream, ProcessLimits, ProviderCoreError,
};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

#[path = "live_design_expansion/browser.rs"]
mod browser;

fn main() -> Result<(), Box<dyn Error>> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Result<(), Box<dyn Error>> {
    let provider = codex_provider()?;
    provider.require_authenticated(&CancellationToken::new()).await?;
    let provider: Arc<dyn ModelProvider> = provider;
    let provider = Arc::new(CountingProvider::new(provider));

    let selected_case = std::env::args().nth(1);
    match selected_case.as_deref() {
        None => {
            qualify_zero_question(Arc::clone(&provider)).await?;
            qualify_one_question(provider).await?;
        }
        Some("zero") => qualify_zero_question(provider).await?,
        Some("question") => qualify_one_question(provider).await?,
        Some("steering") => qualify_steering(provider).await?,
        Some("browser") => browser::qualify(provider).await?,
        Some("browser-recovery") => browser::recover(provider).await?,
        Some("interactive") => qualify_interactive(provider).await?,
        Some(value) => {
            return Err(io::Error::other(format!(
                "unknown qualification case {value:?}; expected zero, question, steering, browser, or browser-recovery",
            ))
            .into());
        }
    }
    eprintln!("live design-expansion qualification passed");
    Ok(())
}

async fn qualify_interactive(provider: Arc<CountingProvider>) -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new([0xD9; 16], [0xDA; 16])?;
    let source = fs::read(fixture.repository.path().join("src/lib.rs"))?;
    let task = "Design a documented public Rust function named answer returning the u32 value 42. Preserve initial_value and follow the existing repository conventions. Use a safe reversible default for routine choices. This is an internal mechanical change; create the ready implementation design without editing workspace source or starting implementation.";
    provider.request_bound.store(16, Ordering::Release);
    let conversation: Arc<dyn ConversationView> = Arc::new(FixedConversation(task.to_owned()));
    let result = fixture.run(provider.clone(), conversation, task.into(), "interactive").await;
    fixture.retain("interactive")?;
    let (outcome, reached_writer) = result?;
    eprintln!("{}", outcome_diagnostic("interactive result", &outcome));
    require_ready_artifacts(&fixture.trace_path)?;
    if reached_writer || fs::read(fixture.repository.path().join("src/lib.rs"))? != source {
        return Err(io::Error::other(
            "guided Design crossed its read-only implementation boundary",
        )
        .into());
    }
    eprintln!(
        "guided Design published without workspace edits in {} requests",
        provider.requests()
    );
    Ok(())
}

async fn qualify_steering(provider: Arc<CountingProvider>) -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new([0xD5; 16], [0xD6; 16])?;
    let task = "Design a documented public Rust function answer returning the u32 value 42. Preserve initial_value and follow the existing repository conventions. This is an internal mechanical change with no unresolved product choice.";
    let conversation = Arc::new(MutableConversation::new(task));
    *provider.steering.lock().expect("steering lock") = Some((
        provider.requests() + 2,
        conversation.clone(),
        format!(
            "{task} Additional instruction: the documentation must state that answer is deterministic."
        ),
    ));
    provider.request_bound.store(provider.requests() + 24, Ordering::Release);
    let initial = provider.requests();
    let view: Arc<dyn ConversationView> = conversation;
    let (outcome, reached_writer) =
        fixture.run(provider.clone(), view, task.into(), "steering").await?;
    fixture.retain("steering")?;
    require_no_question(&outcome, "in-flight design steering")?;
    if !reached_writer {
        return Err(io::Error::other(outcome_diagnostic(
            "steered design never reached Writing",
            &outcome,
        ))
        .into());
    }
    require_ready_artifacts(&fixture.trace_path)?;
    let state: serde_json::Value = serde_json::from_slice(&fs::read(
        fixture.trace_path.with_extension("design-expansion.json"),
    )?)?;
    if state["binding"]["input_revision"] != 2 {
        return Err(
            io::Error::other("published design did not bind the incorporated steering").into()
        );
    }
    let markdown = fs::read_to_string(fixture.trace_path.with_extension("design.md"))?;
    if !markdown.contains("deterministic") || !markdown.contains("initial_value") {
        return Err(io::Error::other(
            "steered design omitted the new instruction or the original preservation requirement",
        )
        .into());
    }
    eprintln!(
        "in-flight steering reached Writing with {} model requests",
        provider.requests() - initial
    );
    Ok(())
}

async fn qualify_zero_question(provider: Arc<CountingProvider>) -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new([0xD1; 16], [0xD2; 16])?;
    let started = Instant::now();
    let initial_requests = provider.requests();
    let task = "Design a documented public Rust function named answer that returns the u32 value 42, with a unit test. Preserve the existing function. Use established repository conventions and safe reversible defaults; no product choice is delegated to the user.";
    let conversation: Arc<dyn ConversationView> = Arc::new(FixedConversation(task.to_owned()));
    let (outcome, reached_writer) =
        fixture.run(provider.clone(), conversation, task.to_owned(), "zero").await?;

    require_no_question(&outcome, "zero-question journey")?;
    if !reached_writer {
        return Err(io::Error::other(
            "zero-question journey did not publish a ready design before cancellation",
        )
        .into());
    }
    require_ready_artifacts(&fixture.trace_path)?;
    let receipt = fixture.trace_path.with_extension("design-receipt.json");
    fs::remove_file(&receipt)?;
    let fresh_requests = provider.requests();
    let reuse_conversation: Arc<dyn ConversationView> =
        Arc::new(FixedConversation(task.to_owned()));
    let (reused, reached_writer) =
        fixture.run(provider.clone(), reuse_conversation, task.to_owned(), "zero-reuse").await?;
    require_no_question(&reused, "unchanged expansion reuse journey")?;
    if !reached_writer || provider.requests() != fresh_requests {
        return Err(io::Error::other(format!(
            "unchanged expansion reuse did not reach Writing with zero provider requests: before={fresh_requests}, after={}",
            provider.requests(),
        ))
        .into());
    }
    if !receipt.is_file() {
        return Err(io::Error::other(
            "exact ready expansion reuse did not recreate its derived receipt",
        )
        .into());
    }
    fixture.record_summary(&serde_json::json!({
        "schema":1,
        "case":"zero-question-and-reuse",
        "provider":"openai-codex-runtime",
        "model":"gpt-6.1-sol",
        "fresh_provider_requests":fresh_requests.saturating_sub(initial_requests),
        "reuse_provider_requests":provider.requests().saturating_sub(fresh_requests),
        "elapsed_ms":started.elapsed().as_millis(),
        "provider_accounting":fixture.provider_metrics()?,
        "terminal":"ready-then-receiptless-exact-reuse",
    }))?;
    fixture.retain("zero-question")?;
    eprintln!("zero-question and zero-request reuse journeys passed");
    Ok(())
}

async fn qualify_one_question(provider: Arc<CountingProvider>) -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new([0xD3; 16], [0xD4; 16])?;
    let started = Instant::now();
    let initial_requests = provider.requests();
    let source = "Design a public command named choice that prints exactly one of the words alpha or beta. Both outcomes are equally acceptable, mutually incompatible, and have no repository, standards, interoperability, safety, cost, or reversibility basis that can resolve the choice. Only the user can select the intended product behavior. Ask exactly one focused material question before implementation, then retain and resume this exact design after the answer.";
    let answer = "Choose alpha. This is the final product decision. Continue the retained design without asking again.";
    let conversation = Arc::new(MutableConversation::new(source));
    let first_view: Arc<dyn ConversationView> = conversation.clone();
    let (first, reached_writer) =
        fixture.run(provider.clone(), first_view, source.to_owned(), "question").await?;
    if reached_writer {
        return Err(io::Error::other(
            "one-question journey entered implementation before receiving the material answer",
        )
        .into());
    }
    let question = first.question().ok_or_else(|| {
        io::Error::other(outcome_diagnostic("expected one material question", &first))
    })?;
    if question.conversation_revision() != 1 || question.message().trim().is_empty() {
        return Err(io::Error::other("material question was not bound to source revision 1").into());
    }
    let source_manifest = fixture.trace_path.with_extension("design-source.json");
    let retained_source = fs::read(&source_manifest)?;

    conversation.answer(answer);
    let second_view: Arc<dyn ConversationView> = conversation;
    let first_requests = provider.requests();
    let (second, reached_writer) =
        fixture.run(provider.clone(), second_view, source.to_owned(), "answer").await?;
    require_no_question(&second, "answered material-question journey")?;
    if !reached_writer {
        return Err(io::Error::other(outcome_diagnostic(
            "answered material-question journey did not resume through ready publication",
            &second,
        ))
        .into());
    }
    if fs::read(&source_manifest)? != retained_source {
        return Err(
            io::Error::other("answer resumption changed the immutable source design").into()
        );
    }
    require_ready_artifacts(&fixture.trace_path)?;
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(
        fixture.trace_path.with_extension("design-expansion.json"),
    )?)?;
    if manifest.pointer("/binding/input_revision").and_then(serde_json::Value::as_u64) != Some(2)
        || manifest.pointer("/binding/source_input_revision").and_then(serde_json::Value::as_u64)
            != Some(1)
        || manifest.get("answered_questions").and_then(serde_json::Value::as_array).map(Vec::len)
            != Some(1)
        || manifest.pointer("/records/material_question").is_some_and(|value| !value.is_null())
    {
        return Err(io::Error::other(
            "answered expansion did not retain exactly one answer bound to source revision 1 and input revision 2",
        )
        .into());
    }
    fixture.record_summary(&serde_json::json!({
        "schema":1,
        "case":"one-material-question",
        "provider":"openai-codex-runtime",
        "model":"gpt-6.1-sol",
        "question_provider_requests":first_requests.saturating_sub(initial_requests),
        "answer_resume_provider_requests":provider.requests().saturating_sub(first_requests),
        "total_provider_requests":provider.requests().saturating_sub(initial_requests),
        "elapsed_ms":started.elapsed().as_millis(),
        "provider_accounting":fixture.provider_metrics()?,
        "terminal":"waiting-then-ready",
        "answered_questions":1,
    }))?;
    fixture.retain("one-question")?;
    eprintln!("one-question answer/resume journey passed");
    Ok(())
}

struct CountingProvider {
    inner: Arc<dyn ModelProvider>,
    requests: AtomicU64,
    steering: Mutex<Option<(u64, Arc<MutableConversation>, String)>>,
    request_bound: AtomicU64,
}

impl CountingProvider {
    fn new(inner: Arc<dyn ModelProvider>) -> Self {
        Self {
            inner,
            requests: AtomicU64::new(0),
            steering: Mutex::new(None),
            request_bound: AtomicU64::new(u64::MAX),
        }
    }

    fn requests(&self) -> u64 {
        self.requests.load(Ordering::Acquire)
    }
}

impl ModelProvider for CountingProvider {
    fn profile(&self) -> &ProviderProfile {
        self.inner.profile()
    }

    fn request_profile(&self, output_tokens: u64) -> Result<ProviderProfile, ProviderCoreError> {
        self.inner.request_profile(output_tokens)
    }

    fn context_input_target(&self) -> u64 {
        self.inner.context_input_target()
    }

    fn prepare_context<'a>(
        &'a self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<(), ProviderCoreError>> {
        self.inner.prepare_context(cancellation)
    }

    fn context_input_fits(
        &self,
        messages: &[peritus_model_protocol::Message],
        tools: &[peritus_model_protocol::ToolDefinition],
    ) -> Result<bool, ProviderCoreError> {
        self.inner.context_input_fits(messages, tools)
    }

    fn reasoning_effort(&self) -> Option<peritus_model_protocol::ReasoningEffort> {
        self.inner.reasoning_effort()
    }

    fn supports_reasoning_effort(&self, effort: peritus_model_protocol::ReasoningEffort) -> bool {
        self.inner.supports_reasoning_effort(effort)
    }

    fn availability(&self) -> peritus_provider_core::ProviderAvailability {
        self.inner.availability()
    }

    fn start(
        &self,
        request: peritus_model_protocol::ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        let request_number = self.requests.fetch_add(1, Ordering::AcqRel) + 1;
        if request_number > self.request_bound.load(Ordering::Acquire) {
            return Box::pin(async {
                Err(ProviderCoreError::configuration(
                    "qualification_request_bound",
                    "design did not publish within the qualification request bound",
                ))
            });
        }
        let mut steering = self.steering.lock().expect("steering lock");
        if steering.as_ref().is_some_and(|(at, _, _)| request_number == *at) {
            let (_, conversation, message) = steering.take().expect("selected steering");
            conversation.answer(&message);
        }
        drop(steering);
        self.inner.start(request, cancellation)
    }
}

fn require_ready_artifacts(trace_path: &Path) -> Result<(), Box<dyn Error>> {
    let design = fs::read_to_string(trace_path.with_extension("design.md"))?;
    let source = fs::read(trace_path.with_extension("design-source.json"))?;
    let expansion: serde_json::Value =
        serde_json::from_slice(&fs::read(trace_path.with_extension("design-expansion.json"))?)?;
    if design.trim().is_empty()
        || source.is_empty()
        || expansion.get("stage") != Some(&serde_json::json!("ready"))
    {
        return Err(
            io::Error::other("ready design, source, or expansion artifact is absent").into()
        );
    }
    Ok(())
}

fn require_no_question(outcome: &ProductRunOutcome, journey: &str) -> Result<(), Box<dyn Error>> {
    if outcome.question().is_some() {
        return Err(io::Error::other(outcome_diagnostic(
            &format!("{journey} unexpectedly asked a question"),
            outcome,
        ))
        .into());
    }
    Ok(())
}

fn outcome_diagnostic(prefix: &str, outcome: &ProductRunOutcome) -> String {
    format!(
        "{prefix}: disposition={:?}; detail={:?}; remaining={:?}; question={:?}",
        outcome.settlement().disposition(),
        outcome.detail(),
        outcome.remaining_work(),
        outcome.question().map(peritus_product_runner::ProductRunQuestion::message),
    )
}

struct Fixture {
    repository: tempfile::TempDir,
    state: tempfile::TempDir,
    run_id: RunId,
    workspace_id: WorkspaceId,
    trace_path: PathBuf,
    cancel_at_writer: bool,
    expected_edit: Option<PathBuf>,
}

impl Fixture {
    fn new(run: [u8; 16], workspace: [u8; 16]) -> Result<Self, Box<dyn Error>> {
        let repository = tempfile::tempdir()?;
        let state = tempfile::tempdir()?;
        initialize_repository(repository.path())?;
        let trace_path = state.path().join("peritus-design.trace");
        Ok(Self {
            repository,
            state,
            run_id: RunId::new(run).expect("nonzero qualification run id"),
            workspace_id: WorkspaceId::new(workspace).expect("nonzero workspace id"),
            trace_path,
            cancel_at_writer: true,
            expected_edit: None,
        })
    }

    fn record_summary(&self, value: &serde_json::Value) -> Result<(), Box<dyn Error>> {
        fs::write(
            self.state.path().join("qualification-summary.json"),
            serde_json::to_vec_pretty(&value)?,
        )?;
        Ok(())
    }

    fn provider_metrics(&self) -> Result<serde_json::Value, Box<dyn Error>> {
        self.provider_metrics_since(0)
    }

    fn provider_metrics_since(
        &self,
        first_line: usize,
    ) -> Result<serde_json::Value, Box<dyn Error>> {
        let text = fs::read_to_string(self.trace_path.with_extension("efficiency.jsonl"))?;
        let mut accounting = None;
        let mut attempts = 0_u64;
        let mut attempt_micros = 0_u64;
        for line in text.lines().skip(first_line) {
            let value: serde_json::Value = serde_json::from_str(line)?;
            match value["event"].as_str() {
                Some("provider-accounting") => accounting = Some(value),
                Some("provider-attempt-work") => {
                    attempts = attempts.saturating_add(1);
                    attempt_micros = attempt_micros
                        .saturating_add(value["elapsed_micros"].as_u64().unwrap_or_default());
                }
                _ => {}
            }
        }
        let accounting = accounting
            .ok_or_else(|| io::Error::other("qualification trace omitted provider accounting"))?;
        Ok(serde_json::json!({
            "model_requests":accounting["model_requests"],
            "input_tokens":accounting["input_tokens"],
            "cached_input_tokens":accounting["cached_input_tokens"],
            "output_tokens":accounting["output_tokens"],
            "attempts":attempts,
            "attempt_elapsed_micros":attempt_micros,
        }))
    }

    fn retain(&self, name: &str) -> Result<(), Box<dyn Error>> {
        let Some(root) = std::env::var_os("PERITUS_DESIGN_QUALIFICATION_OUTPUT") else {
            return Ok(());
        };
        let destination = PathBuf::from(root).join(name);
        if destination.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("qualification evidence already exists: {}", destination.display()),
            )
            .into());
        }
        copy_tree(self.repository.path(), &destination.join("repository"))?;
        copy_tree(self.state.path(), &destination.join("state"))?;
        Ok(())
    }

    async fn run(
        &self,
        provider: Arc<dyn ModelProvider>,
        conversation: Arc<dyn ConversationView>,
        task: String,
        attempt: &str,
    ) -> Result<(ProductRunOutcome, bool), Box<dyn Error>> {
        let process_store = peritus_process::ProcessStore::open(
            self.state.path().join(format!("processes-{attempt}")),
            self.repository.path(),
        )?;
        let command_runtime = peritus_product_runner::CommandRuntime::open(
            self.state.path().join(format!("router-{attempt}")),
            self.repository.path(),
            self.run_id,
            process_store,
        )?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let reached_writer = Arc::new(AtomicBool::new(false));
        let observe_cancelled = Arc::clone(&cancelled);
        let observe_writer = Arc::clone(&reached_writer);
        let cancel_at_writer = self.cancel_at_writer;
        let observer: RunObserver = Arc::new(move |update| {
            eprintln!("{:?}: {}", update.phase, update.status);
            if update.phase == ProductRunPhase::Writing {
                observe_writer.store(true, Ordering::Release);
                if cancel_at_writer {
                    observe_cancelled.store(true, Ordering::Release);
                }
            }
        });
        let cancellation = CancellationToken::new();
        let watch = self.expected_edit.as_ref().map(|path| {
            let path = path.clone();
            let cancelled = Arc::clone(&cancelled);
            let token = cancellation.clone();
            tokio::spawn(async move {
                loop {
                    if fs::read_to_string(&path).is_ok_and(|text| {
                        text.lines().any(|line| {
                            line.trim() == "pub const PERITUS_EXPERT_HANDOFF: bool = true;"
                        })
                    }) {
                        cancelled.store(true, Ordering::Release);
                        let _ = token.cancel();
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
        });
        let input = ProductRunInput {
            workspace_kind: peritus_product_runner::ProductWorkspaceKind::Managed,
            run_id: self.run_id,
            workspace_id: self.workspace_id,
            workspace_root: self.repository.path().to_owned(),
            source_root: self.repository.path().to_owned(),
            trace_path: self.trace_path.clone(),
            command_runtime,
            module_jobs: None,
            finding_state: String::new(),
            task,
            max_elapsed: Some(Duration::from_mins(30)),
            delivery_scope: ProductDeliveryScope::WorkspaceChanges,
            conversation,
            providers: RoleProviders {
                writer: Arc::clone(&provider),
                reviewer: Arc::clone(&provider),
                fixer: provider,
                fallbacks: Vec::new(),
            },
            cancelled,
            provider_cancellation: cancellation,
            resume: None,
        };
        let output = if attempt == "interactive" {
            ProductRunner::converse(
                input,
                peritus_product_runner::ConversationMode::Design,
                observer,
            )
            .await
        } else {
            ProductRunner::run(input, observer).await
        };
        if let Some(watch) = watch {
            watch.abort();
        }
        let output = output?;
        Ok((output, reached_writer.load(Ordering::Acquire)))
    }
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            return Err(io::Error::other(format!(
                "qualification evidence contains an unsupported file type: {}",
                entry.path().display(),
            ))
            .into());
        }
    }
    Ok(())
}

struct FixedConversation(String);

impl ConversationView for FixedConversation {
    fn revision(&self) -> u64 {
        1
    }

    fn render(&self) -> String {
        format!("User:\n{}", self.0)
    }

    fn source_design_user_text(&self, input_revision: u64) -> Result<Option<String>, String> {
        if input_revision == 1 {
            Ok(Some(self.0.clone()))
        } else {
            Err("zero-question qualification revision is stale".to_owned())
        }
    }
}

struct MutableConversation(Mutex<ConversationState>);

struct ConversationState {
    source: String,
    answer: Option<String>,
}

impl MutableConversation {
    fn new(source: &str) -> Self {
        Self(Mutex::new(ConversationState { source: source.to_owned(), answer: None }))
    }

    fn answer(&self, answer: &str) {
        self.0.lock().expect("conversation lock").answer = Some(answer.to_owned());
    }
}

impl ConversationView for MutableConversation {
    fn revision(&self) -> u64 {
        if self.0.lock().expect("conversation lock").answer.is_some() { 2 } else { 1 }
    }

    fn render(&self) -> String {
        let state = self.0.lock().expect("conversation lock");
        state.answer.as_ref().map_or_else(
            || format!("User:\n{}", state.source),
            |answer| format!("User:\n{}\n\nUser:\n{answer}", state.source),
        )
    }

    fn source_design_user_text(&self, input_revision: u64) -> Result<Option<String>, String> {
        let state = self.0.lock().expect("conversation lock");
        match (input_revision, &state.answer) {
            (1, _) => Ok(Some(state.source.clone())),
            (2, Some(answer)) => Ok(Some(answer.clone())),
            _ => Err("one-question qualification revision is stale".to_owned()),
        }
    }
}

fn codex_provider() -> Result<Arc<CodexRuntimeProvider>, Box<dyn Error>> {
    let executable = CodexExecutable::discover()?;
    let config = CodexRuntimeConfig::new(
        executable,
        profile([0xA2; 16], "openai", "gpt-6.1-sol", WireDialect::OpenAiCodexRuntime)?,
        process_limits(2 * 1024 * 1024)?,
    )?;
    Ok(Arc::new(CodexRuntimeProvider::new(config)))
}

fn profile(
    identity: [u8; 16],
    provider: &str,
    model: &str,
    dialect: WireDialect,
) -> Result<ProviderProfile, Box<dyn Error>> {
    Ok(ProviderProfile::new(
        ProviderProfileId::new(identity)
            .map_err(|_| io::Error::other("provider profile identity is invalid"))?,
        1,
        ProviderName::new(provider.to_owned())?,
        ModelName::new(model.to_owned())?,
        dialect,
        CapabilityMatrix::new(
            &[Capability::ToolCalls, Capability::ParallelToolCalls, Capability::UsageDetail],
            &[],
        )?,
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 32_000, 128, 8, 1)?,
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )?)
}

fn process_limits(output_bytes: usize) -> Result<ProcessLimits, Box<dyn Error>> {
    Ok(ProcessLimits::new(output_bytes, output_bytes, 64 * 1024, Duration::from_mins(5))?)
}

fn initialize_repository(root: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"peritus-live-design-expansion\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fs::write(
        root.join("src/lib.rs"),
        "//! Live design-expansion fixture.\n\n#[must_use]\npub const fn initial_value() -> u32 { 1 }\n",
    )?;
    checked(Command::new("git").args(["init", "--quiet"]).current_dir(root), "git init")?;
    checked(
        Command::new("git").args(["add", "Cargo.toml", "src/lib.rs"]).current_dir(root),
        "git add",
    )?;
    checked(
        Command::new("git")
            .args([
                "-c",
                "user.name=Peritus Qualification",
                "-c",
                "user.email=peritus@localhost",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "Initialize live design fixture",
            ])
            .current_dir(root),
        "git commit",
    )?;
    Ok(())
}

fn checked(command: &mut Command, operation: &str) -> Result<(), Box<dyn Error>> {
    let status = command.status()?;
    if !status.success() {
        return Err(io::Error::other(format!("{operation} failed with {status}")).into());
    }
    Ok(())
}
