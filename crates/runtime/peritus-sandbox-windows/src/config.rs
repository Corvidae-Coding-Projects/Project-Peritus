//! Immutable installation configuration and inert post-authorization preparations.

use std::path::{Path, PathBuf};

use peritus_network::ManagedProxyPreparation;
use peritus_secrets::SecretPreparation;
use peritus_types::Sha256Digest;

use crate::{PathPolicy, TokenProfile, WindowsError, WindowsOperation, WindowsPath};

/// Windows installation plus inert proxy/secret preparations.
///
/// Constructing this value performs no network, store, secret-delivery, ACL, token, or process
/// effect. The optional preparations are consumed only by C2's opaque authorized callback.
#[derive(Debug)]
pub struct WindowsBackendConfig {
    pub(crate) helper_path: PathBuf,
    pub(crate) workspace: WindowsPath,
    pub(crate) protected_roots: Vec<WindowsPath>,
    pub(crate) acl_backup_root: PathBuf,
    pub(crate) token: TokenProfile,
    pub(crate) managed_filter_digest: Option<Sha256Digest>,
    pub(crate) proxy: Option<ManagedProxyPreparation>,
    pub(crate) secrets: Option<SecretPreparation>,
}

impl WindowsBackendConfig {
    /// Creates complete checked inert backend configuration.
    ///
    /// # Errors
    /// Rejects non-absolute paths, invalid protected roots, or a proxy without filter identity.
    #[allow(clippy::too_many_arguments, reason = "one explicit value per native owner boundary")]
    pub fn new(
        helper_path: PathBuf,
        workspace: WindowsPath,
        protected_roots: Vec<WindowsPath>,
        acl_backup_root: PathBuf,
        token: TokenProfile,
        managed_filter_digest: Option<Sha256Digest>,
        proxy: Option<ManagedProxyPreparation>,
        secrets: Option<SecretPreparation>,
    ) -> Result<Self, WindowsError> {
        if !helper_path.is_absolute() || !acl_backup_root.is_absolute() {
            return Err(crate::error::invalid(
                WindowsOperation::Validate,
                "helper and ACL backup paths must be absolute",
            ));
        }
        if proxy.is_some() != managed_filter_digest.is_some() {
            return Err(crate::error::invalid(
                WindowsOperation::Validate,
                "managed proxy preparation and filter identity must be configured together",
            ));
        }
        let policy = PathPolicy::new(workspace.clone(), protected_roots)?;
        Ok(Self {
            helper_path,
            workspace,
            protected_roots: policy.protected_roots().to_vec(),
            acl_backup_root,
            token,
            managed_filter_digest,
            proxy,
            secrets,
        })
    }

    /// Returns installed helper path.
    #[must_use]
    pub fn helper_path(&self) -> &Path {
        &self.helper_path
    }
    /// Returns normalized workspace.
    #[must_use]
    pub const fn workspace(&self) -> &WindowsPath {
        &self.workspace
    }
    /// Returns protected metadata roots.
    #[must_use]
    pub fn protected_roots(&self) -> &[WindowsPath] {
        &self.protected_roots
    }
    /// Returns private ACL backup root.
    #[must_use]
    pub fn acl_backup_root(&self) -> &Path {
        &self.acl_backup_root
    }
    /// Returns selected token profile.
    #[must_use]
    pub const fn token(&self) -> &TokenProfile {
        &self.token
    }
    /// Returns the reviewed dynamic WFP controller identity.
    #[must_use]
    pub const fn managed_filter_digest(&self) -> Option<Sha256Digest> {
        self.managed_filter_digest
    }
    /// Reports whether an inert managed proxy preparation is configured.
    #[must_use]
    pub const fn has_proxy_preparation(&self) -> bool {
        self.proxy.is_some()
    }
    /// Reports whether an inert secret preparation is configured.
    #[must_use]
    pub const fn has_secret_preparation(&self) -> bool {
        self.secrets.is_some()
    }
}
