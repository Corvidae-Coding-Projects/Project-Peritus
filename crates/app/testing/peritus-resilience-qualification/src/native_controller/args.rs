//! Strict process arguments and executable identity for the H1 controller.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use crate::digest;

pub(super) struct ControllerPaths {
    pub(super) candidate: PathBuf,
    pub(super) subject_root: PathBuf,
    pub(super) artifact_root: PathBuf,
    pub(super) instance_id: String,
    pub(super) subject_id: String,
    pub(super) build_sha256: String,
    pub(super) executor_sha256: String,
}

impl ControllerPaths {
    pub(super) fn parse(arguments: &[OsString]) -> Result<Self, Box<dyn std::error::Error>> {
        if arguments.len() != 15 || arguments.first().is_none_or(|value| value != "--serve") {
            return Err(usage().into());
        }
        let mut candidate = None;
        let mut subject_root = None;
        let mut artifact_root = None;
        let mut instance_id = None;
        let mut subject_id = None;
        let mut build_sha256 = None;
        let mut executor_sha256 = None;
        for pair in arguments[1..].chunks_exact(2) {
            match pair[0].to_str().ok_or_else(usage)? {
                "--candidate-executable" => set_once(&mut candidate, PathBuf::from(&pair[1]))?,
                "--subject-root" => set_once(&mut subject_root, PathBuf::from(&pair[1]))?,
                "--artifact-root" => set_once(&mut artifact_root, PathBuf::from(&pair[1]))?,
                "--instance-id" => set_once(&mut instance_id, text(&pair[1])?)?,
                "--subject-id" => set_once(&mut subject_id, text(&pair[1])?)?,
                "--build-sha256" => set_once(&mut build_sha256, text(&pair[1])?)?,
                "--executor-sha256" => set_once(&mut executor_sha256, text(&pair[1])?)?,
                _ => return Err(usage().into()),
            }
        }
        let mut paths = Self {
            candidate: candidate.ok_or_else(usage)?,
            subject_root: subject_root.ok_or_else(usage)?,
            artifact_root: artifact_root.ok_or_else(usage)?,
            instance_id: instance_id.ok_or_else(usage)?,
            subject_id: subject_id.ok_or_else(usage)?,
            build_sha256: build_sha256.ok_or_else(usage)?,
            executor_sha256: executor_sha256.ok_or_else(usage)?,
        };
        paths.validate()?;
        Ok(paths)
    }

    fn validate(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.candidate = canonical_file(&self.candidate, "candidate executable")?;
        self.subject_root = canonical_directory(&self.subject_root, "subject root")?;
        self.artifact_root = canonical_directory(&self.artifact_root, "artifact root")?;
        let controller = std::env::current_exe()?;
        let controller = canonical_file(&controller, "controller executable")?;
        if !self.candidate.starts_with(&self.subject_root)
            || !controller.starts_with(&self.subject_root)
            || self.artifact_root.starts_with(&self.subject_root)
        {
            return Err("H1 controller paths violate fresh-subject ownership".into());
        }
        if !lower_sha256(&self.build_sha256)
            || !lower_sha256(&self.executor_sha256)
            || digest::hex(digest::file(&self.candidate)?) != self.build_sha256
            || digest::hex(digest::file(&controller)?) != self.executor_sha256
        {
            return Err("H1 staged executable digest differs from its invocation binding".into());
        }
        if !stable_id(&self.subject_id) || !instance_id(&self.instance_id) {
            return Err("H1 controller identity is malformed".into());
        }
        Ok(())
    }
}

fn canonical_file(path: &Path, label: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let canonical = fs::canonicalize(path)?;
    let metadata = fs::symlink_metadata(&canonical)?;
    if !metadata.file_type().is_file() {
        return Err(format!("H1 {label} is not a regular file").into());
    }
    Ok(canonical)
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let canonical = fs::canonicalize(path)?;
    let metadata = fs::symlink_metadata(&canonical)?;
    if !metadata.file_type().is_dir() {
        return Err(format!("H1 {label} is not a real directory").into());
    }
    Ok(canonical)
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), &'static str> {
    if slot.replace(value).is_some() { Err(usage()) } else { Ok(()) }
}

fn text(value: &OsStr) -> Result<String, &'static str> {
    value.to_str().map(str::to_owned).ok_or_else(usage)
}

pub(super) fn lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn stable_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'.' | b'_' | b'-'))
        })
}

fn instance_id(value: &str) -> bool {
    let Some(suffix) = value.strip_prefix("h1-") else {
        return false;
    };
    let parts = suffix.split('-').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

const fn usage() -> &'static str {
    "usage: peritus-h1-controller --serve --candidate-executable FILE --subject-root DIR --artifact-root DIR --instance-id ID --subject-id ID --build-sha256 HEX --executor-sha256 HEX"
}

#[cfg(test)]
mod tests {
    use super::{instance_id, lower_sha256, stable_id};

    #[test]
    fn wire_identities_are_closed_and_bounded() {
        assert!(instance_id("h1-12-3-456"));
        assert!(!instance_id("h1-live"));
        assert!(stable_id("peritus.release.candidate"));
        assert!(!stable_id("Peritus"));
        assert!(lower_sha256(&"a".repeat(64)));
        assert!(!lower_sha256(&"A".repeat(64)));
    }
}
