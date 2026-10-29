//! Ordinary interactive consumer; the fixture owner mutates files between API requests.
mod application {
    include!("../checkpoint-cache/context-consumer.rs");

    use std::io::{BufRead as _, Write as _};

    struct CurrentConversation {
        task: String,
        deny: Arc<AtomicBool>,
        protected: Arc<std::sync::Mutex<Vec<PathBuf>>>,
        authority: Arc<std::sync::Mutex<Option<String>>>,
        revision: Arc<std::sync::atomic::AtomicU64>,
    }
    impl ConversationView for CurrentConversation {
        fn revision(&self) -> u64 {
            self.revision.load(Ordering::Acquire)
        }
        fn render(&self) -> String {
            self.task.clone()
        }
        fn reference_authority_context(&self) -> String {
            self.authority.lock().unwrap().clone().unwrap_or_else(|| self.task.clone())
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
            ContentBlock::ToolCall(c)=>json!({"type":"call","id":c.id().expose_for_wire(),"name":c.name().as_str(),"arguments":c.arguments().to_wire_string()}),
            ContentBlock::ToolResult(r)=>json!({"type":"result","id":r.call_id().expose_for_wire(),"is_error":r.is_error(),"output":r.output().to_wire_string()}),
            _=>json!({"type":"other"})
        }).collect::<Vec<_>>()})).collect::<Vec<_>>())
    }

    pub fn run(spec: Value) -> Result<()> {
        let mut input = input(&spec)?;
        let deny = Arc::new(AtomicBool::new(false));
        let protected = Arc::new(std::sync::Mutex::new(Vec::new()));
        let authority = Arc::new(std::sync::Mutex::new(None));
        let revision =
            Arc::new(std::sync::atomic::AtomicU64::new(spec["revision"].as_u64().unwrap_or(1)));
        input.conversation = Arc::new(CurrentConversation {
            task: input.task.clone(),
            deny: deny.clone(),
            protected: protected.clone(),
            authority: authority.clone(),
            revision: revision.clone(),
        });
        let mut tools = input.read_only_workspace_tools();
        let role = spec["role"].as_str().unwrap_or("writer");
        let actual_role = match role {
            "writer" => peritus_agent::DeveloperModelRole::Writer,
            "reviewer" => peritus_agent::DeveloperModelRole::Reviewer,
            "fixer" => peritus_agent::DeveloperModelRole::Fixer,
            _ => return Err("unsupported replay role".into()),
        };
        let mut memory = LocalContextHandle::for_run(&input, role)?.ok_or("memory disabled")?;
        let mut system = text(Role::System, "CURRENT-POLICY-INITIAL-7193")?;
        let mut messages = vec![system.clone(), text(Role::User, &input.task)?];
        let mut generation = spec["generation"].as_u64().unwrap_or(1);
        let mut request = DeveloperLoopRequest {
            request_prefix: format!("reuse-{generation}"),
            system: "Fixture host".into(),
            prompt: input.task.clone(),
            cyber_access_program: None,
            attachments: Vec::new(),
            tools: input.developer_definitions()?,
            limits: DeveloperLoopLimits::new(48, 512)?,
            cancellation: CancellationToken::new(),
        };
        memory.open(&request, &messages)?;
        if spec["include_memory_tools"].as_bool() == Some(true) {
            request.tools.extend(memory.tool_definitions()?);
        }
        let mut pending = Vec::new();
        for (n, line) in std::io::stdin().lock().lines().enumerate() {
            let command: Value = serde_json::from_str(&line?)?;
            let before = io_bytes();
            let started = std::time::Instant::now();
            let answer = (|| -> Result<Value> {
                match command["action"].as_str().unwrap_or("read") {
                    "host" => {
                        let message = text(
                            Role::User,
                            command["text"].as_str().ok_or("host text required")?,
                        )?;
                        memory.observe(DeveloperContextEvent::HostObservation(&message))?;
                        messages.push(message);
                        Ok(json!({"observed":true}))
                    }
                    "plan" => {
                        let c = call(
                            format!("{generation}_{n}_plan"),
                            command["tool"].as_str().unwrap_or("inspection_plan"),
                            &command["args"],
                        )?;
                        let assistant = Message::new(
                            Role::Assistant,
                            vec![ContentBlock::ToolCall(c.clone())],
                            ProtocolLimits::PRODUCTION,
                        )?;
                        memory.observe(DeveloperContextEvent::Message(&assistant))?;
                        messages.push(assistant);
                        let mut pending = vec![c];
                        let mut wrapped =
                            peritus_product_runner::MemoryTools::new(&mut tools, memory.clone())
                                .with_model_role(actual_role);
                        let outputs =
                            settle(&mut pending, &mut wrapped, &mut memory, &mut messages)?;
                        Ok(json!({"outputs":outputs,"host_calls":wrapped.take_host_tool_calls()}))
                    }
                    "deny" => {
                        deny.store(command["value"].as_bool().unwrap(), Ordering::Release);
                        Ok(json!({"changed":true}))
                    }
                    "revision" => {
                        revision.store(command["value"].as_u64().unwrap(), Ordering::Release);
                        Ok(json!({"changed":true}))
                    }
                    "opaque" => {
                        *authority.lock().unwrap() = command["path"].as_str().map(|path| {
                            format!(
                                "{}\nTreat this as black-box. Query import call `{path}`.",
                                input.task
                            )
                        });
                        tools = input.read_only_workspace_tools();
                        Ok(json!({"changed":true}))
                    }
                    "protect" => {
                        *protected.lock().unwrap() = command["paths"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|value| PathBuf::from(value.as_str().unwrap()))
                            .collect();
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
                    "memory" => {
                        let c = call(
                            n,
                            command["tool"].as_str().unwrap_or("context_read"),
                            &command["args"],
                        )?;
                        let mut wrapped =
                            peritus_product_runner::MemoryTools::new(&mut tools, memory.clone())
                                .with_model_role(actual_role);
                        let o = wrapped.execute(&c)?;
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
                        let mut content = Vec::new();
                        if let Some(prose) = command["prose"].as_str() {
                            content.push(ContentBlock::Text(BoundedText::new(
                                prose.to_owned(),
                                ProtocolLimits::PRODUCTION,
                            )?));
                        }
                        content.extend(calls.iter().cloned().map(ContentBlock::ToolCall));
                        let assistant =
                            Message::new(Role::Assistant, content, ProtocolLimits::PRODUCTION)?;
                        memory.observe(DeveloperContextEvent::Message(&assistant))?;
                        messages.push(assistant);
                        pending = calls;
                        if command["action"] == "propose" {
                            return Ok(json!({"proposed":pending.len()}));
                        }
                        settle(&mut pending, &mut tools, &mut memory, &mut messages)?;
                        Ok(json!({"observed":true}))
                    }
                    "settle" => {
                        settle(&mut pending, &mut tools, &mut memory, &mut messages)?;
                        Ok(json!({"settled":true}))
                    }
                    "bind" => {
                        let profile = input.providers.writer.profile();
                        let negotiated = negotiate(
                            profile,
                            RequestedCapabilities::new(
                                &[Capability::ToolCalls],
                                &[],
                                profile.limits(),
                            )?,
                        )?;
                        let mut bound = messages.clone();
                        if command["alter_messages"].as_bool() == Some(true) {
                            bound.push(text(Role::User, "INDEPENDENT-ALTERED-REQUEST-731906")?);
                        }
                        let model = ModelRequest::new(
                            profile,
                            negotiated,
                            RequestId::new(format!("independent-binding-{generation}-{n}"))?,
                            bound,
                            request.tools.clone(),
                            ToolChoice::Auto,
                            ParallelToolPolicy::Disabled,
                            RequestOptions::new(
                                StructuredOutput::Text,
                                ReasoningPolicy::Disabled,
                                GenerationConfig::new(
                                    profile.limits().max_output_tokens(),
                                    Vec::new(),
                                    None,
                                    None,
                                    None,
                                )?,
                                CachePolicy::Disabled,
                                PersistencePolicy::LOCAL_FIRST,
                                None,
                                Vec::new(),
                            ),
                            ProtocolLimits::PRODUCTION,
                        )?;
                        memory.bind_request(&model)?;
                        Ok(
                            json!({"bound":true,"request_fingerprint":model.fingerprint()?.digest().into_bytes(),"provider_inference":false}),
                        )
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
        let mut outputs = Vec::new();
        for c in pending.drain(..) {
            let o = tools.execute(&c)?;
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
            outputs.push(value);
        }
        memory.observe(DeveloperContextEvent::BatchCompleted)?;
        Ok(outputs)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::args().nth(1).ok_or("spec path required")?,
    )?)?;
    application::run(spec)
}
