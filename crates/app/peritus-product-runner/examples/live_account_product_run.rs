//! Credentialed end-to-end writer/reviewer/fixer qualification on a temporary Rust repository.

use std::{
    error::Error,
    fs, io,
    process::Command,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, ModelLimits, ModelName,
    OutputLimitEnforcement, ProviderName, ProviderProfile, ResumeKind, StateMode, WireDialect,
};
use peritus_product_runner::{
    ConversationView, ProductRunInput, ProductRunOutcome, ProductRunner, RoleProviders, RunObserver,
};
use peritus_provider_anthropic::{ClaudeExecutable, ClaudeRuntimeConfig, ClaudeRuntimeProvider};
use peritus_provider_core::{CancellationToken, ModelProvider, ProcessLimits};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_types::{ProviderProfileId, RunId};

fn main() -> Result<(), Box<dyn Error>> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Result<(), Box<dyn Error>> {
    let repository = tempfile::tempdir()?;
    initialize_repository(repository.path())?;
    let cancellation = CancellationToken::new();
    let writer = codex_provider()?;
    let reviewer = claude_provider()?;
    writer.require_authenticated(&cancellation).await?;
    reviewer.require_authenticated(&cancellation).await?;
    let writer: Arc<dyn ModelProvider> = writer;
    let reviewer: Arc<dyn ModelProvider> = reviewer;
    let observer: RunObserver = Arc::new(|_| {});
    let task = "Add a documented public function named answer that returns u32 value 42, and add a unit test that proves it. Keep the existing function.".to_owned();
    let output = ProductRunner::run(
        ProductRunInput {
            run_id: RunId::new([0xE4; 16]).expect("nonzero qualification run id"),
            workspace_root: repository.path().to_owned(),
            trace_path: repository.path().join("peritus-run.trace"),
            finding_state: String::new(),
            task: task.clone(),
            conversation: Arc::new(FixedConversation(task)),
            providers: RoleProviders {
                writer: Arc::clone(&writer),
                reviewer,
                fixer: writer,
                fallbacks: Vec::new(),
            },
            cancelled: Arc::new(AtomicBool::new(false)),
            provider_cancellation: cancellation,
        },
        observer,
    )
    .await?;
    let ProductRunOutcome::Complete(output) = output else {
        return Err(io::Error::other("product run unexpectedly asked for clarification").into());
    };
    let source = fs::read_to_string(repository.path().join("src/lib.rs"))?;
    if !source.contains("answer") || !source.contains("42") || output.diff.is_empty() {
        return Err(
            io::Error::other("product run did not produce the requested tested change").into()
        );
    }
    let _ = (output.changed_files(), output.fixer_cycles);
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
}

fn codex_provider() -> Result<Arc<CodexRuntimeProvider>, Box<dyn Error>> {
    let executable = CodexExecutable::discover()?;
    let config = CodexRuntimeConfig::new(
        executable,
        profile([0xA1; 16], "openai", "gpt-5.6-sol", WireDialect::OpenAiCodexRuntime)?,
        process_limits(2 * 1024 * 1024)?,
    )?;
    Ok(Arc::new(CodexRuntimeProvider::new(config)))
}

fn claude_provider() -> Result<Arc<ClaudeRuntimeProvider>, Box<dyn Error>> {
    let executable = ClaudeExecutable::discover()?;
    let config = ClaudeRuntimeConfig::new(
        executable,
        profile([0xA2; 16], "anthropic", "sonnet", WireDialect::AnthropicClaudeRuntime)?,
        process_limits(16 * 1024 * 1024)?,
    )?;
    Ok(Arc::new(ClaudeRuntimeProvider::new(config)))
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
        ModelLimits::new(200_000, 32_000, 32, 8, 1)?,
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
