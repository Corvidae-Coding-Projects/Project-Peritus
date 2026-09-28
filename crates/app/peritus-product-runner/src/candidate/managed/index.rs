//! Preserve index stages, including unresolved merges, without touching the real index.

use super::{Entry, ManagedBaseline, failure, git, validate_path};
use crate::ProductRunnerError;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StagedEntry {
    path: String,
    mode: String,
    object: String,
    stage: u8,
}

impl ManagedBaseline {
    pub(super) fn verify_index_objects(
        &self,
        root: &Path,
        path: &str,
    ) -> Result<(), ProductRunnerError> {
        if let Some(staged) = &self.staged {
            for entry in staged.iter().filter(|entry| entry.path == path && entry.mode != "160000")
            {
                git(root, &["cat-file", "blob", &entry.object], None)?;
            }
        } else {
            git(root, &["cat-file", "-e", &self.index], None)?;
        }
        Ok(())
    }
    pub(super) fn validate_index(&self) -> Result<(), ProductRunnerError> {
        let mut stages = std::collections::BTreeSet::new();
        for entry in self.staged.iter().flatten() {
            validate_path(Path::new(&entry.path))?;
            if entry.stage > 3
                || !matches!(entry.mode.as_str(), "100644" | "100755" | "120000" | "160000")
                || entry.object.len() != self.tree.len()
                || !entry.object.bytes().all(|byte| byte.is_ascii_hexdigit())
                || !stages.insert((&entry.path, entry.stage))
            {
                return Err(failure("invalid retained index entry"));
            }
        }
        for (path, stage) in &stages {
            if *stage != 0 && stages.contains(&(path, 0)) {
                return Err(failure("retained index mixes merged and unmerged stages"));
            }
        }
        Ok(())
    }

    pub(super) fn index_rows(
        &self,
        root: &Path,
        path: &str,
    ) -> Result<Vec<u8>, ProductRunnerError> {
        let Some(staged) = &self.staged else {
            return git(root, &["ls-tree", "-z", &self.index, "--", path], None);
        };
        Ok(staged
            .iter()
            .filter(|entry| entry.path == path)
            .flat_map(|entry| {
                format!("{} {} {}\t{}\0", entry.mode, entry.object, entry.stage, entry.path)
                    .into_bytes()
            })
            .collect())
    }
}

pub(super) fn capture(root: &Path) -> Result<(String, Vec<StagedEntry>), ProductRunnerError> {
    let encoded = git(root, &["ls-files", "--stage", "-z"], None)?;
    let mut staged = Vec::new();
    let mut retained = BTreeMap::new();
    for row in encoded.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let row = std::str::from_utf8(row).map_err(failure)?;
        let (metadata, path) = row.split_once('\t').ok_or_else(|| failure("missing index path"))?;
        validate_path(Path::new(path))?;
        let fields = metadata.split(' ').collect::<Vec<_>>();
        let [mode, object, stage] = fields.as_slice() else {
            return Err(failure("invalid index entry"));
        };
        let stage: u8 = stage.parse().map_err(failure)?;
        if stage > 3 {
            return Err(failure("invalid index stage"));
        }
        staged.push(StagedEntry {
            path: path.to_owned(),
            mode: (*mode).to_owned(),
            object: (*object).to_owned(),
            stage,
        });
        retained.insert(
            format!("{stage}/{path}"),
            Entry { object: (*object).to_owned(), mode: (*mode).to_owned(), permissions: 0 },
        );
    }
    // A synthetic tree pins every stage's blob without requiring the working index to be merged.
    Ok((super::capture::build_tree(root, &retained)?, staged))
}
