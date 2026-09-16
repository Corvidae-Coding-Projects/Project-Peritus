//! Bound retained campaign evidence while preserving manifests and explicit truncation records.

use super::write_json;
use crate::error::XtaskError;
use base64::Engine as _;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};

const FAILURE_BUNDLE_LIMIT_BYTES: u64 = 64 * 1024 * 1024;
const MANIFEST_RESERVE_BYTES: u64 = 64 * 1024;
const LARGE_DIAGNOSTIC_BYTES: u64 = 256 * 1024;
const MAX_CHANGE_RECORDS: usize = 256;

pub(super) fn finalize(evidence: &Path) -> Result<Value, XtaskError> {
    finalize_with_limits(
        evidence,
        FAILURE_BUNDLE_LIMIT_BYTES,
        MANIFEST_RESERVE_BYTES,
        LARGE_DIAGNOSTIC_BYTES,
    )
}

pub(super) fn finalize_with_limits(
    evidence: &Path,
    limit: u64,
    reserve: u64,
    diagnostic_limit: u64,
) -> Result<Value, XtaskError> {
    if reserve >= limit {
        return Err(XtaskError::metadata("evidence manifest reserve must be below bundle limit"));
    }
    let compiler_scratch = evidence.join("scratch");
    let compiler_scratch_removed = compiler_scratch.exists();
    if compiler_scratch_removed {
        fs::remove_dir_all(&compiler_scratch).map_err(|error| {
            XtaskError::io("remove disposable discovery compiler cache", &compiler_scratch, error)
        })?;
    }
    let original_bytes = total_bytes(evidence)?;
    let target = limit - reserve;
    let mut changes = Vec::new();
    for path in regular_files(evidence)? {
        let size = metadata_len(&path)?;
        if is_diagnostic(&path) && size > diagnostic_limit {
            changes.push(bound_file(evidence, &path, diagnostic_limit)?);
        }
    }
    let mut retained_bytes = total_bytes(evidence)?;
    if retained_bytes > target {
        let mut candidates = regular_files(evidence)?;
        candidates.retain(|path| !must_keep(evidence, path));
        candidates.sort_by_key(|path| std::cmp::Reverse(metadata_len(path).unwrap_or(0)));
        for path in candidates {
            if retained_bytes <= target {
                break;
            }
            let size = metadata_len(&path)?;
            let digest = digest(&path)?;
            fs::remove_file(&path).map_err(|error| {
                XtaskError::io("remove oversized discovery evidence", &path, error)
            })?;
            retained_bytes = retained_bytes.saturating_sub(size);
            changes.push(change(evidence, &path, size, 0, &digest));
        }
    }
    if retained_bytes > target {
        return Err(XtaskError::metadata(format!(
            "required discovery manifests use {retained_bytes} bytes, exceeding the {limit}-byte failure-bundle limit"
        )));
    }
    let mut omitted_changes = changes.len().saturating_sub(MAX_CHANGE_RECORDS);
    changes.truncate(MAX_CHANGE_RECORDS);
    let had_changes = !changes.is_empty() || omitted_changes > 0;
    let report = loop {
        let candidate = json!({
            "schema_version": 1,
            "limit_bytes": limit,
            "original_bytes": original_bytes,
            "retained_payload_bytes": retained_bytes,
            "manifest_reserve_bytes": reserve,
            "truncated": had_changes,
            "changes": changes,
            "omitted_change_records": omitted_changes,
            "compiler_scratch_removed": compiler_scratch_removed,
        });
        let encoded = serde_json::to_vec_pretty(&candidate).map_err(XtaskError::metadata_decode)?;
        if u64::try_from(encoded.len()).expect("manifest length fits u64") <= reserve {
            break candidate;
        }
        if changes.pop().is_some() {
            omitted_changes += 1;
        } else {
            return Err(XtaskError::metadata(
                "discovery bundle manifest cannot fit its reserved bytes",
            ));
        }
    };
    let report_path = evidence.join("bundle.json");
    write_json(&report_path, &report)?;
    let final_bytes = total_bytes(evidence)?;
    if final_bytes > limit {
        return Err(XtaskError::metadata(format!(
            "discovery evidence uses {final_bytes} bytes after finalization, exceeding {limit}"
        )));
    }
    Ok(report)
}

fn regular_files(root: &Path) -> Result<Vec<PathBuf>, XtaskError> {
    let mut pending = vec![root.to_owned()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| XtaskError::io("read discovery evidence", &directory, error))?
        {
            let entry = entry.map_err(|error| {
                XtaskError::io("read discovery evidence entry", &directory, error)
            })?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| XtaskError::io("inspect discovery evidence", &path, error))?;
            if metadata.file_type().is_symlink() {
                return Err(XtaskError::metadata(format!(
                    "discovery evidence contains a symlink: {}",
                    path.display()
                )));
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                files.push(path);
            } else {
                return Err(XtaskError::metadata(format!(
                    "discovery evidence contains a special file: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(files)
}

fn total_bytes(root: &Path) -> Result<u64, XtaskError> {
    regular_files(root)?.into_iter().try_fold(0_u64, |total, path| {
        total
            .checked_add(metadata_len(&path)?)
            .ok_or_else(|| XtaskError::metadata("discovery evidence size overflowed"))
    })
}

fn metadata_len(path: &Path) -> Result<u64, XtaskError> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|error| XtaskError::io("inspect discovery evidence size", path, error))
}

fn is_diagnostic(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("log" | "stdout" | "stderr")
    )
}

fn must_keep(root: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
    let extension = path.extension().and_then(|extension| extension.to_str());
    extension == Some("json")
        || extension == Some("patch")
        || name.ends_with("-version.txt")
        || name == "nightly.txt"
        || matches!(name, "inventory.stdout" | "selected-inventory.stdout")
        || ["crash-", "timeout-", "oom-", "leak-"].iter().any(|prefix| name.starts_with(prefix))
        || relative == Path::new("working-tree.patch")
}

fn bound_file(root: &Path, path: &Path, limit: u64) -> Result<Value, XtaskError> {
    let original = metadata_len(path)?;
    let digest = digest(path)?;
    let prefix_len = limit / 2;
    let tail_len = limit - prefix_len;
    let mut source = File::open(path)
        .map_err(|error| XtaskError::io("open oversized discovery diagnostic", path, error))?;
    let mut prefix = vec![
        0_u8;
        usize::try_from(prefix_len).map_err(|_| {
            XtaskError::metadata("diagnostic prefix limit exceeds this platform")
        })?
    ];
    source
        .read_exact(&mut prefix)
        .map_err(|error| XtaskError::io("read discovery diagnostic prefix", path, error))?;
    source
        .seek(SeekFrom::End(
            -i64::try_from(tail_len)
                .map_err(|_| XtaskError::metadata("diagnostic tail limit exceeds seek range"))?,
        ))
        .map_err(|error| XtaskError::io("seek discovery diagnostic tail", path, error))?;
    let mut tail = vec![
        0_u8;
        usize::try_from(tail_len).map_err(|_| XtaskError::metadata(
            "diagnostic tail limit exceeds this platform"
        ))?
    ];
    source
        .read_exact(&mut tail)
        .map_err(|error| XtaskError::io("read discovery diagnostic tail", path, error))?;
    drop(source);
    let mut destination = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|error| XtaskError::io("truncate discovery diagnostic", path, error))?;
    destination
        .write_all(&prefix)
        .and_then(|()| destination.write_all(&tail))
        .and_then(|()| destination.flush())
        .map_err(|error| {
            XtaskError::io("retain discovery diagnostic prefix and tail", path, error)
        })?;
    Ok(change(root, path, original, limit, &digest))
}

fn digest(path: &Path) -> Result<String, XtaskError> {
    let mut file = File::open(path)
        .map_err(|error| XtaskError::io("open discovery evidence for hashing", path, error))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| XtaskError::io("hash discovery evidence", path, error))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(hash.finalize()))
}

fn change(root: &Path, path: &Path, original: u64, retained: u64, digest: &str) -> Value {
    json!({
        "path": path.strip_prefix(root).unwrap_or(path),
        "original_bytes": original,
        "retained_bytes": retained,
        "original_sha256_base64": digest,
        "retention": if retained == 0 { "removed_after_manifest" } else { "prefix_and_tail" },
    })
}
