//! Explicit effect-receipt JSON boundary without derive macros.

use peritus_agent::DeveloperLoopError;
use serde_json::{Map, Value};

use super::{ReceiptRecord, ReceiptState};
use crate::developer_tools::path::tool;

pub(super) fn encode(record: &ReceiptRecord) -> Value {
    let mut fields = Map::new();
    fields.insert("version".to_owned(), Value::from(record.version));
    fields.insert("scope".to_owned(), Value::String(record.scope.clone()));
    fields.insert("ordinal".to_owned(), Value::from(record.ordinal));
    fields.insert("call_id".to_owned(), Value::String(record.call_id.clone()));
    fields.insert("tool".to_owned(), Value::String(record.tool.clone()));
    fields.insert("request_sha256".to_owned(), Value::String(record.request_sha256.clone()));
    fields.insert(
        "state".to_owned(),
        Value::String(
            match &record.state {
                ReceiptState::Started => "started",
                ReceiptState::Completed => "completed",
                ReceiptState::Ambiguous => "ambiguous",
            }
            .to_owned(),
        ),
    );
    if let Some(output) = &record.output {
        fields.insert("output".to_owned(), output.clone());
    }
    if let Some(is_error) = record.is_error {
        fields.insert("is_error".to_owned(), Value::Bool(is_error));
    }
    Value::Object(fields)
}

pub(super) fn decode(value: &Value) -> Result<ReceiptRecord, DeveloperLoopError> {
    let fields = value.as_object().ok_or_else(|| tool("effect receipt is not an object"))?;
    let version = u32::try_from(required_u64(fields, "version")?)
        .map_err(|_| tool("effect receipt version is out of range"))?;
    let ordinal = u32::try_from(required_u64(fields, "ordinal")?)
        .map_err(|_| tool("effect receipt ordinal is out of range"))?;
    let state = match required_text(fields, "state")? {
        "started" => ReceiptState::Started,
        "completed" => ReceiptState::Completed,
        "ambiguous" => ReceiptState::Ambiguous,
        _ => return Err(tool("effect receipt state is unknown")),
    };
    Ok(ReceiptRecord {
        version,
        scope: required_text(fields, "scope")?.to_owned(),
        ordinal,
        call_id: required_text(fields, "call_id")?.to_owned(),
        tool: required_text(fields, "tool")?.to_owned(),
        request_sha256: required_text(fields, "request_sha256")?.to_owned(),
        state,
        output: fields.get("output").cloned(),
        is_error: fields.get("is_error").and_then(Value::as_bool),
    })
}

fn required_text<'a>(
    fields: &'a Map<String, Value>,
    name: &str,
) -> Result<&'a str, DeveloperLoopError> {
    fields
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| tool(format!("effect receipt field {name} is not text")))
}

fn required_u64(fields: &Map<String, Value>, name: &str) -> Result<u64, DeveloperLoopError> {
    fields
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| tool(format!("effect receipt field {name} is not an unsigned integer")))
}
