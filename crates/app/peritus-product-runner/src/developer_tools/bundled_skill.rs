//! Exact read-only pages from bundled skill resources, with no filesystem lookup or grounding.

use peritus_agent::DeveloperLoopError;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::{
    executor::{reviewer_evidence_page, reviewer_evidence_page_with_metadata},
    path::tool,
    wire::required_string,
};

pub fn read(arguments: &Value) -> Result<Value, DeveloperLoopError> {
    let skill = required_string(arguments, "skill")?;
    let path = required_string(arguments, "path")?;
    let bytes = crate::bundled_skills::resource(skill, path).ok_or_else(|| {
        tool("unknown bundled skill resource; use skill_list for exact names and paths")
    })?;
    let (offset, start, maximum) = page_arguments(arguments)?;
    if let Ok(source) = std::str::from_utf8(bytes) {
        if start > source.len() || !source.is_char_boundary(start) {
            return Err(tool("offset must be a UTF-8 boundary within the resource"));
        }
        return reviewer_evidence_page_with_metadata(
            path,
            source,
            offset,
            start,
            maximum,
            &[("skill", Value::String(skill.to_owned()))],
        );
    }
    binary_page(path, bytes, start, maximum)
}

pub fn list(arguments: &Value) -> Result<Value, DeveloperLoopError> {
    let selected = arguments.get("skill").and_then(Value::as_str);
    let catalog = crate::bundled_skills::catalog();
    let value = if let Some(name) = selected {
        let skill = catalog.iter().find(|skill| skill.name() == name).ok_or_else(|| {
            tool("unknown bundled skill; use skill_list without skill to discover names")
        })?;
        Value::Array(skill.resources().iter().map(|resource| serde_json::json!({
            "path": resource.path(),
            "total_bytes": resource.bytes().len(),
            "encoding": if std::str::from_utf8(resource.bytes()).is_ok() { "utf-8" } else { "hex" }
        })).collect())
    } else {
        Value::Array(
            catalog
                .iter()
                .map(|skill| {
                    serde_json::json!({
                        "skill": skill.name(), "description": skill.description()
                    })
                })
                .collect(),
        )
    };
    let source = serde_json::to_string(&value).map_err(|error| tool(error.to_string()))?;
    let (offset, start, maximum) = page_arguments(arguments)?;
    if start > source.len() || !source.is_char_boundary(start) {
        return Err(tool("offset must be a UTF-8 boundary within the catalog"));
    }
    reviewer_evidence_page(selected.unwrap_or("bundled_skills"), &source, offset, start, maximum)
}

fn page_arguments(arguments: &Value) -> Result<(u64, usize, usize), DeveloperLoopError> {
    let offset = arguments.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let start = usize::try_from(offset).map_err(|_| tool("offset exceeds resource length"))?;
    let maximum = arguments
        .get("max_bytes")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(super::DEFAULT_INSPECTION_PAGE_BYTES);
    Ok((offset, start, maximum))
}

fn binary_page(
    path: &str,
    bytes: &[u8],
    start: usize,
    maximum: usize,
) -> Result<Value, DeveloperLoopError> {
    if start > bytes.len() {
        return Err(tool("offset exceeds resource length"));
    }
    let sha256 = hex(&Sha256::digest(bytes));
    let page = |end: usize| {
        let encoded = hex(&bytes[start..end]);
        serde_json::json!({
            "bytes": encoded, "encoding": "hex", "offset": start, "section": path,
            "next_offset": if end < bytes.len() { Some(end) } else { None },
            "sha256": sha256, "total_bytes": bytes.len()
        })
    };
    let fits = |end| super::inspection::encoded_len(&page(end)).map(|length| length <= maximum);
    let mut low = start;
    let mut high = start.saturating_add(maximum / 2).min(bytes.len());
    if high == bytes.len() && fits(high)? {
        return Ok(page(high));
    }
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if fits(middle)? {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    if !fits(low)? || (low == start && start < bytes.len()) {
        return Err(tool("max_bytes is too small for the resource metadata and next byte"));
    }
    Ok(page(low))
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [char::from(HEX[usize::from(byte >> 4)]), char::from(HEX[usize::from(byte & 15)])]
        })
        .collect()
}

/// Archived guidance is reused only for the currently bundled resource version.
pub fn is_current(value: &Value) -> bool {
    let (Some(skill), Some(path), Some(digest)) =
        (value["skill"].as_str(), value["section"].as_str(), value["sha256"].as_str())
    else {
        return false;
    };
    crate::bundled_skills::resource(skill, path)
        .is_some_and(|bytes| hex(&Sha256::digest(bytes)) == digest)
}
