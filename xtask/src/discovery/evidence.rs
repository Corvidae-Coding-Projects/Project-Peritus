//! Source identity and post-campaign filesystem census.

use super::{Operation, capture, write_json};
use crate::error::XtaskError;
use base64::Engine as _;
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn new(root: &Path, operation: Operation) -> Result<PathBuf, XtaskError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| XtaskError::metadata(format!("discovery clock: {error}")))?
        .as_nanos();
    let label: String = format!("{operation:?}")
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character } else { '-' })
        .collect();
    let evidence = root.join("target/discovery").join(format!("{label}-{timestamp}"));
    fs::create_dir_all(&evidence).map_err(|error| XtaskError::io("create", &evidence, error))?;
    write_json(
        &evidence.join("completion.json"),
        &json!({
            "schema_version": 1, "status": "incomplete", "operation": format!("{operation:?}"),
        }),
    )?;
    let sha = capture(root, "git", &["rev-parse", "HEAD"])?;
    let dirty = capture(root, "git", &["status", "--porcelain", "--untracked-files=normal"])?;
    let rust = capture(root, "rustc", &["--version", "--verbose"])?;
    let source_changes = source_changes(root)?;
    fs::write(evidence.join("working-tree.patch"), source_changes)
        .map_err(|error| XtaskError::io("write source patch", &evidence, error))?;
    let hashes = untracked_hashes(root)?;
    write_json(&evidence.join("untracked-source.json"), &json!(hashes))?;
    write_json(
        &evidence.join("source.json"),
        &json!({
            "schema_version": 1, "source_sha": sha.trim(), "working_tree_changes": dirty,
            "rustc": rust, "os": env::consts::OS, "architecture": env::consts::ARCH,
            "build_jobs": 2, "operation": format!("{operation:?}"),
            "evidence_directory": evidence,
        }),
    )?;
    println!("discovery evidence: {}", evidence.display());
    Ok(evidence)
}

pub(super) fn source_changes(root: &Path) -> Result<String, XtaskError> {
    capture(root, "git", &["diff", "--binary", "--no-ext-diff", "--no-textconv", "HEAD"])
}

pub(super) fn untracked_hashes(root: &Path) -> Result<Vec<serde_json::Value>, XtaskError> {
    use sha2::{Digest, Sha256};

    let untracked = capture(root, "git", &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let mut hashes = Vec::new();
    for name in untracked.split('\0').filter(|name| !name.is_empty()) {
        let path = root.join(name);
        let bytes = fs::read(&path)
            .map_err(|error| XtaskError::io("read untracked source", &path, error))?;
        hashes.push(json!({
            "path": name,
            "sha256_base64": base64::engine::general_purpose::STANDARD.encode(Sha256::digest(bytes)),
        }));
    }
    Ok(hashes)
}

pub(super) fn verify_workspace_unchanged(root: &Path, evidence: &Path) -> Result<(), XtaskError> {
    use sha2::{Digest, Sha256};

    let source: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("source.json"))
            .map_err(|error| XtaskError::io("read initial source identity", evidence, error))?,
    )
    .map_err(XtaskError::metadata_decode)?;
    let initial_revision =
        source["source_sha"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| XtaskError::metadata("initial discovery source revision is missing"))?;
    let final_revision = capture(root, "git", &["rev-parse", "HEAD"])?;
    let final_revision = final_revision.trim();
    let initial_patch = fs::read(evidence.join("working-tree.patch"))
        .map_err(|error| XtaskError::io("read initial source patch", evidence, error))?;
    let initial_untracked: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join("untracked-source.json"))
            .map_err(|error| XtaskError::io("read initial untracked census", evidence, error))?,
    )
    .map_err(XtaskError::metadata_decode)?;
    let final_patch = source_changes(root)?.into_bytes();
    let final_untracked = json!(untracked_hashes(root)?);
    let unchanged = initial_revision == final_revision
        && initial_patch == final_patch
        && initial_untracked == final_untracked;
    write_json(
        &evidence.join("filesystem-census.json"),
        &json!({
            "schema_version": 1,
            "status": if unchanged { "unchanged" } else { "changed" },
            "scope": "tracked and non-ignored untracked workspace files",
            "ignored_campaign_artifacts": "target/discovery",
            "initial_source_sha": initial_revision,
            "final_source_sha": final_revision,
            "initial_patch_sha256_base64": base64::engine::general_purpose::STANDARD.encode(Sha256::digest(&initial_patch)),
            "final_patch_sha256_base64": base64::engine::general_purpose::STANDARD.encode(Sha256::digest(&final_patch)),
            "initial_untracked": initial_untracked,
            "final_untracked": final_untracked,
        }),
    )?;
    if unchanged {
        Ok(())
    } else {
        Err(XtaskError::metadata(
            "discovery campaign changed source revision, tracked files, or non-ignored untracked files",
        ))
    }
}
