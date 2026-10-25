//! Immutable source capture from a clean local checkout or pinned public archive.

use crate::{Error, SourceEntry, digest, hex, invalid, market::source::CatalogSet, preparation};
use flate2::read::GzDecoder;
use sha2::{Digest as _, Sha256};
use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Component, Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const SOURCE_BYTES: u64 = 256 * 1024 * 1024;
const SOURCE_FILES: usize = 50_000;

pub(super) struct PreparedSource {
    pub root: PathBuf,
    pub archive_digest: Option<String>,
}

pub(super) fn prepare(
    set: &CatalogSet,
    entry: &SourceEntry,
    temporary: &Path,
    cancelled: &Arc<AtomicBool>,
) -> Result<PreparedSource, Error> {
    let root = temporary.join("source");
    fs::create_dir(&root)?;
    let archive_digest = if let Some(checkout) = &set.local_checkout {
        validate_checkout(checkout, &set.commit)?;
        copy_tree(checkout, &root, cancelled)?;
        None
    } else {
        let specification = preparation::Specification {
            url: format!(
                "https://codeload.github.com/Corvidae-Coding-Projects/Peritus-Module-Market/tar.gz/{}",
                set.commit
            ),
            expected_sha256: None,
            maximum_bytes: SOURCE_BYTES,
        };
        let completed = preparation::fetch(&specification, cancelled).map_err(preparation_error)?;
        extract_archive(&completed.bytes, &root, cancelled)?;
        Some(completed.digest)
    };
    if cancelled.load(Ordering::Acquire) {
        return Err(invalid("source build was cancelled during source capture"));
    }
    let observed = tree_digest(&root)?;
    if observed != entry.source_digest {
        return Err(invalid(format!(
            "source tree digest mismatch: catalog {}, observed {observed}; no build was started",
            entry.source_digest
        )));
    }
    let lock = root.join("Cargo.lock");
    let lock_digest = file_digest(&lock)?;
    if lock_digest != entry.lockfile_digest {
        return Err(invalid(format!(
            "source lockfile digest mismatch: catalog {}, observed {lock_digest}",
            entry.lockfile_digest
        )));
    }
    Ok(PreparedSource { root, archive_digest })
}

fn validate_checkout(root: &Path, commit: &str) -> Result<(), Error> {
    let status = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(root)
        .output()?;
    if !status.status.success() || !status.stdout.is_empty() {
        return Err(invalid("local module-market source changed after refresh"));
    }
    let head = Command::new("git").args(["rev-parse", "HEAD"]).current_dir(root).output()?;
    if !head.status.success()
        || std::str::from_utf8(&head.stdout)
            .map_err(|_| invalid("local module-market HEAD is not UTF-8"))?
            .trim()
            != commit
    {
        return Err(invalid("local module-market HEAD changed after refresh"));
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path, cancelled: &Arc<AtomicBool>) -> Result<(), Error> {
    let mut files = 0usize;
    let mut bytes = 0u64;
    copy_directory(source, source, destination, cancelled, &mut files, &mut bytes)
}

fn copy_directory(
    base: &Path,
    current: &Path,
    destination: &Path,
    cancelled: &Arc<AtomicBool>,
    files: &mut usize,
    bytes: &mut u64,
) -> Result<(), Error> {
    for item in fs::read_dir(current)? {
        if cancelled.load(Ordering::Acquire) {
            return Err(invalid("source build was cancelled during source capture"));
        }
        let item = item?;
        let path = item.path();
        let relative = path.strip_prefix(base).map_err(|_| invalid("source path escaped root"))?;
        if relative.components().next().is_some_and(
            |part| matches!(part, Component::Normal(name) if name == ".git" || name == "target"),
        ) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        let target = destination.join(relative);
        if metadata.file_type().is_symlink() {
            return Err(invalid(format!(
                "source archive contains unsupported symlink {}",
                relative.display()
            )));
        }
        if metadata.is_dir() {
            fs::create_dir_all(&target)?;
            copy_directory(base, &path, destination, cancelled, files, bytes)?;
        } else if metadata.is_file() {
            *files = files.saturating_add(1);
            *bytes = bytes.saturating_add(metadata.len());
            if *files > SOURCE_FILES || *bytes > SOURCE_BYTES {
                return Err(invalid("source tree exceeds the file or byte preparation bound"));
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut input = fs::File::open(&path)?;
            let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&target)?;
            std::io::copy(&mut input, &mut output)?;
            output.sync_all()?;
        } else {
            return Err(invalid("source tree contains an unsupported file type"));
        }
    }
    Ok(())
}

fn extract_archive(
    bytes: &[u8],
    destination: &Path,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), Error> {
    let decoder = GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    let mut files = 0usize;
    let mut total = 0u64;
    for entry in archive.entries().map_err(|error| invalid(error.to_string()))? {
        if cancelled.load(Ordering::Acquire) {
            return Err(invalid("source build was cancelled during archive extraction"));
        }
        let entry = entry.map_err(|error| invalid(error.to_string()))?;
        let path = entry.path().map_err(|error| invalid(error.to_string()))?;
        let mut components = path.components();
        let _archive_root =
            components.next().ok_or_else(|| invalid("source archive entry has no root"))?;
        let relative = components.as_path();
        if relative.as_os_str().is_empty()
            || !relative.components().all(|part| matches!(part, Component::Normal(_)))
        {
            continue;
        }
        let target = destination.join(relative);
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if !kind.is_file() {
            return Err(invalid(format!(
                "source archive contains unsupported entry {}",
                relative.display()
            )));
        }
        files = files.saturating_add(1);
        let size = entry.size();
        total = total.saturating_add(size);
        if files > SOURCE_FILES || total > SOURCE_BYTES {
            return Err(invalid("source archive exceeds the file or byte preparation bound"));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&target)?;
        let copied = std::io::copy(&mut entry.take(size), &mut output)?;
        if copied != size {
            return Err(invalid("source archive entry ended before its declared size"));
        }
        output.flush()?;
        output.sync_all()?;
    }
    Ok(())
}

pub(super) fn tree_digest(root: &Path) -> Result<String, Error> {
    let mut paths = Vec::new();
    collect_files(root, root, &mut paths)?;
    paths.sort();
    let mut hash = Sha256::new();
    hash.update(b"peritus-module-source-tree-v1\0exclude:sources.toml\0");
    for relative in paths {
        if relative == Path::new("sources.toml") {
            continue;
        }
        let encoded = relative
            .to_str()
            .ok_or_else(|| invalid("source tree contains a non-UTF-8 path"))?
            .as_bytes();
        let data = fs::read(root.join(&relative))?;
        hash.update((encoded.len() as u64).to_be_bytes());
        hash.update(encoded);
        hash.update((data.len() as u64).to_be_bytes());
        hash.update(data);
    }
    Ok(hex(&hash.finalize()))
}

fn collect_files(base: &Path, current: &Path, output: &mut Vec<PathBuf>) -> Result<(), Error> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(base).map_err(|_| invalid("source path escaped root"))?;
        if relative.components().next().is_some_and(
            |part| matches!(part, Component::Normal(name) if name == ".git" || name == "target"),
        ) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(invalid("source tree contains a symlink"));
        }
        if metadata.is_dir() {
            collect_files(base, &path, output)?;
        } else if metadata.is_file() {
            output.push(relative.to_path_buf());
        } else {
            return Err(invalid("source tree contains an unsupported file type"));
        }
    }
    Ok(())
}

pub(super) fn file_digest(path: &Path) -> Result<String, Error> {
    Ok(digest(&fs::read(path)?))
}

fn preparation_error(error: preparation::Failure) -> Error {
    match error {
        preparation::Failure::Cancelled => invalid("source build was cancelled during download"),
        preparation::Failure::Rejected(detail) => {
            invalid(format!("source download rejected: {detail}"))
        }
        preparation::Failure::Infrastructure(detail) => {
            invalid(format!("source download failed: {detail}"))
        }
    }
}
