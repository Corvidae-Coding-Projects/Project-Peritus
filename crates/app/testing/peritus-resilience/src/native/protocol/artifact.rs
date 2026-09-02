//! Retained native H1 evidence validation.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

use crate::{EvidenceAnchor, EvidenceDigest, EvidenceId, EvidenceKind, SubjectErrorCode};

use super::super::{digest, subject_error};
use super::response::EvidenceDocument;

const MAX_ARTIFACT_PATH_BYTES: usize = 1_024;

pub(super) fn validate(
    documents: Vec<EvidenceDocument>,
    artifact_root: &Path,
    declared_count: u16,
    declared_bytes: u32,
) -> Result<Vec<EvidenceAnchor>, crate::SubjectError> {
    let mut paths = BTreeSet::new();
    let mut anchors = Vec::with_capacity(documents.len());
    let mut total_bytes = 0_u64;
    for document in documents {
        validate_path(&document.path)?;
        if !paths.insert(document.path.clone()) {
            return Err(invalid("more than one evidence anchor names the same retained path"));
        }
        let expected = parse_sha256(&document.sha256)?;
        let path = artifact_root.join(&document.path);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            invalid(format!("read retained evidence metadata for {}: {error}", document.path))
        })?;
        if !metadata.file_type().is_file() {
            return Err(invalid("retained evidence is not a regular file"));
        }
        let canonical = fs::canonicalize(&path).map_err(|error| {
            invalid(format!("canonicalize retained evidence {}: {error}", document.path))
        })?;
        if !canonical.starts_with(artifact_root) {
            return Err(invalid("retained evidence resolves outside its assigned root"));
        }
        if metadata.len() != document.bytes {
            return Err(invalid("retained evidence byte count does not match its response"));
        }
        if digest::file(&canonical).map_err(|error| invalid(error.to_string()))? != expected {
            return Err(invalid("retained evidence digest does not match its response"));
        }
        total_bytes = total_bytes
            .checked_add(document.bytes)
            .ok_or_else(|| invalid("retained evidence byte accounting overflowed"))?;
        anchors.push(EvidenceAnchor::new(
            evidence_kind(&document.kind)?,
            EvidenceId::new(document.id).map_err(|error| invalid(error.to_string()))?,
            expected,
        ));
    }
    if usize::from(declared_count) != anchors.len() {
        return Err(invalid("declared evidence count does not match retained artifacts"));
    }
    if total_bytes != u64::from(declared_bytes) {
        return Err(invalid("declared evidence bytes do not match retained artifacts"));
    }
    Ok(anchors)
}

fn validate_path(value: &str) -> Result<(), crate::SubjectError> {
    let bytes = value.as_bytes();
    if bytes.is_empty()
        || bytes.len() > MAX_ARTIFACT_PATH_BYTES
        || bytes
            .iter()
            .any(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'.' | b'_' | b'-' | b'/'))
        || value.split('/').any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
        || Path::new(value).components().any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(invalid("retained evidence path is not a portable normal relative path"));
    }
    Ok(())
}

pub(super) fn parse_sha256(value: &str) -> Result<EvidenceDigest, crate::SubjectError> {
    if value.len() != 64 {
        return Err(invalid("evidence digest is not lowercase SHA-256"));
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] =
            decode_nibble(pair[0])?.checked_shl(4).unwrap_or(0) | decode_nibble(pair[1])?;
    }
    Ok(EvidenceDigest::from_bytes(bytes))
}

fn decode_nibble(value: u8) -> Result<u8, crate::SubjectError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(invalid("evidence digest is not lowercase SHA-256")),
    }
}

fn evidence_kind(value: &str) -> Result<EvidenceKind, crate::SubjectError> {
    match value {
        "fault-injection" => Ok(EvidenceKind::FaultInjection),
        "journal" => Ok(EvidenceKind::Journal),
        "recovery" => Ok(EvidenceKind::Recovery),
        "ownership" => Ok(EvidenceKind::Ownership),
        "resource" => Ok(EvidenceKind::Resource),
        "final-state" => Ok(EvidenceKind::FinalState),
        _ => Err(invalid("evidence kind is not a canonical H1 value")),
    }
}

fn invalid(detail: impl Into<String>) -> crate::SubjectError {
    subject_error(SubjectErrorCode::Observation, detail, false)
}
