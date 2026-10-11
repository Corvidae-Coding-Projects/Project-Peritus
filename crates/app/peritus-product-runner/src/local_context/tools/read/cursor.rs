use super::super::{LocalMemory, error, hex};
use super::CursorPosition;
use peritus_agent::DeveloperLoopError;
use serde_json::Value;

pub(super) fn entry_cursor(memory: &LocalMemory, index: usize, offset: u64) -> String {
    format!("{}:{index}:{offset}", cursor_prefix(memory, "entry", None))
}

pub(super) fn search_cursor(memory: &LocalMemory, query: &str, index: usize, offset: u64) -> Value {
    Value::from(format!("{}:{index}:{offset}", cursor_prefix(memory, "search", Some(query))))
}

fn cursor_prefix(memory: &LocalMemory, kind: &str, query: Option<&str>) -> String {
    let scope = hex(memory.store.scope_digest().as_bytes());
    match (kind, query) {
        ("entry", _) => format!("entry:{scope}:{}", memory.model_revision),
        ("search", Some(query)) => {
            use sha2::{Digest as _, Sha256};
            let digest = Sha256::digest(query.as_bytes());
            format!("search:{scope}:{}", hex(&digest))
        }
        _ => format!("search:{scope}"),
    }
}

pub(super) fn cursor(
    memory: &LocalMemory,
    value: Option<&str>,
    kind: &str,
    query: Option<&str>,
) -> Result<CursorPosition, DeveloperLoopError> {
    let Some(value) = value else {
        return Ok(CursorPosition::default());
    };
    let prefix = format!("{}:", cursor_prefix(memory, kind, query));
    let legacy_prefix = format!("{}:", cursor_prefix(memory, kind, None));
    let tail = value
        .strip_prefix(&prefix)
        .or_else(|| (kind == "search").then(|| value.strip_prefix(&legacy_prefix)).flatten())
        .ok_or_else(|| error("cursor scope or kind mismatch"))?;
    let mut parts = tail.split(':');
    let index = parts
        .next()
        .ok_or_else(|| error("invalid cursor"))?
        .parse::<usize>()
        .map_err(|_| error("invalid cursor"))?;
    let offset = parts
        .next()
        .map_or(Ok(0), |part| part.parse::<u64>().map_err(|_| error("invalid cursor")))?;
    if parts.next().is_some() {
        return Err(error("invalid cursor"));
    }
    let bound = if kind == "entry" {
        memory.state.entries(memory.state.binding()).map_err(|_| error("entry cursor scope"))?.len()
    } else {
        memory.sources.len()
    };
    if index > bound {
        return Err(error("cursor out of range"));
    }
    if kind == "entry" && index < bound {
        let entries = memory
            .state
            .entries(memory.state.binding())
            .map_err(|_| error("entry cursor scope"))?;
        if offset > u64::try_from(entries[index].content().bytes().len()).unwrap_or(u64::MAX) {
            return Err(error("entry cursor out of range"));
        }
    }
    Ok(CursorPosition { index, offset })
}
