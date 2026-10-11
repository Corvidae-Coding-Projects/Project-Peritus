//! Read-only pages from an exact, daemon-authenticated immutable attachment selection.

use peritus_agent::DeveloperLoopError;
use peritus_types::Sha256Digest;
use serde_json::Value;

use crate::{AttachmentReadRequest, ConversationView, control::OperationId};

use super::{path::tool, wire::object};

pub(super) fn read(
    view: Option<&dyn ConversationView>,
    arguments: &Value,
) -> Result<Value, DeveloperLoopError> {
    let view = view.ok_or_else(|| tool("immutable attachment reads are unavailable"))?;
    let attachment = operation(arguments, "attachment")?;
    let version = operation(arguments, "version")?;
    let source_digest = digest(arguments, "source_sha256")?;
    let selected_digest = digest(arguments, "selected_sha256")?;
    let source_bytes = unsigned(arguments, "source_bytes")?;
    let range = (unsigned(arguments, "range_start")?, unsigned(arguments, "range_end")?);
    let offset = unsigned(arguments, "offset")?;
    let max_bytes = u32::try_from(unsigned(arguments, "max_bytes")?)
        .map_err(|_| tool("max_bytes is outside the supported range"))?;
    let request = AttachmentReadRequest::new(
        attachment,
        version,
        source_digest,
        selected_digest,
        source_bytes,
        range,
        offset,
        max_bytes,
    )
    .map_err(tool)?;
    let response = view.read_attachment_range(request).map_err(tool)?;
    Ok(object(vec![
        ("source_sha256", Value::String(hex(response.source_digest().as_bytes()))),
        ("selected_sha256", Value::String(hex(response.selected_digest().as_bytes()))),
        ("source_bytes", Value::from(response.source_bytes())),
        ("range_start", Value::from(response.range().0)),
        ("range_end", Value::from(response.range().1)),
        ("offset", Value::from(response.offset())),
        ("next_offset", response.next_offset().map_or(Value::Null, Value::from)),
        ("text", Value::String(response.text().to_owned())),
    ]))
}

fn operation(arguments: &Value, name: &str) -> Result<OperationId, DeveloperLoopError> {
    let bytes = decode_hex::<16>(arguments, name)?;
    OperationId::new(bytes).map_err(|_| tool(format!("{name} is not a valid operation identity")))
}

fn digest(arguments: &Value, name: &str) -> Result<Sha256Digest, DeveloperLoopError> {
    decode_hex::<32>(arguments, name).map(Sha256Digest::new)
}

fn decode_hex<const N: usize>(
    arguments: &Value,
    name: &str,
) -> Result<[u8; N], DeveloperLoopError> {
    let text = arguments
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| tool(format!("{name} must be hexadecimal text")))?;
    if text.len() != N * 2 || !text.is_ascii() {
        return Err(tool(format!("{name} has an invalid hexadecimal length")));
    }
    let mut bytes = [0_u8; N];
    for (index, chunk) in text.as_bytes().chunks_exact(2).enumerate() {
        let hi = hex_digit(chunk[0]).ok_or_else(|| tool(format!("{name} is not hexadecimal")))?;
        let lo = hex_digit(chunk[1]).ok_or_else(|| tool(format!("{name} is not hexadecimal")))?;
        bytes[index] = (hi << 4) | lo;
    }
    Ok(bytes)
}

const fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn unsigned(arguments: &Value, name: &str) -> Result<u64, DeveloperLoopError> {
    arguments
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| tool(format!("{name} must be a nonnegative integer")))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
