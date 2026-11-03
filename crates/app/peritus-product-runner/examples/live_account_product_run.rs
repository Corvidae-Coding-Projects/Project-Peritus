//! Credentialed end-to-end writer/reviewer/fixer qualification on a temporary Rust repository.

use std::{
    error::Error,
    fs, io,
    process::Command,
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Duration,
};

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, ContentBlock,
    ModelLimits, ModelName, OutputLimitEnforcement, ProviderName, ProviderProfile, ResumeKind,
    StateMode, WireDialect,
};
use peritus_product_runner::{
    ConversationView, ProductDeliveryScope, ProductRunInput, ProductRunner, RoleProviders,
    RunObserver,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, OwnedModelStream, ProcessLimits, ProviderCoreError,
};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

fn main() -> Result<(), Box<dyn Error>> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Result<(), Box<dyn Error>> {
    let repository = tempfile::tempdir()?;
    let command_state = tempfile::tempdir()?;
    initialize_repository(repository.path())?;
    let cancellation = CancellationToken::new();
    let writer = codex_provider()?;
    writer.require_authenticated(&cancellation).await?;
    let writer: Arc<dyn ModelProvider> = writer;
    let captured = Arc::new(CapturingProvider::new(writer));
    let writer: Arc<dyn ModelProvider> = captured.clone();
    let reviewer = Arc::clone(&writer);
    let observer: RunObserver = Arc::new(|_| {});
    let task = "Add a documented public function named answer that returns u32 value 42, and add a unit test that proves it. Keep the existing function.".to_owned();
    let run_id = RunId::new([0xE4; 16]).expect("nonzero qualification run id");
    let process_store = peritus_process::ProcessStore::open(
        command_state.path().join("processes"),
        repository.path(),
    )?;
    let command_runtime = peritus_product_runner::CommandRuntime::open(
        command_state.path().join("router"),
        repository.path(),
        run_id,
        process_store,
    )?;
    let output = ProductRunner::run(
        ProductRunInput {
            workspace_kind: peritus_product_runner::ProductWorkspaceKind::Managed,
            run_id,
            workspace_id: WorkspaceId::new([0xE5; 16]).expect("nonzero workspace id"),
            workspace_root: repository.path().to_owned(),
            source_root: repository.path().to_owned(),
            trace_path: command_state.path().join("peritus-run.trace"),
            command_runtime,
            module_jobs: None,
            finding_state: String::new(),
            task: task.clone(),
            max_elapsed: None,
            delivery_scope: ProductDeliveryScope::WorkspaceChanges,
            conversation: Arc::new(FixedConversation(task)),
            providers: RoleProviders {
                writer: Arc::clone(&writer),
                reviewer,
                fixer: writer,
                fallbacks: Vec::new(),
            },
            cancelled: Arc::new(AtomicBool::new(false)),
            provider_cancellation: cancellation,
            resume: None,
        },
        observer,
    )
    .await?;
    if !output.settlement().is_accepted() {
        return Err(io::Error::other(format!(
            "product run was not accepted: {:?}; detail={:?}; remaining={:?}; question={:?}",
            output.settlement().disposition(),
            output.detail(),
            output.remaining_work(),
            output.question().map(peritus_product_runner::ProductRunQuestion::message),
        ))
        .into());
    }
    let output = output
        .candidate()
        .ok_or_else(|| io::Error::other("accepted product run had no candidate"))?;
    let source = fs::read_to_string(repository.path().join("src/lib.rs"))?;
    if !source.contains("answer") || !source.contains("42") || output.diff.is_empty() {
        return Err(
            io::Error::other("product run did not produce the requested tested change").into()
        );
    }
    let design = fs::read_to_string(command_state.path().join("peritus-run.design.md"))?;
    captured.verify_shared_design_contract(&design)?;
    let _ = (output.changed_files(), output.fixer_cycles);
    eprintln!("live account product-run and shared design-contract qualification passed");
    Ok(())
}

struct CapturingProvider {
    inner: Arc<dyn ModelProvider>,
    requests: Mutex<Vec<String>>,
}

impl CapturingProvider {
    fn new(inner: Arc<dyn ModelProvider>) -> Self {
        Self { inner, requests: Mutex::new(Vec::new()) }
    }

    fn verify_shared_design_contract(&self, design: &str) -> Result<(), io::Error> {
        let (writer, reviewer) = {
            let requests = self.requests.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let writer = requests
                .iter()
                .find(|value| value.contains("Accepted implementation design identity"))
                .ok_or_else(|| {
                    io::Error::other("writer request did not carry the design identity")
                })?
                .clone();
            let reviewer = requests
                .iter()
                .find(|value| value.contains("Accepted expanded implementation design identity"))
                .ok_or_else(|| {
                    io::Error::other("reviewer request did not carry the design identity")
                })?
                .clone();
            drop(requests);
            (writer, reviewer)
        };
        if !writer.contains(design) || !reviewer.contains(design) {
            return Err(io::Error::other(
                "writer and reviewer did not both receive the exact expanded Markdown",
            ));
        }
        let writer_identity = expansion_identity_line(&writer)?;
        let reviewer_identity = expansion_identity_line(&reviewer)?;
        if writer_identity != reviewer_identity {
            return Err(io::Error::other(format!(
                "writer/reviewer expansion identities differ: writer={writer_identity}; reviewer={reviewer_identity}",
            )));
        }
        for field in [
            "source_sha256=",
            "manifest_sha256=",
            "expanded_sha256=",
            "task_sha256=",
            "evidence_sha256=",
            "input_revision=",
            "policy=",
        ] {
            if !writer_identity.contains(field) {
                return Err(io::Error::other(format!("shared expansion identity omitted {field}")));
            }
        }
        if !reviewer.contains("independent of the writer's report") {
            return Err(io::Error::other(
                "reviewer request did not preserve independent design authority",
            ));
        }
        Ok(())
    }
}

fn expansion_identity_line(request: &str) -> Result<&str, io::Error> {
    request
        .lines()
        .find(|line| line.starts_with("source_sha256="))
        .ok_or_else(|| io::Error::other("request omitted the complete expansion identity line"))
}

impl ModelProvider for CapturingProvider {
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
        let text = request
            .messages()
            .iter()
            .flat_map(peritus_model_protocol::Message::content)
            .filter_map(|block| match block {
                ContentBlock::Text(value) => Some(value.expose_for_wire()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.requests.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(text);
        self.inner.start(request, cancellation)
    }
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
            Err("qualification conversation revision is stale".to_owned())
        }
    }
}

fn codex_provider() -> Result<Arc<CodexRuntimeProvider>, Box<dyn Error>> {
    let executable = CodexExecutable::discover()?;
    let config = CodexRuntimeConfig::new(
        executable,
        profile([0xA1; 16], "openai", "gpt-6.1-sol", WireDialect::OpenAiCodexRuntime)?,
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

fn initialize_repository(root: &std::path::Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"peritus-live-product-run\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fs::write(
        root.join("src/lib.rs"),
        "//! Live product-run fixture.\n\n#[must_use]\npub const fn initial_value() -> u32 { 1 }\n",
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
                "Initialize live product fixture",
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
