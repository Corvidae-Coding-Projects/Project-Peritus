//! Read-only inspection of absolute paths explicitly named by the user's task.

use std::{
    collections::{BTreeSet, VecDeque},
    ffi::OsStr,
    fs,
    path::{Component, Path, PathBuf},
};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::{
    effect::limit,
    executor::WorkspaceDeveloperTools,
    inspection::{MAX_FILE_BYTES, entry_kind, read_line_range},
    path::{ignored, tool},
    wire::{bounded_usize, object, required_string},
};
use crate::file_metadata;

const MAX_LIST_ENTRIES: usize = 512;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ExplicitReferences {
    roots: Vec<PathBuf>,
}

impl ExplicitReferences {
    pub(super) fn merge(&mut self, other: &Self) {
        self.roots.extend(other.roots.iter().cloned());
        self.roots.sort();
        self.roots.dedup();
    }

    fn from_task(workspace_root: &Path, task: &str) -> Self {
        let mut roots = BTreeSet::new();
        for token in task_tokens(task) {
            let Some(path) = explicit_absolute_path(&token) else { continue };
            if path_starts_with_case_insensitive(&path, workspace_root) {
                continue;
            }
            roots.insert(path);
        }
        Self { roots: roots.into_iter().collect() }
    }

    fn resolve(&self, raw: &str) -> Result<(PathBuf, PathBuf), DeveloperLoopError> {
        let requested = explicit_absolute_path(raw)
            .ok_or_else(|| tool("reference path must be a normal absolute path"))?;
        let root = self
            .roots
            .iter()
            .filter(|root| path_starts_with_case_insensitive(&requested, root))
            .max_by_key(|root| root.components().count())
            .cloned()
            .ok_or_else(|| {
                tool("reference path was not explicitly named by the user's task; no read authority was granted")
            })?;
        let root_components = root.components().count();
        let resolved_root = case_correct_path(&root)?;
        let resolved_requested =
            case_correct_descendant(&resolved_root, &requested, root_components)?;
        Ok((resolved_root, resolved_requested))
    }

    pub(super) fn extend_from_task(&mut self, workspace_root: &Path, task: &str) {
        let added = Self::from_task(workspace_root, task);
        self.roots.extend(added.roots);
        self.roots.sort();
        self.roots.dedup();
    }
}

impl WorkspaceDeveloperTools {
    #[must_use]
    pub(crate) fn with_reference_contract(mut self, task: &str) -> Self {
        self.references = ExplicitReferences::from_task(&self.root, task);
        self
    }
}

pub(super) fn list(
    references: &ExplicitReferences,
    arguments: &Value,
) -> Result<Value, DeveloperLoopError> {
    if arguments.get("cursor").is_some() {
        return Err(tool(
            "workspace continuation cursors cannot be applied to external reference listings",
        ));
    }
    let (root, start) = references.resolve(required_string(arguments, "path")?)?;
    let metadata = fs::symlink_metadata(&start).map_err(|error| reference_io_error(&error))?;
    if !metadata.is_dir() {
        return Err(tool("external reference path is not a directory"));
    }
    let depth = bounded_usize(arguments, "depth", 3, 1, 6);
    let mut queue = VecDeque::from([(start, 0_usize)]);
    let mut entries = Vec::new();
    let mut truncated = false;
    'walk: while let Some((directory, level)) = queue.pop_front() {
        let children = match fs::read_dir(&directory) {
            Ok(children) => children,
            Err(error) if level == 0 => return Err(tool(error.to_string())),
            Err(_) => continue,
        };
        let mut children = children.filter_map(Result::ok).collect::<Vec<_>>();
        children.sort_by_key(fs::DirEntry::file_name);
        for child in children {
            let path = child.path();
            let relative = path.strip_prefix(&root).unwrap_or(&path);
            if ignored(relative) {
                continue;
            }
            let Ok(metadata) = fs::symlink_metadata(&path) else { continue };
            let kind = metadata.file_type();
            entries.push(object(vec![
                ("path", Value::String(path.to_string_lossy().into_owned())),
                ("kind", Value::String(entry_kind(kind).to_owned())),
                ("bytes", Value::from(metadata.len())),
                ("permissions", Value::String(file_metadata::permissions(&metadata))),
            ]));
            if entries.len() == MAX_LIST_ENTRIES {
                truncated = true;
                break 'walk;
            }
            if kind.is_dir() && level + 1 < depth {
                queue.push_back((path, level + 1));
            }
        }
    }
    Ok(object(vec![
        ("reference_root", Value::String(root.to_string_lossy().into_owned())),
        ("path_kind", Value::String("explicit-absolute-reference".to_owned())),
        ("entries", Value::Array(entries)),
        ("truncated", Value::Bool(truncated)),
    ]))
}

pub(super) fn read(
    references: &ExplicitReferences,
    arguments: &Value,
) -> Result<Value, DeveloperLoopError> {
    if arguments.get("cursor").is_some() {
        return Err(tool(
            "workspace continuation cursors cannot be applied to external reference reads",
        ));
    }
    let (root, path) = references.resolve(required_string(arguments, "path")?)?;
    let metadata = fs::symlink_metadata(&path).map_err(|error| reference_io_error(&error))?;
    if !metadata.is_file() {
        return Err(tool("external reference path is not a regular text file"));
    }
    let start = bounded_usize(arguments, "start_line", 1, 1, usize::MAX);
    let end = bounded_usize(arguments, "end_line", start.saturating_add(499), start, usize::MAX);
    let explicit_range =
        arguments.get("start_line").is_some() || arguments.get("end_line").is_some();
    if metadata.len() > MAX_FILE_BYTES as u64 && !explicit_range {
        return Err(tool(
            "reference file exceeds the inline byte bound; specify start_line and end_line",
        ));
    }
    let lines = read_line_range(&path, start, end)?;
    Ok(object(vec![
        ("reference_root", Value::String(root.to_string_lossy().into_owned())),
        ("path", Value::String(path.to_string_lossy().into_owned())),
        ("content", Value::String(limit(&lines))),
        ("start_line", Value::from(start)),
        ("end_line", Value::from(end)),
        ("bytes", Value::from(metadata.len())),
        ("permissions", Value::String(file_metadata::permissions(&metadata))),
    ]))
}

fn reference_io_error(error: &std::io::Error) -> DeveloperLoopError {
    if error.kind() == std::io::ErrorKind::NotFound {
        reference_not_found()
    } else {
        tool(error.to_string())
    }
}

fn reference_not_found() -> DeveloperLoopError {
    tool(
        "not_found: no exact or uniquely case-insensitive match exists for the explicit reference path",
    )
}

fn path_starts_with_case_insensitive(path: &Path, root: &Path) -> bool {
    let mut components = path.components();
    root.components().all(|root_component| {
        components.next().is_some_and(|component| components_match(component, root_component))
    })
}

fn components_match(left: Component<'_>, right: Component<'_>) -> bool {
    match (left, right) {
        (Component::RootDir, Component::RootDir)
        | (Component::CurDir, Component::CurDir)
        | (Component::ParentDir, Component::ParentDir) => true,
        (Component::Prefix(left), Component::Prefix(right)) => {
            names_match(left.as_os_str(), right.as_os_str())
        }
        (Component::Normal(left), Component::Normal(right)) => names_match(left, right),
        _ => false,
    }
}

fn names_match(left: &OsStr, right: &OsStr) -> bool {
    left == right
        || left
            .to_str()
            .zip(right.to_str())
            .is_some_and(|(left, right)| left.to_lowercase() == right.to_lowercase())
}

fn case_correct_path(path: &Path) -> Result<PathBuf, DeveloperLoopError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => current.push(component.as_os_str()),
            Component::Normal(part) => current = case_correct_component(&current, part)?,
            Component::CurDir | Component::ParentDir => {
                return Err(tool("reference path contains an invalid component"));
            }
        }
    }
    // Ancestors of the user-named root may be system aliases such as macOS /var. The named
    // target itself and every descendant must remain ordinary entries rather than redirects.
    reject_symlink(&current)?;
    Ok(current)
}

fn case_correct_descendant(
    root: &Path,
    requested: &Path,
    root_components: usize,
) -> Result<PathBuf, DeveloperLoopError> {
    let mut current = root.to_path_buf();
    for component in requested.components().skip(root_components) {
        let Component::Normal(part) = component else {
            return Err(tool("reference path contains an invalid descendant component"));
        };
        current = case_correct_component(&current, part)?;
        reject_symlink(&current)?;
    }
    Ok(current)
}

fn case_correct_component(parent: &Path, requested: &OsStr) -> Result<PathBuf, DeveloperLoopError> {
    // A successful lookup does not prove the spelling is exact on a case-insensitive filesystem.
    // Enumerate actual names, and defer ambiguity until an exact entry has had a chance to win.
    let mut matched = None;
    let mut ambiguous = false;
    for entry in fs::read_dir(parent).map_err(|error| reference_io_error(&error))? {
        let entry = entry.map_err(|error| tool(error.to_string()))?;
        let actual = entry.file_name();
        if !names_match(&actual, requested) {
            continue;
        }
        if actual == requested {
            return Ok(entry.path());
        }
        ambiguous |= matched.is_some();
        matched = Some(entry.path());
    }
    if ambiguous {
        return Err(tool(
            "ambiguous: multiple filesystem entries match a reference path component when case is ignored; use exact casing",
        ));
    }
    if let Some(path) = matched {
        return Ok(path);
    }
    // Windows short-name aliases may resolve without appearing among the long entry names.
    let alias = parent.join(requested);
    fs::symlink_metadata(&alias).map_err(|error| reference_io_error(&error))?;
    Ok(alias)
}

fn reject_symlink(path: &Path) -> Result<(), DeveloperLoopError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| reference_io_error(&error))?;
    if metadata.file_type().is_symlink() {
        Err(tool("symbolic links are not explicit reference targets"))
    } else {
        Ok(())
    }
}

fn explicit_absolute_path(raw: &str) -> Option<PathBuf> {
    let token = raw.trim_matches(|character: char| {
        matches!(
            character,
            '`' | '\'' | '"' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';'
        )
    });
    let token = token.strip_suffix('.').unwrap_or(token);
    if token.is_empty()
        || token.len() > 4096
        || token.contains("://")
        || token.contains(char::is_control)
    {
        return None;
    }
    let path = PathBuf::from(token);
    let mut normal = 0_usize;
    for component in path.components() {
        match component {
            Component::Normal(_) => normal = normal.saturating_add(1),
            Component::RootDir | Component::Prefix(_) => {}
            Component::CurDir | Component::ParentDir => return None,
        }
    }
    (path.is_absolute() && normal > 0).then_some(path)
}

fn task_tokens(task: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for character in task.chars() {
        if let Some(expected) = quote {
            if character == expected {
                quote = None;
            } else {
                current.push(character);
            }
        } else if current.is_empty() && matches!(character, '`' | '\'' | '"') {
            quote = Some(character);
        } else if character.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_paths_are_exact_bounded_and_outside_the_workspace() {
        let workspace = Path::new("/work/output");
        let references = ExplicitReferences::from_task(
            workspace,
            "match '/reference files/invoices' and /WORK/OUTPUT/src while ignoring https://host/a",
        );
        assert!(references.roots.iter().all(|root| !root.starts_with(workspace)));
        #[cfg(unix)]
        assert_eq!(references.roots, vec![PathBuf::from("/reference files/invoices")]);
    }
}
