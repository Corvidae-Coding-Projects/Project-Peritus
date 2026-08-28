//! Durable per-run D0 provider/tool trace.

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::PathBuf,
};

use peritus_agent::{DeveloperLoopError, DeveloperTrace, DeveloperTraceEvent};
use serde_json::{Map, Value};

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

impl DeveloperTrace for FileDeveloperTrace {
    fn record(&mut self, event: DeveloperTraceEvent<'_>) -> Result<(), DeveloperLoopError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| trace(&error))?;
        }
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
        };
        let length = u64::try_from(payload.len())
            .map_err(|_| DeveloperLoopError::Trace("trace event is too large".to_owned()))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| trace(&error))?;
        file.write_all(&[tag]).map_err(|error| trace(&error))?;
        file.write_all(&length.to_le_bytes()).map_err(|error| trace(&error))?;
        file.write_all(&payload).map_err(|error| trace(&error))?;
        file.sync_data().map_err(|error| trace(&error))
    }
}

fn trace(error: &std::io::Error) -> DeveloperLoopError {
    DeveloperLoopError::Trace(error.to_string())
}
