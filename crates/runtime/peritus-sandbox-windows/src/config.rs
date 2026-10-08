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
    pub(crate) read_only_inputs: Vec<WindowsPath>,
    pub(crate) writable_inputs: Vec<WindowsPath>,
    pub(crate) acl_backup_root: PathBuf,
    pub(crate) token: TokenProfile,
    pub(crate) managed_filter_digest: Option<Sha256Digest>,
    pub(crate) proxy: Option<ManagedProxyPreparation>,
    pub(crate) managed_network_grant: Option<(Vec<u8>, Sha256Digest)>,
    pub(crate) managed_network_cache_root: Option<(PathBuf, Sha256Digest)>,
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
            read_only_inputs: Vec::new(),
            writable_inputs: Vec::new(),
            acl_backup_root,
            token,
            managed_filter_digest,
            proxy,
            managed_network_grant: None,
            managed_network_cache_root: None,
            secrets,
        })
    }

    /// Admits explicitly installed read/execute inputs outside the writable workspace.
    ///
    /// # Errors
    /// Rejects external inputs overlapping the workspace or exceeding policy capacity.
    pub fn with_read_only_inputs(mut self, inputs: Vec<WindowsPath>) -> Result<Self, WindowsError> {
        let policy = PathPolicy::new(self.workspace.clone(), self.protected_roots.clone())?
            .with_writable_inputs(self.writable_inputs.clone())?
            .with_read_only_inputs(inputs.clone())?;
        self.read_only_inputs = policy.read_only_inputs().to_vec();
        Ok(self)
    }

    /// Admits explicitly installed writable inputs outside the workspace.
    ///
    /// This is intended for a host-owned run cache already present in the checked filesystem
    /// plan. It does not grant access by itself: each operation still requires an exact checked
    /// rule and the path must remain disjoint from the workspace, protected roots, and read-only
    /// inputs.
    ///
    /// # Errors
    /// Rejects aliases or overlaps with another path-policy domain.
    pub fn with_writable_inputs(
        mut self,
        inputs: Vec<WindowsPath>,
    ) -> Result<Self, WindowsError> {
        let policy = PathPolicy::new(self.workspace.clone(), self.protected_roots.clone())?
            .with_read_only_inputs(self.read_only_inputs.clone())?
            .with_writable_inputs(inputs.clone())?;
        self.writable_inputs = policy.writable_inputs().to_vec();
        Ok(self)
    }

    /// Binds the nonsensitive trusted-host grant identity used for retained reconstruction.
    ///
    /// # Errors
    /// Rejects empty, unframeable, or digest-mismatched canonical bytes.
    pub fn with_managed_network_grant(
        mut self,
        canonical: Vec<u8>,
        digest: Sha256Digest,
    ) -> Result<Self, WindowsError> {
        if canonical.is_empty()
            || u32::try_from(canonical.len()).is_err()
            || peritus_codec::sha256(&canonical) != digest
        {
            return Err(crate::error::invalid(
                WindowsOperation::Validate,
                "managed network grant identity is invalid",
            ));
        }
        self.managed_network_grant = Some((canonical, digest));
        Ok(self)
    }

    /// Binds the canonical run command-state root used to derive the managed cache.
    ///
    /// # Errors
    /// Rejects an aliased/non-directory root or zero grant identity.
    pub fn with_managed_network_cache_root(
        mut self,
        root: PathBuf,
        grant_digest: Sha256Digest,
    ) -> Result<Self, WindowsError> {
        if !root.is_absolute()
            || !root.is_dir()
            || std::fs::canonicalize(&root).ok().as_ref() != Some(&root)
            || grant_digest == Sha256Digest::new([0; 32])
        {
            return Err(crate::error::invalid(
                WindowsOperation::Validate,
                "managed network cache root is invalid",
            ));
        }
        self.managed_network_cache_root = Some((root, grant_digest));
        Ok(self)
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
    /// Returns exact external roots admitted for checked read/write rules.
    #[must_use]
    pub fn writable_inputs(&self) -> &[WindowsPath] {
        &self.writable_inputs
    }
    /// Returns the stable nonsensitive managed-network grant identity.
    #[must_use]
    pub fn managed_network_grant(&self) -> Option<(&[u8], Sha256Digest)> {
        self.managed_network_grant
            .as_ref()
            .map(|(canonical, digest)| (canonical.as_slice(), *digest))
    }
    /// Returns the exact run command-state root and grant binding for the managed cache.
    #[must_use]
    pub fn managed_network_cache_root(&self) -> Option<(&Path, Sha256Digest)> {
        self.managed_network_cache_root
            .as_ref()
            .map(|(root, digest)| (root.as_path(), *digest))
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

    pub(crate) fn validate_managed_network_identity(&self) -> Result<(), WindowsError> {
        let grant_digest = self.managed_network_grant.as_ref().map(|(_, digest)| *digest);
        let cache_digest = self
            .managed_network_cache_root
            .as_ref()
            .map(|(_, digest)| *digest);
        if self.proxy.is_some() != self.managed_network_grant.is_some()
            || cache_digest.is_some_and(|digest| Some(digest) != grant_digest)
        {
            return Err(crate::error::invalid(
                WindowsOperation::Validate,
                "managed proxy and semantic grant identity must be configured together",
            ));
        }
        Ok(())
    }
}
