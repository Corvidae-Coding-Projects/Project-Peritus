//! Exact read-only pages from bundled skill resources, with no filesystem lookup or grounding.

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::{executor::reviewer_evidence_page, path::tool, wire::required_string};

pub(super) fn read(arguments: &Value) -> Result<Value, DeveloperLoopError> {
    if required_string(arguments, "skill")? != "rust-reliability" {
        return Err(tool("unknown bundled skill"));
    }
    let path = required_string(arguments, "path")?;
    let source = crate::engineering_workflow::rust_reliability::resource(path)
        .ok_or_else(|| tool("unknown bundled skill resource; use its exact relative path"))?;
    let offset = arguments.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let start = usize::try_from(offset).map_err(|_| tool("offset exceeds resource length"))?;
    if start > source.len() || !source.is_char_boundary(start) {
        return Err(tool("offset must be a UTF-8 boundary within the resource"));
    }
    let maximum = arguments
        .get("max_bytes")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(super::DEFAULT_INSPECTION_PAGE_BYTES);
    reviewer_evidence_page(path, source, offset, start, maximum)
}
