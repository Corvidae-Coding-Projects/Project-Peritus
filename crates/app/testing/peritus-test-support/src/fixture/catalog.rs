//! Compatibility catalog layout and required-kind coverage.

use super::{
    FixtureCase, FixtureError, FixtureErrorKind, FixtureKind, FixtureName, FixtureVersion,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Policy for interpreting an entirely empty compatibility root.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CompatibilityPolicy {
    /// Permit an explicitly empty catalog before any schema has been released.
    AllowEmptyPreRelease,
    /// Require at least one released surface/version group.
    RequireFixtures,
}

/// Observable result of applying compatibility coverage policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CompatibilityCoverage {
    /// No released fixtures exist and explicit pre-release policy allowed that absence.
    EmptyPreRelease,
    /// Every present surface/version group contains all mandatory fixture kinds.
    Covered,
}

/// A loaded catalog of canonical compatibility fixture cases.
#[derive(Clone, Debug)]
pub struct FixtureCatalog {
    root: PathBuf,
    cases: Vec<FixtureCase>,
}

impl FixtureCatalog {
    /// Loads exact `surface/version/case` directories and verifies every case.
    ///
    /// Coverage is checked separately by [`Self::verify_compatibility_coverage`].
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`] for unsafe layout, identity mismatch, I/O, or invalid cases.
    pub fn load(root: impl AsRef<Path>) -> Result<Self, FixtureError> {
        let root = root.as_ref().to_path_buf();
        ensure_real_directory(&root)?;
        let mut cases = Vec::new();
        for surface_path in child_directories(&root)? {
            let surface = FixtureName::new(file_name(&surface_path)?)?;
            let version_paths = child_directories(&surface_path)?;
            if version_paths.is_empty() {
                return Err(FixtureError::at(
                    FixtureErrorKind::IncompleteCoverage,
                    &surface_path,
                    "compatibility surface contained no version directories",
                ));
            }
            for version_path in version_paths {
                let version = FixtureVersion::new(file_name(&version_path)?)?;
                let case_paths = child_directories(&version_path)?;
                if case_paths.is_empty() {
                    return Err(FixtureError::at(
                        FixtureErrorKind::IncompleteCoverage,
                        &version_path,
                        "compatibility surface version contained no case directories",
                    ));
                }
                for case_path in case_paths {
                    let case_name = FixtureName::new(file_name(&case_path)?)?;
                    let fixture = FixtureCase::load(&case_path)?;
                    let manifest = fixture.manifest();
                    if manifest.surface() != &surface
                        || manifest.surface_version() != &version
                        || manifest.case() != &case_name
                    {
                        return Err(FixtureError::at(
                            FixtureErrorKind::LayoutMismatch,
                            &case_path,
                            "surface, version, or case directory disagreed with fixture.toml",
                        ));
                    }
                    cases.push(fixture);
                }
            }
        }
        cases.sort_by(|left, right| {
            let left = left.manifest();
            let right = right.manifest();
            (left.surface(), left.surface_version(), left.case()).cmp(&(
                right.surface(),
                right.surface_version(),
                right.case(),
            ))
        });
        Ok(Self { root, cases })
    }

    /// Returns the compatibility root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns cases in deterministic surface/version/case order.
    #[must_use]
    pub fn cases(&self) -> &[FixtureCase] {
        &self.cases
    }

    /// Verifies mandatory minimal, realistic, corrupt, and adversarial coverage per group.
    ///
    /// An explicitly allowed empty pre-release catalog returns
    /// [`CompatibilityCoverage::EmptyPreRelease`], never covered status.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureErrorKind::EmptyCatalog`] when policy requires released fixtures, or
    /// [`FixtureErrorKind::IncompleteCoverage`] when any nonempty group lacks a mandated kind.
    pub fn verify_compatibility_coverage(
        &self,
        policy: CompatibilityPolicy,
    ) -> Result<CompatibilityCoverage, FixtureError> {
        if self.cases.is_empty() {
            return if policy == CompatibilityPolicy::AllowEmptyPreRelease {
                Ok(CompatibilityCoverage::EmptyPreRelease)
            } else {
                Err(FixtureError::at(
                    FixtureErrorKind::EmptyCatalog,
                    &self.root,
                    "released compatibility catalog contained no fixtures",
                ))
            };
        }
        let mut groups: BTreeMap<(&FixtureName, &FixtureVersion), BTreeSet<FixtureKind>> =
            BTreeMap::new();
        for case in &self.cases {
            let manifest = case.manifest();
            groups
                .entry((manifest.surface(), manifest.surface_version()))
                .or_default()
                .insert(manifest.kind());
        }
        let required = [
            FixtureKind::Minimal,
            FixtureKind::Realistic,
            FixtureKind::Corrupt,
            FixtureKind::Adversarial,
        ];
        for ((surface, version), kinds) in groups {
            for kind in required {
                if !kinds.contains(&kind) {
                    return Err(FixtureError::at(
                        FixtureErrorKind::IncompleteCoverage,
                        &self.root,
                        format!(
                            "{}/{} lacks required {kind:?} fixture",
                            surface.as_str(),
                            version.as_str()
                        ),
                    ));
                }
            }
        }
        Ok(CompatibilityCoverage::Covered)
    }
}

fn ensure_real_directory(path: &Path) -> Result<(), FixtureError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| {
        FixtureError::sourced(
            FixtureErrorKind::Io,
            path,
            "could not inspect compatibility root",
            source,
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(FixtureError::at(
            FixtureErrorKind::UnsafeFileType,
            path,
            "catalog level must be a real directory",
        ));
    }
    Ok(())
}

fn child_directories(parent: &Path) -> Result<Vec<PathBuf>, FixtureError> {
    let entries = fs::read_dir(parent).map_err(|source| {
        FixtureError::sourced(
            FixtureErrorKind::Io,
            parent,
            "could not enumerate catalog level",
            source,
        )
    })?;
    let mut directories = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|source| {
                FixtureError::sourced(
                    FixtureErrorKind::Io,
                    parent,
                    "could not read catalog entry",
                    source,
                )
            })?
            .path();
        let metadata = fs::symlink_metadata(&path).map_err(|source| {
            FixtureError::sourced(
                FixtureErrorKind::Io,
                &path,
                "could not inspect catalog entry",
                source,
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(FixtureError::at(
                FixtureErrorKind::UnsafeFileType,
                path,
                "catalog levels may contain only real directories",
            ));
        }
        directories.push(path);
    }
    directories.sort();
    Ok(directories)
}

fn file_name(path: &Path) -> Result<String, FixtureError> {
    path.file_name().and_then(|name| name.to_str()).map(str::to_owned).ok_or_else(|| {
        FixtureError::at(
            FixtureErrorKind::InvalidName,
            path,
            "catalog directory name was not UTF-8",
        )
    })
}
