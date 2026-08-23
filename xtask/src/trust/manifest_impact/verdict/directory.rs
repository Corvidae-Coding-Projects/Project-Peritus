//! Exhaustive one-to-one reconciliation of retained review files.

use super::{DIRECTORY, artifact};
use crate::error::Diagnostic;
use crate::trust::manifest_file;
use crate::trust::manifest_model::{ProofImpactDocument, ProofImpactVerdict};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub(super) fn validate(
    root: &Path,
    document: &ProofImpactDocument,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut referenced = BTreeMap::<String, usize>::new();
    for reference in document.changes.iter().filter_map(|change| change.verdict.as_ref()) {
        *referenced.entry(reference.path.clone()).or_default() += 1;
        let path = Path::new(&reference.path);
        let Some(verdict) = manifest_file::read_toml::<ProofImpactVerdict>(root, path, diagnostics)
        else {
            continue;
        };
        for artifact_path in artifact::declared_paths(&verdict) {
            *referenced.entry(artifact_path.to_owned()).or_default() += 1;
        }
    }
    for (path, count) in &referenced {
        if *count != 1 {
            diagnostics.push(Diagnostic::at(
                path,
                "detached review file is referenced more than once",
                "give every verdict and retained artifact exactly one immutable PCR owner",
            ));
        }
    }
    let mut present = Vec::new();
    collect_review_files(root, !referenced.is_empty(), &mut present, diagnostics);
    for relative in present {
        if relative.to_str().is_none_or(|path| !referenced.contains_key(path)) {
            diagnostics.push(Diagnostic::at(
                &relative,
                "detached review file is not referenced by exactly one PCR artifact inventory",
                "remove the file or add its exact one-to-one content-addressed verdict reference",
            ));
        }
    }
}

fn collect_review_files(
    root: &Path,
    required: bool,
    files: &mut Vec<PathBuf>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    collect_review_files_with(
        root,
        &root.join(DIRECTORY),
        required,
        files,
        diagnostics,
        &|directory| {
            fs::read_dir(directory)
                .map(|entries| entries.map(|entry| entry.map(|value| value.path())).collect())
        },
        &|path| {
            path.symlink_metadata().map(|metadata| {
                if metadata.file_type().is_dir() {
                    ReviewPathKind::Directory
                } else {
                    ReviewPathKind::File
                }
            })
        },
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReviewPathKind {
    Directory,
    File,
}

pub(super) fn collect_review_files_with<List, Inspect>(
    root: &Path,
    directory: &Path,
    required: bool,
    files: &mut Vec<PathBuf>,
    diagnostics: &mut Vec<Diagnostic>,
    list: &List,
    inspect: &Inspect,
) where
    List: Fn(&Path) -> io::Result<Vec<io::Result<PathBuf>>>,
    Inspect: Fn(&Path) -> io::Result<ReviewPathKind>,
{
    let entries = match list(directory) {
        Ok(entries) => entries,
        Err(error) if !required && error.kind() == io::ErrorKind::NotFound => return,
        Err(error) => {
            review_inventory_error(root, directory, &error, diagnostics);
            return;
        }
    };
    for entry in entries {
        let path = match entry {
            Ok(path) => path,
            Err(error) => {
                review_inventory_error(root, directory, &error, diagnostics);
                continue;
            }
        };
        match inspect(&path) {
            Ok(ReviewPathKind::Directory) => {
                collect_review_files_with(root, &path, true, files, diagnostics, list, inspect);
            }
            Ok(ReviewPathKind::File) => match path.strip_prefix(root) {
                Ok(relative) => files.push(relative.to_path_buf()),
                Err(error) => review_inventory_error(
                    root,
                    &path,
                    &io::Error::other(error.to_string()),
                    diagnostics,
                ),
            },
            Err(error) => review_inventory_error(root, &path, &error, diagnostics),
        }
    }
}

fn review_inventory_error(
    root: &Path,
    path: &Path,
    error: &io::Error,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let display = path.strip_prefix(root).unwrap_or(path);
    diagnostics.push(Diagnostic::at(
        display,
        format!("detached review inventory cannot enumerate or inspect an entry: {error}"),
        "restore a completely readable regular-file inventory; unreadable review state fails closed",
    ));
}
