//! Numeric metadata from the official runtime's fresh model cache; never credentials.
use super::{DiscoveredModel, unavailable};
use crate::ProviderCoreError;
use serde_json::Value;
use std::{
    fs::File,
    io::Read as _,
    path::{Path, PathBuf},
    time::Duration,
};
const MAX_BYTES: u64 = 16 * 1024 * 1024;

pub(super) fn enrich(
    models: Vec<DiscoveredModel>,
) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
    let root = std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(|p| PathBuf::from(p).join(".codex"))
    });
    let Some(root) = root else { return Ok(models) };
    enrich_file(models, &root.join("models_cache.json"))
}

fn enrich_file(
    mut models: Vec<DiscoveredModel>,
    path: &Path,
) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(models),
        Err(_) => return Err(unavailable("official runtime model metadata cannot be read")),
    };
    let metadata = file
        .metadata()
        .map_err(|_| unavailable("official runtime model metadata cannot be inspected"))?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err(unavailable(
            "official runtime model metadata exceeds its representation bound",
        ));
    }
    if metadata
        .modified()
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_none_or(|age| age >= Duration::from_mins(5))
    {
        return Ok(models);
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable("official runtime model metadata cannot be read"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(unavailable("official runtime model metadata grew beyond its bound"));
    }
    let cache: Value = serde_json::from_slice(&bytes)
        .map_err(|_| unavailable("official runtime model metadata is malformed"))?;
    let rows = cache["models"]
        .as_array()
        .ok_or_else(|| unavailable("official runtime model metadata has no models"))?;
    for model in &mut models {
        let mut matched = rows.iter().filter(|row| row["slug"].as_str() == Some(model.id.as_str()));
        let Some(row) = matched.next() else { continue };
        if matched.next().is_some() {
            return Err(unavailable(
                "official runtime model metadata has duplicate model identities",
            ));
        }
        // Only enrich models advertised by this exact account-runtime query. A cache
        // entry never adds a model or changes route, selection, capabilities or effort.
        model.context_tokens = number(row, "context_window")?.or(model.context_tokens);
        model.max_context_tokens = number(row, "max_context_window")?;
        if let Some(percent) = number(row, "effective_context_window_percent")? {
            model.effective_context_percent =
                Some(u8::try_from(percent).ok().filter(|p| (1..=100).contains(p)).ok_or_else(
                    || unavailable("official runtime usable context percentage is invalid"),
                )?);
        }
        if let (Some(default), Some(maximum)) = (model.context_tokens, model.max_context_tokens)
            && maximum < default
        {
            return Err(unavailable("official runtime maximum context contradicts its default"));
        }
        model.usable_context_tokens()?;
    }
    Ok(models)
}
fn number(row: &Value, key: &str) -> Result<Option<u64>, ProviderCoreError> {
    match row.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_u64().filter(|v| *v > 0).map(Some).ok_or_else(|| {
            unavailable("official runtime context metadata contains an invalid number")
        }),
    }
}

#[cfg(test)]
#[path = "../../../../../discovery/proptest-praxis/owner/provider_capacity.rs"]
mod replay;
