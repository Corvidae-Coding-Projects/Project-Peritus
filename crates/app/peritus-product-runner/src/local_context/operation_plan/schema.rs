//! Additive opt-in model tools; base tool schemas and authority remain unchanged.

use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::ToolDefinition;
use serde_json::json;

pub(in crate::local_context) fn definitions() -> Result<Vec<ToolDefinition>, DeveloperLoopError> {
    let reference = json!({"type":"object","additionalProperties":false,
        "required":["argument","node","pointer"],"properties":{
            "argument":{"type":"string","minLength":1,"maxLength":64},
            "node":{"type":"string","minLength":1,"maxLength":64},
            "pointer":{"type":"string","maxLength":4096}}});
    let node = json!({"type":"object","additionalProperties":false,
        "required":["id","tool","arguments","depends_on","result_refs"],"properties":{
            "id":{"type":"string","minLength":1,"maxLength":64},
            "tool":{"type":"string","enum":["workspace_list","workspace_search","workspace_read"]},
            "arguments":{"type":"object","description":"Literal ordinary tool arguments only. Omit every field supplied by result_refs entirely; do not include empty-string or null placeholders for referenced arguments."},
            "depends_on":{"type":"array","maxItems":64,"items":{"type":"string","maxLength":64}},
            "result_refs":{"type":"array","maxItems":32,"items":reference}}});
    let schema = json!({"type":"object","additionalProperties":false,
        "required":["version","scope","input_revision","plan","cursor","max_result_bytes"],
        "properties":{
            "version":{"type":"integer","enum":[1]},
            "scope":{"type":"string","minLength":64,"maxLength":64},
            "input_revision":{"type":"integer","minimum":0},
            "plan":{"type":["array","null"],"minItems":1,"maxItems":64,"items":node},
            "cursor":{"type":["string","null"],"maxLength":100},
            "max_result_bytes":{"type":"integer","minimum":2048,"maximum":65536}}});
    Ok(vec![
        crate::local_context::tools::definition_with_strict(
            "inspection_plan",
            "Execute a version-1 read-only plan sequentially under current host admission. Use scope and input_revision from current host metadata. Supply exactly one of plan or cursor, setting the other null. Nodes must be in dependency order; result_refs copy a JSON Pointer from an earlier node's actual output into a named ordinary tool argument. Omit referenced arguments entirely from arguments, including empty or null placeholders: for a referenced path, arguments may contain start_line/end_line but no path key. List every referenced node in depends_on. Each child gets fresh ordinary permission/path checks and a durable exact result; errors or control changes stop further dispatch. No commands, mutations or arbitrary code. max_result_bytes pages this operation, never the task; follow continuation with a new currently bound call. Exact omitted child bytes use inspection_result_read. Do not repeat a settled effect.",
            &schema.to_string(),
            // Child argument maps vary by tool and omit fields supplied by result references.
            // Remote strict decoding cannot close this object without losing valid plans.
            // The host still validates the whole plan and each resolved ordinary tool call.
            false,
        )?,
        read_definition()?,
    ])
}

pub(in crate::local_context) fn read_definition() -> Result<ToolDefinition, DeveloperLoopError> {
    crate::local_context::tools::definition(
        "inspection_result_read",
        "Read exact bytes from one durable inspection-plan child result. step is its fully scoped observation handle; offset and next_offset address the original UTF-8 JSON bytes. Current file/permission checks still apply. This is retrieval, not execution or evidence of a current candidate.",
        r#"{"type":"object","additionalProperties":false,"required":["step","offset","max_bytes"],"properties":{"step":{"type":"string","maxLength":100},"offset":{"type":"integer","minimum":0},"max_bytes":{"type":"integer","minimum":256,"maximum":65536}}}"#,
    )
}
