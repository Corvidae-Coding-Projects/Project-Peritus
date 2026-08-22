//! Digest and containment verification for one fixture case.

use super::{FixtureError, FixtureErrorKind, FixtureManifest, FixturePath};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const MANIFEST_NAME: &str = "fixture.toml";

/// One loaded and fully verified compatibility fixture case.
#[derive(Clone, Debug)]
pub struct FixtureCase {
    directory: PathBuf,
    manifest: FixtureManifest,
}

impl FixtureCase {
    /// Loads a case and verifies manifest syntax, file containment, inventory, and digests.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`] for any invalid, missing, extra, unsafe, or digest-divergent input.
    pub fn load(directory: impl AsRef<Path>) -> Result<Self, FixtureError> {
        let directory = directory.as_ref().to_path_buf();
        ensure_directory(&directory)?;
        let manifest_path = directory.join(MANIFEST_NAME);
        ensure_regular_file(&manifest_path, FixtureErrorKind::MissingFile)?;
        let contents = fs::read_to_string(&manifest_path).map_err(|source| {
            FixtureError::sourced(
                FixtureErrorKind::Io,
                &manifest_path,
                "could not read fixture manifest as UTF-8",
                source,
            )
        })?;
        let manifest = FixtureManifest::parse(&contents, &manifest_path)?;
        let fixture = Self { directory, manifest };
        fixture.verify()?;
        Ok(fixture)
    }

    /// Returns the case directory.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Returns the validated manifest.
    #[must_use]
    pub const fn manifest(&self) -> &FixtureManifest {
        &self.manifest
    }

    /// Re-verifies current file inventory, types, and exact bytes against the manifest.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`] when filesystem state no longer matches the loaded manifest.
    pub fn verify(&self) -> Result<(), FixtureError> {
        ensure_directory(&self.directory)?;
        let declared: BTreeSet<_> =
            self.manifest.files().iter().map(|file| file.path().clone()).collect();
        for file in self.manifest.files() {
            let bytes = read_contained(&self.directory, file.path())?;
            let actual = digest(&bytes);
            if actual != file.sha256() {
                return Err(FixtureError::at(
                    FixtureErrorKind::DigestMismatch,
                    self.directory.join(file.path().as_path()),
                    format!("digest mismatch for {}", file.path().as_str()),
                ));
            }
        }
        let actual = collect_payload_files(&self.directory)?;
        if let Some(path) = declared.difference(&actual).next() {
            return Err(FixtureError::at(
                FixtureErrorKind::MissingFile,
                self.directory.join(path.as_path()),
                format!("manifested file {} is missing", path.as_str()),
            ));
        }
        if let Some(path) = actual.difference(&declared).next() {
            return Err(FixtureError::at(
                FixtureErrorKind::UnexpectedFile,
                self.directory.join(path.as_path()),
                format!("unlisted fixture file {} exists", path.as_str()),
            ));
        }
        Ok(())
    }

    /// Reads one manifested file and re-verifies its digest.
    ///
    /// Bytes are returned exactly; line endings and invalid UTF-8 are never normalized.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`] for an unlisted path, unsafe file, I/O failure, or digest mismatch.
    pub fn read(&self, path: &FixturePath) -> Result<Vec<u8>, FixtureError> {
        let Some(file) = self.manifest.files().iter().find(|file| file.path() == path) else {
            return Err(FixtureError::at(
                FixtureErrorKind::UnexpectedFile,
                self.directory.join(path.as_path()),
                "requested path is not declared by the fixture manifest",
            ));
        };
        let bytes = read_contained(&self.directory, path)?;
        if digest(&bytes) != file.sha256() {
            return Err(FixtureError::at(
                FixtureErrorKind::DigestMismatch,
                self.directory.join(path.as_path()),
                "requested fixture bytes diverged from the manifest",
            ));
        }
        Ok(bytes)
    }
}

fn ensure_directory(path: &Path) -> Result<(), FixtureError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| {
        FixtureError::sourced(
            FixtureErrorKind::Io,
            path,
            "could not inspect fixture directory",
            source,
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(FixtureError::at(
            FixtureErrorKind::UnsafeFileType,
            path,
            "fixture case root must be a real directory",
        ));
    }
    Ok(())
}

fn ensure_regular_file(path: &Path, missing_kind: FixtureErrorKind) -> Result<(), FixtureError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| {
        let kind = if source.kind() == std::io::ErrorKind::NotFound {
            missing_kind
        } else {
            FixtureErrorKind::Io
        };
        FixtureError::sourced(kind, path, "could not inspect fixture file", source)
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(FixtureError::at(
            FixtureErrorKind::UnsafeFileType,
            path,
            "fixture payload must be a regular non-symlink file",
        ));
    }
    Ok(())
}

fn read_contained(root: &Path, relative: &FixturePath) -> Result<Vec<u8>, FixtureError> {
    let mut current = root.to_path_buf();
    let segments: Vec<_> = relative.as_str().split('/').collect();
    for (index, segment) in segments.iter().enumerate() {
        current.push(segment);
        let metadata = fs::symlink_metadata(&current).map_err(|source| {
            let kind = if source.kind() == std::io::ErrorKind::NotFound {
                FixtureErrorKind::MissingFile
            } else {
                FixtureErrorKind::Io
            };
            FixtureError::sourced(
                kind,
                &current,
                "could not inspect contained fixture path",
                source,
            )
        })?;
        if metadata.file_type().is_symlink() || (index + 1 < segments.len() && !metadata.is_dir()) {
            return Err(FixtureError::at(
                FixtureErrorKind::UnsafeFileType,
                &current,
                "fixture path crossed a symlink or non-directory ancestor",
            ));
        }
    }
    ensure_regular_file(&current, FixtureErrorKind::MissingFile)?;
    fs::read(&current).map_err(|source| {
        FixtureError::sourced(
            FixtureErrorKind::Io,
            current,
            "could not read fixture payload",
            source,
        )
    })
}

fn collect_payload_files(root: &Path) -> Result<BTreeSet<FixturePath>, FixtureError> {
    let mut files = BTreeSet::new();
    collect_directory(root, root, &mut files)?;
    Ok(files)
}

fn collect_directory(
    root: &Path,
    directory: &Path,
    files: &mut BTreeSet<FixturePath>,
) -> Result<(), FixtureError> {
    let entries = fs::read_dir(directory).map_err(|source| {
        FixtureError::sourced(
            FixtureErrorKind::Io,
            directory,
            "could not enumerate fixture case",
            source,
        )
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|source| {
                FixtureError::sourced(
                    FixtureErrorKind::Io,
                    directory,
                    "could not read fixture entry",
                    source,
                )
            })?
            .path();
        paths.push(path);
    }
    paths.sort();
    for path in paths {
        let metadata = fs::symlink_metadata(&path).map_err(|source| {
            FixtureError::sourced(
                FixtureErrorKind::Io,
                &path,
                "could not inspect fixture entry",
                source,
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(FixtureError::at(
                FixtureErrorKind::UnsafeFileType,
                path,
                "fixture symlinks are forbidden",
            ));
        }
        if metadata.is_dir() {
            collect_directory(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = path.strip_prefix(root).map_err(|source| {
                FixtureError::sourced(
                    FixtureErrorKind::InvalidPath,
                    &path,
                    "fixture entry escaped its root",
                    source,
                )
            })?;
            if relative == Path::new(MANIFEST_NAME) {
                continue;
            }
            let portable = relative
                .components()
                .map(|component| component.as_os_str().to_str())
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    FixtureError::at(
                        FixtureErrorKind::InvalidPath,
                        &path,
                        "fixture path was not UTF-8",
                    )
                })?
                .join("/");
            files.insert(FixturePath::new(portable)?);
        } else {
            return Err(FixtureError::at(
                FixtureErrorKind::UnsafeFileType,
                path,
                "fixture entry was not a file or directory",
            ));
        }
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> Sha256Digest {
    let computed: [u8; 32] = Sha256::digest(bytes).into();
    Sha256Digest::new(computed)
}
