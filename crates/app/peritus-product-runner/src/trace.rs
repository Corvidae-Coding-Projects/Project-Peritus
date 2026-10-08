//! Durable per-run D0 provider/tool trace.

pub mod accounting;
pub mod local_memory;
mod retry;
mod scoped_tool;

pub(crate) use scoped_tool::ScopedToolReceiptPolicy;

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
    /// One exact provider request admitted before dispatch.
    RetryAttempt,
    /// Terminal, ambiguous, or safe-schedule supersession boundary.
    RetryDisposition,
    /// Additive exact-scope binding to the immediately following host tool observation.
    ScopedToolObservation,
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
            8 => Some(Self::RetryAttempt),
            9 => Some(Self::RetryDisposition),
            10 => Some(Self::ScopedToolObservation),
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
            Self::RetryAttempt => 8,
            Self::RetryDisposition => 9,
            Self::ScopedToolObservation => 10,
        }
    }
}

/// Length-framed append-only trace stored beside the daemon's product-run record.
pub struct FileDeveloperTrace {
    path: PathBuf,
    memory_scope: Option<local_memory::Scope>,
    scoped_tool_receipts: Option<scoped_tool::ScopedToolReceiptBinding>,
}

impl FileDeveloperTrace {
    #[must_use]
    pub const fn new(path: PathBuf) -> Self {
        Self { path, memory_scope: None, scoped_tool_receipts: None }
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

    pub(crate) fn with_scoped_tool_receipts(
        mut self,
        policy: ScopedToolReceiptPolicy,
        request_prefix: &str,
    ) -> Result<Self, DeveloperLoopError> {
        self.scoped_tool_receipts = Some(policy.bind(request_prefix)?);
        Ok(self)
    }

    fn record_tool_observation(
        &mut self,
        call: &peritus_model_protocol::CompletedToolCall,
        observation: &peritus_agent::DeveloperToolObservation,
    ) -> Result<(), DeveloperLoopError> {
        let (source_tag, payload, next_memory_scope) = if let Some(mut scope) = self.memory_scope {
            scope.observed = scope.observed.checked_add(1).ok_or_else(|| {
                DeveloperLoopError::Trace(
                    "local trace observation sequence overflow".to_owned(),
                )
            })?;
            (
                DeveloperTraceFrameKind::LocalMemoryObservation,
                local_memory::tool_payload(scope, call, observation)?,
                Some(scope),
            )
        } else {
            let fields = [
                ("call_id".to_owned(), Value::String(call.id().expose_for_wire().to_owned())),
                ("name".to_owned(), Value::String(call.name().as_str().to_owned())),
                ("arguments".to_owned(), Value::String(call.arguments().to_wire_string())),
                ("output".to_owned(), Value::String(observation.output.to_wire_string())),
                ("is_error".to_owned(), Value::Bool(observation.is_error)),
            ];
            let payload = serde_json::to_vec(&Value::Object(
                fields.into_iter().collect::<Map<_, _>>(),
            ))
            .map_err(|error| DeveloperLoopError::Trace(error.to_string()))?;
            (DeveloperTraceFrameKind::ToolObservation, payload, None)
        };

        let mut file = open(&self.path).map_err(|error| trace(&error))?;
        file.lock().map_err(|error| trace(&error))?;
        if !observation.is_error && let Some(binding) = &self.scoped_tool_receipts {
            let receipt = scoped_tool::receipt_payload(binding, source_tag, &payload)?;
            append_locked(
                &mut file,
                DeveloperTraceFrameKind::ScopedToolObservation.tag(),
                &receipt,
            )
            .map_err(|error| trace(&error))?;
            // A retained source must never exist without its exact scope metadata. Sync the
            // post-execution receipt first; a crash before the source then remains explicit
            // recovery state instead of an apparently unscoped successful observation.
            file.sync_data().map_err(|error| trace(&error))?;
        }
        append_locked(&mut file, source_tag.tag(), &payload).map_err(|error| trace(&error))?;
        file.sync_data().map_err(|error| trace(&error))?;
        self.memory_scope = next_memory_scope;
        Ok(())
    }
}

pub(crate) fn replay_scoped_tool_observations(
    path: &Path,
    policy: &ScopedToolReceiptPolicy,
    observe: &mut dyn FnMut(
        &peritus_model_protocol::CompletedToolCall,
        &peritus_model_protocol::CanonicalJson,
    ) -> Result<(), DeveloperLoopError>,
) -> Result<(), DeveloperLoopError> {
    scoped_tool::replay(path, policy, observe)
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

pub(crate) fn pending_retry(
    path: &Path,
) -> Result<Option<retry::PendingRetry>, DeveloperLoopError> {
    retry::pending(path)
}

pub(crate) fn supersede_pending_retry(
    path: &Path,
    request_prefix: &str,
    turn: u16,
    scheduled_attempt: u64,
) -> Result<(), DeveloperLoopError> {
    retry::supersede(path, request_prefix, turn, scheduled_attempt)
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
                return self.record_tool_observation(call, observation);
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
                let payload = retry::schedule_payload(record)?;
                (DeveloperTraceFrameKind::RetryScheduled, payload)
            }
        };
        if tag == DeveloperTraceFrameKind::RetryScheduled {
            retry::append(&self.path, tag, &payload)
        } else {
            append(&self.path, tag.tag(), &payload).map_err(|error| trace(&error))
        }
    }

    fn recover_retry(
        &mut self,
        request_prefix: &str,
        turn: u16,
        provider_profile_id: peritus_types::ProviderProfileId,
        native_session_digest: Option<peritus_types::Sha256Digest>,
        request_fingerprint: peritus_types::Sha256Digest,
    ) -> Result<Option<peritus_agent::DeveloperRetryRecovery>, DeveloperLoopError> {
        retry::recover(
            &self.path,
            request_prefix,
            turn,
            provider_profile_id,
            native_session_digest,
            request_fingerprint,
        )
    }

    fn begin_retry_attempt(
        &mut self,
        turn: u16,
        attempt: u64,
        request: &peritus_model_protocol::ModelRequest,
    ) -> Result<(), DeveloperLoopError> {
        let payload = retry::attempt_payload(turn, attempt, request)?;
        retry::append(&self.path, DeveloperTraceFrameKind::RetryAttempt, &payload)
    }

    fn finish_retry_attempt(
        &mut self,
        turn: u16,
        attempt: u64,
        request: &peritus_model_protocol::ModelRequest,
        disposition: peritus_agent::DeveloperRetryDisposition,
    ) -> Result<(), DeveloperLoopError> {
        let payload = retry::disposition_payload(turn, attempt, request, disposition)?;
        retry::append(&self.path, DeveloperTraceFrameKind::RetryDisposition, &payload)
    }

    fn supersede_retry(
        &mut self,
        request_prefix: &str,
        turn: u16,
        scheduled_attempt: u64,
    ) -> Result<(), DeveloperLoopError> {
        retry::supersede(&self.path, request_prefix, turn, scheduled_attempt)
    }

    fn supersede_retry_for_provider_selection(
        &mut self,
        request_prefix: &str,
        turn: u16,
        current_selection: peritus_types::Sha256Digest,
    ) -> Result<bool, DeveloperLoopError> {
        retry::supersede_for_provider_selection(
            &self.path,
            request_prefix,
            turn,
            current_selection,
        )
    }
}

fn append(path: &Path, tag: u8, payload: &[u8]) -> io::Result<()> {
    let mut file = open(path)?;
    file.lock()?;
    append_locked(&mut file, tag, payload)?;
    file.sync_data()
}

pub(super) struct TraceFrameLocation {
    pub(super) offset: u64,
    pub(super) end: u64,
}

pub(super) fn append_locked(
    file: &mut File,
    tag: u8,
    payload: &[u8],
) -> io::Result<TraceFrameLocation> {
    let length =
        u64::try_from(payload.len()).map_err(|_| io::Error::other("trace event is too large"))?;
    let offset = file.metadata()?.len();
    let end = offset
        .checked_add(9)
        .and_then(|value| value.checked_add(length))
        .ok_or_else(|| io::Error::other("trace position overflow"))?;
    file.write_all(&[tag])?;
    file.write_all(&length.to_le_bytes())?;
    file.write_all(payload)?;
    Ok(TraceFrameLocation { offset, end })
}

fn open(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).read(true).append(true).open(path)
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
