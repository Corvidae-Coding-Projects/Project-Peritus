//! Immutable installation configuration with inert resource owners.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery, ProbeRequest};
use std::path::{Path, PathBuf};

/// Immutable Linux backend installation configuration.
#[derive(Debug)]
pub struct LinuxBackendConfig {
    pub(crate) workspace_root: PathBuf,
    pub(crate) protected_roots: Vec<PathBuf>,
    pub(crate) probe_request: ProbeRequest,
    pub(crate) managed_proxy: Option<peritus_network::ManagedProxyPreparation>,
    pub(crate) secrets: Option<peritus_secrets::SecretPreparation>,
}

impl LinuxBackendConfig {
    /// Creates a configuration from exact installation paths.
    ///
    /// # Errors
    /// Rejects a relative workspace root or invalid probe request.
    #[allow(clippy::too_many_arguments, reason = "explicit security-sensitive installation paths")]
    pub fn new(
        workspace_root: PathBuf,
        protected_roots: Vec<PathBuf>,
        bubblewrap_path: PathBuf,
        helper_path: PathBuf,
        cgroup_root: PathBuf,
        proxy_route: Option<crate::ProxyRoute>,
    ) -> Result<Self, LinuxError> {
        if !workspace_root.is_absolute() {
            return Err(LinuxError::new(
                LinuxErrorKind::InvalidPlan,
                LinuxOperation::Prepare,
                LinuxRecovery::CorrectRequest,
                "workspace root must be absolute",
            ));
        }
        let probe_request =
            ProbeRequest::new(bubblewrap_path, helper_path, cgroup_root, proxy_route)?;
        Ok(Self {
            workspace_root,
            protected_roots,
            probe_request,
            managed_proxy: None,
            secrets: None,
        })
    }
    /// Supplies inert managed-proxy configuration consumed only inside authorized preparation.
    ///
    /// This setter performs no socket, resolution, or worker effect.
    #[must_use]
    pub fn with_managed_proxy(
        mut self,
        preparation: peritus_network::ManagedProxyPreparation,
    ) -> Self {
        self.managed_proxy = Some(preparation);
        self
    }

    /// Supplies inert exact secret preparation consumed only inside authorized preparation.
    ///
    /// This setter does not access a credential store or materialize secret data.
    #[must_use]
    pub fn with_secret_preparation(
        mut self,
        preparation: peritus_secrets::SecretPreparation,
    ) -> Self {
        self.secrets = Some(preparation);
        self
    }
    /// Returns configured workspace root.
    #[must_use]
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }
    /// Returns protected metadata additions.
    #[must_use]
    pub fn protected_roots(&self) -> &[PathBuf] {
        &self.protected_roots
    }
    /// Returns probe inputs.
    #[must_use]
    pub const fn probe_request(&self) -> &ProbeRequest {
        &self.probe_request
    }
}
