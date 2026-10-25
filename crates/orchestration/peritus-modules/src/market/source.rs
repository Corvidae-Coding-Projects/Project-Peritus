//! Additive source catalog and atomic current-client snapshot.

use super::{Catalog, hex, plain, safe_path};
use crate::{Error, invalid, publish};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::PathBuf};

pub const CATALOG_SET_SCHEMA: u32 = 2;

/// One source-only native Rust package advertised alongside the legacy executable catalog.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEntry {
    /// Stable module identity.
    pub id: String,
    /// Human-readable module name.
    pub name: String,
    /// Source package semantic version.
    pub version: String,
    /// Human-readable purpose.
    pub description: String,
    /// Repository-relative source package root.
    pub path: String,
    /// Canonical source tree digest, excluding the self-referential source catalog.
    pub source_digest: String,
    /// Exact Cargo.lock digest.
    pub lockfile_digest: String,
    /// Pinned rustup toolchain identity.
    pub toolchain: String,
    /// Exact Linux compilation target.
    pub target: String,
    /// Cargo package selected by the recipe.
    pub package: String,
    /// Expected release executable filename.
    pub executable: String,
    /// Repository-relative package manifest template.
    pub manifest_template: String,
    /// Closed host-owned build recipe identifier.
    pub recipe: String,
    /// Optional discovery labels; these grant no authority.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Independently versioned source inventory. Legacy clients never parse this file.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCatalog {
    /// Source catalog schema, currently one.
    pub schema: u32,
    /// Human-readable catalog name.
    pub name: String,
    /// Independently versioned source packages.
    pub modules: Vec<SourceEntry>,
}

impl SourceCatalog {
    /// Parses and validates an inert source catalog without building anything.
    ///
    /// # Errors
    ///
    /// Returns an error when the catalog is too large, malformed, unsupported, or contains an
    /// invalid or repeated package identity.
    pub fn parse(text: &str) -> Result<Self, Error> {
        if text.len() > 4 * 1024 * 1024 {
            return Err(invalid("source catalog exceeds 4 MiB"));
        }
        let catalog: Self = toml::from_str(text)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.schema != 1 || !plain(&self.name) {
            return Err(invalid("unsupported or invalid source catalog"));
        }
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for entry in &self.modules {
            if !crate::definition::identifier(&entry.id)
                || !ids.insert(&entry.id)
                || !paths.insert(&entry.path)
                || !crate::definition::version(&entry.version)
                || !plain(&entry.name)
                || !plain(&entry.description)
                || !safe_path(&entry.path)
                || !safe_path(&entry.manifest_template)
                || !identifier(&entry.package)
                || !identifier(&entry.executable)
                || !identifier(&entry.recipe)
                || !target(&entry.target)
                || !toolchain(&entry.toolchain)
                || !hex(&entry.source_digest, 64)
                || !hex(&entry.lockfile_digest, 64)
                || entry.tags.iter().any(|tag| !plain(tag))
            {
                return Err(invalid("invalid/repeated source package metadata or identity"));
            }
            if entry.recipe != "cargo-release-v1"
                || !entry.manifest_template.starts_with(&format!("{}/", entry.path))
            {
                return Err(invalid("unsupported source build recipe or template location"));
            }
        }
        Ok(())
    }
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn target(value: &str) -> bool {
    identifier(value) && value.contains("-unknown-linux-")
}

fn toolchain(value: &str) -> bool {
    identifier(value)
        && value.split_once('-').is_some_and(|(version, _)| crate::definition::version(version))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSet {
    pub schema: u32,
    pub commit: String,
    pub fetched_at_unix: u64,
    pub executable: Catalog,
    pub source: Option<SourceCatalog>,
    pub local_checkout: Option<PathBuf>,
}

impl CatalogSet {
    pub(crate) fn new(
        commit: String,
        fetched_at_unix: u64,
        executable: Catalog,
        source: Option<SourceCatalog>,
        local_checkout: Option<PathBuf>,
    ) -> Result<Self, Error> {
        if !hex(&commit, 40) || local_checkout.as_ref().is_some_and(|path| !path.is_absolute()) {
            return Err(invalid("invalid source catalog snapshot identity"));
        }
        executable.validate()?;
        if let Some(source) = &source {
            source.validate()?;
        }
        Ok(Self {
            schema: CATALOG_SET_SCHEMA,
            commit,
            fetched_at_unix,
            executable,
            source,
            local_checkout,
        })
    }

    pub(crate) fn load(root: &std::path::Path) -> Result<Self, Error> {
        let value: Self = serde_json::from_slice(&fs::read(root.join("market/catalog-set.json"))?)?;
        if value.schema != CATALOG_SET_SCHEMA || !hex(&value.commit, 40) {
            return Err(invalid("invalid market catalog-set snapshot"));
        }
        value.executable.validate()?;
        if let Some(source) = &value.source {
            source.validate()?;
        }
        Ok(value)
    }

    pub(crate) fn publish(&self, root: &std::path::Path) -> Result<(), Error> {
        publish(&root.join("market/catalog-set.json"), &serde_json::to_vec_pretty(self)?)
    }
}
