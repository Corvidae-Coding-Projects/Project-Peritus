//! Retained artifact and evidence-set validation for native H2 responses.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

use crate::{
    EvidenceEntry, EvidenceKind, EvidenceSet, EvidenceText, QualificationError, Sha256Digest,
    digest_file,
};

use super::EvidenceDocument;
use crate::native::native_error;

const MAX_ARTIFACT_PATH_BYTES: usize = 1_024;

pub(super) fn evidence_set(
    entries: Vec<EvidenceDocument>,
    declared_artifact_count: u32,
    artifact_root: &Path,
    maximum_artifact_bytes: u64,
) -> Result<(EvidenceSet, BTreeSet<String>, u64), QualificationError> {
    let mut set = EvidenceSet::new();
    let mut artifact_paths = BTreeSet::new();
    let mut artifact_bytes = 0_u64;
    for entry in entries {
        let (label, kind) = match entry {
            EvidenceDocument::Fact { label, value } => (label, EvidenceKind::Fact(value)),
            EvidenceDocument::Count { label, value } => (label, EvidenceKind::Count(value)),
            EvidenceDocument::Text { label, value } => {
                (label, EvidenceKind::Text(EvidenceText::new(value)?))
            }
            EvidenceDocument::Digest { label, path, sha256, bytes } => {
                if !artifact_paths.insert(path.clone()) {
                    return Err(native_error(
                        "validate native H2 response",
                        "more than one digest names the same retained artifact",
                    ));
                }
                artifact_bytes = artifact_bytes.checked_add(bytes).ok_or_else(|| {
                    native_error("validate native H2 response", "artifact bytes overflowed")
                })?;
                if artifact_bytes > maximum_artifact_bytes {
                    return Err(native_error(
                        "validate native H2 response",
                        "retained artifacts exceed the aggregate byte limit",
                    ));
                }
                let sha256 = parse_sha256(&sha256)?;
                validate_artifact(artifact_root, &path, sha256, bytes, maximum_artifact_bytes)?;
                (label, EvidenceKind::Digest { sha256, bytes })
            }
        };
        set.insert(EvidenceEntry::new(label, kind)?)?;
    }
    let observed = u32::try_from(artifact_paths.len()).map_err(|_| {
        native_error("validate native H2 response", "retained-artifact count exceeds u32")
    })?;
    if observed != declared_artifact_count {
        return Err(native_error(
            "validate native H2 response",
            "declared artifact count differs from retained digest evidence",
        ));
    }
    Ok((set, artifact_paths, artifact_bytes))
}

pub(super) fn validate_artifact(
    artifact_root: &Path,
    relative_path: &str,
    expected_sha256: Sha256Digest,
    expected_bytes: u64,
    maximum_bytes: u64,
) -> Result<(), QualificationError> {
    validate_artifact_path(relative_path)?;
    let path = artifact_root.join(relative_path);
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        native_error("validate native H2 artifact", format!("artifact metadata: {error}"))
    })?;
    if !metadata.file_type().is_file() || metadata.len() != expected_bytes {
        return Err(native_error(
            "validate native H2 artifact",
            "retained artifact type or byte count differs from its response",
        ));
    }
    let canonical = fs::canonicalize(&path).map_err(|error| {
        native_error("validate native H2 artifact", format!("canonicalize artifact: {error}"))
    })?;
    if !canonical.starts_with(artifact_root) {
        return Err(native_error(
            "validate native H2 artifact",
            "retained artifact resolves outside its assigned root",
        ));
    }
    if digest_file(&canonical, maximum_bytes)?.sha256() != expected_sha256 {
        return Err(native_error(
            "validate native H2 artifact",
            "retained artifact digest differs from its response",
        ));
    }
    Ok(())
}

pub(super) fn parse_sha256(value: &str) -> Result<Sha256Digest, QualificationError> {
    let digest = Sha256Digest::from_hex(value)?;
    if digest.to_hex() != value {
        return Err(native_error(
            "decode native H2 response",
            "evidence digest is not lowercase SHA-256",
        ));
    }
    Ok(digest)
}

fn validate_artifact_path(value: &str) -> Result<(), QualificationError> {
    let bytes = value.as_bytes();
    if bytes.is_empty()
        || bytes.len() > MAX_ARTIFACT_PATH_BYTES
        || bytes
            .iter()
            .any(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'.' | b'_' | b'-' | b'/'))
        || value.split('/').any(|part| part.is_empty() || matches!(part, "." | ".."))
        || Path::new(value).components().any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(native_error(
            "validate native H2 artifact",
            "retained-artifact path is not a portable normal relative path",
        ));
    }
    Ok(())
}
