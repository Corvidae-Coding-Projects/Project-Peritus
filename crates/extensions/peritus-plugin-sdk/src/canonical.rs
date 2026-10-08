//! Deterministic JSON rendering for manifest trust material and plugin payloads.

use std::io::{self, Write as _};

use serde::Serialize;
use serde_json::Value;

use crate::{SdkError, SdkErrorKind};

pub fn bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, SdkError> {
    let value = serde_json::to_value(value).map_err(|error| {
        SdkError::new(SdkErrorKind::InvalidManifest, "serialize canonical value", error.to_string())
    })?;
    value_bytes(&value, usize::MAX)
}

pub fn value_bytes(value: &Value, maximum: usize) -> Result<Vec<u8>, SdkError> {
    let mut output = CanonicalOutput::new(maximum);
    write_value(value, &mut output)?;
    Ok(output.bytes)
}

fn write_value(value: &Value, output: &mut CanonicalOutput) -> Result<(), SdkError> {
    match value {
        Value::Null => output.write_literal(b"null")?,
        Value::Bool(false) => output.write_literal(b"false")?,
        Value::Bool(true) => output.write_literal(b"true")?,
        Value::Number(number) => output.write_literal(number.to_string().as_bytes())?,
        Value::String(text) => write_string(text, output)?,
        Value::Array(values) => {
            output.write_literal(b"[")?;
            for (index, item) in values.iter().enumerate() {
                if index != 0 {
                    output.write_literal(b",")?;
                }
                write_value(item, output)?;
            }
            output.write_literal(b"]")?;
        }
        Value::Object(values) => {
            output.write_literal(b"{")?;
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index != 0 {
                    output.write_literal(b",")?;
                }
                write_string(key, output)?;
                output.write_literal(b":")?;
                write_value(item, output)?;
            }
            output.write_literal(b"}")?;
        }
    }
    Ok(())
}

fn write_string(text: &str, output: &mut CanonicalOutput) -> Result<(), SdkError> {
    let result = serde_json::to_writer(&mut *output, text);
    if output.exceeded {
        return Err(limit());
    }
    result.map_err(|error| {
        SdkError::new(
            SdkErrorKind::InvalidJson,
            "render canonical string",
            error.to_string(),
        )
    })
}

struct CanonicalOutput {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded: bool,
}

impl CanonicalOutput {
    fn new(maximum: usize) -> Self {
        Self { bytes: Vec::new(), maximum, exceeded: false }
    }

    fn write_literal(&mut self, bytes: &[u8]) -> Result<(), SdkError> {
        self.write_all(bytes).map_err(|_| limit())
    }
}

impl io::Write for CanonicalOutput {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let Some(length) = self.bytes.len().checked_add(buffer.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("canonical JSON length overflowed"));
        };
        if length > self.maximum {
            self.exceeded = true;
            return Err(io::Error::other("canonical JSON exceeds its byte capacity"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn limit() -> SdkError {
    SdkError::new(
        SdkErrorKind::LimitExceeded,
        "render canonical value",
        "canonical JSON exceeds its byte capacity",
    )
}
