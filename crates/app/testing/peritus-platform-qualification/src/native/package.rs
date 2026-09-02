//! Exact package-copy staging for one fresh H2 subject.

use std::fs;
use std::path::Path;

use crate::{PackageManifest, QualificationError, digest_file};

use super::{NativeControllerLimits, native_error};

pub(super) fn stage(
    source_root: &Path,
    destination_root: &Path,
    manifest: &PackageManifest,
    limits: NativeControllerLimits,
) -> Result<(), QualificationError> {
    fs::create_dir(destination_root).map_err(|error| {
        native_error("stage native H2 package", format!("create package root: {error}"))
    })?;
    for artifact in manifest.artifacts() {
        let source = source_root.join(artifact.path().as_str());
        let metadata = fs::symlink_metadata(&source).map_err(|error| {
            native_error("stage native H2 package", format!("inspect package artifact: {error}"))
        })?;
        if !metadata.file_type().is_file() {
            return Err(native_error(
                "stage native H2 package",
                "package artifact is not a regular file",
            ));
        }
        let observed = digest_file(&source, limits.package_artifact_bytes())?;
        if observed != artifact.digest() {
            return Err(native_error(
                "stage native H2 package",
                "package source bytes differ from the exact manifest",
            ));
        }
        let destination = destination_root.join(artifact.path().as_str());
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                native_error("stage native H2 package", format!("create artifact parent: {error}"))
            })?;
        }
        fs::copy(&source, &destination).map_err(|error| {
            native_error("stage native H2 package", format!("copy package artifact: {error}"))
        })?;
        if digest_file(&destination, limits.package_artifact_bytes())? != artifact.digest() {
            return Err(native_error(
                "stage native H2 package",
                "staged package bytes differ from the exact manifest",
            ));
        }
    }
    fs::write(destination_root.join("manifest.toml"), manifest.canonical_bytes()).map_err(
        |error| native_error("stage native H2 package", format!("write exact manifest: {error}")),
    )?;
    fs::write(destination_root.join("SHA256SUMS"), manifest.checksums()).map_err(|error| {
        native_error("stage native H2 package", format!("write exact checksums: {error}"))
    })?;
    Ok(())
}
