//! Lock and construct every affected Git index before changing any workspace bytes.

use super::super::transaction::{Journal, Kind, Lock, own_directory};
use super::super::{
    ManagedBaseline, capture::private_git, failure, git, recovery, retention::Replacements, text,
};
use crate::ProductRunnerError;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(super) struct Indexes(BTreeMap<PathBuf, PreparedIndex>);

pub(super) struct Heads(BTreeMap<PathBuf, super::super::head::Prepared>);

impl Heads {
    pub(super) fn prepare(
        baseline: &ManagedBaseline,
        root: &Path,
        paths: &[PathBuf],
        replacements: &Replacements,
        journal: Option<&mut Journal>,
    ) -> Result<Self, ProductRunnerError> {
        let mut heads = Self(BTreeMap::new());
        heads.prepare_nested(baseline, root, paths, replacements, journal)?;
        Ok(heads)
    }

    fn prepare_nested(
        &mut self,
        baseline: &ManagedBaseline,
        root: &Path,
        paths: &[PathBuf],
        replacements: &Replacements,
        mut journal: Option<&mut Journal>,
    ) -> Result<(), ProductRunnerError> {
        for (prefix, child) in &baseline.nested {
            if !paths.iter().any(|path| path.starts_with(prefix)) {
                continue;
            }
            let child_path = root.join(prefix);
            let child_root = replacements.root(&child_path);
            if let Some(head) = super::super::head::prepare(
                child_root,
                baseline.entries.get(prefix).map(|entry| entry.object.as_str()),
                &replacements.restored_root(child_root)?,
                journal.as_deref_mut(),
            )? {
                self.0.insert(child_root.to_path_buf(), head);
            }
            let children = paths
                .iter()
                .filter_map(|path| path.strip_prefix(prefix).ok())
                .filter(|path| !path.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .collect();
            let children = replacements.children(child_root, child, children)?;
            self.prepare_nested(
                child,
                child_root,
                &children,
                replacements,
                journal.as_deref_mut(),
            )?;
        }
        Ok(())
    }

    pub(super) fn publish(
        &mut self,
        root: &Path,
        journal: Option<&mut Journal>,
    ) -> Result<Option<PathBuf>, ProductRunnerError> {
        self.0.remove(root).map(|head| head.publish(journal)).transpose()
    }
}

impl Indexes {
    pub(super) fn prepare(
        baseline: &ManagedBaseline,
        root: &Path,
        paths: &[PathBuf],
        replacements: &Replacements,
        journal: Option<&mut Journal>,
    ) -> Result<Self, ProductRunnerError> {
        let mut indexes = Self(BTreeMap::new());
        indexes.prepare_repository(baseline, root, paths, replacements, journal)?;
        Ok(indexes)
    }

    fn prepare_repository(
        &mut self,
        baseline: &ManagedBaseline,
        root: &Path,
        paths: &[PathBuf],
        replacements: &Replacements,
        mut journal: Option<&mut Journal>,
    ) -> Result<(), ProductRunnerError> {
        let mut local = Vec::new();
        for path in paths {
            if baseline
                .nested
                .keys()
                .any(|prefix| path != Path::new(prefix) && path.starts_with(prefix))
            {
                continue;
            }
            let name = super::super::git_path::tree_name(path)?;
            if baseline.entries.get(&name).is_some_and(|entry| entry.mode == "160000")
                && !baseline.nested.contains_key(&name)
            {
                continue;
            }
            local.push(name);
        }
        if !local.is_empty() {
            let mut rows = Vec::new();
            for path in &local {
                rows.extend(format!("0 {}\t{path}\0", "0".repeat(baseline.tree.len())).as_bytes());
            }
            for path in local {
                rows.extend(baseline.index_rows(root, &path)?);
            }
            self.0.insert(
                root.to_path_buf(),
                PreparedIndex::new(root, &rows, journal.as_deref_mut())?,
            );
        }
        for (prefix, child) in &baseline.nested {
            let children = paths
                .iter()
                .filter_map(|path| path.strip_prefix(prefix).ok())
                .filter(|path| !path.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .collect::<Vec<_>>();
            let child_path = root.join(prefix);
            let children =
                replacements.children(replacements.root(&child_path), child, children)?;
            if !children.is_empty() {
                self.prepare_repository(
                    child,
                    replacements.root(&child_path),
                    &children,
                    replacements,
                    journal.as_deref_mut(),
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn publish(
        &mut self,
        root: &Path,
        journal: Option<&mut Journal>,
    ) -> Result<(), ProductRunnerError> {
        if let Some(index) = self.0.remove(root) {
            index.publish(journal)?;
        }
        Ok(())
    }
}

struct PreparedIndex {
    path: PathBuf,
    lock: Option<Lock>,
    directory: Option<tempfile::TempDir>,
}

impl PreparedIndex {
    fn new(
        root: &Path,
        rows: &[u8],
        mut journal: Option<&mut Journal>,
    ) -> Result<Self, ProductRunnerError> {
        let path = PathBuf::from(text(git(
            root,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
            None,
        )?)?);
        let mut lock_name = path.as_os_str().to_os_string();
        lock_name.push(".lock");
        let lock_path = PathBuf::from(lock_name);
        let lock = Lock::create(&lock_path, journal.as_deref_mut()).map_err(|error| {
            failure(format!("cannot lock Git index {} before discard: {error}", path.display()))
        })?;
        let mut prepared = Self { path, lock: Some(lock), directory: None };
        prepared.write_index(root, rows, journal)?;
        Ok(prepared)
    }

    fn write_index(
        &mut self,
        root: &Path,
        rows: &[u8],
        mut journal: Option<&mut Journal>,
    ) -> Result<(), ProductRunnerError> {
        let parent = self.path.parent().ok_or_else(|| failure("Git index has no parent"))?;
        let mut temporary = tempfile::Builder::new()
            .prefix("peritus-index-")
            .tempdir_in(parent)
            .map_err(failure)?;
        own_directory(temporary.path(), Kind::Index, journal.as_deref_mut())?;
        temporary.disable_cleanup(journal.is_some());
        let index = temporary.path().join("index");
        let permissions = match fs::copy(&self.path, &index) {
            Ok(_) => Some(fs::metadata(&self.path).map_err(failure)?.permissions()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                private_git(root, &index, &["read-tree", "--empty"], None)?;
                None
            }
            Err(error) => return Err(failure(error)),
        };
        private_git(root, &index, &["update-index", "-z", "--index-info"], Some(rows))?;
        if let Some(permissions) = permissions {
            fs::set_permissions(&index, permissions).map_err(failure)?;
        }
        fs::File::open(&index).and_then(|file| file.sync_all()).map_err(failure)?;
        recovery::sync_directory(temporary.path())?;
        if let Some(journal) = journal {
            journal.seal_directory(temporary.path())?;
        }
        self.directory = Some(temporary);
        Ok(())
    }

    fn publish(mut self, journal: Option<&mut Journal>) -> Result<(), ProductRunnerError> {
        let directory =
            self.directory.take().ok_or_else(|| failure("Git index was not prepared"))?;
        super::publish_file(&directory.path().join("index"), &self.path)?;
        recovery::sync_directory(
            self.path.parent().ok_or_else(|| failure("Git index has no parent"))?,
        )?;
        self.lock.take().ok_or_else(|| failure("Git index lock is closed"))?.release()?;
        if let Some(journal) = journal {
            journal.cleanup_directory(directory.path())?;
        }
        #[cfg(test)]
        super::super::transaction::fault::pause(super::super::transaction::fault::Stage::Index);
        Ok(())
    }
}
