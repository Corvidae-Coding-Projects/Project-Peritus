//! Inert installed-resource preparation configuration.

use std::path::PathBuf;

use peritus_network::ManagedProxyPreparation;
use peritus_secrets::SecretPreparation;
use peritus_types::Sha256Digest;

use crate::{MacosError, MacosOperation, error};

/// Installation paths and inert protected-resource preparations for one backend instance.
#[derive(Debug)]
pub struct PreparationConfig {
    pub(super) helper_path: PathBuf,
    pub(super) seatbelt_path: PathBuf,
    pub(super) additional_protected_roots: Vec<PathBuf>,
    pub(super) proxy: Option<ManagedProxyPreparation>,
    pub(super) managed_network_grant: Option<(Vec<u8>, Sha256Digest)>,
    pub(super) managed_network_cache_root: Option<(PathBuf, Sha256Digest)>,
    pub(super) secrets: Option<SecretPreparation>,
}

impl PreparationConfig {
    /// Creates checked installation configuration.
    ///
    /// # Errors
    /// Rejects non-absolute executable/protected paths or excessive protected roots. The paths are
    /// re-resolved during authorized preparation. Proxy and secret values remain inert until then.
    pub fn new(
        helper_path: PathBuf,
        seatbelt_path: PathBuf,
        mut additional_protected_roots: Vec<PathBuf>,
        proxy: Option<ManagedProxyPreparation>,
        secrets: Option<SecretPreparation>,
    ) -> Result<Self, MacosError> {
        if !helper_path.is_absolute() || !seatbelt_path.is_absolute() {
            return Err(error::invalid(
                MacosOperation::Validate,
                "helper and Seatbelt executable paths must be absolute",
            ));
        }
        if additional_protected_roots.len() > 256 {
            return Err(error::limited(
                MacosOperation::Validate,
                "too many protected metadata roots",
            ));
        }
        if additional_protected_roots.iter().any(|path| !path.is_absolute()) {
            return Err(error::invalid(
                MacosOperation::Validate,
                "protected metadata roots must be absolute",
            ));
        }
        additional_protected_roots.sort();
        additional_protected_roots.dedup();
        Ok(Self {
            helper_path,
            seatbelt_path,
            additional_protected_roots,
            proxy,
            managed_network_grant: None,
            managed_network_cache_root: None,
            secrets,
        })
    }

    /// Binds the nonsensitive trusted-host grant identity used for retained reconstruction.
    ///
    /// # Errors
    /// Rejects empty, unframeable, or digest-mismatched canonical bytes.
    pub fn with_managed_network_grant(
        mut self,
        canonical: Vec<u8>,
        digest: Sha256Digest,
    ) -> Result<Self, MacosError> {
        if canonical.is_empty()
            || u32::try_from(canonical.len()).is_err()
            || peritus_codec::sha256(&canonical) != digest
        {
            return Err(error::invalid(
                MacosOperation::Validate,
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
    ) -> Result<Self, MacosError> {
        if !root.is_absolute()
            || !root.is_dir()
            || std::fs::canonicalize(&root).ok().as_ref() != Some(&root)
            || grant_digest == Sha256Digest::new([0; 32])
        {
            return Err(error::invalid(
                MacosOperation::Validate,
                "managed network cache root is invalid",
            ));
        }
        self.managed_network_cache_root = Some((root, grant_digest));
        Ok(self)
    }

    /// Returns the installed helper path.
    #[must_use]
    pub fn helper_path(&self) -> &std::path::Path {
        &self.helper_path
    }

    /// Returns the checked Seatbelt executable path.
    #[must_use]
    pub fn seatbelt_path(&self) -> &std::path::Path {
        &self.seatbelt_path
    }

    /// Returns protected metadata roots in canonical input order.
    #[must_use]
    pub fn additional_protected_roots(&self) -> &[PathBuf] {
        &self.additional_protected_roots
    }

    /// Returns inert managed-proxy preparation configuration, if configured.
    #[must_use]
    pub const fn proxy(&self) -> Option<&ManagedProxyPreparation> {
        self.proxy.as_ref()
    }

    /// Returns inert secret preparation configuration, if configured.
    #[must_use]
    pub const fn secrets(&self) -> Option<&SecretPreparation> {
        self.secrets.as_ref()
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
    pub fn managed_network_cache_root(&self) -> Option<(&std::path::Path, Sha256Digest)> {
        self.managed_network_cache_root
            .as_ref()
            .map(|(root, digest)| (root.as_path(), *digest))
    }

    pub(super) fn validate_managed_network_identity(&self) -> Result<(), MacosError> {
        let grant_digest = self.managed_network_grant.as_ref().map(|(_, digest)| *digest);
        let cache_digest = self
            .managed_network_cache_root
            .as_ref()
            .map(|(_, digest)| *digest);
        if self.proxy.is_some() != self.managed_network_grant.is_some()
            || cache_digest.is_some_and(|digest| Some(digest) != grant_digest)
        {
            return Err(error::invalid(
                MacosOperation::Validate,
                "managed proxy and semantic grant identity must be configured together",
            ));
        }
        Ok(())
    }
}
