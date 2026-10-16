//! Production LocalContextHandle assembly on copied closed state; no provider inference/effects.
use peritus_agent::{DeveloperContextAssembly,DeveloperContextPort,DeveloperLoopLimits,estimate_developer_request_tokens};
use peritus_context::{ContextNodeId,working::*};
use peritus_model_protocol::{CancellationKind,Capability,CapabilityMatrix,CapabilityProvenance,ContentBlock,ModelLimits,ModelName,OutputLimitEnforcement,ProtocolLimits,ProviderName,ProviderProfile,ResumeKind,Role,StateMode,WireDialect,decode_messages,encode_messages};
use peritus_product_runner::context_projection::LocalContextHandle;
use peritus_product_runner::{CommandRuntime,ConversationView,LocalContextConfig,ProductDeliveryScope,ProductRunInput,ProductWorkspaceKind,RoleProviders,inspect_local_context};
use peritus_provider_core::{CancellationToken,ModelProvider,ProcessLimits};
use peritus_provider_openai::{CodexExecutable,CodexRuntimeConfig,CodexRuntimeProvider};
use peritus_types::{ProviderProfileId,RunId,WorkspaceId};
use peritus_role::HarnessRole;
use serde_json::{Value,json};
use std::{error::Error,fs,path::Path,sync::{Arc,atomic::AtomicBool},time::Duration,collections::BTreeSet};
fn bytes<const N:usize>(value:&str)->[u8;N]{let mut out=[0;N];for(i,b)in out.iter_mut().enumerate(){*b=u8::from_str_radix(&value[i*2..i*2+2],16).unwrap();}out}
fn hex(value:&[u8])->String{value.iter().map(|b|format!("{b:02x}")).collect()}
fn artifact(root:&Path,desc:&Value)->Vec<u8>{let hash=hex(&desc["digest"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()as u8).collect::<Vec<_>>());let data=fs::read(root.join("artifacts/objects/sha256").join(&hash[..2]).join(&hash)).unwrap();assert_eq!(data.len()as u64,desc["bytes"].as_u64().unwrap());assert_eq!(hex(peritus_codec::sha256(&data).as_bytes()),hash);data}
fn checkpoint_state(root:&Path,checkpoint:&Value,binding:WorkingBinding)->Result<WorkingState,Box<dyn Error>>{
 let bytes=artifact(root,&checkpoint["working_state"]);
 if bytes.starts_with(b"PWMS"){return Ok(decode_working_state(&bytes,binding,WorkingLimits::standard())?);}
 let manifest:Value=serde_json::from_slice(&bytes)?;assert_eq!(manifest["version"],1);assert_eq!(manifest["kind"],"working");
 let mut decoder=WorkingStatePageDecoder::new(binding,WorkingLimits::standard());
 for page in manifest["pages"].as_array().unwrap(){decoder.push(&artifact(root,page))?;}
 let state=decoder.finish()?;assert_eq!(Some(state.through_observation()),manifest["records"].as_u64());Ok(state)
}
fn checkpoint_sources(root:&Path,checkpoint:&Value)->Result<Vec<Value>,Box<dyn Error>>{
 let bytes=artifact(root,&checkpoint["source_index"]);let data:Value=serde_json::from_slice(&bytes)?;
 if let Some(values)=data.as_array(){return Ok(values.clone());}
 assert_eq!(data["version"],1);assert_eq!(data["kind"],"sources");let mut result=Vec::new();
 for page in data["pages"].as_array().unwrap(){let values:Vec<Value>=serde_json::from_slice(&artifact(root,page))?;result.extend(values);}
 assert_eq!(Some(result.len()as u64),data["records"].as_u64());for(index,source)in result.iter().enumerate(){assert_eq!(source["sequence"].as_u64(),Some(index as u64+1));}Ok(result)
}
struct Conversation(u64);
impl ConversationView for Conversation{fn revision(&self)->u64{self.0}fn render(&self)->String{"Retain the current governing request from the exact copied durable pins.".into()}fn uses_explicit_media(&self)->bool{true}}
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

fn config_contract()->Result<(),Box<dyn Error>>{
 for value in [0,1,4096,16384,16385,u64::MAX]{
  let config:LocalContextConfig=serde_json::from_value(json!({"working_state_max_tokens":value,"retrieved_evidence_max_tokens":value}))?;
  config.validate()?;let encoded=serde_json::to_value(&config)?;
  assert!(encoded.get("working_state_max_tokens").is_none()&&encoded.get("retrieved_evidence_max_tokens").is_none());
  println!("{}",json!({"case":"legacy-config-compatible","value":value,"result":"PASS"}));
 }
 let limits=DeveloperLoopLimits::new(48,512)?;
 assert_eq!(limits.max_output_tokens(),u64::MAX);
 for value in [32769,64000,u64::MAX]{assert_eq!(limits.with_max_output_tokens(value)?.max_output_tokens(),value);}
 assert!(limits.with_max_output_tokens(0).is_err());
 println!("{}",json!({"case":"uncapped-default-explicit-output-api","result":"PASS"}));Ok(())
}
fn main()->Result<(),Box<dyn Error>>{
 config_contract()?;
 let argument=std::env::args().nth(1).expect("usage: host-consumer PREPARED_HOST_DIRECTORY");let host=Path::new(&argument);
 let settings:Value=serde_json::from_slice(&fs::read(host.join("settings.json"))?)?;
 let snapshot:Value=serde_json::from_slice(&fs::read(host.join("snapshot.json"))?)?;
 let root=host.join("replay.context/writer");
 let run=RunId::new(bytes(settings["run_id"].as_str().unwrap())).expect("validated run metadata");
 let workspace=WorkspaceId::new(bytes(settings["workspace_id"].as_str().unwrap())).expect("validated workspace metadata");
 let binding=WorkingBinding::new(run,workspace,ContextNodeId::new(run.into_bytes()).expect("validated task metadata"),HarnessRole::Writer,0);
 let mut state=checkpoint_state(&root,&snapshot["checkpoint"],binding)?;
 for record in snapshot["events"].as_array().unwrap(){
  if let Some(observation)=record.get("observation"){state=apply_working_event(&state,&decode_working_event(&artifact(&root,&observation["reducer"]),state.binding(),WorkingLimits::standard())?)?;}
  else if let Some(event)=record.get("state_event"){state=apply_working_event(&state,&decode_working_event(&artifact(&root,&event["reducer"]),state.binding(),WorkingLimits::standard())?)?;}
 }
 let exact_before=encode_working_state(&state)?;fs::write(host.join("recovered-before.pwms"),&exact_before)?;
 let mut pins=Vec::new();
 for pin in state.protocol(state.binding())?.requirements(){let source=snapshot["sources"].as_array().unwrap().iter().find(|s|s["sequence"].as_u64()==Some(pin.get())).unwrap();pins.extend(decode_messages(&artifact(&root,&source["artifact"]),ProtocolLimits::PRODUCTION)?);}
 let system=pins.iter().find(|p|p.role()==Role::System).unwrap().clone();
 let workspace_root=Path::new(settings["workspace"].as_str().unwrap()).to_path_buf();
 let provider=Arc::new(CodexRuntimeProvider::new(CodexRuntimeConfig::new(CodexExecutable::pin(settings["executable"].as_str().unwrap())?,profile(&settings)?,ProcessLimits::new(2*1024*1024,2*1024*1024,64*1024,Duration::from_secs(60))?)?));
 let processes=peritus_process::ProcessStore::open(host.join("commands/processes"),&workspace_root)?;
 let runtime=CommandRuntime::open_direct(host.join("commands/router"),&workspace_root,run,processes)?;
 let provider_dyn:Arc<dyn ModelProvider>=provider.clone();
 let input=ProductRunInput{workspace_kind:ProductWorkspaceKind::InPlace{protected_paths:vec![],baseline_revision:state.binding().conversation_revision()},run_id:run,workspace_id:workspace,workspace_root,trace_path:host.join("replay.trace"),command_runtime:runtime,finding_state:String::new(),task:String::new(),max_elapsed:None,delivery_scope:ProductDeliveryScope::WorkspaceChanges,conversation:Arc::new(Conversation(state.binding().conversation_revision())),providers:RoleProviders{writer:provider_dyn.clone(),reviewer:provider_dyn.clone(),fixer:provider_dyn,fallbacks:vec![]},cancelled:Arc::new(AtomicBool::new(false)),provider_cancellation:CancellationToken::new(),resume:None};
 let mut context=LocalContextHandle::for_run(&input,"writer")?.expect("working memory enabled");
 let additional=context.additional_request_framing_tokens()?;
 if std::env::args().any(|arg|arg=="--load-only"){
  println!("{}",json!({"case":"production-context-load-over-1gib-trace","result":"PASS","trace_bytes":fs::metadata(host.join("replay.trace"))?.len(),"retained_sources":state.through_observation(),"retained_entries":state.entries(state.binding())?.len(),"required_framing_tokens":additional,"provider_requests":0,"effects_dispatched":0}));return Ok(());
 }
 let mut tools=input.developer_definitions()?;tools.extend(context.tool_definitions()?);
 let view=context.assemble(DeveloperContextAssembly{provider:provider.as_ref(),governing_input:None,invocation_policy:&system,messages:&pins,tools:&tools,profile:provider.profile()})?;
 let body=view.iter().flat_map(|m|m.content()).filter_map(|b|if let ContentBlock::Text(t)=b{Some(t.expose_for_wire())}else{None}).filter(|t|t.starts_with("LOCAL WORKING STATE —")).collect::<Vec<_>>().join("\n");
 let entries=state.entries(state.binding())?;
 if !entries.is_empty(){assert!(body.contains("All entries and exact source bytes remain durable")&&body.contains("context_read"),"provider-sized view lacks exact retrieval notice");}
 let ids:BTreeSet<_>=entries.iter().map(|entry|hex(entry.id().as_bytes())).collect();let scope=hex(&snapshot["checkpoint"]["scope"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()as u8).collect::<Vec<_>>());
 let mut visible=0;
 for line in body.lines(){
  let Ok(record)=serde_json::from_str::<Value>(line)else{continue};let Some(key)=record["id"].as_str().and_then(|v|v.strip_prefix("entry:"))else{continue};
  assert!(ids.contains(key),"provider view fabricated an entry");let entry=entries.iter().find(|entry|hex(entry.id().as_bytes())==key).unwrap();
  assert_eq!(record["authority"],"none");assert_eq!(record["text"].as_str().unwrap(),String::from_utf8_lossy(entry.content().bytes()));
  assert_eq!(record["supports"],json!(entry.links().supports().iter().map(|id|format!("obs:{scope}:{:06}",id.get())).collect::<Vec<_>>()));
  assert_eq!(record["contradicts"],json!(entry.links().contradicts().iter().map(|id|format!("obs:{scope}:{:06}",id.get())).collect::<Vec<_>>()));
  assert_eq!(record["depends_on"],json!(entry.links().depends_on().iter().map(|id|format!("entry:{}",hex(id.as_bytes()))).collect::<Vec<_>>()));visible+=1;
 }
 assert!(provider.context_input_fits(&view,&tools)?);
 let serialized=encode_messages(&view,ProtocolLimits::PRODUCTION)?;fs::write(host.join("assembled.c5"),&serialized)?;
 context.checkpoint(&view)?;drop(context);
 let published:Value=serde_json::from_str(&inspect_local_context(&input.trace_path,run,workspace,HarnessRole::Writer)?)?;
 let after=checkpoint_state(&root,&published["checkpoint"],binding)?;let sources=checkpoint_sources(&root,&published["checkpoint"])?;assert_eq!(sources.len()as u64,after.through_observation());
 let after_ids:BTreeSet<_>=after.entries(after.binding())?.iter().map(|e|hex(e.id().as_bytes())).collect();assert_eq!(ids,after_ids);
 for entry in state.entries(state.binding())?{let found=after.entry(after.binding(),entry.id())?;assert_eq!(found.content(),entry.content());assert_eq!(found.links(),entry.links());}
 fs::write(host.join("published-inspection.json"),serde_json::to_vec(&published)?)?;
 println!("{}",json!({"case":"copied-real-host-assembly","result":"PASS","entries":ids.len(),"visible_exact_entries":visible,"published_omitted_entries":published["validation"]["omitted_entries"],"checkpoint_schema":published["checkpoint"]["schema_version"],"published_sources":sources.len(),"required_framing_tokens":additional,"assembled_estimated_tokens":estimate_developer_request_tokens(&view,&tools),"provider_input_target":provider.context_input_target(),"actual_provider_encoding_fits":true,"assembly_bytes":serialized.len(),"assembly_sha256":hex(peritus_codec::sha256(&serialized).as_bytes()),"entry_content_and_source_links_preserved":true,"uncovered_tail_recovered":snapshot["events"].as_array().unwrap().len(),"provider_requests":0,"effects_dispatched":0}));
 Ok(())
}
