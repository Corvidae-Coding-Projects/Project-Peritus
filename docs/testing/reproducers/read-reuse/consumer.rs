//! Drive production memory/tool APIs; the independent fixture owner controls inputs and mutations.
mod application {
    include!("../checkpoint-cache/context-consumer.rs");

    use std::io::{BufRead as _, Write as _};

    struct CurrentConversation {
        task: String,
        deny: Arc<AtomicBool>,
        protected: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    }
    impl ConversationView for CurrentConversation {
        fn revision(&self) -> u64 {
            1
        }
        fn render(&self) -> String {
            self.task.clone()
        }
        fn effective_permissions(&self) -> peritus_product_runner::control::HostPermissions {
            let all = peritus_product_runner::control::HostPermissions::all();
            if self.deny.load(Ordering::Acquire) {
                all.without(peritus_product_runner::control::PermissionCapability::Read)
            } else {
                all
            }
        }
        fn protected_paths(&self) -> Vec<PathBuf> {
            self.protected.lock().unwrap().clone()
        }
    }

    fn io_bytes() -> u64 {
        fs::read_to_string("/proc/self/io")
            .ok()
            .and_then(|s| {
                s.lines().find_map(|l| l.strip_prefix("rchar: ").and_then(|s| s.parse().ok()))
            })
            .unwrap_or(0)
    }
    fn text(role: Role, value: &str) -> Result<Message> {
        Ok(Message::new(
            role,
            vec![ContentBlock::Text(BoundedText::new(
                value.to_owned(),
                ProtocolLimits::PRODUCTION,
            )?)],
            ProtocolLimits::PRODUCTION,
        )?)
    }
    fn wire(messages: &[Message]) -> Value {
        json!(messages.iter().map(|m| json!({"role":format!("{:?}",m.role()),"blocks":m.content().iter().map(|b| match b {
            ContentBlock::Text(t)=>json!({"type":"text","text":t.expose_for_wire()}),
            ContentBlock::ToolCall(c)=>json!({"type":"call","id":c.id().expose_for_wire(),"name":c.name().as_str()}),
            ContentBlock::ToolResult(r)=>json!({"type":"result","id":r.call_id().expose_for_wire(),"is_error":r.is_error(),"output":r.output().to_wire_string()}),
            _=>json!({"type":"other"})
        }).collect::<Vec<_>>()})).collect::<Vec<_>>())
    }

    pub fn run_live(spec: Value) -> Result<Value> {
        tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(live(&spec))
    }

    pub fn run(spec: Value) -> Result<()> {
        let mut input = input(&spec)?;
        let deny = Arc::new(AtomicBool::new(false));
        let protected = Arc::new(std::sync::Mutex::new(Vec::new()));
        input.conversation = Arc::new(CurrentConversation {
            task: input.task.clone(),
            deny: deny.clone(),
            protected: protected.clone(),
        });
        let mut tools = input.read_only_workspace_tools();
        let mut memory = LocalContextHandle::for_run(&input, "writer")?.ok_or("memory disabled")?;
        let mut system = text(Role::System, "CURRENT-POLICY-INITIAL-7193")?;
        let mut messages = vec![system.clone(), text(Role::User, &input.task)?];
        let mut generation = 1;
        let mut request = DeveloperLoopRequest {
            request_prefix: format!("reuse-{generation}"),
            system: "Fixture host".into(),
            prompt: input.task.clone(),
            attachments: Vec::new(),
            tools: input.developer_definitions()?,
            limits: DeveloperLoopLimits::new(48, 512)?,
            cancellation: CancellationToken::new(),
        };
        memory.open(&request, &messages)?;
        let mut pending = Vec::new();
        for (n, line) in std::io::stdin().lock().lines().enumerate() {
            let command: Value = serde_json::from_str(&line?)?;
            let before = io_bytes();
            let started = std::time::Instant::now();
            let answer = (|| -> Result<Value> {
                match command["action"].as_str().unwrap_or("read") {
                    "deny" => {
                        deny.store(command["value"].as_bool().unwrap(), Ordering::Release);
                        Ok(json!({"changed":true}))
                    }
                    "protect" => {
                        input.workspace_kind = ProductWorkspaceKind::InPlace {
                            protected_paths: command["paths"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|v| PathBuf::from(v.as_str().unwrap()))
                                .collect(),
                            baseline_revision: 1,
                        };
                        tools = input.read_only_workspace_tools();
                        Ok(json!({"changed":true}))
                    }
                    "cancel" => {
                        input
                            .cancelled
                            .store(command["value"].as_bool().unwrap(), Ordering::Release);
                        Ok(json!({"changed":true}))
                    }
                    "policy" => {
                        system = text(Role::System, command["text"].as_str().unwrap())?;
                        Ok(json!({"changed":true}))
                    }
                    "begin" => {
                        generation += 1;
                        request.request_prefix = format!("reuse-{generation}");
                        request.prompt = command["text"].as_str().unwrap().to_owned();
                        messages = vec![system.clone(), text(Role::User, &request.prompt)?];
                        memory.open(&request, &messages)?;
                        Ok(json!({"changed":true}))
                    }
                    "read" => {
                        let c = call(
                            n,
                            command["tool"].as_str().unwrap_or("workspace_read"),
                            &command["args"],
                        )?;
                        let o = tools.execute(&c)?;
                        Ok(
                            json!({"is_error":o.is_error,"output":serde_json::from_slice::<Value>(o.output.canonical_bytes())?}),
                        )
                    }
                    "observe" | "propose" => {
                        let calls = command["calls"]
                            .as_array()
                            .ok_or("calls required")?
                            .iter()
                            .enumerate()
                            .map(|(i, v)| {
                                call(format!("{generation}_{n}_{i}"), v[0].as_str().unwrap(), &v[1])
                            })
                            .collect::<Result<Vec<_>>>()?;
                        let assistant = Message::new(
                            Role::Assistant,
                            calls.iter().cloned().map(ContentBlock::ToolCall).collect(),
                            ProtocolLimits::PRODUCTION,
                        )?;
                        memory.observe(DeveloperContextEvent::Message(&assistant))?;
                        messages.push(assistant);
                        pending = calls;
                        if command["action"] == "propose" {
                            return Ok(json!({"proposed":pending.len()}));
                        }
                        let outputs = settle(&mut pending, &mut tools, &mut memory, &mut messages)?;
                        Ok(json!({"outputs":outputs}))
                    }
                    "settle" => {
                        let outputs = settle(&mut pending, &mut tools, &mut memory, &mut messages)?;
                        Ok(json!({"outputs":outputs}))
                    }
                    "role" => {
                        memory =
                            LocalContextHandle::for_run(&input, command["role"].as_str().unwrap())?
                                .ok_or("memory disabled")?;
                        generation += 1;
                        request.request_prefix = format!("reuse-{generation}");
                        messages = vec![system.clone(), text(Role::User, &request.prompt)?];
                        memory.open(&request, &messages)?;
                        Ok(json!({"role":command["role"]}))
                    }
                    "assemble" => {
                        let state = command["state"]
                            .as_str()
                            .map(|s| text(Role::Developer, s))
                            .transpose()?;
                        let governing = command["governing"]
                            .as_str()
                            .map(|s| text(Role::User, s))
                            .transpose()?;
                        messages = memory.assemble(DeveloperContextAssembly {
                            provider: input.providers.writer.as_ref(),
                            governing_input: governing.as_ref(),
                            invocation_policy: &system,
                            invocation_state: state.as_ref(),
                            messages: &messages,
                            tools: &request.tools,
                            profile: input.providers.writer.profile(),
                        })?;
                        memory.checkpoint(&messages)?;
                        Ok(json!({"messages":wire(&messages)}))
                    }
                    _ => Err("unknown fixture command".into()),
                }
            })();
            let answer = match answer {
                Ok(value) => json!({"ok":true,"value":value}),
                Err(error) => json!({"ok":false,"error":error.to_string()}),
            };
            println!(
                "{}",
                json!({"answer":answer,"rchar":io_bytes().saturating_sub(before),"elapsed_us":started.elapsed().as_micros()})
            );
            std::io::stdout().flush()?;
        }
        Ok(())
    }
    fn settle(
        pending: &mut Vec<CompletedToolCall>,
        tools: &mut impl DeveloperToolExecutor,
        memory: &mut LocalContextHandle,
        messages: &mut Vec<Message>,
    ) -> Result<Vec<Value>> {
        let mut results = Vec::new();
        let mut wrapper = peritus_product_runner::MemoryTools::new(tools, memory.clone());
        wrapper.observe_model_context(messages)?;
        for c in pending.drain(..) {
            let o = wrapper.execute(&c)?;
            results.push(json!({"is_error":o.is_error,"output":serde_json::from_slice::<Value>(o.output.canonical_bytes())?}));
            memory.observe(DeveloperContextEvent::ToolObservation { call: &c, observation: &o })?;
            let mut value: Value = serde_json::from_slice(o.output.canonical_bytes())?;
            if let Some(reference) = memory.source_reference(&c)? {
                value["local_context"] = serde_json::from_slice(reference.canonical_bytes())?;
            }
            let result = Message::new(
                Role::Tool,
                vec![ContentBlock::ToolResult(ToolResult::new(
                    c.id().clone(),
                    CanonicalJson::parse(
                        &value.to_string(),
                        JsonBounds::value(ProtocolLimits::PRODUCTION),
                    )?,
                    o.is_error,
                ))],
                ProtocolLimits::PRODUCTION,
            )?;
            memory.observe(DeveloperContextEvent::Message(&result))?;
            messages.push(result);
        }
        memory.observe(DeveloperContextEvent::BatchCompleted)?;
        Ok(results)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::args().nth(1).ok_or("spec path required")?,
    )?)?;
    if spec["live"].as_bool().unwrap_or(false) {
        println!("{}", application::run_live(spec)?);
        Ok(())
    } else {
        application::run(spec)
    }
}
