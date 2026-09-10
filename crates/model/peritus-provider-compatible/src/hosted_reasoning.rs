//! Closed hosted reasoning fields preserved for exact subsequent tool-result replay.

use crate::error;
use peritus_provider_core::{ProviderCoreError, hosted::HostedService};
use serde_json::{Map, Value};

pub fn accepts(service: HostedService, field: &str) -> bool {
    match service {
        HostedService::OpenRouter => matches!(field, "reasoning" | "reasoning_details"),
        HostedService::Groq => field == "reasoning",
        HostedService::Together => matches!(field, "reasoning" | "reasoning_content"),
        HostedService::OpenCodeZen
        | HostedService::OpenCodeGo
        | HostedService::Fireworks
        | HostedService::DeepSeek => field == "reasoning_content",
    }
}

pub fn append(
    fields: &mut Map<String, Value>,
    name: &str,
    value: &Value,
    maximum: usize,
) -> Result<(), ProviderCoreError> {
    if value.is_null() {
        return Ok(());
    }
    if name == "reasoning_details" {
        let incoming = value
            .as_array()
            .ok_or_else(|| error::malformed("reasoning details are not an array"))?;
        let stored = fields
            .entry(name)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| error::malformed("reasoning detail storage changed type"))?;
        for detail in incoming {
            let object = detail
                .as_object()
                .ok_or_else(|| error::malformed("reasoning detail is not an object"))?;
            if object.keys().any(|name| {
                !matches!(
                    name.as_str(),
                    "type" | "id" | "format" | "index" | "text" | "summary" | "data" | "signature"
                )
            }) {
                return Err(error::malformed("reasoning detail contains an unmapped field"));
            }
            let prior = stored.iter_mut().find(|entry| {
                object
                    .get("index")
                    .filter(|v| v.is_u64())
                    .is_some_and(|index| entry.get("index") == Some(index))
                    || object
                        .get("id")
                        .filter(|v| v.is_string())
                        .is_some_and(|id| entry.get("id") == Some(id))
            });
            if let Some(prior) = prior {
                let prior = prior
                    .as_object_mut()
                    .ok_or_else(|| error::malformed("reasoning detail storage changed type"))?;
                for (key, value) in object {
                    if value.is_null() {
                        continue;
                    }
                    if matches!(key.as_str(), "text" | "summary" | "data" | "signature") {
                        append_text(prior, key, value)?;
                    } else if prior.get(key).is_some_and(|old| !old.is_null() && old != value) {
                        return Err(error::malformed("reasoning detail identity changed"));
                    } else {
                        prior.insert(key.clone(), value.clone());
                    }
                }
            } else {
                stored.push(detail.clone());
            }
        }
    } else {
        append_text(fields, name, value)?;
    }
    let bytes = serde_json::to_vec(fields)
        .map_err(|_| error::malformed("reasoning state serialization failed"))?;
    if bytes.len() > maximum {
        return Err(error::limit("reasoning state exceeded its aggregate bound"));
    }
    Ok(())
}

fn append_text(
    fields: &mut Map<String, Value>,
    name: &str,
    value: &Value,
) -> Result<(), ProviderCoreError> {
    let text = value.as_str().ok_or_else(|| error::malformed("reasoning delta is not a string"))?;
    match fields.entry(name) {
        serde_json::map::Entry::Vacant(entry) => {
            entry.insert(Value::String(text.to_owned()));
        }
        serde_json::map::Entry::Occupied(mut entry) => {
            if entry.get().is_null() {
                entry.insert(Value::String(text.to_owned()));
            } else if let Value::String(stored) = entry.get_mut() {
                stored.push_str(text);
            } else {
                return Err(error::malformed("reasoning delta changed type"));
            }
        }
    }
    Ok(())
}
