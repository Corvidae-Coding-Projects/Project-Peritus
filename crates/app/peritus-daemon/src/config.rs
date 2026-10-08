//! Strict version-one daemon configuration.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

mod approval;
mod catalog;
mod context;
mod deserialize;
mod folder;
mod network;
mod paths;
mod product;
mod provider;

pub use approval::ApprovalRegistryDeclaration;
pub use catalog::{ProjectDeclaration, ToolPolicy, WorkspaceDeclaration};
pub use context::ContextPolicy;
pub use folder::FolderDeclaration;
pub use network::{
    ManagedGateNetworkDestinationDeclaration, ManagedGateNetworkGrantDeclaration,
};
pub use paths::DaemonPaths;
pub use product::ProductRunPolicy;
pub use provider::{ProviderProfileDeclaration, ProviderRoute, ProviderRouteKind};

pub const DAEMON_VERSION: &str = "0.0.0";

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
#[derive(Clone, Debug, Eq, PartialEq)]
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

/// Bounded runtime queue and concurrency limits.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DaemonLimits {
    authority_queue: usize,
    connection_queue: usize,
    maximum_connections: usize,
    maximum_workers: usize,
    #[serde(default)]
    journal_maximum_pages: Option<u64>,
    #[serde(default)]
    maximum_artifact_bytes: Option<u64>,
    #[serde(default)]
    artifact_quota_bytes: Option<u64>,
    #[serde(default)]
    artifact_minimum_free_bytes: u64,
    shutdown_millis: u64,
}

impl DaemonLimits {
    /// Production defaults sized for a single-user local harness daemon.
    pub const PRODUCTION: Self = Self {
        authority_queue: 1_024,
        connection_queue: 256,
        maximum_connections: 64,
        maximum_workers: 32,
        journal_maximum_pages: None,
        maximum_artifact_bytes: None,
        artifact_quota_bytes: None,
        artifact_minimum_free_bytes: 0,
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
    /// Returns the explicitly configured SQLite database page ceiling.
    #[must_use]
    pub const fn journal_maximum_pages(self) -> Option<u64> {
        self.journal_maximum_pages
    }
    /// Builds the journal open policy selected by this daemon configuration.
    #[must_use]
    pub const fn journal_options(self) -> peritus_journal::SqliteJournalOptions {
        match self.journal_maximum_pages {
            Some(maximum_pages) => {
                peritus_journal::SqliteJournalOptions::native()
                    .with_maximum_pages(maximum_pages)
            }
            None => peritus_journal::SqliteJournalOptions::native(),
        }
    }
    /// Returns the maximum size of one immutable artifact.
    #[must_use]
    pub const fn maximum_artifact_bytes(self) -> u64 {
        match self.maximum_artifact_bytes {
            Some(limit) => limit,
            None => match self.artifact_quota_bytes {
                Some(quota) => quota,
                None => i64::MAX as u64,
            },
        }
    }
    /// Returns the optional total logical immutable artifact quota.
    #[must_use]
    pub const fn artifact_quota_bytes(self) -> Option<u64> {
        self.artifact_quota_bytes
    }
    /// Returns physical space retained after each artifact reservation.
    #[must_use]
    pub const fn artifact_minimum_free_bytes(self) -> u64 {
        self.artifact_minimum_free_bytes
    }

    pub(crate) fn artifact_store_config(
        self,
        root: impl Into<PathBuf>,
    ) -> Result<peritus_artifact_store::StoreConfig, peritus_artifact_store::ArtifactStoreError> {
        let config = match self.artifact_quota_bytes {
            Some(quota) => peritus_artifact_store::StoreConfig::new(
                root,
                self.maximum_artifact_bytes(),
                quota,
            )?,
            None => peritus_artifact_store::StoreConfig::for_available_space(
                root,
                self.maximum_artifact_bytes(),
            )?,
        };
        config.with_minimum_free_bytes(self.artifact_minimum_free_bytes)
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
            || self.journal_maximum_pages.is_some_and(|pages| pages == 0)
            || self
                .journal_maximum_pages
                .is_some_and(|pages| pages > i64::MAX as u64)
            || self.maximum_artifact_bytes.is_some_and(|limit| limit == 0)
            || self
                .maximum_artifact_bytes
                .is_some_and(|limit| limit > i64::MAX as u64)
            || self.artifact_quota_bytes.is_some_and(|quota| quota == 0)
            || self.artifact_quota_bytes.is_some_and(|quota| quota > i64::MAX as u64)
            || matches!(
                (self.maximum_artifact_bytes, self.artifact_quota_bytes),
                (Some(limit), Some(quota)) if limit > quota
            )
            || self
                .maximum_artifact_bytes()
                .checked_add(self.artifact_minimum_free_bytes)
                .is_none()
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonConfig {
    version: u16,
    store_id: String,
    paths: DaemonPaths,
    approval_registry: ApprovalRegistryDeclaration,
    limits: DaemonLimits,
    human: LocalHumanPrincipal,
    projects: Vec<ProjectDeclaration>,
    workspaces: Vec<WorkspaceDeclaration>,
    folders: Vec<FolderDeclaration>,
    tools: ToolPolicy,
    providers: Vec<ProviderRoute>,
    product: ProductRunPolicy,
    context: ContextPolicy,
    managed_gate_network: Vec<ManagedGateNetworkGrantDeclaration>,
    telemetry: TelemetryExport,
    process_crash_watchdog: Option<PathBuf>,
    configuration_digest: [u8; 32],
}

impl DaemonConfig {
    /// Parses strict TOML configuration and rejects unknown authority-relevant fields.
    ///
    /// # Errors
    ///
    /// Returns a typed configuration error for malformed, unsupported, or unsafe values.
    pub fn parse(text: &str) -> Result<Self, DaemonError> {
        let mut config: Self = toml::from_str(text).map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::InvalidInput,
                DaemonRecovery::CorrectRequest,
                "parse daemon configuration",
                "configuration is not strict version-one TOML",
                error,
            )
        })?;
        config.configuration_digest = Sha256::digest(text.as_bytes()).into();
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
    /// Returns the digest of the exact strict TOML bytes accepted by [`Self::parse`].
    #[must_use]
    pub const fn configuration_digest(&self) -> peritus_types::Sha256Digest {
        peritus_types::Sha256Digest::new(self.configuration_digest)
    }
    /// Borrows protected paths.
    #[must_use]
    pub const fn paths(&self) -> &DaemonPaths {
        &self.paths
    }
    pub(crate) fn with_process_crash_watchdog(mut self, executable: PathBuf) -> Self {
        self.process_crash_watchdog = Some(executable);
        self
    }
    pub(crate) fn process_crash_watchdog(&self) -> Option<&Path> {
        self.process_crash_watchdog.as_deref()
    }
    /// Borrows the required public approval credential-registry declaration.
    #[must_use]
    pub const fn approval_registry(&self) -> &ApprovalRegistryDeclaration {
        &self.approval_registry
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

    /// Direct directories authorized for conversational work, never synthetic Git registrations.
    #[must_use]
    pub fn folders(&self) -> &[FolderDeclaration] {
        &self.folders
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
    /// Returns explicit product-run recovery behavior.
    #[must_use]
    pub const fn product(&self) -> ProductRunPolicy {
        self.product
    }
    /// Borrows local context policy and the optional local-only task-provider admission mode.
    #[must_use]
    pub const fn context(&self) -> &ContextPolicy {
        &self.context
    }
    /// Borrows trusted exact-command managed-network grants.
    #[must_use]
    pub fn managed_gate_network(&self) -> &[ManagedGateNetworkGrantDeclaration] {
        &self.managed_gate_network
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
        self.approval_registry.validate()?;
        self.limits.validate()?;
        catalog::validate(&self.projects, &self.workspaces, &self.tools)?;
        folder::validate(&self.folders)?;
        provider::validate(&self.providers)?;
        self.product.validate()?;
        self.context.validate(&self.providers, self.product)?;
        network::validate(&self.managed_gate_network)?;
        if let TelemetryExport::LocalFile { directory, quota_bytes } = &self.telemetry
            && (!directory.is_absolute()
                || directory.components().any(|part| part == Component::ParentDir)
                || *quota_bytes == 0)
        {
            return Err(invalid("local telemetry export path or quota is invalid"));
        }
        Ok(())
    }
}

pub fn decode_identifier(value: &str, field: &'static str) -> Result<[u8; 16], DaemonError> {
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
