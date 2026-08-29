//! Durable per-run D0 provider/tool trace.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use peritus_agent::{DeveloperLoopError, DeveloperTrace, DeveloperTraceEvent};
use serde_json::{Map, Value};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Length-framed append-only trace stored beside the daemon's product-run record.
pub struct FileDeveloperTrace {
    path: PathBuf,
}

impl FileDeveloperTrace {
    #[must_use]
    pub const fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

/// Creates the durable trace before the first provider request without truncating prior events.
pub fn prepare(path: &Path) -> Result<(), ProductRunnerError> {
    open(path).map(drop).map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::Repository,
            "prepare durable developer trace",
            error.to_string(),
        )
    })
}

impl DeveloperTrace for FileDeveloperTrace {
    fn record(&mut self, event: DeveloperTraceEvent<'_>) -> Result<(), DeveloperLoopError> {
        let (tag, payload) = match event {
            DeveloperTraceEvent::ProviderEnvelope(bytes) => (1_u8, bytes.to_vec()),
            DeveloperTraceEvent::ToolObservation { call, observation } => {
                let fields = [
                    ("call_id".to_owned(), Value::String(call.id().expose_for_wire().to_owned())),
                    ("name".to_owned(), Value::String(call.name().as_str().to_owned())),
                    ("arguments".to_owned(), Value::String(call.arguments().to_wire_string())),
                    ("output".to_owned(), Value::String(observation.output.to_wire_string())),
                    ("is_error".to_owned(), Value::Bool(observation.is_error)),
                ];
                let payload =
                    serde_json::to_vec(&Value::Object(fields.into_iter().collect::<Map<_, _>>()))
                        .map_err(|error| DeveloperLoopError::Trace(error.to_string()))?;
                (2, payload)
            }
            DeveloperTraceEvent::ContextCompaction(record) => {
                let fields = [
                    (
                        "policy_sha256".to_owned(),
                        Value::String(digest_hex(record.policy_digest().as_bytes())),
                    ),
                    (
                        "source_sha256".to_owned(),
                        Value::String(digest_hex(record.source_digest().as_bytes())),
                    ),
                    (
                        "replacement_sha256".to_owned(),
                        Value::String(digest_hex(record.replacement_digest().as_bytes())),
                    ),
                    (
                        "source_messages".to_owned(),
                        Value::from(u64::from(record.source_messages())),
                    ),
                    ("replaced_tokens".to_owned(), Value::from(record.replaced_tokens())),
                    ("replacement_tokens".to_owned(), Value::from(record.replacement_tokens())),
                ];
                let payload =
                    serde_json::to_vec(&Value::Object(fields.into_iter().collect::<Map<_, _>>()))
                        .map_err(|error| DeveloperLoopError::Trace(error.to_string()))?;
                (3, payload)
            }
            DeveloperTraceEvent::RetryScheduled(record) => {
                let fields = [
                    ("turn".to_owned(), Value::from(u64::from(record.turn()))),
                    ("attempt".to_owned(), Value::from(u64::from(record.attempt()))),
                    ("max_attempts".to_owned(), Value::from(u64::from(record.max_attempts()))),
                    ("elapsed_millis".to_owned(), Value::from(record.elapsed_millis())),
                    ("delay_millis".to_owned(), Value::from(record.delay_millis())),
                    (
                        "retry_after_millis".to_owned(),
                        record.retry_after_millis().map_or(Value::Null, Value::from),
                    ),
                    ("reason".to_owned(), Value::String(record.reason().as_str().to_owned())),
                ];
                let payload =
                    serde_json::to_vec(&Value::Object(fields.into_iter().collect::<Map<_, _>>()))
                        .map_err(|error| DeveloperLoopError::Trace(error.to_string()))?;
                (4, payload)
            }
        };
        let length = u64::try_from(payload.len())
            .map_err(|_| DeveloperLoopError::Trace("trace event is too large".to_owned()))?;
        let mut file = open(&self.path).map_err(|error| trace(&error))?;
        file.write_all(&[tag]).map_err(|error| trace(&error))?;
        file.write_all(&length.to_le_bytes()).map_err(|error| trace(&error))?;
        file.write_all(&payload).map_err(|error| trace(&error))?;
        file.sync_data().map_err(|error| trace(&error))
    }
}

fn open(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

fn digest_hex(bytes: &[u8; 32]) -> String {
    use core::fmt::Write as _;

    let mut output = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn trace(error: &io::Error) -> DeveloperLoopError {
    DeveloperLoopError::Trace(error.to_string())
}
