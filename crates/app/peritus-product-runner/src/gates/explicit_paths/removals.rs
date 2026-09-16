//! Literal removal directives and their final-state checks; never executes cleanup.

use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use super::{
    clause_start, explicitly_delimited, negation, normalized, output_verb, prose_abbreviation,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Removal {
    pub(super) relative: PathBuf,
    contents_only: bool,
    retained: BTreeSet<PathBuf>,
}

impl Removal {
    pub(super) const fn new(relative: PathBuf, contents_only: bool) -> Self {
        Self { relative, contents_only, retained: BTreeSet::new() }
    }

    pub(super) fn covers(&self, path: &Path) -> bool {
        path.starts_with(&self.relative) && (!self.contents_only || path != self.relative)
    }

    pub(super) fn retain_output(&mut self, path: &Path) -> bool {
        if path == self.relative {
            return false;
        }
        if path.starts_with(&self.relative) {
            self.retained.insert(path.to_path_buf());
        }
        true
    }

    pub(super) fn remaining(&self, root: &Path) -> io::Result<Option<PathBuf>> {
        let mut ancestor = PathBuf::new();
        for component in self.relative.components() {
            ancestor.push(component.as_os_str());
            match fs::symlink_metadata(root.join(&ancestor)) {
                Ok(metadata) if metadata.is_symlink() => return Ok(Some(ancestor)),
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error),
            }
        }
        let metadata = match fs::symlink_metadata(root.join(&self.relative)) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if !self.contents_only && self.retained.is_empty() || !metadata.is_dir() {
            return Ok(Some(self.relative.clone()));
        }
        self.remaining_children(root, &self.relative)
    }

    fn remaining_children(&self, root: &Path, directory: &Path) -> io::Result<Option<PathBuf>> {
        for entry in fs::read_dir(root.join(directory))? {
            let entry = entry?;
            let relative = directory.join(entry.file_name());
            if self.retained.iter().any(|retained| relative.starts_with(retained)) {
                continue;
            }
            let needed_parent =
                self.retained.iter().any(|retained| retained.starts_with(&relative));
            if !needed_parent || !entry.file_type()?.is_dir() {
                return Ok(Some(relative));
            }
            if let Some(remaining) = self.remaining_children(root, &relative)? {
                return Ok(Some(remaining));
            }
        }
        Ok(None)
    }
}

pub(super) fn check_all(
    root: &Path,
    removals: &[Removal],
    checked: &mut Vec<String>,
    failures: &mut Vec<String>,
) {
    for removal in removals {
        match removal.remaining(root) {
            Ok(None) => {
                checked.push(format!("  {}: removal satisfied", removal.relative.display()));
            }
            Ok(Some(path)) => {
                failures.push(format!("path remains after explicit removal: {}", path.display()));
            }
            Err(error) => failures.push(format!(
                "could not inspect explicit removal {}: {error}",
                removal.relative.display(),
            )),
        }
    }
}

/// Recognizes an unconditional imperative in the current clause, not historical prose.
pub(super) fn directive(words: &[&str], index: usize) -> Option<bool> {
    let start = clause_start(&words[..index]);
    let end = words[index..]
        .iter()
        .position(|word| word.ends_with(['.', '?', '!', ';']) && !prose_abbreviation(word))
        .map_or(words.len(), |offset| index + offset + 1);
    if words[start..end].iter().any(|word| matches!(normalized(word).as_str(), "if" | "unless")) {
        return None;
    }
    let context = &words[start..index];
    let trigger = context.iter().rposition(|word| removal_verb(word))?;
    if explicitly_delimited(context[trigger])
        || context[..trigger].iter().any(|word| negation(word))
        || context[..trigger].iter().any(|word| {
            matches!(normalized(word).as_str(), "read" | "record" | "log" | "quote" | "describe")
        })
        || context[trigger + 1..].iter().any(|word| output_verb(word))
    {
        return None;
    }
    let prefix = &context[..trigger];
    // Imperatives may be bullets or carry a polite/modal instruction prefix.
    // Past/future narrative ("we removed", "the report says delete") is not authority.
    if prefix.iter().any(|word| {
        !matches!(normalized(word).as_str(), "" | "please" | "then" | "now" | "must" | "you")
    }) {
        return None;
    }
    let trailing = &context[trigger + 1..];
    let has_path_noun = trailing.iter().any(|word| {
        matches!(
            normalized(word).as_str(),
            "file"
                | "files"
                | "directory"
                | "directories"
                | "folder"
                | "folders"
                | "artifact"
                | "artifacts"
                | "contents"
        )
    });
    let cue = normalized(context.last()?);
    let direct_path = removal_verb(context.last()?)
        || matches!(
            cue.as_str(),
            "file"
                | "files"
                | "directory"
                | "directories"
                | "folder"
                | "folders"
                | "artifact"
                | "artifacts"
                | "contents"
        )
        || matches!(cue.as_str(), "at" | "in" | "under" | "named" | "called") && has_path_noun
        || cue == "the" && trailing.len() == 1;
    if !direct_path {
        return None;
    }
    Some(
        matches!(cue.as_str(), "under" | "in")
            && trailing.iter().any(|word| {
                matches!(
                    normalized(word).as_str(),
                    "files" | "directories" | "folders" | "contents"
                )
            }),
    )
}

fn removal_verb(word: &str) -> bool {
    matches!(normalized(word).as_str(), "remove" | "delete")
}
