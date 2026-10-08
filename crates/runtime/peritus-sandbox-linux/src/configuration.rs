//! Immutable installation configuration with inert resource owners.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery, ProbeRequest};
use peritus_types::Sha256Digest;
use std::path::{Path, PathBuf};

/// Immutable Linux backend installation configuration.
#[derive(Debug)]
pub struct LinuxBackendConfig {
    pub(crate) workspace_root: PathBuf,
    pub(crate) protected_roots: Vec<PathBuf>,
    pub(crate) probe_request: ProbeRequest,
    pub(crate) managed_proxy: Option<peritus_network::ManagedProxyPreparation>,
    pub(crate) managed_network_grant: Option<(Vec<u8>, Sha256Digest)>,
    pub(crate) managed_network_cache_root: Option<(PathBuf, Sha256Digest)>,
    pub(crate) secrets: Option<peritus_secrets::SecretPreparation>,
    pub(crate) private_filesystem: bool,
    pub(crate) read_only_replacements: Vec<(PathBuf, PathBuf)>,
    pub(crate) writable_inputs: Vec<PathBuf>,
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
            managed_network_grant: None,
            managed_network_cache_root: None,
            secrets: None,
            private_filesystem: false,
            read_only_replacements: Vec::new(),
            writable_inputs: Vec::new(),
        })
    }
    /// Omits the ordinary read-only host tree, mounting only declared files and runtime inputs.
    /// This stricter view is suitable for credential-free auxiliary inference. The caller must
    /// explicitly grant the target's runtime library trees; no writable or network grant is added.
    #[must_use]
    pub const fn with_private_filesystem(mut self) -> Self {
        self.private_filesystem = true;
        self
    }
    /// Replaces exact logical read-only paths with immutable native sources in the private mount.
    ///
    /// Each pair is `(source, target)`. The checked sandbox contract and target command continue
    /// to name `target`; preparation mounts the canonical regular-file `source` at that exact
    /// namespace path.
    ///
    /// # Errors
    /// Rejects aliases, non-files, relative paths, identity mappings, or duplicate targets.
    pub fn with_read_only_replacements(
        mut self,
        mut replacements: Vec<(PathBuf, PathBuf)>,
    ) -> Result<Self, LinuxError> {
        replacements.sort_by(|left, right| left.1.cmp(&right.1));
        for (index, (source, target)) in replacements.iter().enumerate() {
            if !source.is_absolute()
                || !target.is_absolute()
                || source == target
                || !source.is_file()
                || !target.is_file()
                || std::fs::canonicalize(source).ok().as_ref() != Some(source)
                || std::fs::canonicalize(target).ok().as_ref() != Some(target)
                || !replacement_source_is_read_only(source)
                || replacements[..index]
                    .iter()
                    .any(|(_, prior_target)| prior_target == target)
            {
                return Err(LinuxError::new(
                    LinuxErrorKind::InvalidPlan,
                    LinuxOperation::Prepare,
                    LinuxRecovery::CorrectRequest,
                    "read-only replacement must bind one canonical file to one absolute target",
                ));
            }
        }
        self.read_only_replacements = replacements;
        Ok(self)
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

    /// Binds the nonsensitive trusted-host grant identity used to reconstruct a fresh proxy.
    ///
    /// # Errors
    /// Rejects empty, unframeable, or digest-mismatched canonical bytes.
    pub fn with_managed_network_grant(
        mut self,
        canonical: Vec<u8>,
        digest: Sha256Digest,
    ) -> Result<Self, LinuxError> {
        if canonical.is_empty()
            || u32::try_from(canonical.len()).is_err()
            || peritus_codec::sha256(&canonical) != digest
        {
            return Err(LinuxError::new(
                LinuxErrorKind::InvalidPlan,
                LinuxOperation::Prepare,
                LinuxRecovery::CorrectRequest,
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
    ) -> Result<Self, LinuxError> {
        if !root.is_absolute()
            || !root.is_dir()
            || std::fs::canonicalize(&root).ok().as_ref() != Some(&root)
            || grant_digest == Sha256Digest::new([0; 32])
        {
            return Err(LinuxError::new(
                LinuxErrorKind::InvalidPlan,
                LinuxOperation::Prepare,
                LinuxRecovery::CorrectRequest,
                "managed network cache root is invalid",
            ));
        }
        self.managed_network_cache_root = Some((root, grant_digest));
        Ok(self)
    }

    /// Admits exact canonical host-owned writable roots outside the candidate workspace.
    ///
    /// The roots carry no permission by themselves; they only allow matching checked filesystem
    /// rules to be projected as writable mounts after C2 authorization.
    ///
    /// # Errors
    /// Rejects absent, aliased, overlapping, workspace, or protected paths.
    pub fn with_writable_inputs(
        mut self,
        mut inputs: Vec<PathBuf>,
    ) -> Result<Self, LinuxError> {
        inputs.sort();
        for (index, input) in inputs.iter().enumerate() {
            let protected_overlap = self.protected_roots.iter().any(|protected| {
                let protected = if protected.is_absolute() {
                    protected.clone()
                } else {
                    self.workspace_root.join(protected)
                };
                protected.starts_with(input) || input.starts_with(protected)
            });
            if !input.is_absolute()
                || !input.is_dir()
                || std::fs::canonicalize(input).ok().as_ref() != Some(input)
                || input.starts_with(&self.workspace_root)
                || self.workspace_root.starts_with(input)
                || protected_overlap
                || inputs[..index]
                    .iter()
                    .any(|prior| prior.starts_with(input) || input.starts_with(prior))
            {
                return Err(LinuxError::new(
                    LinuxErrorKind::InvalidPlan,
                    LinuxOperation::Prepare,
                    LinuxRecovery::CorrectRequest,
                    "writable input must be one disjoint canonical host directory",
                ));
            }
        }
        self.writable_inputs = inputs;
        Ok(self)
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
    /// Returns exact external roots admitted for checked writable mounts.
    #[must_use]
    pub fn writable_inputs(&self) -> &[PathBuf] {
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

    pub(crate) fn validate_managed_network_identity(&self) -> Result<(), LinuxError> {
        let grant_digest = self.managed_network_grant.as_ref().map(|(_, digest)| *digest);
        let cache_digest = self
            .managed_network_cache_root
            .as_ref()
            .map(|(_, digest)| *digest);
        if self.managed_proxy.is_some() != self.managed_network_grant.is_some()
            || cache_digest.is_some_and(|digest| Some(digest) != grant_digest)
        {
            return Err(LinuxError::new(
                LinuxErrorKind::InvalidPlan,
                LinuxOperation::Prepare,
                LinuxRecovery::CorrectRequest,
                "managed proxy and semantic grant identity must be configured together",
            ));
        }
        Ok(())
    }
}

#[cfg(unix)]
fn replacement_source_is_read_only(source: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(source)
        .is_ok_and(|metadata| metadata.permissions().mode() & 0o222 == 0)
}

#[cfg(not(unix))]
fn replacement_source_is_read_only(source: &Path) -> bool {
    std::fs::metadata(source).is_ok_and(|metadata| metadata.permissions().readonly())
}
