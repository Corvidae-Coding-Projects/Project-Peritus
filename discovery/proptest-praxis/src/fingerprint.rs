//! Build and replay source identity. These hashes describe inputs, never semantic expectations.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

pub fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [char::from(DIGITS[usize::from(byte >> 4)]), char::from(DIGITS[usize::from(byte & 15)])]
        })
        .collect()
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output =
        Command::new("git").arg("-C").arg(root).args(args).output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("git {args:?} failed"));
    }
    Ok(output.stdout)
}

fn files_digest(root: &Path, paths: &[&str]) -> Result<String, String> {
    let mut digest = Sha256::new();
    for relative in paths {
        let bytes = fs::read(root.join(relative)).map_err(|e| format!("read {relative}: {e}"))?;
        digest.update((relative.len() as u64).to_be_bytes());
        digest.update(relative.as_bytes());
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    Ok(hex(&digest.finalize()))
}

pub fn snapshot(manifest: &Path) -> Result<Value, String> {
    let root = manifest.parent().and_then(Path::parent).ok_or("missing source root")?;
    let head = String::from_utf8(git(root, &["rev-parse", "HEAD"])?).map_err(|e| e.to_string())?;
    let diff =
        git(root, &["diff", "--binary", "HEAD", "--", "crates", "Cargo.toml", "Cargo.lock"])?;
    let paths = String::from_utf8(git(
        root,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "crates",
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            ".cargo/config.toml",
        ],
    )?)
    .map_err(|e| e.to_string())?;
    let mut paths = paths.split('\0').filter(|p| !p.is_empty()).collect::<Vec<_>>();
    paths.sort_unstable();
    paths.dedup();
    let mut harness = vec!["Cargo.toml".to_owned(), "Cargo.lock".to_owned(), "build.rs".to_owned()];
    for folder in ["src", "owner", "support"] {
        rust_files(manifest, &manifest.join(folder), &mut harness)?;
    }
    harness.sort();
    let harness = harness.iter().map(String::as_str).collect::<Vec<_>>();
    Ok(json!({"source_head":head.trim(),"production_diff_sha256":digest(&diff),
        "production_dirty":!diff.is_empty(),"production_source_sha256":files_digest(root,&paths)?,
        "harness_sha256":files_digest(manifest,&harness)?}))
}

fn rust_files(base: &Path, folder: &Path, paths: &mut Vec<String>) -> Result<(), String> {
    if !folder.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(folder).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.file_name().is_some_and(|name| name == "__pycache__") {
            continue;
        }
        if path.is_dir() {
            rust_files(base, &path, paths)?;
        } else if path.is_file() {
            paths.push(
                path.strip_prefix(base).map_err(|e| e.to_string())?.to_string_lossy().into_owned(),
            );
        }
    }
    Ok(())
}
