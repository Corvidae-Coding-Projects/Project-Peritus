use peritus_agent::{
    DeveloperContextAssembly, DeveloperContextPort, DeveloperLoopLimits, DeveloperLoopRequest,
    estimate_developer_request_tokens,
};
use peritus_model_protocol::{
    BoundedText, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    ContentBlock, Message, ModelLimits, ModelName, OutputLimitEnforcement, ProtocolLimits,
    ProviderName, ProviderProfile, ResumeKind, Role, StateMode, WireDialect,
};
use peritus_product_runner::context_projection::{
    DeveloperEvidencePrompt, LocalContextHandle, evidence_tool_definition,
    project_developer_evidence,
};
use peritus_product_runner::{
    CommandRuntime, ConversationView, ProductDeliveryScope, ProductRunInput, ProductWorkspaceKind,
    RoleProviders, inspect_local_context,
};
use peritus_provider_core::{CancellationToken, ModelProvider, ProcessLimits};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
fn text(role: Role, value: String) -> Message {
    Message::new(
        role,
        vec![ContentBlock::Text(BoundedText::new(value, ProtocolLimits::PRODUCTION).unwrap())],
        ProtocolLimits::PRODUCTION,
    )
    .unwrap()
}
fn bytes<const N: usize>(value: &str) -> [u8; N] {
    let mut out = [0; N];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).unwrap();
    }
    out
}
struct Conversation(String);
impl ConversationView for Conversation {
    fn revision(&self) -> u64 {
        14
    }
    fn render(&self) -> String {
        self.0.clone()
    }
    fn uses_explicit_media(&self) -> bool {
        true
    }
}
fn profile(settings: &Value) -> Result<ProviderProfile, Box<dyn Error>> {
    let p = &settings["profile"];
    let mut capabilities = Vec::new();
    for c in p["capabilities"].as_array().unwrap() {
        capabilities.push(match c.as_str().unwrap() {
            "image-input" => Capability::ImageInput,
            "parallel-tool-calls" => Capability::ParallelToolCalls,
            "prompt-caching" => Capability::PromptCaching,
            "reasoning-controls" => Capability::ReasoningControls,
            "tool-calls" => Capability::ToolCalls,
            "usage-detail" => Capability::UsageDetail,
            c => panic!("unsupported actual capability {c}"),
        });
    }
    Ok(ProviderProfile::new(
        ProviderProfileId::new(bytes(p["profile_id"].as_str().unwrap())).unwrap(),
        p["revision"].as_u64().unwrap(),
        ProviderName::new("openai".to_owned())?,
        ModelName::new(p["model"].as_str().unwrap().to_owned())?,
        WireDialect::OpenAiCodexRuntime,
        CapabilityMatrix::new(&capabilities, &[])?,
        CapabilityProvenance::Profiled,
        ModelLimits::new(
            p["max_input_tokens"].as_u64().unwrap(),
            p["max_output_tokens"].as_u64().unwrap(),
            p["max_tools"].as_u64().unwrap() as u32,
            p["max_parallel_tool_calls"].as_u64().unwrap() as u32,
            p["max_inline_media_bytes"].as_u64().unwrap(),
        )?,
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )?)
}
fn main() -> Result<(), Box<dyn Error>> {
    let arguments = std::env::args().collect::<Vec<_>>();
    let original = Path::new(&arguments[1]);
    let host = Path::new(&arguments[2]);
    let settings: Value = serde_json::from_slice(&fs::read(host.join("provider.json"))?)?;
    let old = fs::read_to_string(original.join("pin-9768.txt"))?;
    let system = fs::read_to_string(original.join("pin-9767.txt"))?;
    let (literal, evidence) =
        old.split_once("\n\nExact failed checks and conserved findings to address:\n").unwrap();
    let (diff, rest) = evidence
        .strip_prefix("Current diff:\n")
        .unwrap()
        .split_once("\n\nExact-target checks:\n")
        .unwrap();
    let (gates, ledger) = rest.rsplit_once("\n\nConserved review ledger:\n").unwrap();
    let instructions =
        format!("{literal}\n\nExact failed checks and conserved findings to address:\n{ledger}");
    let conversation = literal
        .strip_prefix("Conversation and task:\n")
        .unwrap()
        .split_once("\n\nApproved implementation design:\n")
        .unwrap()
        .0;
    let run = RunId::new(bytes("81f9302c94ce667a2fff398d434f6192")).unwrap();
    let workspace = WorkspaceId::new(bytes("a9f0183050b6c78adcf62641de464744")).unwrap();
    let profile = profile(&settings)?;
    let provider = Arc::new(CodexRuntimeProvider::new(CodexRuntimeConfig::new(
        CodexExecutable::pin(settings["executable"].as_str().unwrap())?,
        profile,
        ProcessLimits::new(2 * 1024 * 1024, 2 * 1024 * 1024, 64 * 1024, Duration::from_secs(60))?,
    )?));
    let process_store = peritus_process::ProcessStore::open(
        host.join("commands/processes"),
        host.join("workspace"),
    )?;
    let command_runtime = CommandRuntime::open_direct(
        host.join("commands/router"),
        host.join("workspace"),
        run,
        process_store,
    )?;
    let provider_dyn: Arc<dyn ModelProvider> = provider.clone();
    let cancellation = CancellationToken::new();
    let input = ProductRunInput {
        workspace_kind: ProductWorkspaceKind::InPlace {
            protected_paths: Vec::new(),
            baseline_revision: 14,
        },
        run_id: run,
        workspace_id: workspace,
        workspace_root: host.join("workspace"),
        trace_path: host.join("replay.trace"),
        command_runtime,
        finding_state: String::new(),
        task: conversation.to_owned(),
        max_elapsed: None,
        delivery_scope: ProductDeliveryScope::WorkspaceChanges,
        conversation: Arc::new(Conversation(conversation.to_owned())),
        providers: RoleProviders {
            writer: provider_dyn.clone(),
            reviewer: provider_dyn.clone(),
            fixer: provider_dyn,
            fallbacks: Vec::new(),
        },
        cancelled: Arc::new(AtomicBool::new(false)),
        provider_cancellation: cancellation.clone(),
        resume: None,
    };
    let mut context = LocalContextHandle::for_run(&input, "fixer")?.unwrap();
    let mut tools = input.developer_definitions()?;
    tools.push(evidence_tool_definition()?);
    tools.extend(context.tool_definitions()?);
    let reserve = context.additional_request_framing_tokens()?;
    let prompt = project_developer_evidence(&DeveloperEvidencePrompt {
        system: &system,
        instructions: &instructions,
        tools: &tools,
        diff,
        gates,
        max_input_tokens: provider.context_input_target(),
        additional_framing_tokens: reserve,
    })?;
    assert!(prompt.starts_with(&instructions));
    assert!(prompt.contains("User: design and build https support"));
    let request = DeveloperLoopRequest {
        request_prefix: format!(
            "peritus-81f9302c94ce667a2fff398d434f6192-fixer-1-revision-14-invocation-host-replay-{}",
            std::process::id()
        ),
        system: system.clone(),
        prompt: prompt.clone(),
        attachments: Vec::new(),
        tools: tools.clone(),
        limits: DeveloperLoopLimits::new(48, 512)?,
        cancellation,
    };
    let initial = vec![text(Role::System, system), text(Role::User, prompt.clone())];
    DeveloperContextPort::open(&mut context, &request, &initial)?;
    let governing = text(
        Role::User,
        format!(
            "Current governing conversation (revision 14); incorporate the latest user message and respect its scope:\n{conversation}"
        ),
    );
    let view = context.assemble(DeveloperContextAssembly {
        provider: provider.as_ref(),
        governing_input: Some(&governing),
        invocation_policy: &initial[0],
        messages: &initial,
        tools: &tools,
        profile: provider.profile(),
    })?;
    let estimated = estimate_developer_request_tokens(&view, &tools);
    assert!(provider.context_input_fits(&view, &tools)?);
    context.checkpoint(&view)?;
    drop(context);
    let published: Value = serde_json::from_str(&inspect_local_context(
        &input.trace_path,
        run,
        workspace,
        peritus_role::HarnessRole::Writer,
    )?)?;
    let selected = published["validation"]["selected_observations"].as_array().unwrap();
    assert!(!selected.iter().any(|id| id.as_u64() == Some(9768)));
    assert!(selected.iter().any(|id| id.as_u64() == Some(5927)));
    let sources = published["source_index"].as_array().unwrap();
    let old_source =
        sources.iter().find(|source| source["sequence"].as_u64() == Some(9768)).unwrap();
    assert_eq!(old_source["artifact"]["bytes"], 675149);
    let original_sha = [
        203, 157, 169, 137, 251, 153, 87, 81, 0, 80, 35, 170, 232, 53, 184, 11, 187, 88, 56, 59,
        44, 44, 248, 55, 172, 169, 102, 11, 86, 59, 79, 30,
    ];
    assert_eq!(old_source["artifact"]["digest"], json!(original_sha));
    let mut required_ids = std::collections::BTreeSet::new();
    let mut unknown_present = false;
    let mut oversized_user = false;
    for m in &view {
        for b in m.content() {
            if let ContentBlock::Text(t) = b {
                let s = t.expose_for_wire();
                if m.role() == Role::User && s.len() >= 675129 {
                    oversized_user = true;
                }
                for line in s.lines() {
                    if line.starts_with("entry:") {
                        required_ids.insert(line[..38].to_owned());
                    }
                }
                if s.starts_with("PENDING HOST OPERATIONS") {
                    let pending: Value = serde_json::from_str(s.split_once('\n').unwrap().1)?;
                    assert_eq!(pending.as_array().unwrap().len(), 1);
                    assert_eq!(pending[0]["source"], 5927);
                    assert_eq!(pending[0]["state"], "unknown");
                    unknown_present = true;
                }
            }
        }
    }
    assert!(!oversized_user);
    assert!(unknown_present);
    assert_eq!(required_ids.len(), 122);
    let mut reopened = LocalContextHandle::for_run(&input, "fixer")?.unwrap();
    let reopened_view = reopened.assemble(DeveloperContextAssembly {
        provider: provider.as_ref(),
        governing_input: Some(&governing),
        invocation_policy: &initial[0],
        messages: &view,
        tools: &tools,
        profile: provider.profile(),
    })?;
    assert_eq!(view, reopened_view);
    assert!(provider.context_input_fits(&reopened_view, &tools)?);
    drop(reopened);
    let report = json!({"ordinary_and_memory_tools":tools.len(),"provider_kind":settings["kind"],"provider_model":provider.profile().model().as_str(),"provider_target":provider.context_input_target(),"provider_profile_input":provider.profile().limits().max_input_tokens(),"reserved_memory_tokens":reserve,"projected_input_bytes":prompt.len(),"assembled_estimated_tokens":estimated,"old_oversized_pin_selected":false,"old_oversized_archive_bytes":675149,"old_oversized_archive_digest_preserved":true,"preserved_unknown_source":5927,"required_entries":required_ids.len(),"published_generation":published["checkpoint"]["generation"],"reopened_view_exactly_equal":true,"real_adapter_context_fits":true,"provider_requests_sent":0});
    fs::write(host.join("host-report.json"), serde_json::to_vec_pretty(&report)?)?;
    println!("{report}");
    Ok(())
}
