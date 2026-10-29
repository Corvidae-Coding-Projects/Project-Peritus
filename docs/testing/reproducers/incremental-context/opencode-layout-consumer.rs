//! Direct ordinary consumer of C5 and real host-owned workspace tools.
mod application {
    include!("../checkpoint-cache/context-consumer.rs");

    fn usage(value: UsageCounters) -> Value {
        json!({"input_tokens":value.input_tokens(),"cached_input_tokens":value.cached_input_tokens(),
            "cache_creation_input_tokens":value.cache_creation_input_tokens(),"output_tokens":value.output_tokens(),
            "reasoning_output_tokens":value.reasoning_output_tokens(),"tool_tokens":value.tool_tokens(),
            "total_tokens":value.total_tokens(),"provider_cost_microunits":value.provider_cost_microunits()})
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
    pub async fn run(spec: Value) -> Result<Value> {
        let input = input(&spec)?;
        let selected = if spec["reselect"].as_bool() == Some(true) {
            input.providers.writer.select_model(input.providers.writer.profile().model().clone())?
        } else {
            Arc::clone(&input.providers.writer)
        };
        let provider = selected.as_ref();
        let tools = input.developer_definitions()?;
        let choice = match spec["choice"].as_str().unwrap_or("auto") {
            "auto" => ToolChoice::Auto,
            "none" => ToolChoice::None,
            "required" => ToolChoice::Required,
            "specific" => ToolChoice::Specific(ToolName::new("workspace_read".to_owned())?),
            "unavailable" => {
                ToolChoice::Specific(ToolName::new("not_declared_for_this_task".to_owned())?)
            }
            _ => return Err("unknown selection".into()),
        };
        let limit = spec["parallel"].as_u64().unwrap_or(1);
        let parallel = if limit == 1 {
            ParallelToolPolicy::Disabled
        } else {
            ParallelToolPolicy::Allowed(limit as u32)
        };
        let profile = provider.profile();
        let request = ModelRequest::new(
            profile,
            negotiate(
                profile,
                RequestedCapabilities::new(
                    &[Capability::ToolCalls],
                    &[Capability::UsageDetail, Capability::ParallelToolCalls],
                    profile.limits(),
                )?,
            )?,
            RequestId::new("real-opencode-layout-control".to_owned())?,
            vec![
                text(
                    Role::System,
                    "Use only the declared Peritus tools. Tool proposals are inert until the host executes them. Preserve literal current task requirements.",
                )?,
                text(
                    Role::Developer,
                    spec["host_state"].as_str().unwrap_or("CURRENT HOST STATE A-728619"),
                )?,
                text(Role::User, &input.task)?,
            ],
            tools,
            choice,
            parallel,
            RequestOptions::new(
                StructuredOutput::Text,
                ReasoningPolicy::Disabled,
                GenerationConfig::new(4096, Vec::new(), None, None, None)?,
                CachePolicy::Disabled,
                PersistencePolicy::LOCAL_FIRST,
                None,
                Vec::new(),
            ),
            ProtocolLimits::PRODUCTION,
        );
        let request = match request {
            Ok(value) => value,
            Err(error) => {
                return Ok(json!({"rejected_before_start":true,"error":error.to_string()}));
            }
        };
        fs::write(
            spec["request_file"].as_str().ok_or("request_file required")?,
            request.canonical_bytes()?,
        )?;
        let definitions = request.tools().iter().map(|tool| {
            json!({"name":tool.name().as_str(),
                "description":tool.description().map(|value|value.expose_for_wire()),
                "arguments_schema":serde_json::from_slice::<Value>(tool.parameters().canonical_bytes()).expect("canonical schema")})
        }).collect::<Vec<_>>();
        let started = std::time::Instant::now();
        let mut stream = match provider.start(request, CancellationToken::new()).await {
            Ok(value) => value,
            Err(error) => {
                return Ok(json!({"rejected_before_stream":true,"error":error.to_string()}));
            }
        };
        let projections=stream.projections().iter().map(|p|json!({"layout":p.layout(),"reason":p.reason(),
            "part_bytes":p.part_bytes(),"part_characters":p.part_characters(),"native_outcome":p.native_outcome(),
            "native_usage":p.native_usage().map(usage)})).collect::<Vec<_>>();
        let mut reducer =
            ResponseReducer::new(profile.provider().clone(), ProtocolLimits::PRODUCTION);
        let mut corrections = Vec::new();
        while let Some(envelope) = stream.pull().await? {
            if let ModelEvent::ProviderEvent(value) = envelope.event() {
                if value.name().as_str() == "peritus.opencode_turn_correction" {
                    corrections
                        .push(serde_json::from_slice::<Value>(value.value().canonical_bytes())?);
                }
            }
            reducer.push(envelope)?;
        }
        let mut calls = Vec::new();
        let mut content = String::new();
        let mut host = input.read_only_workspace_tools();
        for item in reducer.completed_items() {
            match item {
                ReducedItem::Text { text, .. } => content.push_str(text.expose_for_wire()),
                ReducedItem::ToolCall { call, .. } => {
                    let output = host.execute(call)?;
                    calls.push(json!({"name":call.name().as_str(),"arguments":serde_json::from_slice::<Value>(call.arguments().canonical_bytes())?,
                        "host_is_error":output.is_error,"host_output":serde_json::from_slice::<Value>(output.output.canonical_bytes())?}));
                }
                _ => {}
            }
        }
        Ok(
            json!({"terminal":format!("{:?}",reducer.terminal()),"text":content,"calls":calls,"corrections":corrections,
            "usage":usage(reducer.usage_high_water()),"projections":projections,"definitions":definitions,"elapsed_seconds":started.elapsed().as_secs_f64()}),
        )
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec =
        serde_json::from_slice(&std::fs::read(std::env::args().nth(1).ok_or("spec required")?)?)?;
    println!("{}", application::run(spec).await?);
    Ok(())
}
