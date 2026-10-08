//! Raw-byte Git trees built with a private index; never runs clean/smudge filters.

use super::{Entry, ManagedBaseline, failure, git, text, validate_path};
use crate::{ProductRunnerError, file_metadata, workspace_filter};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

mod directories;

impl ManagedBaseline {
    pub(crate) fn capture(root: &Path, retain: bool) -> Result<Self, ProductRunnerError> {
        let mut snapshot = Self::capture_with_baseline(root, retain, None)?;
        if retain && !snapshot.nested.is_empty() {
            super::retention::capture_children(root, &mut snapshot)?;
        }
        Ok(snapshot)
    }

    pub(super) fn capture_with_baseline(
        root: &Path,
        retain: bool,
        prior: Option<&Self>,
    ) -> Result<Self, ProductRunnerError> {
        let mut paths = BTreeSet::new();
        if let Some(prior) = prior {
            paths.extend(prior.entries.keys().cloned());
            paths.extend(prior.nested.keys().cloned());
        }
        let tracked = git(root, &["ls-files", "-z", "--cached"], None)?;
        for path in tracked.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            paths.insert(std::str::from_utf8(path).map_err(failure)?.to_owned());
        }
        let untracked = git(root, &["ls-files", "-z", "--others", "--exclude-standard"], None)?;
        for path in untracked.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            let path = std::str::from_utf8(path).map_err(failure)?.trim_end_matches('/');
            if !workspace_filter::generated(Path::new(path)) {
                paths.insert(path.to_owned());
            }
        }
        directories::expand(root, prior, &mut paths)?;
        let mut entries = BTreeMap::new();
        let mut regular = Vec::new();
        let mut nested = BTreeMap::new();
        for path in paths {
            if nested.keys().any(|prefix: &String| Path::new(&path).starts_with(prefix)) {
                continue;
            }
            validate_path(Path::new(&path))?;
            if !super::paths::parent_is_directory(root, Path::new(&path))? {
                continue;
            }
            let absolute = root.join(&path);
            let metadata = match fs::symlink_metadata(&absolute) {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(failure(error)),
            };
            let (mode, object) = if metadata.is_dir() && absolute.join(".git").exists() {
                let head = nested_head(&absolute)?;
                nested.insert(
                    path.clone(),
                    Self::capture_with_baseline(
                        &absolute,
                        retain,
                        prior.and_then(|value| value.nested.get(&path)),
                    )?,
                );
                // An unborn repository has source and an index but no commit to encode
                // as a Git link. Its nested snapshot still owns those source preimages.
                let Some(object) = head else { continue };
                ("160000", object)
            } else if metadata.file_type().is_symlink() {
                let target = fs::read_link(&absolute).map_err(failure)?;
                let bytes = target.as_os_str().as_encoded_bytes();
                ("120000", text(git(root, &["hash-object", "-w", "--stdin"], Some(bytes))?)?)
            } else if metadata.is_file() {
                regular.push(path.clone());
                (file_metadata::git_file_mode(&metadata), String::new())
            } else if metadata.is_dir() {
                continue;
            } else {
                return Err(failure(format!("cannot retain non-file {}", absolute.display())));
            };
            entries.insert(
                path,
                Entry {
                    mode: mode.to_owned(),
                    object,
                    permissions: file_metadata::permission_fingerprint(&metadata),
                },
            );
        }
        for chunk in regular.chunks(128) {
            let mut arguments = vec!["hash-object", "--no-filters", "-w", "--"];
            arguments.extend(chunk.iter().map(String::as_str));
            let hashes = text(git(root, &arguments, None)?)?;
            let hashes = hashes.lines().collect::<Vec<_>>();
            if hashes.len() != chunk.len() {
                return Err(failure("incomplete raw file hashes"));
            }
            for (path, object) in chunk.iter().zip(hashes) {
                object.clone_into(
                    &mut entries
                        .get_mut(path)
                        .ok_or_else(|| failure("missing snapshot entry"))?
                        .object,
                );
            }
        }
        let tree = build_tree(root, &entries)?;
        let (index, staged) = if retain {
            let (index, staged) = super::index::capture(root)?;
            (index, Some(staged))
        } else {
            (tree.clone(), None)
        };
        if retain {
            retain_objects(root, &tree, &index)?;
        }
        Ok(Self { tree, index, staged, entries, nested, retained: None })
    }
}

fn retain_objects(root: &Path, tree: &str, index: &str) -> Result<(), ProductRunnerError> {
    // A parent's gitlink does not keep this repository's commit reachable.
    let head = nested_head(root)?;
    for object in [tree, index].into_iter().chain(head.as_deref()) {
        git(root, &["update-ref", &format!("refs/peritus/task-baselines/{object}"), object], None)?;
    }
    Ok(())
}

pub(super) fn nested_head(root: &Path) -> Result<Option<String>, ProductRunnerError> {
    let mut command = super::repository_command(root)?;
    command.args(["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]);
    let output = command_output(command)?;
    if output.status.success() {
        return text(output.stdout).map(Some);
    }
    if output.status.code() != Some(1) {
        return Err(failure(String::from_utf8_lossy(&output.stderr)));
    }
    // Only a symbolic HEAD whose branch does not exist is an unborn repository.
    // Corruption, detached invalid HEADs and repository access errors still surface.
    let reference = text(git(root, &["symbolic-ref", "--quiet", "HEAD"], None)?)?;
    let mut command = super::repository_command(root)?;
    command.args(["show-ref", "--verify", "--quiet", &reference]);
    let branch = command_output(command)?;
    if branch.status.code() == Some(1) {
        return Ok(None);
    }
    Err(failure("nested repository HEAD does not resolve to a valid object"))
}

struct CommandOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn command_output(command: std::process::Command) -> Result<CommandOutput, ProductRunnerError> {
    let mut stdout = Vec::new();
    let completed = crate::candidate::process::stream_current(
        command,
        None,
        &mut stdout,
        "inspect nested candidate repository",
    )?;
    Ok(CommandOutput {
        status: completed.status,
        stdout,
        stderr: completed.stderr,
    })
}

pub(super) fn build_tree(
    root: &Path,
    entries: &BTreeMap<String, Entry>,
) -> Result<String, ProductRunnerError> {
    let temporary = tempfile::tempdir().map_err(failure)?;
    let index_path = temporary.path().join("index");
    let mut input = Vec::new();
    for (path, entry) in entries {
        input.extend_from_slice(format!("{} {}\t{path}\0", entry.mode, entry.object).as_bytes());
    }
    private_git(root, &index_path, &["read-tree", "--empty"], None)?;
    private_git(root, &index_path, &["update-index", "-z", "--index-info"], Some(&input))?;
    text(private_git(root, &index_path, &["-c", "core.fsync=all", "write-tree"], None)?)
}

pub(super) fn private_git(
    root: &Path,
    index: &Path,
    arguments: &[&str],
    input: Option<&[u8]>,
) -> Result<Vec<u8>, ProductRunnerError> {
    let mut command = super::repository_command(root)?;
    command
        .args(arguments)
        .env("GIT_INDEX_FILE", index)
        .env("GIT_LITERAL_PATHSPECS", "1");
    let mut stdout = Vec::new();
    let completed = crate::candidate::process::stream_current(
        command,
        input,
        &mut stdout,
        "capture candidate with a private Git index",
    )?;
    if !completed.status.success() {
        return Err(failure(String::from_utf8_lossy(&completed.stderr)));
    }
    Ok(stdout)
}
