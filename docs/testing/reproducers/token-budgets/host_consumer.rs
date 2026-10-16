//! Production LocalContextHandle assembly on copied closed state; no provider inference/effects.
use peritus_agent::{
    DeveloperContextAssembly, DeveloperContextPort, DeveloperLoopLimits,
    estimate_developer_request_tokens,
};
use peritus_context::{ContextNodeId, working::*};
use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, ContentBlock,
    ModelLimits, ModelName, OutputLimitEnforcement, ProtocolLimits, ProviderName, ProviderProfile,
    ResumeKind, Role, StateMode, WireDialect, decode_messages, encode_messages,
};
use peritus_product_runner::context_projection::LocalContextHandle;
use peritus_product_runner::{
    CommandRuntime, ConversationView, LocalContextConfig, ProductDeliveryScope, ProductRunInput,
    ProductWorkspaceKind, RoleProviders, inspect_local_context,
};
use peritus_provider_core::{CancellationToken, ModelProvider, ProcessLimits};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_role::HarnessRole;
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    error::Error,
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
fn bytes<const N: usize>(value: &str) -> [u8; N] {
    let mut out = [0; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).unwrap();
    }
    out
}
fn hex(value: &[u8]) -> String {
    value.iter().map(|b| format!("{b:02x}")).collect()
}
fn artifact(root: &Path, desc: &Value) -> Vec<u8> {
    let hash = hex(&desc["digest"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect::<Vec<_>>());
    let data =
        fs::read(root.join("artifacts/objects/sha256").join(&hash[..2]).join(&hash)).unwrap();
    assert_eq!(data.len() as u64, desc["bytes"].as_u64().unwrap());
    assert_eq!(hex(peritus_codec::sha256(&data).as_bytes()), hash);
    data
}
struct Conversation(u64);
impl ConversationView for Conversation {
    fn revision(&self) -> u64 {
        self.0
    }
    fn render(&self) -> String {
        "Retain the current governing request from the exact copied durable pins.".into()
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

fn config_contract() -> Result<(), Box<dyn Error>> {
    for value in [0, 1, 4096, 16384, 16385, u64::MAX] {
        let config: LocalContextConfig = serde_json::from_value(
            json!({"working_state_max_tokens":value,"retrieved_evidence_max_tokens":value}),
        )?;
        config.validate()?;
        let encoded = serde_json::to_value(&config)?;
        assert!(
            encoded.get("working_state_max_tokens").is_none()
                && encoded.get("retrieved_evidence_max_tokens").is_none()
        );
        println!("{}", json!({"case":"legacy-config-compatible","value":value,"result":"PASS"}));
    }
    let limits = DeveloperLoopLimits::new(48, 512)?;
    assert_eq!(limits.max_output_tokens(), u64::MAX);
    for value in [32769, 64000, u64::MAX] {
        assert_eq!(limits.with_max_output_tokens(value)?.max_output_tokens(), value);
    }
    assert!(limits.with_max_output_tokens(0).is_err());
    println!("{}", json!({"case":"uncapped-default-explicit-output-api","result":"PASS"}));
    Ok(())
}
fn main() -> Result<(), Box<dyn Error>> {
    config_contract()?;
    let argument = std::env::args().nth(1).expect("usage: host-consumer PREPARED_HOST_DIRECTORY");
    let host = Path::new(&argument);
    let settings: Value = serde_json::from_slice(&fs::read(host.join("settings.json"))?)?;
    let snapshot: Value = serde_json::from_slice(&fs::read(host.join("snapshot.json"))?)?;
    let root = host.join("replay.context/writer");
    let run =
        RunId::new(bytes(settings["run_id"].as_str().unwrap())).expect("validated run metadata");
    let workspace = WorkspaceId::new(bytes(settings["workspace_id"].as_str().unwrap()))
        .expect("validated workspace metadata");
    let binding = WorkingBinding::new(
        run,
        workspace,
        ContextNodeId::new(run.into_bytes()).expect("validated task metadata"),
        HarnessRole::Writer,
        0,
    );
    let mut state = decode_working_state(
        &artifact(&root, &snapshot["checkpoint"]["working_state"]),
        binding,
        WorkingLimits::standard(),
    )?;
    for record in snapshot["events"].as_array().unwrap() {
        if let Some(observation) = record.get("observation") {
            state = apply_working_event(
                &state,
                &decode_working_event(
                    &artifact(&root, &observation["reducer"]),
                    state.binding(),
                    WorkingLimits::standard(),
                )?,
            )?;
        } else if let Some(event) = record.get("state_event") {
            state = apply_working_event(
                &state,
                &decode_working_event(
                    &artifact(&root, &event["reducer"]),
                    state.binding(),
                    WorkingLimits::standard(),
                )?,
            )?;
        }
    }
    let exact_before = encode_working_state(&state)?;
    fs::write(host.join("recovered-before.pwms"), &exact_before)?;
    let mut pins = Vec::new();
    for pin in state.protocol(state.binding())?.requirements() {
        let source = snapshot["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["sequence"].as_u64() == Some(pin.get()))
            .unwrap();
        pins.extend(decode_messages(
            &artifact(&root, &source["artifact"]),
            ProtocolLimits::PRODUCTION,
        )?);
    }
    let system = pins.iter().find(|p| p.role() == Role::System).unwrap().clone();
    let workspace_root = Path::new(settings["workspace"].as_str().unwrap()).to_path_buf();
    let provider = Arc::new(CodexRuntimeProvider::new(CodexRuntimeConfig::new(
        CodexExecutable::pin(settings["executable"].as_str().unwrap())?,
        profile(&settings)?,
        ProcessLimits::new(2 * 1024 * 1024, 2 * 1024 * 1024, 64 * 1024, Duration::from_secs(60))?,
    )?));
    let processes =
        peritus_process::ProcessStore::open(host.join("commands/processes"), &workspace_root)?;
    let runtime =
        CommandRuntime::open_direct(host.join("commands/router"), &workspace_root, run, processes)?;
    let provider_dyn: Arc<dyn ModelProvider> = provider.clone();
    let input = ProductRunInput {
        workspace_kind: ProductWorkspaceKind::InPlace {
            protected_paths: vec![],
            baseline_revision: state.binding().conversation_revision(),
        },
        run_id: run,
        workspace_id: workspace,
        workspace_root,
        trace_path: host.join("replay.trace"),
        command_runtime: runtime,
        finding_state: String::new(),
        task: String::new(),
        max_elapsed: None,
        delivery_scope: ProductDeliveryScope::WorkspaceChanges,
        conversation: Arc::new(Conversation(state.binding().conversation_revision())),
        providers: RoleProviders {
            writer: provider_dyn.clone(),
            reviewer: provider_dyn.clone(),
            fixer: provider_dyn,
            fallbacks: vec![],
        },
        cancelled: Arc::new(AtomicBool::new(false)),
        provider_cancellation: CancellationToken::new(),
        resume: None,
    };
    let mut context =
        LocalContextHandle::for_run(&input, "writer")?.expect("working memory enabled");
    let additional = context.additional_request_framing_tokens()?;
    let mut tools = input.developer_definitions()?;
    tools.extend(context.tool_definitions()?);
    let view = context.assemble(DeveloperContextAssembly {
        provider: provider.as_ref(),
        governing_input: None,
        invocation_policy: &system,
        messages: &pins,
        tools: &tools,
        profile: provider.profile(),
    })?;
    let body = view
        .iter()
        .flat_map(|m| m.content())
        .filter_map(
            |b| if let ContentBlock::Text(t) = b { Some(t.expose_for_wire()) } else { None },
        )
        .filter(|t| t.starts_with("LOCAL WORKING STATE —"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!body.is_empty());
    let mut ids = BTreeSet::new();
    for entry in state.entries(state.binding())? {
        let key = hex(entry.id().as_bytes());
        assert!(body.contains(&format!("entry:{key};")), "stored working entry omitted: {key}");
        let original = std::str::from_utf8(entry.content().bytes())?;
        assert!(body.contains(&format!("{original:?}")), "working entry content omitted");
        ids.insert(key);
        for source in entry.links().supports().iter().chain(entry.links().contradicts()) {
            assert!(body.contains(&format!("obs:{:06}", source.get())), "source reference lost");
        }
    }
    assert!(provider.context_input_fits(&view, &tools)?);
    let serialized = encode_messages(&view, ProtocolLimits::PRODUCTION)?;
    fs::write(host.join("assembled.c5"), &serialized)?;
    context.checkpoint(&view)?;
    drop(context);
    let published: Value = serde_json::from_str(&inspect_local_context(
        &input.trace_path,
        run,
        workspace,
        HarnessRole::Writer,
    )?)?;
    assert_eq!(published["validation"]["omitted_entries"], 0);
    let after = decode_working_state(
        &artifact(&root, &published["checkpoint"]["working_state"]),
        binding,
        WorkingLimits::standard(),
    )?;
    let after_ids: BTreeSet<_> =
        after.entries(after.binding())?.iter().map(|e| hex(e.id().as_bytes())).collect();
    assert_eq!(ids, after_ids);
    for entry in state.entries(state.binding())? {
        let found = after.entry(after.binding(), entry.id())?;
        assert_eq!(found.content(), entry.content());
        assert_eq!(found.links(), entry.links());
    }
    fs::write(host.join("published-inspection.json"), serde_json::to_vec(&published)?)?;
    println!(
        "{}",
        json!({"case":"copied-real-host-assembly","result":"PASS","entries":ids.len(),"required_framing_tokens":additional,"assembled_estimated_tokens":estimate_developer_request_tokens(&view,&tools),"provider_input_target":provider.context_input_target(),"actual_provider_encoding_fits":true,"assembly_bytes":serialized.len(),"assembly_sha256":hex(peritus_codec::sha256(&serialized).as_bytes()),"entry_content_and_source_links_preserved":true,"uncovered_tail_recovered":snapshot["events"].as_array().unwrap().len(),"provider_requests":0,"effects_dispatched":0})
    );
    Ok(())
}
