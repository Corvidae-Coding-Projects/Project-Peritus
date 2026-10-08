//! Native-path resolution for task-named external references.

use std::{
    ffi::OsStr,
    fs,
    path::{Component, Path, PathBuf},
};

use peritus_agent::DeveloperLoopError;

use super::{
    path::tool,
    reference::{reference_io_error, reference_not_found},
};

pub(super) fn path_starts_with_native(path: &Path, root: &Path) -> bool {
    let root_count = root.components().count();
    let mut prefix = PathBuf::new();
    for component in path.components().take(root_count) {
        prefix.push(component.as_os_str());
    }
    if prefix == root {
        return true;
    }
    let (Ok(prefix), Ok(root)) = (case_correct_path(&prefix), case_correct_path(root)) else {
        return false;
    };
    prefix
        .components()
        .zip(root.components())
        .all(|(left, right)| resolved_components_match(left, right))
        && prefix.components().count() == root.components().count()
}

fn resolved_components_match(left: Component<'_>, right: Component<'_>) -> bool {
    match (left, right) {
        (Component::RootDir, Component::RootDir)
        | (Component::CurDir, Component::CurDir)
        | (Component::ParentDir, Component::ParentDir) => true,
        (Component::Prefix(left), Component::Prefix(right)) => {
            prefixes_match(left.as_os_str(), right.as_os_str())
        }
        (Component::Normal(left), Component::Normal(right)) => left == right,
        _ => false,
    }
}

#[cfg(windows)]
fn prefixes_match(left: &OsStr, right: &OsStr) -> bool {
    names_match(left, right)
}

#[cfg(not(windows))]
fn prefixes_match(left: &OsStr, right: &OsStr) -> bool {
    left == right
}

fn names_match(left: &OsStr, right: &OsStr) -> bool {
    left == right
        || left
            .to_str()
            .zip(right.to_str())
            .is_some_and(|(left, right)| left.to_lowercase() == right.to_lowercase())
}

pub(super) fn case_correct_path(path: &Path) -> Result<PathBuf, DeveloperLoopError> {
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

pub(super) fn case_correct_descendant(
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
    // Resolve a case alias only when the filesystem itself accepts that spelling.
    let alias = parent.join(requested);
    let alias_resolves = fs::symlink_metadata(&alias).is_ok();
    let mut matched = None;
    let mut ambiguous = false;
    for entry in fs::read_dir(parent).map_err(|error| reference_io_error(&error))? {
        let entry = entry.map_err(|error| tool(error.to_string()))?;
        let actual = entry.file_name();
        if actual == requested {
            return Ok(entry.path());
        }
        if !alias_resolves || !names_match(&actual, requested) {
            continue;
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
    if alias_resolves {
        // Windows short-name aliases may resolve without appearing among long entry names.
        Ok(alias)
    } else {
        Err(reference_not_found())
    }
}

fn reject_symlink(path: &Path) -> Result<(), DeveloperLoopError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| reference_io_error(&error))?;
    if metadata.file_type().is_symlink() {
        Err(tool("symbolic links are not explicit reference targets"))
    } else {
        Ok(())
    }
}

pub(super) fn explicit_absolute_path(raw: &str, quoted: bool) -> Option<PathBuf> {
    let wrapped = matching_quote(raw).is_some();
    let preserve_punctuation = quoted || wrapped;
    let token = if quoted {
        matching_quote(raw).unwrap_or(raw)
    } else if let Some(inner) = matching_quote(raw) {
        inner
    } else {
        raw.trim_matches(|character: char| {
            matches!(
                character,
                '`' | '\'' | '"' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';'
            )
        })
    };
    let token = if preserve_punctuation { token } else { token.strip_suffix('.').unwrap_or(token) };
    if token.is_empty() || token.contains("://") || token.contains(char::is_control) {
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

fn matching_quote(value: &str) -> Option<&str> {
    let first = value.chars().next()?;
    let last = value.chars().next_back()?;
    (value.len() >= 2 && matches!(first, '`' | '\'' | '"') && first == last)
        .then(|| &value[first.len_utf8()..value.len() - last.len_utf8()])
}

pub(super) fn task_tokens(task: &str) -> Vec<(String, bool)> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut characters = task.chars();
    while let Some(character) = characters.next() {
        if matches!(character, '`' | '\'' | '"') {
            if !current.is_empty() {
                tokens.push((std::mem::take(&mut current), false));
            }
            let mut quoted = String::new();
            for next in characters.by_ref() {
                if next == character {
                    break;
                }
                quoted.push(next);
            }
            if !quoted.is_empty() {
                tokens.push((quoted, true));
            }
        } else if character.is_whitespace() {
            if !current.is_empty() {
                tokens.push((std::mem::take(&mut current), false));
            }
        } else {
            current.push(character);
        }
    }
    if !current.is_empty() {
        tokens.push((current, false));
    }
    tokens
}
