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
    let fixture = LiveFixture::prepare()?;
    let (repository, command_state, reuse) =
        (&fixture.repository, &fixture.command_state, fixture.reuse);
    let cancellation = CancellationToken::new();
    let writer = codex_provider()?;
    writer.require_authenticated(&cancellation).await?;
    let writer: Arc<dyn ModelProvider> = peritus_provider_core::select_reasoning_effort(
        writer,
        peritus_model_protocol::ReasoningEffort::High,
    )?;
    let captured = Arc::new(CapturingProvider::new(writer));
    let writer: Arc<dyn ModelProvider> = captured.clone();
    let reviewer = Arc::clone(&writer);
    let observer: RunObserver =
        Arc::new(|update| eprintln!("{:?}: {}", update.phase, update.status));
    let delivery_campaign = std::env::var("PERITUS_DELIVERY_CAMPAIGN").as_deref() == Ok("1");
    let task = if delivery_campaign {
        "Implement these separate required capabilities in this native Rust program: (1) a documented public answer() -> u32 returning 42 while preserving initial_value(); (2) a real CLI `answer` printing exactly 42 and a newline; (3) a real CLI `double N` accepting a decimal u32 and printing twice it and a newline; (4) reject missing arguments, unknown commands, invalid integers, extra arguments, and overflow with a nonzero status, an explanatory stderr message and empty stdout. Replace the existing blanket unsupported handler with real operations. Use actual CLI verification, including failure cases and preservation of the existing library behavior; do not add a unit-test suite or dependencies. Run instructions should be cargo run -- answer."
    } else {
        "Add a documented public function named answer that returns u32 value 42, and add a unit test that proves it. Keep the existing function."
    }.to_owned();
    if delivery_campaign && !reuse {
        fs::write(
            repository.as_path().join("src/main.rs"),
            "fn main() { eprintln!(\"unsupported\"); std::process::exit(2); }\n",
        )?;
        checked(
            Command::new("git").args(["add", "src/main.rs"]).current_dir(repository.as_path()),
            "track original CLI",
        )?;
    }
    let run_id = RunId::new([0xE4; 16]).expect("nonzero qualification run id");
    let process_store = peritus_process::ProcessStore::open(
        command_state.as_path().join("processes"),
        repository.as_path(),
    )?;
    let command_runtime = peritus_product_runner::CommandRuntime::open(
        command_state.as_path().join("router"),
        repository.as_path(),
        run_id,
        process_store,
    )?;
    let output = ProductRunner::run(
        ProductRunInput {
            workspace_kind: peritus_product_runner::ProductWorkspaceKind::Managed,
            run_id,
            workspace_id: WorkspaceId::new([0xE5; 16]).expect("nonzero workspace id"),
            workspace_root: repository.as_path().to_owned(),
            source_root: repository.as_path().to_owned(),
            trace_path: command_state.as_path().join("peritus-run.trace"),
            command_runtime,
            module_jobs: None,
            finding_state: String::new(),
            task: task.clone(),
            max_elapsed: None,
            delivery_scope: ProductDeliveryScope::WorkspaceChanges,
            conversation: Arc::new(FixedConversation(task, reuse)),
            pipeline_handoff: peritus_product_runner::PipelineHandoff::CurrentConversation,
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
    let source = fs::read_to_string(repository.as_path().join("src/lib.rs"))?;
    if !source.contains("answer") || !source.contains("42") || output.diff.is_empty() {
        return Err(
            io::Error::other("product run did not produce the requested tested change").into()
        );
    }
    let design = fs::read_to_string(command_state.as_path().join("peritus-run.design.md"))?;
    captured.verify_shared_design_contract(&design)?;
    if delivery_campaign {
        verify_delivery_campaign(repository.as_path(), command_state.as_path())?;
    }
    let _ = (output.changed_files(), output.fixer_cycles);
    eprintln!("live account product-run and shared design-contract qualification passed");
    Ok(())
}

fn verify_delivery_campaign(
    repository: &std::path::Path,
    state: &std::path::Path,
) -> Result<(), Box<dyn Error>> {
    for arguments in
        [vec!["answer"], vec!["double", "21"], vec!["double", "0"], vec!["double", "2147483647"]]
    {
        let output = Command::new("cargo")
            .args(["run", "--quiet", "--"])
            .args(&arguments)
            .current_dir(repository)
            .output()?;
        let expected = match arguments.as_slice() {
            ["answer"] | ["double", "21"] => b"42\n".as_slice(),
            ["double", "0"] => b"0\n".as_slice(),
            _ => b"4294967294\n".as_slice(),
        };
        if !output.status.success() || output.stdout != expected {
            return Err(
                io::Error::other(format!("real CLI failed {arguments:?}: {output:?}")).into()
            );
        }
    }
    for arguments in [
        vec![],
        vec!["unknown"],
        vec!["double"],
        vec!["double", "no"],
        vec!["double", "2147483648"],
        vec!["double", "4294967296"],
        vec!["answer", "extra"],
        vec!["double", "1", "extra"],
    ] {
        let output = Command::new("cargo")
            .args(["run", "--quiet", "--"])
            .args(&arguments)
            .current_dir(repository)
            .output()?;
        if output.status.success() || !output.stdout.is_empty() || output.stderr.is_empty() {
            return Err(io::Error::other(format!(
                "real CLI accepted invalid input {arguments:?}: {output:?}"
            ))
            .into());
        }
    }
    fs::write(
        state.join("consumer.rs"),
        "fn main() { assert_eq!(peritus_live_product_run::initial_value(), 1); assert_eq!(peritus_live_product_run::answer(), 42); }\n",
    )?;
    checked(
        Command::new("cargo").args(["build", "--quiet"]).current_dir(repository),
        "build the actual library",
    )?;
    checked(
        Command::new("rustc")
            .arg(state.join("consumer.rs"))
            .args(["--edition", "2024", "--extern"])
            .arg(format!(
                "peritus_live_product_run={}",
                repository.join("target/debug/libperitus_live_product_run.rlib").display()
            ))
            .arg("-o")
            .arg(state.join("consumer")),
        "compile independent public-library consumer",
    )?;
    checked(
        &mut Command::new(state.join("consumer")),
        "operate original and new public-library behavior",
    )?;
    let ledger: serde_json::Value =
        serde_json::from_slice(&fs::read(state.join("peritus-run.delivery.json"))?)?;
    let plan =
        ledger["plan"].as_array().ok_or_else(|| io::Error::other("no durable capability plan"))?;
    let drafted = ledger["drafted"]
        .as_array()
        .ok_or_else(|| io::Error::other("no durable draft coverage"))?;
    let assessments = ledger["qualification"][1]["capabilities"]
        .as_array()
        .ok_or_else(|| io::Error::other("no independent capability evidence"))?;
    if plan.len() < 2
        || drafted.len() != plan.len()
        || assessments.len() != plan.len()
        || assessments.iter().any(|item| item["satisfied"] != true)
    {
        return Err(io::Error::other("durable plan and independent qualification disagree").into());
    }
    eprintln!(
        "PASS: {} separate capabilities, durable draft coverage and independent evidence; 4 native success journeys and 8 native failure journeys",
        plan.len()
    );
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

    fn prepare_context<'a>(
        &'a self,
        output_tokens: u64,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<peritus_provider_core::PreparedModelContext, ProviderCoreError>> {
        self.inner.prepare_context(output_tokens, cancellation)
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

struct FixedConversation(String, bool);

impl ConversationView for FixedConversation {
    fn revision(&self) -> u64 {
        1
    }

    fn render(&self) -> String {
        format!("User:\n{}", self.0)
    }

    fn stable_request_context(&self) -> String {
        if self.1 {
            format!(
                "{}\n\nDisposable restart replay: construction drafts are already retained. This new process has no admitted successful-command vector. Run fresh real-interface verification before handing off completion; reuse the existing design and draft coverage.",
                self.render()
            )
        } else {
            self.render()
        }
    }

    fn source_design_user_text(&self, input_revision: u64) -> Result<Option<String>, String> {
        if input_revision == 1 {
            Ok(Some(self.0.clone()))
        } else {
            Err("qualification conversation revision is stale".to_owned())
        }
    }
}

struct LiveFixture {
    repository: std::path::PathBuf,
    command_state: std::path::PathBuf,
    reuse: bool,
    _repository_guard: Option<tempfile::TempDir>,
    _state_guard: Option<tempfile::TempDir>,
}

impl LiveFixture {
    fn prepare() -> Result<Self, Box<dyn Error>> {
        let reuse = std::env::var_os("PERITUS_LIVE_WORKSPACE").is_some();
        if reuse != std::env::var_os("PERITUS_LIVE_STATE").is_some() {
            return Err(io::Error::other(
                "set both PERITUS_LIVE_WORKSPACE and PERITUS_LIVE_STATE for a disposable replay",
            )
            .into());
        }
        let (repository, repository_guard) = fixture_location("PERITUS_LIVE_WORKSPACE")?;
        let (command_state, state_guard) = fixture_location("PERITUS_LIVE_STATE")?;
        if !reuse {
            initialize_repository(repository.as_path())?;
        }
        if !fs::read_to_string(repository.join("Cargo.toml"))?.contains("peritus-live-product-run")
        {
            return Err(
                io::Error::other("replay requires the disposable native campaign fixture").into()
            );
        }
        Ok(Self {
            repository,
            command_state,
            reuse,
            _repository_guard: repository_guard,
            _state_guard: state_guard,
        })
    }
}

fn fixture_location(
    variable: &str,
) -> Result<(std::path::PathBuf, Option<tempfile::TempDir>), io::Error> {
    std::env::var_os(variable).map_or_else(
        || {
            let directory = tempfile::tempdir()?;
            Ok((directory.path().to_owned(), Some(directory)))
        },
        |path| Ok((path.into(), None)),
    )
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
            &[
                Capability::ToolCalls,
                Capability::ParallelToolCalls,
                Capability::UsageDetail,
                Capability::ReasoningControls,
            ],
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
