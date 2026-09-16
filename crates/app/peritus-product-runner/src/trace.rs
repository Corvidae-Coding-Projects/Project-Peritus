//! Durable per-run D0 provider/tool trace.

pub mod accounting;
pub mod local_memory;

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use peritus_agent::{DeveloperLoopError, DeveloperTrace, DeveloperTraceEvent};
use serde_json::{Map, Value};

use crate::failover::ProviderSwitch;
use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Stable record kinds in the product runner's length-framed developer trace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeveloperTraceFrameKind {
    /// One canonical provider event envelope.
    ProviderEnvelope,
    /// One legacy unscoped tool observation.
    ToolObservation,
    /// One context-compaction record.
    ContextCompaction,
    /// One scheduled provider retry.
    RetryScheduled,
    /// One provider failover transition.
    ProviderSwitch,
    /// One role-scoped local-memory checkpoint.
    LocalMemoryCheckpoint,
    /// One role-scoped tool observation retained for local-memory recovery.
    LocalMemoryObservation,
}

impl DeveloperTraceFrameKind {
    /// Decodes a stable trace tag.
    #[must_use]
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::ProviderEnvelope),
            2 => Some(Self::ToolObservation),
            3 => Some(Self::ContextCompaction),
            4 => Some(Self::RetryScheduled),
            5 => Some(Self::ProviderSwitch),
            6 => Some(Self::LocalMemoryCheckpoint),
            7 => Some(Self::LocalMemoryObservation),
            _ => None,
        }
    }

    /// Returns the stable trace tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::ProviderEnvelope => 1,
            Self::ToolObservation => 2,
            Self::ContextCompaction => 3,
            Self::RetryScheduled => 4,
            Self::ProviderSwitch => 5,
            Self::LocalMemoryCheckpoint => 6,
            Self::LocalMemoryObservation => 7,
        }
    }
}

/// Length-framed append-only trace stored beside the daemon's product-run record.
pub struct FileDeveloperTrace {
    path: PathBuf,
    memory_scope: Option<local_memory::Scope>,
}

impl FileDeveloperTrace {
    #[must_use]
    pub const fn new(path: PathBuf) -> Self {
        Self { path, memory_scope: None }
    }

    /// Binds complete tool observations to one durable local-memory invocation (trace tag 7).
    #[must_use]
    pub const fn with_memory_scope(
        mut self,
        scope: peritus_types::Sha256Digest,
        invocation: u64,
    ) -> Self {
        self.memory_scope =
            Some(local_memory::Scope { digest: scope.into_bytes(), invocation, observed: 0 });
        self
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

pub fn record_provider_switch(
    path: &Path,
    role: &str,
    cycle: u32,
    switch: ProviderSwitch,
) -> Result<(), ProductRunnerError> {
    let fields = [
        ("role".to_owned(), Value::String(role.to_owned())),
        ("cycle".to_owned(), Value::from(u64::from(cycle))),
        ("previous_profile".to_owned(), Value::String(profile_hex(switch.previous()))),
        ("next_profile".to_owned(), Value::String(profile_hex(switch.next()))),
        ("reason".to_owned(), Value::String(switch.reason().to_owned())),
    ];
    let payload = serde_json::to_vec(&Value::Object(fields.into_iter().collect::<Map<_, _>>()))
        .map_err(|error| repository(error.to_string()))?;
    append(path, DeveloperTraceFrameKind::ProviderSwitch.tag(), &payload)
        .map_err(|error| repository(error.to_string()))
}

impl DeveloperTrace for FileDeveloperTrace {
    fn record(&mut self, event: DeveloperTraceEvent<'_>) -> Result<(), DeveloperLoopError> {
        let (tag, payload) = match event {
            DeveloperTraceEvent::ProviderEnvelope(bytes) => {
                (DeveloperTraceFrameKind::ProviderEnvelope, bytes.to_vec())
            }
            DeveloperTraceEvent::ToolObservation { call, observation } => {
                if let Some(mut scope) = self.memory_scope {
                    scope.observed = scope.observed.checked_add(1).ok_or_else(|| {
                        DeveloperLoopError::Trace(
                            "local trace observation sequence overflow".to_owned(),
                        )
                    })?;
                    let payload = local_memory::tool_payload(scope, call, observation)?;
                    append(
                        &self.path,
                        DeveloperTraceFrameKind::LocalMemoryObservation.tag(),
                        &payload,
                    )
                    .map_err(|error| trace(&error))?;
                    self.memory_scope = Some(scope);
                    return Ok(());
                }
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
                (DeveloperTraceFrameKind::ToolObservation, payload)
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
                (DeveloperTraceFrameKind::ContextCompaction, payload)
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
                (DeveloperTraceFrameKind::RetryScheduled, payload)
            }
        };
        append(&self.path, tag.tag(), &payload).map_err(|error| trace(&error))
    }
}

fn append(path: &Path, tag: u8, payload: &[u8]) -> io::Result<()> {
    let length =
        u64::try_from(payload.len()).map_err(|_| io::Error::other("trace event is too large"))?;
    let mut file = open(path)?;
    file.write_all(&[tag])?;
    file.write_all(&length.to_le_bytes())?;
    file.write_all(payload)?;
    file.sync_data()
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

fn profile_hex(id: peritus_types::ProviderProfileId) -> String {
    id.as_bytes().iter().fold(String::new(), |mut output, byte| {
        use core::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
        output
    })
}

fn repository(detail: String) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Repository, "record provider failover", detail)
}

fn trace(error: &io::Error) -> DeveloperLoopError {
    DeveloperLoopError::Trace(error.to_string())
}
