//! Bounded filesystem discovery and exact artifact identification.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read as _,
    path::{Path, PathBuf},
};

use peritus_plugin_sdk::{ManifestDigest, PluginId, PluginManifest, PluginVersion};
use sha2::{Digest as _, Sha256};

use crate::{HostError, HostFailureClass, RecoveryDisposition};

const MANIFEST_NAME: &str = "peritus-plugin.toml";

/// Discovery cardinality and artifact-size ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscoveryLimits {
    /// Maximum roots to inspect.
    pub roots: usize,
    /// Maximum plugin manifests accepted across roots.
    pub plugins: usize,
    /// Maximum manifest size in bytes.
    pub manifest_bytes: u64,
    /// Maximum executable/module artifact size in bytes.
    pub artifact_bytes: u64,
}

impl DiscoveryLimits {
    /// Conservative production discovery limits.
    pub const PRODUCTION: Self = Self {
        roots: 16,
        plugins: 256,
        manifest_bytes: 256 * 1024,
        artifact_bytes: 512 * 1024 * 1024,
    };
}

/// Validated manifest plus exact resolved artifact identity.
#[derive(Clone, Debug)]
pub struct DiscoveredPlugin {
    manifest: PluginManifest,
    manifest_path: PathBuf,
    root: PathBuf,
    artifact_path: PathBuf,
    manifest_digest: ManifestDigest,
    artifact_sha256: [u8; 32],
}

impl DiscoveredPlugin {
    /// Borrows the validated manifest.
    #[must_use]
    pub const fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Borrows the exact manifest path.
    #[must_use]
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    /// Borrows the canonical plugin directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Borrows the canonical executable/module artifact path.
    #[must_use]
    pub fn artifact_path(&self) -> &Path {
        &self.artifact_path
    }

    /// Returns the complete canonical manifest digest.
    #[must_use]
    pub const fn manifest_digest(&self) -> ManifestDigest {
        self.manifest_digest
    }

    /// Returns the exact artifact SHA-256.
    #[must_use]
    pub const fn artifact_sha256(&self) -> [u8; 32] {
        self.artifact_sha256
    }
}

/// Canonical duplicate-free discovered plugin catalog.
#[derive(Clone, Debug, Default)]
pub struct PluginCatalog {
    entries: BTreeMap<(PluginId, PluginVersion), DiscoveredPlugin>,
}

impl PluginCatalog {
    /// Looks up one exact plugin version.
    #[must_use]
    pub fn get(&self, id: &PluginId, version: PluginVersion) -> Option<&DiscoveredPlugin> {
        self.entries.get(&(id.clone(), version))
    }

    /// Iterates plugins in canonical identity/version order.
    pub fn iter(
        &self,
    ) -> std::collections::btree_map::Values<'_, (PluginId, PluginVersion), DiscoveredPlugin> {
        self.entries.values()
    }

    /// Returns the number of discovered versions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether no plugins were discovered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<'catalog> IntoIterator for &'catalog PluginCatalog {
    type Item = &'catalog DiscoveredPlugin;
    type IntoIter =
        std::collections::btree_map::Values<'catalog, (PluginId, PluginVersion), DiscoveredPlugin>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.values()
    }
}

/// Discovers immediate plugin directories beneath configured roots.
///
/// Symlinked roots, plugin directories, manifests, and artifacts are rejected. Every accepted
/// artifact must resolve beneath its canonical plugin directory.
///
/// # Errors
///
/// Returns a typed discovery failure for I/O, limits, manifest invalidity, duplicate identities,
/// symlink traversal, or missing artifacts.
pub fn discover(roots: &[PathBuf], limits: DiscoveryLimits) -> Result<PluginCatalog, HostError> {
    if roots.len() > limits.roots {
        return Err(discovery_error("plugin discovery root count exceeds its bound"));
    }
    let mut catalog = PluginCatalog::default();
    for configured_root in roots {
        reject_symlink(configured_root, "plugin discovery root is a symlink")?;
        let root = fs::canonicalize(configured_root)
            .map_err(|error| discovery_source("canonicalize plugin discovery root", error))?;
        let mut entries = fs::read_dir(&root)
            .map_err(|error| discovery_source("read plugin discovery root", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| discovery_source("enumerate plugin discovery root", error))?;
        entries.sort_unstable_by_key(fs::DirEntry::file_name);
        for entry in entries {
            if !entry
                .file_type()
                .map_err(|error| discovery_source("inspect plugin directory entry", error))?
                .is_dir()
            {
                continue;
            }
            let directory = entry.path();
            reject_symlink(&directory, "plugin directory is a symlink")?;
            let manifest_path = directory.join(MANIFEST_NAME);
            if !manifest_path.exists() {
                continue;
            }
            if catalog.len() >= limits.plugins {
                return Err(discovery_error("discovered plugin count exceeds its bound"));
            }
            let discovered = load_plugin(&directory, &manifest_path, limits)?;
            let key = (discovered.manifest().id().clone(), discovered.manifest().version());
            if catalog.entries.insert(key, discovered).is_some() {
                return Err(discovery_error("duplicate plugin identity and version"));
            }
        }
    }
    Ok(catalog)
}

fn load_plugin(
    directory: &Path,
    manifest_path: &Path,
    limits: DiscoveryLimits,
) -> Result<DiscoveredPlugin, HostError> {
    reject_symlink(manifest_path, "plugin manifest is a symlink")?;
    let manifest_metadata = fs::metadata(manifest_path)
        .map_err(|error| discovery_source("inspect plugin manifest", error))?;
    if !manifest_metadata.is_file() || manifest_metadata.len() > limits.manifest_bytes {
        return Err(discovery_error("plugin manifest is not a bounded regular file"));
    }
    let manifest_text = fs::read_to_string(manifest_path)
        .map_err(|error| discovery_source("read plugin manifest", error))?;
    let manifest = PluginManifest::parse_toml(&manifest_text).map_err(|error| {
        HostError::with_source(
            HostFailureClass::Discovery,
            RecoveryDisposition::CorrectRequest,
            "parse plugin manifest",
            error.to_string(),
            error,
        )
    })?;
    let root = fs::canonicalize(directory)
        .map_err(|error| discovery_source("canonicalize plugin directory", error))?;
    let unresolved_artifact = directory.join(manifest.entrypoint().artifact());
    reject_symlink(&unresolved_artifact, "plugin artifact is a symlink")?;
    let artifact_path = fs::canonicalize(&unresolved_artifact)
        .map_err(|error| discovery_source("canonicalize plugin artifact", error))?;
    if !artifact_path.starts_with(&root) {
        return Err(discovery_error("plugin artifact escapes its plugin directory"));
    }
    let artifact_metadata = fs::metadata(&artifact_path)
        .map_err(|error| discovery_source("inspect plugin artifact", error))?;
    if !artifact_metadata.is_file() || artifact_metadata.len() > limits.artifact_bytes {
        return Err(discovery_error("plugin artifact is not a bounded regular file"));
    }
    let artifact_sha256 = hash_file(&artifact_path, limits.artifact_bytes)?;
    let manifest_digest = manifest.digest().map_err(|error| {
        HostError::with_source(
            HostFailureClass::Discovery,
            RecoveryDisposition::CorrectRequest,
            "digest plugin manifest",
            error.to_string(),
            error,
        )
    })?;
    Ok(DiscoveredPlugin {
        manifest,
        manifest_path: manifest_path.to_path_buf(),
        root,
        artifact_path,
        manifest_digest,
        artifact_sha256,
    })
}

fn hash_file(path: &Path, maximum: u64) -> Result<[u8; 32], HostError> {
    let mut file =
        File::open(path).map_err(|error| discovery_source("open plugin artifact", error))?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| discovery_source("hash plugin artifact", error))?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or_else(|| discovery_error("plugin artifact size overflowed"))?;
        if total > maximum {
            return Err(discovery_error("plugin artifact exceeds its byte bound"));
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().into())
}

fn reject_symlink(path: &Path, detail: &'static str) -> Result<(), HostError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| discovery_source("inspect plugin path", error))?;
    if metadata.file_type().is_symlink() { Err(discovery_error(detail)) } else { Ok(()) }
}

fn discovery_error(detail: &'static str) -> HostError {
    HostError::new(
        HostFailureClass::Discovery,
        RecoveryDisposition::CorrectRequest,
        "discover plugins",
        detail,
    )
}

fn discovery_source(operation: &'static str, error: std::io::Error) -> HostError {
    HostError::with_source(
        HostFailureClass::Discovery,
        RecoveryDisposition::CorrectRequest,
        operation,
        error.to_string(),
        error,
    )
}
