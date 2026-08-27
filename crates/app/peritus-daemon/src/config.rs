//! Strict version-one daemon configuration.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

mod catalog;
mod provider;

pub use catalog::{ProjectDeclaration, ToolPolicy, WorkspaceDeclaration};
pub use provider::{ProviderProfileDeclaration, ProviderRoute, ProviderRouteKind};

/// Offline-provisioned local human identity bound to the operating-system account.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalHumanPrincipal {
    actor_id: String,
}

impl LocalHumanPrincipal {
    /// Parses the configured nonzero actor identity.
    ///
    /// # Errors
    ///
    /// Returns invalid input if configuration was constructed without validation.
    pub fn actor_identity(&self) -> Result<peritus_types::ActorId, DaemonError> {
        let bytes = decode_identifier(&self.actor_id, "local actor identity")?;
        peritus_types::ActorId::new(bytes)
            .map_err(|_| invalid("local actor identity must be nonzero"))
    }
}

/// Closed telemetry export policy supported by G0.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
pub enum TelemetryExport {
    /// Start no exporter task.
    Disabled,
    /// Synchronize bounded batches beneath this protected local directory.
    LocalFile {
        /// Protected spool directory.
        directory: PathBuf,
        /// Maximum retained spool bytes.
        quota_bytes: u64,
    },
}

/// Protected daemon filesystem roots.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DaemonPaths {
    state_root: PathBuf,
    artifact_root: PathBuf,
    evidence_root: PathBuf,
    workspace_root: PathBuf,
    process_root: PathBuf,
    transaction_root: PathBuf,
    backup_root: PathBuf,
}

impl DaemonPaths {
    /// Creates and validates absolute, lexically normalized daemon roots.
    ///
    /// # Errors
    ///
    /// Returns invalid input for relative paths, parent traversal, or duplicate roots.
    pub fn new(
        state_root: PathBuf,
        artifact_root: PathBuf,
        evidence_root: PathBuf,
        workspace_root: PathBuf,
        process_root: PathBuf,
        transaction_root: PathBuf,
        backup_root: PathBuf,
    ) -> Result<Self, DaemonError> {
        let paths = Self {
            state_root,
            artifact_root,
            evidence_root,
            workspace_root,
            process_root,
            transaction_root,
            backup_root,
        };
        paths.validate()?;
        Ok(paths)
    }

    /// Returns the protected daemon state root.
    #[must_use]
    pub fn state_root(&self) -> &Path {
        &self.state_root
    }
    /// Returns the immutable artifact root.
    #[must_use]
    pub fn artifact_root(&self) -> &Path {
        &self.artifact_root
    }
    /// Returns the acceptance-evidence root.
    #[must_use]
    pub fn evidence_root(&self) -> &Path {
        &self.evidence_root
    }
    /// Returns the registered-workspace parent.
    #[must_use]
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }
    /// Returns the native process registry root.
    #[must_use]
    pub fn process_root(&self) -> &Path {
        &self.process_root
    }
    /// Returns the C1 mutation transaction parent.
    #[must_use]
    pub fn transaction_root(&self) -> &Path {
        &self.transaction_root
    }
    /// Returns the migration backup root.
    #[must_use]
    pub fn backup_root(&self) -> &Path {
        &self.backup_root
    }
    /// Returns the shared SQLite database path.
    #[must_use]
    pub fn database(&self) -> PathBuf {
        self.state_root.join("peritus.sqlite3")
    }

    fn validate(&self) -> Result<(), DaemonError> {
        let children = [
            &self.artifact_root,
            &self.evidence_root,
            &self.workspace_root,
            &self.process_root,
            &self.transaction_root,
            &self.backup_root,
        ];
        for path in std::iter::once(&self.state_root).chain(children) {
            if !path.is_absolute()
                || path
                    .components()
                    .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
            {
                return Err(invalid(
                    "daemon paths must be absolute and contain no parent traversal",
                ));
            }
        }
        for child in children {
            if !child.starts_with(&self.state_root) || child == &self.state_root {
                return Err(invalid("daemon protected component roots must be beneath state_root"));
            }
        }
        for (index, left) in children.iter().enumerate() {
            if children
                .iter()
                .skip(index + 1)
                .any(|right| left.starts_with(right.as_path()) || right.starts_with(left.as_path()))
            {
                return Err(invalid("daemon protected component roots must not overlap"));
            }
        }
        Ok(())
    }
}

/// Bounded runtime queue and concurrency limits.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DaemonLimits {
    authority_queue: usize,
    connection_queue: usize,
    maximum_connections: usize,
    maximum_workers: usize,
    maximum_artifact_bytes: u64,
    artifact_quota_bytes: u64,
    shutdown_millis: u64,
}

impl DaemonLimits {
    /// Production defaults sized for a single-user local harness daemon.
    pub const PRODUCTION: Self = Self {
        authority_queue: 1_024,
        connection_queue: 256,
        maximum_connections: 64,
        maximum_workers: 32,
        maximum_artifact_bytes: 1_073_741_824,
        artifact_quota_bytes: 68_719_476_736,
        shutdown_millis: 30_000,
    };

    /// Returns authority queue capacity.
    #[must_use]
    pub const fn authority_queue(self) -> usize {
        self.authority_queue
    }
    /// Returns per-connection queue capacity.
    #[must_use]
    pub const fn connection_queue(self) -> usize {
        self.connection_queue
    }
    /// Returns maximum simultaneous authenticated connections.
    #[must_use]
    pub const fn maximum_connections(self) -> usize {
        self.maximum_connections
    }
    /// Returns maximum owned effect tasks.
    #[must_use]
    pub const fn maximum_workers(self) -> usize {
        self.maximum_workers
    }
    /// Returns the maximum size of one immutable artifact.
    #[must_use]
    pub const fn maximum_artifact_bytes(self) -> u64 {
        self.maximum_artifact_bytes
    }
    /// Returns total logical immutable artifact quota.
    #[must_use]
    pub const fn artifact_quota_bytes(self) -> u64 {
        self.artifact_quota_bytes
    }
    /// Returns bounded orderly shutdown duration.
    #[must_use]
    pub const fn shutdown_millis(self) -> u64 {
        self.shutdown_millis
    }

    fn validate(self) -> Result<(), DaemonError> {
        if self.authority_queue == 0
            || self.authority_queue > 65_536
            || self.connection_queue == 0
            || self.connection_queue > 4_096
            || self.maximum_connections == 0
            || self.maximum_connections > 1_024
            || self.maximum_workers == 0
            || self.maximum_workers > 1_024
            || self.maximum_artifact_bytes == 0
            || self.maximum_artifact_bytes > self.artifact_quota_bytes
            || self.artifact_quota_bytes > i64::MAX as u64
            || self.shutdown_millis == 0
            || self.shutdown_millis > 600_000
        {
            return Err(invalid("daemon runtime limits are outside production bounds"));
        }
        Ok(())
    }
}

impl Default for DaemonLimits {
    fn default() -> Self {
        Self::PRODUCTION
    }
}

/// Complete strict version-one daemon configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DaemonConfig {
    version: u16,
    store_id: String,
    paths: DaemonPaths,
    #[serde(default)]
    limits: DaemonLimits,
    human: LocalHumanPrincipal,
    #[serde(default)]
    projects: Vec<ProjectDeclaration>,
    #[serde(default)]
    workspaces: Vec<WorkspaceDeclaration>,
    #[serde(default)]
    tools: ToolPolicy,
    #[serde(default)]
    providers: Vec<ProviderRoute>,
    telemetry: TelemetryExport,
}

impl DaemonConfig {
    /// Parses strict TOML configuration and rejects unknown authority-relevant fields.
    ///
    /// # Errors
    ///
    /// Returns a typed configuration error for malformed, unsupported, or unsafe values.
    pub fn parse(text: &str) -> Result<Self, DaemonError> {
        let config: Self = toml::from_str(text).map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::InvalidInput,
                DaemonRecovery::CorrectRequest,
                "parse daemon configuration",
                "configuration is not strict version-one TOML",
                error,
            )
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Loads strict TOML configuration from a regular file.
    ///
    /// # Errors
    ///
    /// Returns a typed filesystem or configuration error.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, DaemonError> {
        let path = path.as_ref();
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::Storage,
                DaemonRecovery::CorrectRequest,
                "inspect daemon configuration",
                "configuration path cannot be inspected",
                error,
            )
        })?;
        if !metadata.file_type().is_file() {
            return Err(invalid("daemon configuration must be a regular file"));
        }
        let text = fs::read_to_string(path).map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::Storage,
                DaemonRecovery::CorrectRequest,
                "read daemon configuration",
                "configuration cannot be read as UTF-8",
                error,
            )
        })?;
        Self::parse(&text)
    }

    /// Returns configuration schema version one.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }
    /// Parses the stable nonzero journal store identity.
    ///
    /// # Errors
    ///
    /// Returns invalid input if configuration was constructed outside [`Self::parse`].
    pub fn store_identity(&self) -> Result<peritus_journal::StoreId, DaemonError> {
        let bytes = decode_identifier(&self.store_id, "daemon store identity")?;
        peritus_journal::StoreId::new(bytes).map_err(|_| invalid("daemon store identity is zero"))
    }
    /// Borrows protected paths.
    #[must_use]
    pub const fn paths(&self) -> &DaemonPaths {
        &self.paths
    }
    /// Returns bounded runtime limits.
    #[must_use]
    pub const fn limits(&self) -> DaemonLimits {
        self.limits
    }
    /// Borrows the offline-provisioned local human identity.
    #[must_use]
    pub const fn human(&self) -> &LocalHumanPrincipal {
        &self.human
    }
    /// Borrows the exact configured project inventory.
    #[must_use]
    pub fn projects(&self) -> &[ProjectDeclaration] {
        &self.projects
    }
    /// Borrows the exact configured workspace inventory.
    #[must_use]
    pub fn workspaces(&self) -> &[WorkspaceDeclaration] {
        &self.workspaces
    }
    /// Borrows the explicit tool allowlist.
    #[must_use]
    pub const fn tools(&self) -> &ToolPolicy {
        &self.tools
    }
    /// Borrows the exact provider-profile routes configured for startup.
    #[must_use]
    pub fn providers(&self) -> &[ProviderRoute] {
        &self.providers
    }
    /// Borrows telemetry export policy.
    #[must_use]
    pub const fn telemetry(&self) -> &TelemetryExport {
        &self.telemetry
    }

    fn validate(&self) -> Result<(), DaemonError> {
        if self.version != 1 {
            return Err(invalid("unsupported daemon configuration version"));
        }
        if self.store_id.len() != 32
            || !self
                .store_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(invalid("daemon store identity must be 32 lowercase hexadecimal digits"));
        }
        self.store_identity()?;
        self.human.actor_identity()?;
        self.paths.validate()?;
        self.limits.validate()?;
        catalog::validate(&self.projects, &self.workspaces, &self.tools)?;
        provider::validate(&self.providers)?;
        if let TelemetryExport::LocalFile { directory, quota_bytes } = &self.telemetry {
            if !directory.is_absolute()
                || directory.components().any(|part| part == Component::ParentDir)
                || *quota_bytes == 0
            {
                return Err(invalid("local telemetry export path or quota is invalid"));
            }
        }
        Ok(())
    }
}

pub(super) fn decode_identifier(value: &str, field: &'static str) -> Result<[u8; 16], DaemonError> {
    if value.len() != 32 {
        return Err(invalid(field));
    }
    let mut bytes = [0_u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0]).ok_or_else(|| invalid(field))?;
        let low = hex_nibble(pair[1]).ok_or_else(|| invalid(field))?;
        bytes[index] = (high << 4) | low;
    }
    Ok(bytes)
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "validate daemon configuration",
        detail,
    )
}
