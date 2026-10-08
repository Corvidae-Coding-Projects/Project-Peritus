//! Explicit local-only context policy and optional bounded subprocess configuration.

use crate::{ProductRunnerError, ProductRunnerErrorKind};
use peritus_context::working::WorkingLimits;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

mod sandbox_wire;

#[allow(
    clippy::derivable_impls,
    reason = "the formal API audit supports explicit impls, not enum default expansion attributes"
)]
impl Default for LocalContextEngine {
    fn default() -> Self {
        Self::Deterministic
    }
}
#[allow(
    clippy::derivable_impls,
    reason = "the formal API audit supports explicit impls, not enum default expansion attributes"
)]
impl Default for LocalSemanticBackend {
    fn default() -> Self {
        Self::Disabled
    }
}

/// Deterministic context engine. There is no remote memory service variant.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LocalContextEngine {
    /// Local reducers plus bounded, source-backed agent-authored updates.
    Deterministic,
}

/// Optional auxiliary semantic inference, separate from the selected task model.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LocalSemanticBackend {
    /// No auxiliary inference; deterministic assembly and normal task-model updates only.
    Disabled,
    /// An explicitly configured local executable with preinstalled weights.
    LocalProcess,
}

/// No-network auxiliary inference envelope; executable and weights are never downloaded.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalProcessConfig {
    /// Explicit installed native sandbox resources. Unsupported hosts fail locally, never raw.
    pub sandbox: LocalCompactorSandbox,
    /// Absolute path to the local JSON-in/JSON-out compactor executable.
    pub executable: PathBuf,
    /// Absolute path to one preinstalled weights file; directories are not admitted.
    pub model_path: PathBuf,
    /// Explicit caller-selected wall-clock deadline in milliseconds; zero selects no deadline.
    pub timeout_millis: u64,
    /// Maximum bytes in one physical semantic-process input page.
    ///
    /// Version-one configuration already declared this physical page ownership. The field name
    /// remains unchanged for strict-decoder compatibility; it is not a lifetime-input allowance.
    pub max_input_bytes: usize,
    /// Maximum bytes in one durable semantic-process output segment.
    ///
    /// Version-one configuration declared this as one physical output page. The existing field is
    /// retained as the durable segment allocation; it is not a cumulative output allowance and
    /// reaching one segment boundary does not stop the process.
    pub max_output_bytes: usize,
    /// Selected resident-memory ceiling; zero explicitly selects no memory ceiling.
    pub memory_bytes: u64,
}

/// Local working-memory policy; disabled explicitly selects legacy context handling.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct LocalContextConfig {
    /// Enables local memory by default; false selects legacy compaction explicitly.
    pub enabled: bool,
    /// Local processing engine.
    pub engine: LocalContextEngine,
    /// Optional auxiliary inference backend; no remote fallback exists.
    pub semantic_backend: LocalSemanticBackend,
    /// Optional local subprocess envelope, required only for `local_process`.
    pub local_process: Option<LocalProcessConfig>,
    /// Input-capacity percentage at which optional transcript detail is reduced.
    pub trigger_percent: u8,
    /// Recent message preference; complete exchanges and semantic pins take precedence.
    pub retain_recent_messages: usize,
    /// Preferred token ceiling for working entries; required closure may borrow input headroom.
    pub working_state_max_tokens: u64,
    /// Preferred token allocation for optional automatically retrieved older observations.
    pub retrieved_evidence_max_tokens: u64,
    /// Maximum operations in one physical reducer page of an atomic logical update.
    pub max_update_operations: usize,
    /// Maximum UTF-8 bytes in an agent-authored entry.
    pub max_entry_bytes: usize,
    /// Maximum bytes in one evidence read response.
    pub max_read_bytes: usize,
    /// Unprivileged filesystem bytes retained after each local-memory artifact reservation.
    /// Zero keeps preallocation advisory while actual filesystem exhaustion remains retryable.
    pub storage_minimum_free_bytes: u64,
    /// Polling cadence while durable local-memory writes wait for physical storage pressure to
    /// clear. This is a cadence, not a deadline; cancellation remains the only implicit stop.
    pub storage_pressure_retry_millis: u64,
    /// Publishes a checkpoint after each completed batch in addition to each next model view.
    pub checkpoint_every_completed_batch: bool,
}

impl Default for LocalContextConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            engine: LocalContextEngine::Deterministic,
            semantic_backend: LocalSemanticBackend::Disabled,
            local_process: None,
            trigger_percent: 85,
            retain_recent_messages: 8,
            working_state_max_tokens: 4096,
            retrieved_evidence_max_tokens: 4096,
            max_update_operations: 32,
            max_entry_bytes: 2048,
            max_read_bytes: 16_384,
            storage_minimum_free_bytes: 0,
            storage_pressure_retry_millis: 100,
            checkpoint_every_completed_batch: true,
        }
    }
}

impl LocalContextConfig {
    /// Validates the complete local policy before opening any memory store or model request.
    ///
    /// # Errors
    /// Rejects zero/excessive bounds, inconsistent backend configuration, or ambient paths.
    pub fn validate(&self) -> Result<(), ProductRunnerError> {
        self.working_limits()?;
        if !(1..=100).contains(&self.trigger_percent)
            || self.retain_recent_messages == 0
            || self.working_state_max_tokens == 0
            || !(1..=65_536).contains(&self.max_read_bytes)
            || self.storage_pressure_retry_millis == 0
        {
            return Err(error("invalid context policy bounds"));
        }
        match (&self.semantic_backend, &self.local_process) {
            (LocalSemanticBackend::Disabled, None) => Ok(()),
            (LocalSemanticBackend::LocalProcess, Some(process)) => process.validate(),
            _ => Err(error("inconsistent local semantic backend configuration")),
        }
    }

    pub(crate) fn working_limits(&self) -> Result<WorkingLimits, ProductRunnerError> {
        WorkingLimits::new(65_535, 512, self.max_entry_bytes, 32, self.max_update_operations)
            .map_err(|_| error("invalid working-state bounds"))
    }
}

impl LocalProcessConfig {
    pub(crate) fn validate(&self) -> Result<(), ProductRunnerError> {
        self.sandbox.validate()?;
        if !self.executable.is_absolute()
            || !self.model_path.is_absolute()
            || self.max_input_bytes == 0
            || self.max_output_bytes == 0
        {
            return Err(error("invalid local process envelope"));
        }
        Ok(())
    }

    /// Returns the selected wall deadline, or `None` for explicitly unlimited execution.
    #[must_use]
    pub const fn wall_timeout_millis(&self) -> Option<u64> {
        if self.timeout_millis == 0 { None } else { Some(self.timeout_millis) }
    }

    /// Returns the selected physical input-page allocation.
    #[must_use]
    pub const fn input_page_bytes(&self) -> usize {
        self.max_input_bytes
    }

    /// Returns the selected durable output-segment allocation.
    #[must_use]
    pub const fn output_segment_bytes(&self) -> usize {
        self.max_output_bytes
    }

    /// Returns the selected memory ceiling, or `None` when the caller selected no ceiling.
    #[must_use]
    pub const fn memory_limit_bytes(&self) -> Option<u64> {
        if self.memory_bytes == 0 { None } else { Some(self.memory_bytes) }
    }
}

/// Installed C3 backend resources for no-network, credential-free auxiliary inference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalCompactorSandbox {
    /// Native Linux namespaces, seccomp/Landlock, and delegated cgroup v2 enforcement.
    Linux {
        /// Absolute installed bubblewrap executable.
        bubblewrap: PathBuf,
        /// Absolute installed Peritus Linux sandbox helper.
        helper: PathBuf,
        /// Exact delegated cgroup directory; never the host-wide cgroup root.
        cgroup_root: PathBuf,
    },
    /// Native macOS Seatbelt plus process-owned resource enforcement.
    Macos {
        /// Absolute installed Peritus macOS sandbox helper.
        helper: PathBuf,
        /// Absolute installed sandbox-exec executable.
        seatbelt: PathBuf,
    },
    /// Native Windows `AppContainer` and Job Object enforcement.
    Windows {
        /// Absolute installed Peritus Windows sandbox helper.
        helper: PathBuf,
    },
}

impl LocalCompactorSandbox {
    fn validate(&self) -> Result<(), ProductRunnerError> {
        let paths: &[&PathBuf] = match self {
            Self::Linux { bubblewrap, helper, cgroup_root } => &[bubblewrap, helper, cgroup_root],
            Self::Macos { helper, seatbelt } => &[helper, seatbelt],
            Self::Windows { helper } => &[helper],
        };
        if paths.iter().any(|path| {
            !path.is_absolute()
                || path.components().any(|part| matches!(part, std::path::Component::ParentDir))
        }) {
            return Err(error("local sandbox paths must be absolute and traversal-free"));
        }
        if matches!(self,Self::Linux { cgroup_root,.. } if cgroup_root == std::path::Path::new("/sys/fs/cgroup"))
        {
            return Err(error("local compactor requires an exact delegated cgroup subtree"));
        }
        Ok(())
    }
}

fn error(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "local context configuration",
        detail,
    )
}
