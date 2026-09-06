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
    /// Wall-clock deadline in milliseconds, at most 60 seconds.
    pub timeout_millis: u64,
    /// Maximum complete input bytes, at most one MiB.
    pub max_input_bytes: usize,
    /// Maximum complete output bytes, at most 256 KiB.
    pub max_output_bytes: usize,
    /// Maximum resident memory under the platform's enforced resource envelope.
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
    /// Maximum tokens allocated to structured working entries.
    pub working_state_max_tokens: u64,
    /// Maximum tokens allocated to automatically retrieved older observations.
    pub retrieved_evidence_max_tokens: u64,
    /// Maximum operations in an agent-authored atomic update.
    pub max_update_operations: usize,
    /// Maximum UTF-8 bytes in an agent-authored entry.
    pub max_entry_bytes: usize,
    /// Maximum bytes in one evidence read response.
    pub max_read_bytes: usize,
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
            || !(1..=128).contains(&self.retain_recent_messages)
            || !(1..=16_384).contains(&self.working_state_max_tokens)
            || self.retrieved_evidence_max_tokens > 16_384
            || !(256..=65_536).contains(&self.max_read_bytes)
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
    fn validate(&self) -> Result<(), ProductRunnerError> {
        self.sandbox.validate()?;
        if !self.executable.is_absolute()
            || !self.model_path.is_absolute()
            || !(1..=60_000).contains(&self.timeout_millis)
            || !(1..=1_048_576).contains(&self.max_input_bytes)
            || !(1..=262_144).contains(&self.max_output_bytes)
            || !(16 * 1024 * 1024..=64 * 1024 * 1024 * 1024).contains(&self.memory_bytes)
        {
            return Err(error("invalid local process envelope"));
        }
        Ok(())
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
