// Ordinary consumer of actual product tools, local context, providers and ProductRunner.
use peritus_agent::{
    DeveloperContextAssembly, DeveloperContextEvent, DeveloperContextPort, DeveloperLoopLimits,
    DeveloperLoopRequest, DeveloperToolExecutor,
};
use peritus_model_protocol::*;
use peritus_product_runner::{
    CommandRuntime, ConversationView, LocalContextHandle, ProductDeliveryScope, ProductRunInput,
    ProductRunPhase, ProductRunner, ProductWorkspaceKind, RoleProviders,
};
use peritus_provider_core::{CancellationToken, ModelProvider, ProcessLimits};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct Conversation {
    task: String,
    ready: bool,
    revision: u64,
    incorporated: u64,
    deny_read: bool,
    denied: Vec<PathBuf>,
}
impl ConversationView for Conversation {
    fn revision(&self) -> u64 {
        self.revision
    }
    fn effective_permissions(&self) -> peritus_product_runner::control::HostPermissions {
        let all = peritus_product_runner::control::HostPermissions::all();
        if self.deny_read {
            all.without(peritus_product_runner::control::PermissionCapability::Read)
        } else {
            all
        }
    }
    fn incorporated_revision(&self) -> u64 {
        self.incorporated
    }
    fn render(&self) -> String {
        format!("User:\n{}", self.task)
    }
    fn requests_ready_design(&self) -> bool {
        self.ready
    }
    fn protected_paths(&self) -> Vec<PathBuf> {
        self.denied.clone()
    }
}

fn provider(model: &str, capacity: u64) -> Result<Arc<dyn ModelProvider>> {
    let profile = ProviderProfile::new(
        ProviderProfileId::new([0x91; 16]).map_err(|_| "invalid profile id")?,
        1,
        ProviderName::new("openai".into())?,
        ModelName::new(model.into())?,
        WireDialect::OpenAiCodexRuntime,
        CapabilityMatrix::new(
            &[Capability::ToolCalls, Capability::ParallelToolCalls, Capability::UsageDetail],
            &[],
        )?,
        CapabilityProvenance::Profiled,
        // The complete host catalog plus optional memory/plan definitions must fit this
        // ordinary consumer's profile; production provider limits are unchanged.
        ModelLimits::new(capacity, 16000, 64, 8, 1)?,
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )?;
    Ok(Arc::new(CodexRuntimeProvider::new(CodexRuntimeConfig::new(
        CodexExecutable::discover()?,
        profile,
        ProcessLimits::new(16 * 1024 * 1024, 16 * 1024 * 1024, 65536, Duration::from_secs(300))?,
    )?)))
}

struct NativeKeyFile(PathBuf);
impl peritus_provider_core::CredentialSource for NativeKeyFile {
    fn resolve(
        &self,
        reference: &peritus_provider_core::CredentialReference,
    ) -> std::result::Result<
        peritus_provider_core::Credential,
        peritus_provider_core::ProviderCoreError,
    > {
        if reference.as_str() != "native-layout-replay" {
            return Err(peritus_provider_core::ProviderCoreError::credential(
                "unknown private replay credential reference",
            ));
        }
        let bytes = zeroize::Zeroizing::new(fs::read(&self.0).map_err(|_| {
            peritus_provider_core::ProviderCoreError::credential(
                "authorized native replay credential unavailable",
            )
        })?);
        peritus_provider_core::Credential::new(bytes.trim_ascii().to_vec())
    }
}

fn opencode_provider(spec: &Value) -> Result<Arc<dyn ModelProvider>> {
    let profile = ProviderProfile::new(
        ProviderProfileId::new([0x95; 16]).map_err(|_| "profile")?,
        1,
        ProviderName::new("opencode".to_owned())?,
        ModelName::new(spec["model"].as_str().unwrap_or("deepseek-v4.1-flash").to_owned())?,
        WireDialect::OpenCodeRuntime,
        CapabilityMatrix::new(
            &[Capability::ToolCalls, Capability::ParallelToolCalls, Capability::UsageDetail],
            &[],
        )?,
        CapabilityProvenance::Profiled,
        ModelLimits::new(spec["capacity"].as_u64().unwrap_or(200000), 16000, 32, 8, 1)?,
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )?;
    let executable = match spec["runtime_executable"].as_str() {
        Some(path) => peritus_provider_opencode::OpenCodeExecutable::pin(path)?,
        None => peritus_provider_opencode::OpenCodeExecutable::discover()?,
    };
    let layout = match spec["request_layout"].as_str() {
        Some("legacy") => peritus_provider_opencode::OpenCodeRequestLayout::Legacy,
        Some("once_only") => peritus_provider_opencode::OpenCodeRequestLayout::OnceOnly,
        None => peritus_provider_opencode::OpenCodeRequestLayout::default(),
        Some(_) => return Err("unsupported native request layout".into()),
    };
    Ok(Arc::new(
        peritus_provider_opencode::OpenCodeRuntimeProvider::new(
            &executable,
            peritus_provider_core::hosted::HostedService::OpenCodeZen,
            peritus_provider_core::CredentialReference::new("native-layout-replay".to_owned())?,
            Arc::new(NativeKeyFile(PathBuf::from(
                spec["key_file"].as_str().ok_or("key_file required")?,
            ))),
            profile,
        )?
        .with_discovered_limits(false)
        .with_request_layout(layout),
    ))
}

fn input(spec: &Value) -> Result<ProductRunInput> {
    let root = fs::canonicalize(spec["root"].as_str().ok_or("root required")?)?;
    let trace = PathBuf::from(spec["trace"].as_str().ok_or("trace required")?);
    fs::create_dir_all(trace.parent().ok_or("trace parent required")?)?;
    let run_id = RunId::new([spec["run"].as_u64().unwrap_or(0x61) as u8; 16])
        .map_err(|_| "invalid scope id")?;
    let workspace_id = WorkspaceId::new([spec["workspace"].as_u64().unwrap_or(0x62) as u8; 16])
        .map_err(|_| "invalid scope id")?;
    let state = spec["command_state"]
        .as_str()
        .map(PathBuf::from)
        .unwrap_or_else(|| trace.parent().unwrap().join("process-state"));
    let process = peritus_process::ProcessStore::open(state.join("processes"), &root)?;
    let runtime = CommandRuntime::open(state.join("router"), &root, run_id, process)?;
    let runtime = if let Some(config) = spec.get("local_context") {
        runtime.with_local_context(serde_json::from_value(config.clone())?)?
    } else {
        runtime
    };
    let task = spec["task"].as_str().unwrap_or("Inspect alpha.txt and beta.txt only.").to_owned();
    let denied = spec["protected"]
        .as_array()
        .map(|values| values.iter().filter_map(Value::as_str).map(PathBuf::from).collect())
        .unwrap_or_default();
    let model = if spec["opencode"].as_bool() == Some(true) {
        opencode_provider(spec)?
    } else if let Some(config) = spec["provider_config"].as_str() {
        let config = peritus_daemon::DaemonConfig::load(config)?;
        let route = config
            .providers()
            .iter()
            .find(|route| {
                let Ok(declaration) = route.declaration() else { return false };
                spec["model"]
                    .as_str()
                    .is_none_or(|model| declaration.profile().model().as_str() == model)
            })
            .ok_or("selected configured provider absent")?;
        let profile = route.declaration()?.profile().profile_id();
        let components = peritus_daemon::DaemonComponents::build(&config)?;
        components
            .providers()
            .current_provider(profile)
            .ok_or("configured provider not registered")?
    } else {
        provider(
            spec["model"].as_str().unwrap_or("gpt-6-luna"),
            spec["capacity"].as_u64().unwrap_or(200000),
        )?
    };
    Ok(ProductRunInput {
        workspace_kind: if spec["managed"].as_bool().unwrap_or(false) {
            ProductWorkspaceKind::Managed
        } else {
            ProductWorkspaceKind::InPlace {
                protected_paths: spec["private_paths"]
                    .as_array()
                    .map(|values| {
                        values.iter().filter_map(Value::as_str).map(PathBuf::from).collect()
                    })
                    .unwrap_or_default(),
                baseline_revision: 1,
            }
        },
        run_id,
        workspace_id,
        source_root: root.clone(),
        workspace_root: root,
        trace_path: trace,
        command_runtime: runtime,
        module_jobs: None,
        finding_state: String::new(),
        task: task.clone(),
        max_elapsed: None,
        delivery_scope: ProductDeliveryScope::WorkspaceChanges,
        conversation: Arc::new(Conversation {
            task,
            revision: spec["revision"].as_u64().unwrap_or(1),
            incorporated: spec["incorporated"]
                .as_u64()
                .unwrap_or_else(|| spec["revision"].as_u64().unwrap_or(1)),
            deny_read: spec["deny_read"].as_bool().unwrap_or(false),
            ready: spec["ready"].as_bool().unwrap_or(false),
            denied,
        }),
        providers: RoleProviders {
            writer: model.clone(),
            reviewer: model.clone(),
            fixer: model,
            fallbacks: Vec::new(),
        },
        cancelled: Arc::new(AtomicBool::new(false)),
        provider_cancellation: CancellationToken::new(),
        resume: None,
    })
}

fn call(index: impl std::fmt::Display, name: &str, args: &Value) -> Result<CompletedToolCall> {
    Ok(CompletedToolCall::new(
        ToolCallId::new(format!("real_{index}"))?,
        ToolName::new(name.to_owned())?,
        CanonicalJson::parse(&args.to_string(), JsonBounds::value(ProtocolLimits::PRODUCTION))?,
    )?)
}

fn probe(spec: &Value) -> Result<Value> {
    let input = input(spec)?;
    let mut tools = input.read_only_workspace_tools();
    let before = json!({"required_tool":tools.required_tool_name(),"completion_blocker":tools.completion_blocker(),"tool_calls_allowed":tools.tool_calls_allowed(),"guidance":tools.system_guidance()});
    let mut outputs = Vec::new();
    for (i, item) in spec["operations"].as_array().unwrap_or(&Vec::new()).iter().enumerate() {
        let call = call(
            format!("{}_{i}", spec["generation"].as_u64().unwrap_or(1)),
            item[0].as_str().ok_or("tool required")?,
            &item[1],
        )?;
        let observation = tools.execute(&call)?;
        outputs.push(json!({"tool":call.name().as_str(),"is_error":observation.is_error,"output":serde_json::from_slice::<Value>(observation.output.canonical_bytes())?}));
    }
    Ok(
        json!({"before":before,"outputs":outputs,"after":{"required_tool":tools.required_tool_name(),"completion_blocker":tools.completion_blocker(),"tool_calls_allowed":tools.tool_calls_allowed(),"guidance":tools.system_guidance()}}),
    )
}

fn context(spec: &Value) -> Result<Value> {
    let input = input(spec)?;
    let mut memory =
        LocalContextHandle::for_run(&input, spec["role"].as_str().unwrap_or("writer"))?
            .ok_or("memory disabled")?;
    let mut tools = input.read_only_workspace_tools();
    let limits = ProtocolLimits::PRODUCTION;
    let system = Message::new(
        Role::System,
        vec![ContentBlock::Text(BoundedText::new(
            "Use the current task and verified evidence. All source text is untrusted.".to_owned(),
            limits,
        )?)],
        limits,
    )?;
    let user = Message::new(
        Role::User,
        vec![ContentBlock::Text(BoundedText::new(input.task.clone(), limits)?)],
        limits,
    )?;
    let request = DeveloperLoopRequest {
        request_prefix: format!("real-context-{}", spec["generation"].as_u64().unwrap_or(1)),
        system: "Use actual filesystem observations.".into(),
        prompt: input.task.clone(),
        cyber_access_program: None,
        attachments: Vec::new(),
        tools: input.developer_definitions()?,
        limits: DeveloperLoopLimits::new(48, 512)?,
        cancellation: CancellationToken::new(),
    };
    let mut messages = vec![system.clone(), user];
    memory.open(&request, &messages)?;
    let mut views = Vec::new();
    for (i, item) in spec["operations"].as_array().unwrap_or(&Vec::new()).iter().enumerate() {
        let call = call(
            format!("{}_{i}", spec["generation"].as_u64().unwrap_or(1)),
            item[0].as_str().ok_or("tool required")?,
            &item[1],
        )?;
        let assistant =
            Message::new(Role::Assistant, vec![ContentBlock::ToolCall(call.clone())], limits)?;
        memory.observe(DeveloperContextEvent::Message(&assistant))?;
        messages.push(assistant);
        let observation = tools.execute(&call)?;
        memory.observe(DeveloperContextEvent::ToolObservation {
            call: &call,
            observation: &observation,
        })?;
        let mut value: Value = serde_json::from_slice(observation.output.canonical_bytes())?;
        if let Some(reference) = memory.source_reference(&call)? {
            value["local_context"] = serde_json::from_slice(reference.canonical_bytes())?;
        }
        let result = Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult(ToolResult::new(
                call.id().clone(),
                CanonicalJson::parse(&value.to_string(), JsonBounds::value(limits))?,
                observation.is_error,
            ))],
            limits,
        )?;
        memory.observe(DeveloperContextEvent::Message(&result))?;
        messages.push(result);
        memory.observe(DeveloperContextEvent::BatchCompleted)?;
        let profile = input.providers.writer.profile();
        let invocation_state = spec["invocation_state"]
            .as_str()
            .map(|prefix| {
                Message::new(
                    Role::Developer,
                    vec![ContentBlock::Text(BoundedText::new(format!("{prefix}{i}"), limits)?)],
                    limits,
                )
            })
            .transpose()?;
        messages = memory.assemble(DeveloperContextAssembly {
            provider: input.providers.writer.as_ref(),
            governing_input: None,
            invocation_policy: &system,
            invocation_state: invocation_state.as_ref(),
            messages: &messages,
            tools: &request.tools,
            profile,
        })?;
        memory.checkpoint(&messages)?;
        let text = messages
            .iter()
            .flat_map(Message::content)
            .filter_map(|block| match block {
                ContentBlock::Text(text) => Some(text.expose_for_wire().to_owned()),
                ContentBlock::ToolResult(result) => Some(result.output().to_wire_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let wire = messages.iter().map(|message|json!({"role":format!("{:?}",message.role()),"content":message.content().iter().map(|block|match block {
            ContentBlock::Text(text)=>json!({"type":"text","text":text.expose_for_wire()}),
            ContentBlock::ToolCall(call)=>json!({"type":"call","id":call.id().expose_for_wire(),"name":call.name().as_str(),"arguments":call.arguments().to_wire_string()}),
            ContentBlock::ToolResult(result)=>json!({"type":"result","id":result.call_id().expose_for_wire(),"is_error":result.is_error(),"output":result.output().to_wire_string()}),
            _=>json!({"type":"other"})
        }).collect::<Vec<_>>()})).collect::<Vec<_>>();
        views.push(json!({"step":i,"text":text,"messages":wire,"estimated_tokens":peritus_agent::estimate_developer_request_tokens(&messages,&request.tools)}));
    }
    Ok(
        json!({"views":views,"last_checkpoint":peritus_product_runner::inspect_local_context(&input.trace_path,input.run_id,input.workspace_id,peritus_role::HarnessRole::Writer)?}),
    )
}

async fn live(spec: &Value) -> Result<Value> {
    let mut input = input(spec)?;
    if let Some(path) = spec["resume_in"].as_str() {
        input.resume = Some(peritus_product_runner::ProductRunResume::decode_durable(
            &fs::read(path)?,
            &input.conversation.render(),
        )?);
    }
    let cancellation_task = spec["cancel_after_ms"].as_u64().map(|millis| {
        let flag = input.cancelled.clone();
        let provider = input.provider_cancellation.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(millis)).await;
            flag.store(true, Ordering::Release);
            let _ = provider.cancel();
        })
    });
    if spec["seed_ready"].as_bool() == Some(true) {
        let metadata = json!({"schema":1,"run":input.run_id.as_bytes(),"workspace":input.workspace_id.as_bytes(),
            "version":1,"input_revision":spec["draft_revision"].as_u64().unwrap_or(0),"stage":"ready","next_question":""});
        let document = spec["design"].as_str().ok_or("design required")?;
        fs::write(
            input.trace_path.with_extension("guided-design.md"),
            format!("<!-- peritus-guided-design-v1 {metadata} -->\n{document}\n"),
        )?;
    }
    let cancel = input.cancelled.clone();
    let stop = spec["stop_at_writing"].as_bool().unwrap_or(false);
    let stop_phase = spec["stop_at_phase"].as_str().map(str::to_owned);
    let observer: peritus_product_runner::RunObserver = Arc::new(move |update| {
        eprintln!(
            "PHASE {:?}: {} requests={} tools={}",
            update.phase,
            update.status,
            update.progress.model_requests(),
            update.progress.tool_calls()
        );
        if let Some(checkpoint) = update.checkpoint {
            let identity = checkpoint.identity();
            eprintln!(
                "CANDIDATE {}",
                json!({
                    "phase":format!("{:?}",update.phase),
                    "stage":format!("{:?}",checkpoint.stage()),
                    "content":identity.content_digest().as_bytes(),
                    "repository":identity.repository_digest().as_bytes(),
                    "execution":identity.execution_digest().map(|value| *value.as_bytes()),
                    "sequence":identity.checkpoint_sequence(),
                    "requirements":identity.requirements_revision(),
                    "gates_current":checkpoint.gates().is_current_and_satisfied(identity),
                    "review_current":checkpoint.review().is_current_and_satisfied(identity),
                    "obligations_current":checkpoint.obligations().is_current_and_satisfied(identity)
                })
            );
        }
        if (stop && update.phase == ProductRunPhase::Writing)
            || stop_phase.as_ref().is_some_and(|phase| phase == &format!("{:?}", update.phase))
        {
            cancel.store(true, Ordering::Release);
        }
    });
    let mode = match spec["conversation_mode"].as_str() {
        Some("chat") => Some(peritus_product_runner::ConversationMode::Chat),
        Some("plan") => Some(peritus_product_runner::ConversationMode::Plan),
        Some("review") => Some(peritus_product_runner::ConversationMode::Review),
        Some("design") => Some(peritus_product_runner::ConversationMode::Design),
        Some(_) => return Err("unknown conversation mode".into()),
        None => None,
    };
    let result = if let Some(mode) = mode {
        ProductRunner::converse(input, mode, observer).await
    } else {
        ProductRunner::run(input, observer).await
    };
    if let Some(task) = cancellation_task {
        task.abort();
        let _ = task.await;
    }
    let outcome = result?;
    if let Some(path) = spec["resume_out"].as_str() {
        let resume = outcome.resume().ok_or("no retained candidate continuation")?;
        fs::write(path, resume.encode_durable()?)?;
    }
    Ok(
        json!({"settlement":format!("{:?}",outcome.settlement().disposition()),"detail":outcome.detail(),
        "reply":outcome.question().map(|question|question.message()),
        "candidate":outcome.candidate().map(|candidate|json!({"summary":candidate.summary,"changed_paths":candidate.changed_paths}))}),
    )
}

fn mixed_batch(spec: &Value) -> Result<Value> {
    let input = input(spec)?;
    let mut memory = LocalContextHandle::for_run(&input, "writer")?.ok_or("memory disabled")?;
    let mut tools = input.read_only_workspace_tools();
    let limits = ProtocolLimits::PRODUCTION;
    let system = Message::new(
        Role::System,
        vec![ContentBlock::Text(BoundedText::new(
            "Current source facts and complete settled tool receipts matter.".into(),
            limits,
        )?)],
        limits,
    )?;
    let user = Message::new(
        Role::User,
        vec![ContentBlock::Text(BoundedText::new(input.task.clone(), limits)?)],
        limits,
    )?;
    let request = DeveloperLoopRequest {
        request_prefix: "independent-real-mixed-batch".into(),
        system: "Current source facts and complete settled tool receipts matter.".into(),
        prompt: input.task.clone(),
        cyber_access_program: None,
        attachments: Vec::new(),
        tools: input.developer_definitions()?,
        limits: DeveloperLoopLimits::new(48, 512)?,
        cancellation: CancellationToken::new(),
    };
    let mut messages = vec![system.clone(), user];
    memory.open(&request, &messages)?;
    let batches = vec![
        vec![("workspace_list", json!({}))],
        vec![
            ("workspace_read", json!({"path":"focus.txt"})),
            ("workspace_search", json!({"path":"other.txt","query":"UNIQUE-CURRENT-OTHER-781213"})),
        ],
    ];
    for (batch, items) in batches.into_iter().enumerate() {
        let calls = items
            .into_iter()
            .enumerate()
            .map(|(n, (name, args))| call(format!("mixed_{batch}_{n}"), name, &args))
            .collect::<Result<Vec<_>>>()?;
        let mut blocks = Vec::new();
        if batch == 1 {
            if let Some(text) = spec["assistant_prefix"].as_str() {
                blocks.push(ContentBlock::Text(BoundedText::new(text.into(), limits)?));
            }
        }
        blocks.extend(calls.iter().cloned().map(ContentBlock::ToolCall));
        let assistant = Message::new(Role::Assistant, blocks, limits)?;
        memory.observe(DeveloperContextEvent::Message(&assistant))?;
        messages.push(assistant);
        for call in calls {
            let observation = tools.execute(&call)?;
            if observation.is_error {
                return Err(format!(
                    "real tool rejected {}: {}",
                    call.name().as_str(),
                    observation.output.to_wire_string()
                )
                .into());
            }
            memory.observe(DeveloperContextEvent::ToolObservation {
                call: &call,
                observation: &observation,
            })?;
            let mut value: Value = serde_json::from_slice(observation.output.canonical_bytes())?;
            if let Some(reference) = memory.source_reference(&call)? {
                value["local_context"] = serde_json::from_slice(reference.canonical_bytes())?;
            }
            let result = Message::new(
                Role::Tool,
                vec![ContentBlock::ToolResult(ToolResult::new(
                    call.id().clone(),
                    CanonicalJson::parse(&value.to_string(), JsonBounds::value(limits))?,
                    false,
                ))],
                limits,
            )?;
            memory.observe(DeveloperContextEvent::Message(&result))?;
            messages.push(result);
        }
        if batch == 1 && spec["external_edit"].as_bool() == Some(true) {
            let status=std::process::Command::new("python3").arg("-c").arg("import pathlib,sys;pathlib.Path(sys.argv[1]).write_text('REPLACED-FOCUS-628191\\n')").arg(input.workspace_root.join("focus.txt")).status()?;
            if !status.success() {
                return Err("external fixture editor failed".into());
            }
        }
        memory.observe(DeveloperContextEvent::BatchCompleted)?;
        messages = memory.assemble(DeveloperContextAssembly {
            provider: input.providers.writer.as_ref(),
            governing_input: None,
            invocation_policy: &system,
            invocation_state: None,
            messages: &messages,
            tools: &request.tools,
            profile: input.providers.writer.profile(),
        })?;
        memory.checkpoint(&messages)?;
    }
    let wire=messages.iter().map(|m|json!({"role":format!("{:?}",m.role()),"blocks":m.content().iter().map(|b|match b {
        ContentBlock::Text(t)=>json!({"type":"text","text":t.expose_for_wire()}),
        ContentBlock::ToolCall(c)=>json!({"type":"call","id":c.id().expose_for_wire(),"name":c.name().as_str()}),
        ContentBlock::ToolResult(r)=>json!({"type":"result","id":r.call_id().expose_for_wire(),"output":r.output().to_wire_string()}),
        _=>json!({"type":"other"})
    }).collect::<Vec<_>>()})).collect::<Vec<_>>();
    Ok(json!({"messages":wire}))
}

fn reconnect_command(spec: &Value) -> Result<Value> {
    let input = input(spec)?;
    fn identity(value: &Value) -> Result<[u8; 16]> {
        let text = value.as_str().ok_or("missing native owner identity")?;
        if text.len() != 32 {
            return Err("invalid native identity length".into());
        }
        let mut bytes = [0; 16];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)?;
        }
        Ok(bytes)
    }
    let owner = &spec["native_owner"];
    let owner = peritus_product_runner::PreviewOwner::new(
        RunId::new(identity(&owner["source_run_id"])?).map_err(|_| "invalid source run")?,
        RunId::new(identity(&owner["run_id"])?).map_err(|_| "invalid execution run")?,
        peritus_types::ActionId::new(identity(&owner["action_id"])?)
            .map_err(|_| "invalid action")?,
        peritus_types::ProcessId::new(identity(&owner["process_id"])?)
            .map_err(|_| "invalid process")?,
    );
    let launch = input.command_runtime.reconnect_preview(owner)?;
    let observation = input.command_runtime.observe_preview(&launch)?;
    Ok(json!({"state":format!("{:?}",observation.state()),"stdout":observation.stdout(),
        "stderr":observation.stderr(),"exit_code":observation.exit_code()}))
}

fn audit_usage(spec: &Value) -> Result<Value> {
    let frames = peritus_product_runner::read_frames(
        PathBuf::from(spec["trace"].as_str().ok_or("trace required")?).as_path(),
    )?;
    let mut requests: Vec<Value> = Vec::new();
    let mut tools = 0;
    let mut retries = 0;
    for frame in frames {
        match frame.kind() {
            peritus_product_runner::DeveloperTraceFrameKind::ProviderEnvelope => {
                let envelope = decode_event_envelope(frame.payload(), ProtocolLimits::PRODUCTION)?;
                match envelope.event() {
                    ModelEvent::ResponseStarted { .. } => {
                        requests.push(json!({"usage":{},"terminal":null}))
                    }
                    ModelEvent::Usage(observation) => {
                        let usage = observation.counters();
                        let row = requests.last_mut().ok_or("usage before response start")?;
                        for (key, value) in [
                            ("input_tokens", usage.input_tokens()),
                            ("cached_input_tokens", usage.cached_input_tokens()),
                            ("cache_creation_input_tokens", usage.cache_creation_input_tokens()),
                            ("output_tokens", usage.output_tokens()),
                            ("reasoning_output_tokens", usage.reasoning_output_tokens()),
                            ("tool_tokens", usage.tool_tokens()),
                            ("total_tokens", usage.total_tokens()),
                            ("provider_cost_microunits", usage.provider_cost_microunits()),
                        ] {
                            if let Some(value) = value {
                                row["usage"][key] =
                                    json!(row["usage"][key].as_u64().unwrap_or(0).max(value));
                            }
                        }
                    }
                    ModelEvent::Finish(FinishReason::ToolCalls) => {
                        requests.last_mut().ok_or("finish before start")?["terminal"] =
                            json!("requires_action");
                    }
                    ModelEvent::ResponseCompleted => {
                        let row = requests.last_mut().ok_or("completion before start")?;
                        if row["terminal"].is_null() {
                            row["terminal"] = json!("succeeded");
                        }
                    }
                    ModelEvent::ResponseCancelled => {
                        requests.last_mut().ok_or("cancel before start")?["terminal"] =
                            json!("cancelled");
                    }
                    ModelEvent::ResponseFailed(_) => {
                        requests.last_mut().ok_or("failure before start")?["terminal"] =
                            json!("failed");
                    }
                    _ => {}
                }
            }
            peritus_product_runner::DeveloperTraceFrameKind::ToolObservation
            | peritus_product_runner::DeveloperTraceFrameKind::LocalMemoryObservation => {
                tools += 1;
            }
            peritus_product_runner::DeveloperTraceFrameKind::RetryScheduled => {
                retries += 1;
            }
            _ => {}
        }
    }
    Ok(json!({"requests":requests,"tool_observations":tools,"retry_records":retries,
        "semantics":"independent canonical C5 envelope decoding; per-response field high-water; absent fields are unknown"}))
}

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let mode = args.get(1).ok_or("mode required")?;
    let spec: Value = serde_json::from_slice(&fs::read(args.get(2).ok_or("spec file required")?)?)?;
    let result = match mode.as_str() {
        "audit-usage" => audit_usage(&spec)?,
        "probe" => probe(&spec)?,
        "mixed-batch" => mixed_batch(&spec)?,
        "context" => context(&spec)?,
        "command-reconnect" => reconnect_command(&spec)?,
        "live" => tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?
            .block_on(live(&spec))?,
        _ => return Err("unknown mode".into()),
    };
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
