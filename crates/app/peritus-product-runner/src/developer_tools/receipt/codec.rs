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
        "native_owner".to_owned(),
        record.native_owner.map_or(Value::Null, |owner| {
            Value::Object(Map::from_iter([
                ("source_run_id".to_owned(), Value::String(id_hex(owner.source_run.as_bytes()))),
                ("run_id".to_owned(), Value::String(id_hex(owner.execution_run.as_bytes()))),
                ("action_id".to_owned(), Value::String(id_hex(owner.action.as_bytes()))),
                ("process_id".to_owned(), Value::String(id_hex(owner.process.as_bytes()))),
            ]))
        }),
    );
    fields.insert("owner_inactive".to_owned(), Value::Bool(record.owner_inactive));
    fields.insert(
        "state".to_owned(),
        Value::String(
            match &record.state {
                ReceiptState::Started => "started",
                ReceiptState::Applied => "applied",
                ReceiptState::Completed => "completed",
                ReceiptState::Ambiguous => "ambiguous",
                ReceiptState::Reviewed => "reviewed",
            }
            .to_owned(),
        ),
    );
    if record.state != ReceiptState::Completed
        && let Some(output) = &record.output
    {
        fields.insert("output".to_owned(), output.clone());
    }
    if record.state != ReceiptState::Completed
        && let Some(is_error) = record.is_error
    {
        fields.insert("is_error".to_owned(), Value::Bool(is_error));
    }
    Value::Object(fields)
}

pub(super) fn decode(value: &Value) -> Result<ReceiptRecord, DeveloperLoopError> {
    let fields = value.as_object().ok_or_else(|| tool("effect receipt is not an object"))?;
    let version = u32::try_from(required_u64(fields, "version")?)
        .map_err(|_| tool("effect receipt version is out of range"))?;
    if version != 1 && version != super::FORMAT_VERSION {
        return Err(tool(format!("unsupported effect receipt format version {version}")));
    }
    let ordinal = u32::try_from(required_u64(fields, "ordinal")?)
        .map_err(|_| tool("effect receipt ordinal is out of range"))?;
    let state = match required_text(fields, "state")? {
        "started" => ReceiptState::Started,
        "applied" => ReceiptState::Applied,
        "completed" => ReceiptState::Completed,
        "ambiguous" => ReceiptState::Ambiguous,
        "reviewed" => ReceiptState::Reviewed,
        _ => return Err(tool("effect receipt state is unknown")),
    };
    let native_owner = if version == 1 {
        None
    } else {
        match fields.get("native_owner") {
            Some(Value::Null) => None,
            Some(Value::Object(owner)) => Some(decode_owner(owner)?),
            _ => return Err(tool("effect receipt native_owner field is malformed")),
        }
    };
    let owner_inactive = if version == 1 {
        false
    } else {
        fields
            .get("owner_inactive")
            .and_then(Value::as_bool)
            .ok_or_else(|| tool("effect receipt owner_inactive field is not boolean"))?
    };
    Ok(ReceiptRecord {
        version,
        scope: required_text(fields, "scope")?.to_owned(),
        ordinal,
        call_id: required_text(fields, "call_id")?.to_owned(),
        tool: required_text(fields, "tool")?.to_owned(),
        request_sha256: required_text(fields, "request_sha256")?.to_owned(),
        state,
        native_owner,
        owner_inactive,
        output: fields.get("output").cloned(),
        is_error: fields.get("is_error").and_then(Value::as_bool),
    })
}

fn decode_owner(
    fields: &Map<String, Value>,
) -> Result<super::NativeCommandOwner, DeveloperLoopError> {
    Ok(super::NativeCommandOwner {
        source_run: peritus_types::RunId::new(id_bytes(required_text(fields, "source_run_id")?)?)
            .map_err(|_| tool("effect receipt source run owner is malformed"))?,
        execution_run: peritus_types::RunId::new(id_bytes(required_text(fields, "run_id")?)?)
            .map_err(|_| tool("effect receipt run owner is malformed"))?,
        action: peritus_types::ActionId::new(id_bytes(required_text(fields, "action_id")?)?)
            .map_err(|_| tool("effect receipt action owner is malformed"))?,
        process: peritus_types::ProcessId::new(id_bytes(required_text(fields, "process_id")?)?)
            .map_err(|_| tool("effect receipt process owner is malformed"))?,
    })
}

fn id_hex(bytes: &[u8; 16]) -> String {
    use core::fmt::Write as _;

    let mut output = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn id_bytes(value: &str) -> Result<[u8; 16], DeveloperLoopError> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(tool("effect receipt native owner is not a 16-byte hex ID"));
    }
    let mut output = [0_u8; 16];
    for (index, byte) in output.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(&value[offset..offset + 2], 16)
            .map_err(|_| tool("effect receipt native owner hex is malformed"))?;
    }
    Ok(output)
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
