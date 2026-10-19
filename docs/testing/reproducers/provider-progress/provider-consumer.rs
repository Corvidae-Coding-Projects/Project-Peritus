use peritus_model_protocol::*;
use peritus_provider_core::{CancellationToken,ModelProvider,ProcessLimits};
use peritus_provider_openai::{CodexExecutable,CodexRuntimeConfig,CodexRuntimeProvider};
use peritus_types::ProviderProfileId;
use serde_json::{json,Value};
use std::{error::Error,time::{Duration,Instant},fs,io::{Write,BufWriter}};
type Result<T> = std::result::Result<T,Box<dyn Error>>;
#[tokio::main(flavor="current_thread")]
async fn main()->Result<()> {
 let out=std::path::PathBuf::from(std::env::args().nth(1).ok_or("output required")?);fs::create_dir_all(&out)?;
 let mut log=BufWriter::new(fs::File::create(out.join("events.jsonl"))?);
 let caps=[Capability::Streaming,Capability::ToolCalls,Capability::PromptCaching,Capability::ReasoningControls,Capability::ReasoningSummaries,Capability::UsageDetail];
 let limits=ModelLimits::new(200000,4096,32,8,1)?;
 let profile=ProviderProfile::new(ProviderProfileId::new([0x97;16]).map_err(|_|"profile id")?,1,ProviderName::new("openai".into())?,ModelName::new("gpt-6.1-sol".into())?,WireDialect::OpenAiCodexRuntime,CapabilityMatrix::new(&caps,&[])?,CapabilityProvenance::Profiled,limits,OutputLimitEnforcement::Advisory,StateMode::StatelessReplay,ResumeKind::Unsupported,CancellationKind::BestEffortLocalAbort)?;
 let provider=CodexRuntimeProvider::new(CodexRuntimeConfig::new(CodexExecutable::discover()?,profile.clone(),ProcessLimits::new(4<<20,4<<20,65536,Duration::from_secs(120))?)?);
 let text=|role,body:String|Message::new(role,vec![ContentBlock::Text(BoundedText::new(body,ProtocolLimits::PRODUCTION)?)],ProtocolLimits::PRODUCTION);
 let unicode="µ漢字🪶".repeat(64);
 let cases=[("short-cached-text", "Return exactly PROGRESS-ORACLE-ASCII-449173 as the content of the required response, with no tool proposals.".to_owned(),CachePolicy::Automatic,Some("PROGRESS-ORACLE-ASCII-449173".to_owned())),("unicode-uncached-text",format!("Return exactly the following literal string as your response content with no added whitespace or tool proposals:\n{unicode}"),CachePolicy::Disabled,Some(unicode.clone())),("cached-host-tool", "Propose exactly one call to probe_echo with value set to the exact string ACK-🪶-é-739241. Leave response content empty. Do not execute any tool. This is an inert proposal for the host.".into(),CachePolicy::Automatic,None),("cancel-after-accepted", "Produce a long response with256 numbered paragraphs about fictional colors, each paragraph at least100words. No tools.".into(),CachePolicy::Automatic,None)];
 let mut results=Vec::new();
 for (n,(name,prompt,cache,expected)) in cases.into_iter().enumerate() {
  let is_tool=n==2;let is_cancel=n==3;
  let toolname=ToolName::new("probe_echo".into())?;
  let tools=if is_tool{vec![ToolDefinition::new(toolname.clone(),Some(BoundedText::new("An inert echo proposal. The consumer never executes this tool.".into(),ProtocolLimits::PRODUCTION)?),JsonSchema::parse(r#"{"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false}"#,SchemaDialect::Draft202012,JsonBounds::schema(ProtocolLimits::PRODUCTION))?,true)]}else{Vec::new()};
  let request=ModelRequest::new(&profile,negotiate(&profile,RequestedCapabilities::new(&caps,&[],limits)?)?,RequestId::new(format!("peritus-real-progress-{n}"))?,vec![text(Role::System,"Follow the current literal output request. This disposable real provider consumer tests progress reporting. Return the required structured response. Never use native tools.".into())?,text(Role::User,prompt)?],tools,if is_tool{ToolChoice::Specific(toolname)}else{ToolChoice::None},ParallelToolPolicy::Disabled,RequestOptions::new(StructuredOutput::Text,ReasoningPolicy::Effort{effort:ReasoningEffort::Low,summary:SummaryPolicy::None},GenerationConfig::new(4096,Vec::new(),None,None,None)?,cache.clone(),PersistencePolicy::LOCAL_FIRST,None,Vec::new()),ProtocolLimits::PRODUCTION)?;
  let started=Instant::now();let cancel=CancellationToken::new();let mut stream=provider.start(request,cancel.clone()).await?;let mut output=Vec::new();let mut arguments=Vec::new();let mut usage=None;let mut complete=false;let mut cancelled=false;let mut phases=Vec::new();let mut events=0;let mut accepted_at=None;let mut generating_at=None;let mut first_public_at=None;let mut cancelled_at=None;let mut tools_seen=0;let mut previous_sequence=0;let mut previous_bytes=0;
  while let Some(envelope)=stream.pull().await? {
   assert!(envelope.sequence()>previous_sequence,"nonmonotonic event sequence");previous_sequence=envelope.sequence();events+=1;let elapsed=started.elapsed().as_secs_f64();
   let event=match envelope.event(){
    ModelEvent::ResponseStarted{..}=>json!({"type":"response_started"}),
    ModelEvent::ProviderEvent(ext)=>{
     if ext.name().as_str()=="peritus.runtime_progress" {
      let payload:Value=serde_json::from_slice(ext.value().canonical_bytes())?;
      let obj=payload.as_object().ok_or("progress not object")?;assert_eq!(obj.len(),2,"unexpected progress field could expose text");assert!(obj.contains_key("phase")&&obj.contains_key("output_bytes"));let phase=payload["phase"].as_str().ok_or("phase missing")?;assert!(["accepted","reasoning","generating"].contains(&phase));let bytes=payload["output_bytes"].as_u64().ok_or("byte count missing")?;
      if phase=="accepted"{accepted_at.get_or_insert(elapsed);if is_cancel && cancelled_at.is_none(){let _=cancel.cancel();cancelled_at=Some(Instant::now());}}
      if phase=="generating"{assert!(bytes>0);assert!(bytes>=previous_bytes);previous_bytes=bytes;generating_at.get_or_insert(elapsed);}
      phases.push(json!({"phase":phase,"seconds":elapsed,"output_bytes":bytes}));json!({"type":"progress","payload":payload})
     }else{json!({"type":"other_provider_event","name":ext.name().as_str()})}
    },
    ModelEvent::TextDelta{fragment,..}=>{first_public_at.get_or_insert(elapsed);output.extend(fragment.expose());json!({"type":"text","bytes":fragment.len()})},
    ModelEvent::ToolCallStarted{name,..}=>{first_public_at.get_or_insert(elapsed);assert_eq!(name.as_str(),"probe_echo");tools_seen+=1;json!({"type":"tool_call_started","name":name.as_str()})},
    ModelEvent::ToolArgumentDelta{fragment,..}=>{arguments.extend(fragment.expose());json!({"type":"tool_arguments","bytes":fragment.len()})},
    ModelEvent::ReasoningSummaryDelta{fragment,..}=>json!({"type":"public_reasoning_summary","bytes":fragment.len()}),
    ModelEvent::ReasoningReplayDelta{..}=>return Err("private reasoning replay unexpectedly surfaced".into()),
    ModelEvent::Usage(value)=>{usage=Some(value.counters());json!({"type":"usage"})},
    ModelEvent::ResponseCompleted=>{complete=true;json!({"type":"completed"})},
    ModelEvent::ResponseCancelled=>{cancelled=true;json!({"type":"cancelled"})},
    ModelEvent::ResponseFailed(failure)=>{let event=json!({"case":name,"seconds":elapsed,"type":"failure","detail":format!("{failure:?}")});writeln!(log,"{event}")?;log.flush()?;return Err(format!("provider failure: {failure:?}").into())},
    _=>json!({"type":"other"}),
   };
   writeln!(log,"{}",json!({"case":name,"sequence":envelope.sequence(),"seconds":elapsed,"event":event}))?;log.flush()?;
  }
  assert!(accepted_at.is_some(),"no accepted progress");
  if is_cancel {assert!(cancelled&&!complete);assert!(output.is_empty()&&arguments.is_empty()&&tools_seen==0,"cancelled turn released unvalidated content");assert!(cancelled_at.ok_or("cancel not sent")?.elapsed()<Duration::from_secs(15));}
  else {assert!(complete&&!cancelled);if cache==CachePolicy::Automatic {assert!(generating_at.is_some(),"cached structured generation progress absent");assert!(generating_at.unwrap()<=first_public_at.ok_or("no public output")?);}if let Some(expected)=expected {assert_eq!(String::from_utf8(output.clone())?,expected);assert_eq!(tools_seen,0);}if is_tool {assert_eq!(tools_seen,1);assert!(output.is_empty());let args:Value=serde_json::from_slice(&arguments)?;assert_eq!(args,json!({"value":"ACK-🪶-é-739241"}));}}
  let usage=usage.map(|u|json!({"input_tokens":u.input_tokens(),"cached_input_tokens":u.cached_input_tokens(),"output_tokens":u.output_tokens()}));
  let result=json!({"case":name,"passed":true,"seconds":started.elapsed().as_secs_f64(),"events":events,"phases":phases,"first_public_seconds":first_public_at,"accepted_seconds":accepted_at,"first_generating_seconds":generating_at,"final_text_bytes":output.len(),"tool_proposals":tools_seen,"no_tool_execution":true,"cancelled":cancelled,"usage":usage});println!("{result}");std::io::stdout().flush()?;results.push(result);
 }
 fs::write(out.join("result.json"),serde_json::to_vec_pretty(&results)?)?;Ok(())
}
