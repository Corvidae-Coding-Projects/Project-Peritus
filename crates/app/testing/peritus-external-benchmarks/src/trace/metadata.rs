//! Validation for trace metadata that does not alter projected conversation history.

use std::{collections::BTreeSet, path::Path};

use serde_json::{Map, Value};

use crate::BenchmarkError;
use peritus_product_runner::DeveloperTraceFrameKind;

pub(super) fn validate(
    path: &Path,
    kind: DeveloperTraceFrameKind,
    payload: &[u8],
) -> Result<(), BenchmarkError> {
    let value: Value = serde_json::from_slice(payload)
        .map_err(|error| BenchmarkError::trace(path, error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| BenchmarkError::trace(path, "trace metadata is not an object"))?;
    match kind {
        DeveloperTraceFrameKind::ContextCompaction => validate_compaction(path, object),
        DeveloperTraceFrameKind::RetryScheduled => validate_retry(path, object),
        DeveloperTraceFrameKind::ProviderSwitch => validate_provider_switch(path, object),
        DeveloperTraceFrameKind::LocalMemoryCheckpoint => {
            validate_local_memory_checkpoint(path, object)
        }
        DeveloperTraceFrameKind::LocalMemoryObservation => {
            validate_local_memory_observation(path, object)
        }
        _ => Err(BenchmarkError::trace(path, "trace metadata tag was not validated")),
    }
}

fn validate_local_memory_checkpoint(
    path: &Path,
    object: &Map<String, Value>,
) -> Result<(), BenchmarkError> {
    exact_keys(
        path,
        object,
        &[
            "schema_version",
            "scope",
            "generation",
            "manifest_sha256",
            "manifest_bytes",
            "view_sha256",
            "state_revision",
            "estimated_input_tokens",
            "validation",
        ],
    )?;
    for field in [
        "schema_version",
        "generation",
        "manifest_bytes",
        "state_revision",
        "estimated_input_tokens",
    ] {
        require_u64(path, object, field)?;
    }
    for field in ["scope", "manifest_sha256", "view_sha256"] {
        require_byte_array(path, object, field, 32)?;
    }
    if !object.get("validation").is_some_and(Value::is_object) {
        return Err(invalid(path, "validation"));
    }
    Ok(())
}

fn validate_local_memory_observation(
    path: &Path,
    object: &Map<String, Value>,
) -> Result<(), BenchmarkError> {
    exact_keys(
        path,
        object,
        &[
            "schema_version",
            "scope",
            "invocation",
            "tool_sequence",
            "call_id",
            "name",
            "arguments",
            "output",
            "is_error",
        ],
    )?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(invalid(path, "schema_version"));
    }
    require_byte_array(path, object, "scope", 32)?;
    for field in ["invocation", "tool_sequence"] {
        require_positive_u64(path, object, field)?;
    }
    for field in ["call_id", "name", "arguments", "output"] {
        require_text(path, object, field, 32 * 1024 * 1024)?;
    }
    if !object.get("is_error").is_some_and(Value::is_boolean) {
        return Err(invalid(path, "is_error"));
    }
    Ok(())
}

fn validate_compaction(path: &Path, object: &Map<String, Value>) -> Result<(), BenchmarkError> {
    exact_keys(
        path,
        object,
        &[
            "policy_sha256",
            "source_sha256",
            "replacement_sha256",
            "source_messages",
            "replaced_tokens",
            "replacement_tokens",
        ],
    )?;
    for field in ["policy_sha256", "source_sha256", "replacement_sha256"] {
        require_hex(path, object, field, 64)?;
    }
    for field in ["source_messages", "replaced_tokens", "replacement_tokens"] {
        require_u64(path, object, field)?;
    }
    Ok(())
}

fn validate_retry(path: &Path, object: &Map<String, Value>) -> Result<(), BenchmarkError> {
    exact_keys(
        path,
        object,
        &[
            "turn",
            "attempt",
            "max_attempts",
            "elapsed_millis",
            "delay_millis",
            "retry_after_millis",
            "reason",
        ],
    )?;
    for field in ["turn", "attempt", "max_attempts", "elapsed_millis", "delay_millis"] {
        require_u64(path, object, field)?;
    }
    if !object.get("retry_after_millis").is_some_and(|value| value.is_null() || value.is_u64()) {
        return Err(invalid(path, "retry_after_millis"));
    }
    require_text(path, object, "reason", 64)
}

fn validate_provider_switch(
    path: &Path,
    object: &Map<String, Value>,
) -> Result<(), BenchmarkError> {
    exact_keys(path, object, &["role", "cycle", "previous_profile", "next_profile", "reason"])?;
    require_text(path, object, "role", 64)?;
    require_u64(path, object, "cycle")?;
    require_hex(path, object, "previous_profile", 32)?;
    require_hex(path, object, "next_profile", 32)?;
    require_text(path, object, "reason", 64)
}

fn exact_keys(
    path: &Path,
    object: &Map<String, Value>,
    expected: &[&str],
) -> Result<(), BenchmarkError> {
    let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(BenchmarkError::trace(path, "trace metadata fields do not match its tag"));
    }
    Ok(())
}

fn require_u64(
    path: &Path,
    object: &Map<String, Value>,
    field: &'static str,
) -> Result<(), BenchmarkError> {
    if !object.get(field).is_some_and(Value::is_u64) {
        return Err(invalid(path, field));
    }
    Ok(())
}

fn require_positive_u64(
    path: &Path,
    object: &Map<String, Value>,
    field: &'static str,
) -> Result<(), BenchmarkError> {
    if object.get(field).and_then(Value::as_u64).is_none_or(|value| value == 0) {
        return Err(invalid(path, field));
    }
    Ok(())
}

fn require_text(
    path: &Path,
    object: &Map<String, Value>,
    field: &'static str,
    maximum: usize,
) -> Result<(), BenchmarkError> {
    let valid = object
        .get(field)
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty() && value.len() <= maximum);
    if !valid {
        return Err(invalid(path, field));
    }
    Ok(())
}

fn require_byte_array(
    path: &Path,
    object: &Map<String, Value>,
    field: &'static str,
    length: usize,
) -> Result<(), BenchmarkError> {
    let valid = object.get(field).and_then(Value::as_array).is_some_and(|values| {
        values.len() == length
            && values
                .iter()
                .all(|value| value.as_u64().is_some_and(|byte| u8::try_from(byte).is_ok()))
    });
    if !valid {
        return Err(invalid(path, field));
    }
    Ok(())
}

fn require_hex(
    path: &Path,
    object: &Map<String, Value>,
    field: &'static str,
    length: usize,
) -> Result<(), BenchmarkError> {
    let valid = object.get(field).and_then(Value::as_str).is_some_and(|value| {
        value.len() == length
            && value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    });
    if !valid {
        return Err(invalid(path, field));
    }
    Ok(())
}

fn invalid(path: &Path, field: &'static str) -> BenchmarkError {
    BenchmarkError::trace(path, format!("trace metadata field {field} is invalid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_switch_requires_exact_bounded_evidence() {
        let valid = br#"{"role":"writer","cycle":1,"previous_profile":"11111111111111111111111111111111","next_profile":"22222222222222222222222222222222","reason":"rate_limited"}"#;
        validate(Path::new("trace"), DeveloperTraceFrameKind::ProviderSwitch, valid)
            .expect("provider switch metadata");

        let missing_reason = br#"{"role":"writer","cycle":1,"previous_profile":"11111111111111111111111111111111","next_profile":"22222222222222222222222222222222"}"#;
        assert!(
            validate(Path::new("trace"), DeveloperTraceFrameKind::ProviderSwitch, missing_reason,)
                .is_err()
        );
    }
}
