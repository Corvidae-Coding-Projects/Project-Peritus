//! Durable per-run D0 provider/tool trace.

pub mod accounting;
pub mod local_memory;

use std::{
    fmt,
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

/// One decoded record of a durable developer trace, in append order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceFrame {
    kind: DeveloperTraceFrameKind,
    payload: Vec<u8>,
}

impl TraceFrame {
    /// Stable record kind of this frame.
    #[must_use]
    pub const fn kind(&self) -> DeveloperTraceFrameKind {
        self.kind
    }

    /// Exact recorded payload bytes, unmodified.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// Why one durable developer trace could not be read.
#[derive(Debug)]
pub enum TraceReadError {
    /// The trace file could not be read from disk.
    Read(io::Error),
    /// The trace framing is corrupt: an unknown record tag, or a length that cannot be satisfied.
    Framing(String),
}

impl fmt::Display for TraceReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "{error}"),
            Self::Framing(detail) => formatter.write_str(detail),
        }
    }
}

impl std::error::Error for TraceReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            Self::Framing(_) => None,
        }
    }
}

/// Reads every complete record of one durable developer trace without modifying it.
///
/// A trailing incomplete record is ignored: the writer appends one synchronized frame at a time,
/// so an unterminated tail can only be a record a running daemon has not finished writing. An
/// unknown record tag is reported instead of silently discarded.
///
/// # Errors
///
/// Returns [`TraceReadError::Read`] when the trace cannot be read from disk and
/// [`TraceReadError::Framing`] when a record tag or declared length is not usable.
pub fn read_frames(path: &Path) -> Result<Vec<TraceFrame>, TraceReadError> {
    let bytes = fs::read(path).map_err(TraceReadError::Read)?;
    let mut frames = Vec::new();
    let mut offset = 0_usize;
    while let Some(header) = bytes.get(offset..offset.saturating_add(9)) {
        let tag = DeveloperTraceFrameKind::from_tag(header[0]).ok_or_else(|| {
            TraceReadError::Framing(format!("unknown record tag {} at byte {offset}", header[0]))
        })?;
        // A nine-byte header always yields an eight-byte length; a shorter slice cannot occur.
        let declared = <[u8; 8]>::try_from(&header[1..9]).map_or(u64::MAX, u64::from_le_bytes);
        let declared = usize::try_from(declared).unwrap_or(usize::MAX);
        // The file is already in memory, so a corrupt length cannot allocate beyond it.
        let Some(payload) =
            bytes.get(offset + 9..offset.saturating_add(9).saturating_add(declared))
        else {
            break;
        };
        frames.push(TraceFrame { kind: tag, payload: payload.to_vec() });
        offset = offset.saturating_add(9).saturating_add(declared);
    }
    Ok(frames)
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
