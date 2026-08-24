//! Inert installed-resource preparation configuration.

use std::path::PathBuf;

use peritus_network::ManagedProxyPreparation;
use peritus_secrets::SecretPreparation;

use crate::{MacosError, MacosOperation, error};

/// Installation paths and inert protected-resource preparations for one backend instance.
#[derive(Debug)]
pub struct PreparationConfig {
    pub(super) helper_path: PathBuf,
    pub(super) seatbelt_path: PathBuf,
    pub(super) additional_protected_roots: Vec<PathBuf>,
    pub(super) proxy: Option<ManagedProxyPreparation>,
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
        Ok(Self { helper_path, seatbelt_path, additional_protected_roots, proxy, secrets })
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
}
