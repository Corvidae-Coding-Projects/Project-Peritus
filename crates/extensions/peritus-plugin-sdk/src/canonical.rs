//! Deterministic JSON rendering for manifest trust material.

use serde::Serialize;
use serde_json::Value;

use crate::{SdkError, SdkErrorKind};

pub fn bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, SdkError> {
    let value = serde_json::to_value(value).map_err(|error| {
        SdkError::new(SdkErrorKind::InvalidManifest, "serialize canonical value", error.to_string())
    })?;
    let mut output = Vec::new();
    write_value(&value, &mut output)?;
    Ok(output)
}

fn write_value(value: &Value, output: &mut Vec<u8>) -> Result<(), SdkError> {
    match value {
        Value::Null => output.extend_from_slice(b"null"),
        Value::Bool(false) => output.extend_from_slice(b"false"),
        Value::Bool(true) => output.extend_from_slice(b"true"),
        Value::Number(number) if number.is_i64() || number.is_u64() => {
            output.extend_from_slice(number.to_string().as_bytes());
        }
        Value::Number(_) => {
            return Err(SdkError::new(
                SdkErrorKind::InvalidJson,
                "render canonical value",
                "floating-point values are not canonical plugin data",
            ));
        }
        Value::String(text) => output.extend_from_slice(
            serde_json::to_string(text)
                .map_err(|error| {
                    SdkError::new(
                        SdkErrorKind::InvalidJson,
                        "render canonical string",
                        error.to_string(),
                    )
                })?
                .as_bytes(),
        ),
        Value::Array(values) => {
            output.push(b'[');
            for (index, item) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_value(item, output)?;
            }
            output.push(b']');
        }
        Value::Object(values) => {
            output.push(b'{');
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                output.extend_from_slice(
                    serde_json::to_string(key)
                        .map_err(|error| {
                            SdkError::new(
                                SdkErrorKind::InvalidJson,
                                "render canonical object key",
                                error.to_string(),
                            )
                        })?
                        .as_bytes(),
                );
                output.push(b':');
                write_value(item, output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}
