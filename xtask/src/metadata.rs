use crate::error::XtaskError;
use crate::model::{ArchitecturePolicy, CargoMetadata, ToolchainPolicy};
use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;
use std::process::Command;

pub(crate) fn architecture_policy(root: &Path) -> Result<ArchitecturePolicy, XtaskError> {
    read_toml(&root.join("architecture.toml"))
}

pub(crate) fn toolchain_policy(root: &Path) -> Result<ToolchainPolicy, XtaskError> {
    read_toml(&root.join("toolchains.toml"))
}

pub(crate) fn read_toml<T: DeserializeOwned>(path: &Path) -> Result<T, XtaskError> {
    let contents = fs::read_to_string(path).map_err(|error| XtaskError::io("read", path, error))?;
    toml::from_str(&contents).map_err(|error| XtaskError::parse_policy(path, error))
}

pub(crate) fn cargo_metadata(root: &Path) -> Result<CargoMetadata, XtaskError> {
    let manifest = root.join("Cargo.toml");
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--locked", "--no-deps", "--manifest-path"])
        .arg(&manifest)
        .current_dir(root)
        .output()
        .map_err(|error| XtaskError::io("execute cargo metadata for", &manifest, error))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(XtaskError::metadata(format!(
            "cargo metadata failed with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    serde_json::from_slice(&output.stdout).map_err(XtaskError::metadata_decode)
}
