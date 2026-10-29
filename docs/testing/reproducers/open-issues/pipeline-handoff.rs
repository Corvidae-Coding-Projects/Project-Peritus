use peritus_model_protocol::*;
use peritus_product_runner::{
    ConversationMode, ConversationView, ProductDeliveryScope, ProductRunInput, ProductRunner,
    ProductWorkspaceKind, RoleProviders,
};
use peritus_provider_core::{
    CancellationToken, Credential, CredentialReference, CredentialSource, ModelProvider,
    ProcessLimits, ProviderCoreError,
};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_provider_opencode::{OpenCodeExecutable, OpenCodeRuntimeProvider};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

struct Conversation(String);
impl ConversationView for Conversation {
    fn revision(&self) -> u64 {
        1
    }
    fn render(&self) -> String {
        self.0.clone()
    }
}
struct Key;
impl CredentialSource for Key {
    fn resolve(&self, _: &CredentialReference) -> Result<Credential, ProviderCoreError> {
        let file = std::env::var_os("PERITUS_REAL_KEY_FILE")
            .ok_or_else(|| ProviderCoreError::credential("select PERITUS_REAL_KEY_FILE for Zen"))?;
        let bytes = fs::read(file)
            .map_err(|_| ProviderCoreError::credential("authorized test key unavailable"))?;
        Credential::new(bytes.trim_ascii().to_vec())
    }
}
fn provider(zen: bool) -> Result<Arc<dyn ModelProvider>, Box<dyn Error>> {
    let profile = ProviderProfile::new(
        ProviderProfileId::new([0x95; 16]).map_err(|_| "profile")?,
        1,
        ProviderName::new(if zen { "opencode" } else { "openai" }.to_owned())?,
        ModelName::new(if zen { "deepseek-v4.1-flash" } else { "gpt-6.1-sol" }.to_owned())?,
        if zen { WireDialect::OpenCodeRuntime } else { WireDialect::OpenAiCodexRuntime },
        CapabilityMatrix::new(
            &[Capability::ToolCalls, Capability::ParallelToolCalls, Capability::UsageDetail],
            &[],
        )?,
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 16000, 32, 8, 1)?,
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )?;
    if zen {
        let executable = match std::env::var_os("PERITUS_REAL_CAPTURE") {
            Some(path) => OpenCodeExecutable::pin(path)?,
            None => OpenCodeExecutable::discover()?,
        };
        Ok(Arc::new(OpenCodeRuntimeProvider::new(
            &executable,
            peritus_provider_core::hosted::HostedService::OpenCodeZen,
            CredentialReference::new("real-issue159-key".into())?,
            Arc::new(Key),
            profile,
        )?))
    } else {
        Ok(Arc::new(CodexRuntimeProvider::new(CodexRuntimeConfig::new(
            CodexExecutable::discover()?,
            profile,
            ProcessLimits::new(
                16 * 1024 * 1024,
                16 * 1024 * 1024,
                65536,
                Duration::from_secs(300),
            )?,
        )?)))
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let arguments = std::env::args().collect::<Vec<_>>();
    let directory = PathBuf::from(arguments.get(1).ok_or("output directory required")?);
    if !directory.is_absolute()
        || (directory.exists() && fs::read_dir(&directory)?.next().is_some())
    {
        return Err("select an absent or empty absolute disposable output directory".into());
    }
    let transcript = fs::read_to_string(arguments.get(2).ok_or("transcript file required")?)?;
    fs::create_dir_all(directory.join("workspace/data/runtime/logs"))?;
    fs::create_dir_all(directory.join("private"))?;
    let root = directory.join("workspace").canonicalize()?;
    fs::write(
        root.join("data/runtime/logs/latest.log"),
        "Discovered mod jars: alpha.jar, beta.jar\n",
    )?;
    fs::write(root.join("pack_lock.json"), "[\"alpha.jar\",\"beta.jar\"]\n")?;
    fs::write(root.join("keep.txt"), "UNCHANGED-159\n")?;
    let run_id = RunId::new([0x96; 16]).map_err(|_| "run")?;
    let process = peritus_process::ProcessStore::open(directory.join("private/processes"), &root)?;
    let command_runtime = peritus_product_runner::CommandRuntime::open(
        directory.join("private/router"),
        &root,
        run_id,
        process,
    )?;
    let model = provider(arguments.iter().any(|a| a == "--zen"))?;
    let input = ProductRunInput {
        workspace_kind: ProductWorkspaceKind::InPlace {
            protected_paths: vec![],
            baseline_revision: 1,
        },
        run_id,
        workspace_id: WorkspaceId::new([0x97; 16]).map_err(|_| "workspace")?,
        source_root: root.clone(),
        workspace_root: root.clone(),
        trace_path: directory.join("private/run.trace"),
        command_runtime,
        module_jobs: None,
        finding_state: String::new(),
        task: transcript.clone(),
        max_elapsed: None,
        delivery_scope: ProductDeliveryScope::WorkspaceChanges,
        conversation: Arc::new(Conversation(transcript)),
        providers: RoleProviders {
            writer: model.clone(),
            reviewer: model.clone(),
            fixer: model,
            fallbacks: vec![],
        },
        cancelled: Arc::new(AtomicBool::new(false)),
        provider_cancellation: CancellationToken::new(),
        resume: None,
    };
    let mode = if arguments.iter().any(|a| a == "--plan") {
        ConversationMode::Plan
    } else {
        ConversationMode::Chat
    };
    let outcome = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?
        .block_on(ProductRunner::converse(
            input,
            mode,
            Arc::new(|update| eprintln!("PHASE {:?}: {}", update.phase, update.status)),
        ))?;
    let summary = serde_json::json!({"settlement":format!("{:?}",outcome.settlement().disposition()),
        "reply":outcome.question().map(|q|q.message().to_owned()),"detail":outcome.detail(),
        "report":fs::read_to_string(root.join("identity-report.md")).ok(),
        "keep":fs::read_to_string(root.join("keep.txt"))?});
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
