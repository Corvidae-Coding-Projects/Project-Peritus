//! Deterministic ordinary-Git source consumption with each repository's own index.

use super::{
    super::{ManagedBaseline, failure, git, git_path},
    SnapshotOrigin,
};
use crate::ProductRunnerError;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug)]
pub(super) struct ExportContext {
    origin: SnapshotOrigin,
    roots: BTreeSet<String>,
    paths: BTreeSet<String>,
}

impl ExportContext {
    pub(super) const fn new(origin: SnapshotOrigin) -> Self {
        Self { origin, roots: BTreeSet::new(), paths: BTreeSet::new() }
    }

    pub(super) const fn origin(&self) -> SnapshotOrigin {
        self.origin
    }

    pub(super) fn record_roots(
        &mut self,
        prefix: &Path,
        baseline: &ManagedBaseline,
    ) -> Result<(), ProductRunnerError> {
        let root = if prefix.as_os_str().is_empty() {
            String::new()
        } else {
            git_path::tree_name(prefix)?
        };
        self.roots.insert(root);
        for (path, child) in &baseline.nested {
            self.record_roots(&prefix.join(path), child)?;
        }
        Ok(())
    }

    pub(super) fn record_paths(
        &mut self,
        root: &Path,
        prefix: &str,
        original: &str,
        current: &str,
    ) -> Result<(), ProductRunnerError> {
        let bytes = git(
            root,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--name-only",
                "-z",
                original,
                current,
                "--",
            ],
            None,
        )?;
        for path in bytes.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
            let path = std::str::from_utf8(path).map_err(failure)?;
            self.paths.insert(format!("{prefix}{path}"));
        }
        Ok(())
    }

    pub(super) fn finish(self, bytes: Vec<u8>) -> Result<Vec<u8>, ProductRunnerError> {
        if self.roots.len() < 2 || bytes.is_empty() {
            return Ok(bytes);
        }
        let mut owned = self
            .roots
            .iter()
            .map(|root| (root.clone(), BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();
        for path in &self.paths {
            if self.roots.contains(path) {
                // Producer gitlinks remain in the raw patch, but source consumption
                // stages the consumer's child commits after applying its file changes.
                continue;
            }
            let root = self.owner(path)?;
            let relative = relative(root, path)?;
            owned
                .get_mut(root)
                .ok_or_else(|| failure("missing source root"))?
                .insert(relative.to_owned());
        }
        let mut header = String::from(
            "# Peritus source patch: separate Git repositories have separate indexes.\n\
             # Commands below are JSON argv, not shell text. Replace PATCH with this file's absolute path.\n\
             # Run from the consumer parent with its genuine submodules initialized at the source baseline.\n\
             # Apply once, then stage each owning root; one parent git apply --index cannot own child files.\n\
             # Producer gitlink hunks are retained as metadata and excluded from this source workflow.\n\
             # Consumer child commits preserve existing histories; their resulting links are staged in parents.\n",
        );
        let mut apply = vec!["git".to_owned(), "apply".to_owned(), "--allow-empty".to_owned()];
        for root in self.roots.iter().filter(|root| !root.is_empty()) {
            apply.push(format!("--exclude={}", literal_pattern(root)));
        }
        apply.push("PATCH".to_owned());
        command(&mut header, "apply", "", &apply)?;
        for (root, paths) in &owned {
            if paths.is_empty() {
                continue;
            }
            let mut stage = git_at(root);
            stage.extend(["add".to_owned(), "--".to_owned()]);
            stage.extend(paths.iter().cloned());
            command(&mut header, "stage-source", root, &stage)?;
        }
        header.push_str("# Deepest children first: inspect the listed scope, commit only when it has staged changes, then stage its link.\n");
        let mut children = self.roots.iter().filter(|root| !root.is_empty()).collect::<Vec<_>>();
        children.sort_by_key(|root| (std::cmp::Reverse(root.split('/').count()), *root));
        for child in children {
            let paths = owned.get(child).ok_or_else(|| failure("missing child source scope"))?;
            scoped_commit(&mut header, child, paths)?;
            let parent = self.parent(child)?;
            let link = relative(parent, child)?;
            let mut stage = git_at(parent);
            stage.extend(["add".to_owned(), "--".to_owned(), link.to_owned()]);
            command(&mut header, "stage-child-link-after-commit", parent, &stage)?;
            owned
                .get_mut(parent)
                .ok_or_else(|| failure("missing parent source scope"))?
                .insert(link.to_owned());
        }
        header.push_str(
            "# Inspect and commit the parent last; --only preserves unrelated staged paths.\n",
        );
        scoped_commit(
            &mut header,
            "",
            owned.get("").ok_or_else(|| failure("missing parent scope"))?,
        )?;
        header.push('\n');
        let mut output = header.into_bytes();
        output.extend(bytes);
        Ok(output)
    }

    fn owner<'a>(&'a self, path: &str) -> Result<&'a str, ProductRunnerError> {
        let mut ancestor = path;
        while let Some((parent, _)) = ancestor.rsplit_once('/') {
            if let Some(root) = self.roots.get(parent) {
                return Ok(root);
            }
            ancestor = parent;
        }
        self.roots.get("").map(String::as_str).ok_or_else(|| failure("missing parent source root"))
    }

    fn parent<'a>(&'a self, root: &str) -> Result<&'a str, ProductRunnerError> {
        self.owner(root)
    }
}

fn relative<'a>(root: &str, path: &'a str) -> Result<&'a str, ProductRunnerError> {
    if root.is_empty() {
        Ok(path)
    } else {
        path.strip_prefix(root)
            .and_then(|path| path.strip_prefix('/'))
            .ok_or_else(|| failure("path is outside source root"))
    }
}

fn git_at(root: &str) -> Vec<String> {
    if root.is_empty() {
        vec!["git".to_owned(), "--literal-pathspecs".to_owned()]
    } else {
        vec!["git".to_owned(), "--literal-pathspecs".to_owned(), "-C".to_owned(), root.to_owned()]
    }
}

fn literal_pattern(path: &str) -> String {
    let mut pattern = String::with_capacity(path.len());
    for character in path.chars() {
        if matches!(character, '\\' | '*' | '?' | '[' | ']') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern
}

fn command(
    header: &mut String,
    action: &str,
    root: &str,
    argv: &[String],
) -> Result<(), ProductRunnerError> {
    let value = serde_json::json!({"action": action, "root": if root.is_empty() { "." } else { root }, "argv": argv});
    header.push_str("# ");
    header.push_str(&serde_json::to_string(&value).map_err(failure)?);
    header.push('\n');
    Ok(())
}

fn scoped_commit(
    header: &mut String,
    root: &str,
    paths: &BTreeSet<String>,
) -> Result<(), ProductRunnerError> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut inspect = git_at(root);
    inspect.extend([
        "diff".to_owned(),
        "--cached".to_owned(),
        "--quiet".to_owned(),
        "--".to_owned(),
    ]);
    inspect.extend(paths.iter().cloned());
    command(header, "scope-has-changes-exit-1", root, &inspect)?;
    let mut commit = git_at(root);
    commit.extend([
        "commit".to_owned(),
        "--only".to_owned(),
        "-m".to_owned(),
        "Apply Peritus source changes".to_owned(),
        "--".to_owned(),
    ]);
    commit.extend(paths.iter().cloned());
    command(header, "commit-scope-if-changed", root, &commit)
}
