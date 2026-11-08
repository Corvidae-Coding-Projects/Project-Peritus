//! Semantic recovery is a projection of an immutable source, never a second fact.
use super::super::{record::ArchiveKind, source_policy};
use super::{LocalMemory, error, handle, hex, rejected, sequence};
use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::ContentBlock;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    source: String,
    offset: usize,
    max_bytes: usize,
}

pub(super) fn retrieve(
    memory: &mut LocalMemory,
    call: &peritus_model_protocol::CompletedToolCall,
) -> Result<Value, DeveloperLoopError> {
    if let Some(value) = memory.activity.context_candidate(call)
        && cached_allowed(memory, call.arguments().canonical_bytes())?
    {
        return Ok(value);
    }
    execute(memory, call.arguments().canonical_bytes())
}

pub(in crate::local_context) fn execute(
    memory: &mut LocalMemory,
    bytes: &[u8],
) -> Result<Value, DeveloperLoopError> {
    let request: Request = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return Ok(rejected(memory, "invalid semantic recovery request")),
    };
    if !(256..=memory.config.max_read_bytes).contains(&request.max_bytes) {
        return Ok(rejected(memory, "semantic recovery page size is outside its declared bounds"));
    }
    let id = match sequence(memory, &request.source) {
        Ok(id) => id,
        Err(e) => return Ok(rejected(memory, &e.to_string())),
    };
    if !source_policy::allowed(memory, id)? {
        return Ok(rejected(memory, "source is excluded by current read/reference/private policy"));
    }
    memory.retrieval_calls =
        memory.retrieval_calls.checked_add(1).ok_or_else(|| error("retrieval counter overflow"))?;
    let source = memory.archived(id)?;
    let content = content(memory, id)?;
    let text = content.to_string();
    if request.offset > text.len() || !text.is_char_boundary(request.offset) {
        return Ok(rejected(memory, "offset is outside the semantic UTF-8 source"));
    }
    let mut result = json!({"authority":"none","representation":"semantic-json-v1","source":handle(memory,id),"source_kind":source.kind,"archive_sha256":hex(source.artifact.digest.as_bytes()),"archive_bytes":source.artifact.bytes,"sha256":hex(peritus_codec::sha256(text.as_bytes()).as_bytes()),"total_bytes":text.len(),"offset":request.offset,"next_offset":null,"data":"","archive_tool":"context_read"});
    // Search byte boundaries in place; an archive-sized character-offset table would
    // turn a bounded page request into a large secondary memory allocation.
    let (mut low, mut high) = (request.offset, text.len());
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        let mut end = text.floor_char_boundary(middle);
        if end == low {
            end = text.ceil_char_boundary(middle);
        }
        result["data"] = json!(&text[request.offset..end]);
        result["next_offset"] = if end < text.len() { json!(end) } else { Value::Null };
        if result.to_string().len() <= request.max_bytes {
            low = end;
        } else {
            high = text.floor_char_boundary(end - 1);
        }
    }
    let end = low;
    result["data"] = json!(&text[request.offset..end]);
    result["next_offset"] = if end < text.len() { json!(end) } else { Value::Null };
    if result.to_string().len() > request.max_bytes || end == request.offset && end < text.len() {
        return Ok(rejected(
            memory,
            "max_bytes cannot fit semantic source metadata; increase the page size",
        ));
    }
    Ok(result)
}
fn content(memory: &LocalMemory, id: u64) -> Result<Value, DeveloperLoopError> {
    let source = memory.archived(id)?;
    if source.kind == ArchiveKind::ToolOutput {
        let value: Value = serde_json::from_slice(&memory.artifact(id)?)
            .map_err(|_| error("decode original tool evidence"))?;
        return Ok(
            if source.call.as_ref().is_some_and(|c| {
                matches!(
                    c.name.as_str(),
                    "context_read" | "context_recover" | "context_sources" | "context_update"
                )
            }) {
                receipt(&value)
            } else {
                value
            },
        );
    }
    if source.kind == ArchiveKind::HostObservation
        && super::super::operation_plan::decode_message(&memory.artifact(id)?)?.is_some()
        && let super::super::operation_plan::PlanRecord::Step { output, .. } =
            super::super::operation_plan::load_record(memory, id)?
    {
        return serde_json::from_slice(&memory.store.read(output)?)
            .map_err(|_| error("decode original plan-step evidence"));
    }
    let message = memory.message(id)?;
    let blocks=message.content().iter().map(|block|match block {
        ContentBlock::Refusal(text)=>Ok(json!({"kind":"refusal","text":text.expose_for_wire()})),
        ContentBlock::Text(text)=>Ok(json!({"kind":"text","text":text.expose_for_wire()})),
        ContentBlock::ToolCall(call)=>Ok(json!({"kind":"historical_tool_proposal","effect_proof":false,"name":call.name().as_str(),"arguments":serde_json::from_slice::<Value>(call.arguments().canonical_bytes()).map_err(|_|error("decode archived proposal"))?})),
        ContentBlock::ToolResult(result)=>{
            let original=memory.sources.iter().rev().find(|s|s.sequence<id && s.invocation==source.invocation && s.call.as_ref().is_some_and(|c|c.id==result.call_id().expose_for_wire()));
            if let Some(original)=original {
                if !source_policy::allowed(memory,original.sequence)? {return Err(error("original tool source is excluded by current read/reference/private policy"));}
                return Ok(json!({"kind":"original_tool_output","is_error":result.is_error(),"source":handle(memory,original.sequence),"content":content(memory,original.sequence)?}));
            }
            Ok(json!({"kind":"historical_tool_result","is_error":result.is_error(),"content":serde_json::from_slice::<Value>(result.output().canonical_bytes()).map_err(|_|error("decode archived result"))?}))
        }
        _=>Ok(json!({"kind":"non_text_content","exact_content_retained":true,"archive_tool":"context_read"})),
    }).collect::<Result<Vec<Value>,DeveloperLoopError>>()?;
    Ok(
        json!({"kind":"historical_message","role":format!("{:?}",message.role()).to_ascii_lowercase(),"content":blocks}),
    )
}
fn receipt(value: &Value) -> Value {
    if value["entries"].is_array() {
        return json!({"kind":"historical_working_state","authority":"none","entries":value["entries"],"base_revision":value["base_revision"]});
    }
    let mut sources = value["sources"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s["handle"].as_str())
        .collect::<Vec<_>>();
    if sources.is_empty()
        && let Some(source) = value["source"].as_str()
    {
        sources.push(source);
    }
    json!({"kind":"retrieval_receipt","task_evidence":false,"original_sources":sources,"note":"This records delivery of older sources. Recover a named original source only if the current unfinished action needs its content; do not recursively recover this receipt."})
}

pub(in crate::local_context) fn cached_allowed(
    memory: &LocalMemory,
    bytes: &[u8],
) -> Result<bool, DeveloperLoopError> {
    let Ok(request) = serde_json::from_slice::<Request>(bytes) else { return Ok(false) };
    let Ok(id) = sequence(memory, &request.source) else { return Ok(false) };
    source_policy::allowed(memory, id)
}
