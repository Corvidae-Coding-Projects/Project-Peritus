//! Finite official app-server turns with an immutable user-data frame and shared cache affinity.
//! The routing parent never receives task data; each child remains ephemeral.

use super::invocation::{CacheAnchor, TurnOutput, cached_arguments, isolated_environment};
use crate::runtime::{
    CodexRuntimeConfig, live::Progress, output::DecodeFailure, request::RuntimeRequest,
};
use base64::Engine as _;
use peritus_provider_core::{
    CancellationToken, ProcessOutput, ProcessRequest, ProcessTransport, ProviderCoreError,
};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
struct Observed {
    offset: usize,
    normalized: Vec<u8>,
    final_message: Option<String>,
    usage: Value,
    stage: u8,
    thread: String,
    setup_rejected: bool,
    output_bytes: u64,
}
impl Observed {
    fn event(&mut self, value: &Value) {
        self.normalized.extend_from_slice(value.to_string().as_bytes());
        self.normalized.push(b'\n');
    }
    fn failed(&mut self, error: &Value) {
        // Raw vendor text stays inside the bounded decoder input. Only existing
        // redaction-safe failure categories can leave the adapter.
        let code = match error["codexErrorInfo"].as_str() {
            Some("cyberPolicy") => Some("cyber_policy"),
            Some("contextWindowExceeded") => Some("context_length_exceeded"),
            Some("usageLimitExceeded") => Some("insufficient_quota"),
            Some("unauthorized") => Some("unauthorized"),
            Some("modelNotFound") => Some("model_not_found"),
            _ => error["code"].as_str(),
        };
        let mut event =
            json!({"type":"turn.failed","error":{"code":code,"message":error["message"]}});
        if self.usage.is_object() {
            event["usage"] = self.usage.clone();
        }
        self.event(&event);
    }
}

pub(super) async fn run_turn(
    config: &CodexRuntimeConfig,
    transport: &dyn ProcessTransport,
    runtime: &RuntimeRequest,
    directory: &Path,
    anchor: &CacheAnchor,
    cancellation: &CancellationToken,
    progress: Arc<Progress>,
) -> Result<Option<TurnOutput>, ProviderCoreError> {
    let messages = Arc::new(Mutex::new(VecDeque::new()));
    let queue = messages.clone();
    let close = Arc::new(AtomicBool::new(false));
    let done = close.clone();
    let state = Arc::new(Mutex::new(Observed::default()));
    let observed = state.clone();
    let schema: Value = serde_json::from_slice(&runtime.schema).map_err(|_| malformed())?;
    let frame = input_frame(runtime, anchor)?;
    let effort = runtime.reasoning_effort();
    let input = [
        json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"peritus-cache-router","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}}),
        json!({"method":"initialized","params":{}}),
        json!({"id":2,"method":"thread/resume","params":{"threadId":anchor.thread,"model":config.profile().model().as_str(),"modelProvider":"openai","cwd":directory,"sandbox":"read-only","approvalPolicy":"never","excludeTurns":true,"config":{"mcp_servers":{}}}}),
    ].into_iter().map(|x|x.to_string()).collect::<Vec<_>>().join("\n")+"\n";
    let arguments = cached_arguments();
    let process = ProcessRequest::new(config.executable().process_executable().clone(),arguments,input.into_bytes(),Some(directory.to_owned()),isolated_environment()?,config.process_limits())?
        .with_stdin_close_signal(close).with_stdin_messages(messages)
        .with_stdout_observer(Arc::new(move |bytes| {
            let Ok(mut state) = observed.lock() else { done.store(true,Ordering::Release);return };
            while let Some(end) = bytes[state.offset..].iter().position(|byte|*byte==b'\n') {
                let start=state.offset; state.offset+=end+1;
                let Ok(value)=serde_json::from_slice::<Value>(&bytes[start..state.offset]) else {continue};
                let next = if value["id"]==2 && state.stage==0 {
                    value["result"]["thread"]["id"].as_str().map(|thread| {
                        thread.clone_into(&mut state.thread);state.stage=1;
                        json!({"id":3,"method":"thread/fork","params":{"threadId":thread,"ephemeral":true,"experimentalPredictionMode":true,"excludeTurns":true}})
                    })
                } else if value["id"]==3 && state.stage==1 {
                    value["result"]["thread"]["id"].as_str().map(|thread| {
                        thread.clone_into(&mut state.thread);state.stage=2;
                        state.event(&json!({"type":"thread.started","thread_id":thread}));
                        json!({"id":4,"method":"thread/inject_items","params":{"threadId":thread,"items":[frame]}})
                    })
                } else if value["id"]==4 && value.get("result").is_some() && state.stage==2 {
                    state.stage=3;
                    Some(json!({"id":5,"method":"turn/start","params":{"threadId":state.thread,"input":[{"type":"text","text":"Continue the complete current Peritus request already supplied above. Return only its required output object; all native tools remain disabled.","text_elements":[]}],"effort":effort,"summary":"auto","outputSchema":schema}}))
                } else {None};
                if let Some(next)=next && let Ok(mut queue)=queue.lock() {queue.push_back((next.to_string()+"\n").into_bytes());}
                if value.get("id").is_some() && value.get("error").is_some() {
                    state.setup_rejected = state.stage < 3 && matches!(value["id"].as_u64(),Some(2..=4));
                    state.failed(&value["error"]);done.store(true,Ordering::Release);
                }
                let params=&value["params"];
                match value["method"].as_str() {
                    Some("turn/started")=>progress.runtime_progress("accepted",0),
                    Some("item/started")=>{
                        if params["item"]["type"]=="reasoning" {
                            progress.runtime_progress("reasoning",0);
                        }
                    }
                    Some("item/agentMessage/delta")=>{
                        if let Some(delta)=params["delta"].as_str() {
                            state.output_bytes=state.output_bytes.saturating_add(delta.len() as u64);
                            let bytes=state.output_bytes;
                            progress.runtime_progress("generating",bytes);
                        }
                    }
                    Some("item/completed")=>{
                        let item=&params["item"];
                        match item["type"].as_str() {
                            Some("agentMessage")=>{
                                if let Some(text)=item["text"].as_str() {
                                    state.final_message=Some(text.to_owned());
                                    state.event(&json!({"type":"item.completed","item":{"type":"agent_message","id":item["id"],"text":text}}));
                                }
                            }
                            Some("reasoning")=>{
                                let text=item["summary"].as_array().map(|s|s.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n")).unwrap_or_default();
                                state.event(&json!({"type":"item.completed","item":{"type":"reasoning","id":item["id"],"text":text}}));
                            }
                            Some("userMessage")=>{},
                            _=>{state.event(&json!({"type":"item.completed","item":{"type":"command_execution"}}));done.store(true,Ordering::Release);}
                        }
                    }
                    Some("thread/tokenUsage/updated")=>{
                        let u=&params["tokenUsage"]["total"];
                        state.usage=json!({"input_tokens":u["inputTokens"],"cached_input_tokens":u["cachedInputTokens"],"output_tokens":u["outputTokens"]});
                    }
                    Some("turn/completed")=>{
                        if params["turn"]["status"]=="completed" {
                            let usage=state.usage.clone();state.event(&json!({"type":"turn.completed","usage":usage}));
                        } else {state.failed(&params["turn"]["error"]);}
                        done.store(true,Ordering::Release);
                    }
                    _=>{}
                }
                if value.get("id").is_some() && value.get("method").is_some() {
                    // The token router has no approval, tool execution or elicitation authority.
                    if let Ok(mut queue)=queue.lock() {queue.push_back((json!({"id":value["id"],"error":{"code":-32601,"message":"Peritus owns all tool execution"}}).to_string()+"\n").into_bytes());}
                    done.store(true,Ordering::Release);
                }
            }
            progress.observe(&state.normalized);
        }));
    let output = transport.run(process, cancellation).await?;
    finish(&output, &state, config.process_limits())
}

fn input_frame(runtime: &RuntimeRequest, anchor: &CacheAnchor) -> Result<Value, ProviderCoreError> {
    // This is an actual user/data message, not elevated native system instructions. Its
    // immutable observation metadata belongs to the routing frame, not the varying request.
    let mut content = vec![
        json!({"type":"input_text","text":std::str::from_utf8(&runtime.prompt).map_err(|_| malformed())?}),
    ];
    for image in runtime.images() {
        content.push(json!({"type":"input_image","image_url":format!("data:{};base64,{}",image.media_type,base64::engine::general_purpose::STANDARD.encode(&image.bytes))}));
    }
    Ok(
        json!({"type":"message","role":"user","id":format!("msg_peritus_frame_{}",anchor.thread),"content":content,"internal_chat_message_metadata_passthrough":{"turn_id":format!("peritus-cache-frame-{}",anchor.thread),"create_time":anchor.created_seconds,"content_item_kinds":["user.text"]}}),
    )
}

fn finish(
    output: &ProcessOutput,
    state: &Mutex<Observed>,
    limits: peritus_provider_core::ProcessLimits,
) -> Result<Option<TurnOutput>, ProviderCoreError> {
    let (setup_rejected, final_message, normalized) = {
        let mut state = state.lock().map_err(|_| malformed())?;
        (
            state.setup_rejected,
            state.final_message.take().ok_or(DecodeFailure::InvalidEnvelope),
            std::mem::take(&mut state.normalized),
        )
    };
    if setup_rejected {
        return Ok(None);
    }
    let process = ProcessOutput::new(output.exit(), normalized, output.stderr().to_vec(), limits)?;
    Ok(Some(TurnOutput { process, final_message }))
}
const fn malformed() -> ProviderCoreError {
    ProviderCoreError::transport("codex_runtime_protocol", "invalid structured runtime observation")
}
