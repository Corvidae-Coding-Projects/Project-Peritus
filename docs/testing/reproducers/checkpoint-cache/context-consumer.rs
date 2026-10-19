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
        ModelLimits::new(capacity, 16000, 32, 8, 1)?,
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
    let task = spec["task"].as_str().unwrap_or("Inspect alpha.txt and beta.txt only.").to_owned();
    let denied = spec["protected"]
        .as_array()
        .map(|values| values.iter().filter_map(Value::as_str).map(PathBuf::from).collect())
        .unwrap_or_default();
    let model = provider(
        spec["model"].as_str().unwrap_or("gpt-6-luna"),
        spec["capacity"].as_u64().unwrap_or(200000),
    )?;
    Ok(ProductRunInput {
        workspace_kind: ProductWorkspaceKind::InPlace {
            protected_paths: spec["private_paths"]
                .as_array()
                .map(|values| values.iter().filter_map(Value::as_str).map(PathBuf::from).collect())
                .unwrap_or_default(),
            baseline_revision: 1,
        },
        run_id,
        workspace_id,
        workspace_root: root,
        trace_path: trace,
        command_runtime: runtime,
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
        let invocation_state = spec["invocation_state"].as_str().map(|prefix| {
            Message::new(Role::Developer, vec![ContentBlock::Text(BoundedText::new(format!("{prefix}{i}"), limits)?)], limits)
        }).transpose()?;
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
    let input = input(spec)?;
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
    let observer: peritus_product_runner::RunObserver = Arc::new(move |update| {
        eprintln!(
            "PHASE {:?}: {} requests={} tools={}",
            update.phase,
            update.status,
            update.progress.model_requests(),
            update.progress.tool_calls()
        );
        if stop && update.phase == ProductRunPhase::Writing {
            cancel.store(true, Ordering::Release);
        }
    });
    let outcome = ProductRunner::run(input, observer).await?;
    Ok(
        json!({"settlement":format!("{:?}",outcome.settlement().disposition()),"detail":outcome.detail(),
        "candidate":outcome.candidate().map(|candidate|json!({"summary":candidate.summary,"changed_paths":candidate.changed_paths}))}),
    )
}


fn mixed_batch(spec: &Value) -> Result<Value> {
    let input=input(spec)?;
    let mut memory=LocalContextHandle::for_run(&input,"writer")?.ok_or("memory disabled")?;
    let mut tools=input.read_only_workspace_tools();let limits=ProtocolLimits::PRODUCTION;
    let system=Message::new(Role::System,vec![ContentBlock::Text(BoundedText::new("Current source facts and complete settled tool receipts matter.".into(),limits)?)],limits)?;
    let user=Message::new(Role::User,vec![ContentBlock::Text(BoundedText::new(input.task.clone(),limits)?)],limits)?;
    let request=DeveloperLoopRequest{request_prefix:"independent-real-mixed-batch".into(),system:"Current source facts and complete settled tool receipts matter.".into(),prompt:input.task.clone(),attachments:Vec::new(),tools:input.developer_definitions()?,limits:DeveloperLoopLimits::new(48,512)?,cancellation:CancellationToken::new()};
    let mut messages=vec![system.clone(),user];memory.open(&request,&messages)?;
    let batches=vec![vec![("workspace_list",json!({}))],vec![("workspace_read",json!({"path":"focus.txt"})),("workspace_search",json!({"path":"other.txt","query":"UNIQUE-CURRENT-OTHER-781213"}))]];
    for (batch,items) in batches.into_iter().enumerate(){
        let calls=items.into_iter().enumerate().map(|(n,(name,args))|call(format!("mixed_{batch}_{n}"),name,&args)).collect::<Result<Vec<_>>>()?;
        let mut blocks=Vec::new();if batch==1 {if let Some(text)=spec["assistant_prefix"].as_str(){blocks.push(ContentBlock::Text(BoundedText::new(text.into(),limits)?));}}
        blocks.extend(calls.iter().cloned().map(ContentBlock::ToolCall));
        let assistant=Message::new(Role::Assistant,blocks,limits)?;memory.observe(DeveloperContextEvent::Message(&assistant))?;messages.push(assistant);
        for call in calls {
            let observation=tools.execute(&call)?;if observation.is_error{return Err(format!("real tool rejected {}: {}",call.name().as_str(),observation.output.to_wire_string()).into())}
            memory.observe(DeveloperContextEvent::ToolObservation{call:&call,observation:&observation})?;
            let mut value:Value=serde_json::from_slice(observation.output.canonical_bytes())?;if let Some(reference)=memory.source_reference(&call)?{value["local_context"]=serde_json::from_slice(reference.canonical_bytes())?;}
            let result=Message::new(Role::Tool,vec![ContentBlock::ToolResult(ToolResult::new(call.id().clone(),CanonicalJson::parse(&value.to_string(),JsonBounds::value(limits))?,false))],limits)?;memory.observe(DeveloperContextEvent::Message(&result))?;messages.push(result);
        }
        if batch==1 && spec["external_edit"].as_bool()==Some(true) {
            let status=std::process::Command::new("python3").arg("-c").arg("import pathlib,sys;pathlib.Path(sys.argv[1]).write_text('REPLACED-FOCUS-628191\\n')").arg(input.workspace_root.join("focus.txt")).status()?;if !status.success(){return Err("external fixture editor failed".into())}
        }
        memory.observe(DeveloperContextEvent::BatchCompleted)?;
        messages=memory.assemble(DeveloperContextAssembly{provider:input.providers.writer.as_ref(),governing_input:None,invocation_policy:&system,invocation_state:None,messages:&messages,tools:&request.tools,profile:input.providers.writer.profile()})?;memory.checkpoint(&messages)?;
    }
    let wire=messages.iter().map(|m|json!({"role":format!("{:?}",m.role()),"blocks":m.content().iter().map(|b|match b {
        ContentBlock::Text(t)=>json!({"type":"text","text":t.expose_for_wire()}),
        ContentBlock::ToolCall(c)=>json!({"type":"call","id":c.id().expose_for_wire(),"name":c.name().as_str()}),
        ContentBlock::ToolResult(r)=>json!({"type":"result","id":r.call_id().expose_for_wire(),"output":r.output().to_wire_string()}),
        _=>json!({"type":"other"})
    }).collect::<Vec<_>>()})).collect::<Vec<_>>();Ok(json!({"messages":wire}))
}

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let mode = args.get(1).ok_or("mode required")?;
    let spec: Value = serde_json::from_slice(&fs::read(args.get(2).ok_or("spec file required")?)?)?;
    let result = match mode.as_str() {
        "probe" => probe(&spec)?,
        "mixed-batch" => mixed_batch(&spec)?,
        "context" => context(&spec)?,
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
