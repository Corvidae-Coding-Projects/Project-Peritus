//! Materialize regular Git blobs without checkout filters, hooks, or symlink traversal.

use crate::error::XtaskError;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const MAX_FILES: usize = 30_000;
const MAX_BYTES: usize = 512 * 1024 * 1024;

pub(super) struct CandidateTree {
    directory: PathBuf,
}

impl CandidateTree {
    pub(super) fn materialize(repository: &Path, tree: &str) -> Result<Self, XtaskError> {
        let entries = entries(repository, tree)?;
        let candidate = Self::create()?;
        let mut total = 0_usize;
        for (relative, object) in entries {
            let bytes = git(repository, &["cat-file", "blob", &object])?;
            total = total.saturating_add(bytes.len());
            if total > MAX_BYTES {
                return Err(XtaskError::metadata(
                    "reviewed candidate exceeds materialization limit",
                ));
            }
            let path = candidate.directory.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| XtaskError::io("create candidate directory", parent, error))?;
            }
            fs::write(&path, bytes)
                .map_err(|error| XtaskError::io("materialize reviewed blob", &path, error))?;
        }
        Ok(candidate)
    }

    pub(super) fn create() -> Result<Self, XtaskError> {
        for _ in 0..100 {
            let serial = NEXT.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir()
                .join(format!("peritus-reviewed-tree-{}-{serial}", std::process::id()));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                builder.mode(0o700);
            }
            match builder.create(&directory) {
                Ok(()) => return Ok(Self { directory }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(XtaskError::io("create candidate tree", &directory, error));
                }
            }
        }
        Err(XtaskError::metadata("could not allocate an unused reviewed-candidate directory"))
    }

    pub(super) fn root(&self) -> &Path {
        &self.directory
    }
}

impl Drop for CandidateTree {
    fn drop(&mut self) {
        // Only this freshly created owned directory is removed, including partial materialization.
        let _ = fs::remove_dir_all(&self.directory);
    }
}

pub(super) fn entries(
    repository: &Path,
    tree: &str,
) -> Result<BTreeMap<PathBuf, String>, XtaskError> {
    if tree.len() != 40 || !tree.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(XtaskError::metadata("reviewed tree requires a full Git object identity"));
    }
    let output = git(repository, &["ls-tree", "-r", "-z", "--full-tree", tree])?;
    parse_entries(&output)
}

fn parse_entries(output: &[u8]) -> Result<BTreeMap<PathBuf, String>, XtaskError> {
    let mut entries = BTreeMap::new();
    for row in output.split(|byte| *byte == 0).filter(|row| !row.is_empty()) {
        let row = std::str::from_utf8(row)
            .map_err(|_| XtaskError::metadata("reviewed tree contains a non-UTF-8 path"))?;
        let (header, relative) = row
            .split_once('\t')
            .ok_or_else(|| XtaskError::metadata("malformed reviewed Git tree entry"))?;
        let fields = header.split_ascii_whitespace().collect::<Vec<_>>();
        if fields.len() != 3
            || !matches!(fields[0], "100644" | "100755")
            || fields[1] != "blob"
            || fields[2].len() != 40
            || !fields[2].bytes().all(|byte| byte.is_ascii_hexdigit())
            || !normal_path(relative)
        {
            return Err(XtaskError::metadata(format!("unsafe reviewed tree entry `{relative}`")));
        }
        if entries.insert(PathBuf::from(relative), fields[2].to_owned()).is_some()
            || entries.len() > MAX_FILES
        {
            return Err(XtaskError::metadata("duplicate or excessive reviewed tree entries"));
        }
    }
    Ok(entries)
}

fn normal_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && !value.contains('\n')
        && !value.contains('\r')
        && !value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part.eq_ignore_ascii_case(".git"))
        && Path::new(value).components().all(|component| matches!(component, Component::Normal(_)))
}

pub(super) fn git(root: &Path, arguments: &[&str]) -> Result<Vec<u8>, XtaskError> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| XtaskError::io("inspect reviewed Git object", root, error))?;
    if !output.status.success() {
        return Err(XtaskError::metadata(format!(
            "reviewed Git inspection failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::{normal_path, parse_entries};

    #[test]
    fn rejects_non_regular_and_escaping_tree_entries() {
        let oid = "a".repeat(40);
        for (mode, path) in [
            ("120000", "link"),
            ("160000", "submodule"),
            ("100644", "../escape"),
            ("100644", "/absolute"),
            ("100644", ".git/config"),
            ("100644", "a/.GiT/config"),
            ("100644", "a//b"),
            ("100644", "a/./b"),
            ("100644", "a\\b"),
        ] {
            assert!(
                parse_entries(format!("{mode} blob {oid}\t{path}\0").as_bytes()).is_err(),
                "{path}"
            );
        }
        assert!(!normal_path(""));
        assert!(
            parse_entries(format!("100644 blob {oid}\tcrates/a/src/lib.rs\0").as_bytes()).is_ok()
        );
    }
}
