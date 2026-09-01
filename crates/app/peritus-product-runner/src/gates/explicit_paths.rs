//! Deterministic reconciliation of literal task paths with the candidate workspace.

use std::{
    collections::BTreeMap,
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

use peritus_gates::GateExecutionRecord;

#[derive(Clone, Debug, Eq, PartialEq)]
struct PathMention {
    relative: PathBuf,
    required_output: bool,
}

pub(super) fn run(root: &Path, transcript: &str, changed_paths: &[PathBuf]) -> GateExecutionRecord {
    let mentions = extract(root, transcript);
    let mut checked = Vec::new();
    let mut failures = Vec::new();
    for mention in &mentions {
        let present = match fs::symlink_metadata(root.join(&mention.relative)) {
            Ok(_) => true,
            Err(error) if error.kind() == ErrorKind::NotFound => false,
            Err(error) => {
                failures.push(format!(
                    "could not inspect explicit path {}: {error}",
                    mention.relative.display(),
                ));
                false
            }
        };
        if mention.required_output {
            checked.push(format!(
                "  {}: {}",
                mention.relative.display(),
                if present { "present" } else { "MISSING" },
            ));
            if !present {
                failures.push(format!(
                    "required explicit output path is missing: {}",
                    mention.relative.display(),
                ));
            }
        }
        if present {
            continue;
        }
        let expected_name = mention.relative.file_name();
        for candidate in changed_paths.iter().filter(|candidate| {
            candidate.as_path() != mention.relative && candidate.file_name() == expected_name
        }) {
            failures.push(format!(
                "candidate {} has the requested basename but not the explicit path {}",
                candidate.display(),
                mention.relative.display(),
            ));
        }
    }

    failures.sort();
    failures.dedup();
    let mut output = if checked.is_empty() {
        "No explicit output paths require deterministic presence checks.\n".to_owned()
    } else {
        format!("Required explicit output paths ({}):\n{}\n", checked.len(), checked.join("\n"))
    };
    if failures.is_empty() {
        output.push_str("Explicit path reconciliation: PASS\n");
    } else {
        output.push_str("Explicit path reconciliation failures:\n");
        for failure in &failures {
            output.push_str("  - ");
            output.push_str(failure);
            output.push('\n');
        }
    }
    GateExecutionRecord {
        command: "peritus-internal explicit-output-paths".to_owned(),
        label: "Explicit output paths".to_owned(),
        exit_code: Some(i32::from(!failures.is_empty())),
        output,
    }
}

fn extract(root: &Path, transcript: &str) -> Vec<PathMention> {
    let mut paths = BTreeMap::<PathBuf, bool>::new();
    for line in transcript.lines() {
        let words = line.split_whitespace().collect::<Vec<_>>();
        for (index, word) in words.iter().enumerate() {
            let required_output = output_context(&words[..index]);
            let quoted_bare_name =
                required_output && path_noun_context(&words[..index]) && explicitly_delimited(word);
            let Some(relative) = parse_path(root, word, required_output, quoted_bare_name) else {
                continue;
            };
            paths
                .entry(relative)
                .and_modify(|required| *required |= required_output)
                .or_insert(required_output);
        }
    }
    paths
        .into_iter()
        .map(|(relative, required_output)| PathMention { relative, required_output })
        .collect()
}

pub(super) fn required_outputs(root: &Path, transcript: &str) -> Vec<PathBuf> {
    extract(root, transcript)
        .into_iter()
        .filter_map(|mention| mention.required_output.then_some(mention.relative))
        .collect()
}

pub(super) fn requests_single_file(transcript: &str) -> bool {
    transcript.lines().any(|line| {
        let words = line.split_whitespace().collect::<Vec<_>>();
        words.iter().enumerate().any(|(index, word)| {
            let normalized_word = normalized(word);
            let phrase = normalized_word == "singlefile"
                || (normalized_word == "single"
                    && words.get(index + 1).is_some_and(|next| normalized(next) == "file"));
            if !phrase {
                return false;
            }
            let context = &words[index.saturating_sub(6)..index];
            context.iter().any(|candidate| output_verb(candidate))
                && !context.iter().rev().take(4).any(|candidate| negation(candidate))
        })
    })
}

fn parse_path(
    root: &Path,
    raw: &str,
    required_output: bool,
    quoted_bare_name: bool,
) -> Option<PathBuf> {
    if prose_abbreviation(raw) {
        return None;
    }
    let token = trim_delimiters(raw);
    let token = token.strip_suffix('.').unwrap_or(token);
    let token = trim_delimiters(token);
    if token.is_empty()
        || token.contains("://")
        || token.contains('*')
        || token.contains('$')
        || token.contains('=')
        || (email_address(token) && !quoted_bare_name)
    {
        return None;
    }
    let path = Path::new(token);
    let relative = if path.is_absolute() {
        path.strip_prefix(root).ok()?.to_path_buf()
    } else {
        if !required_output || (!token.contains('/') && !token.contains('.') && !quoted_bare_name) {
            return None;
        }
        path.to_path_buf()
    };
    if relative.as_os_str().is_empty()
        || relative.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        return None;
    }
    Some(relative)
}

fn email_address(token: &str) -> bool {
    let Some((local, domain)) = token.split_once('@') else {
        return false;
    };
    !local.is_empty() && !domain.is_empty() && domain.contains('.') && !token.contains('/')
}

fn prose_abbreviation(raw: &str) -> bool {
    if explicitly_delimited(raw) {
        return false;
    }
    let token = raw
        .trim_matches(|character: char| {
            matches!(character, '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';' | ':')
        })
        .to_ascii_lowercase();
    matches!(token.as_str(), "e.g." | "i.e.")
}

fn trim_delimiters(raw: &str) -> &str {
    raw.trim_matches(|character: char| {
        matches!(
            character,
            '`' | '\'' | '"' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';' | ':'
        )
    })
}

fn output_context(words: &[&str]) -> bool {
    let start = words.len().saturating_sub(12);
    let context = &words[start..];
    let Some(trigger) = context.iter().rposition(|word| output_verb(word)) else {
        return false;
    };
    let negation_start = trigger.saturating_sub(2);
    !context[negation_start..trigger].iter().any(|word| negation(word))
}

fn path_noun_context(words: &[&str]) -> bool {
    let start = words.len().saturating_sub(6);
    words[start..].iter().any(|word| {
        matches!(
            normalized(word).as_str(),
            "artifact"
                | "binary"
                | "directory"
                | "executable"
                | "file"
                | "folder"
                | "program"
                | "script"
        )
    })
}

fn explicitly_delimited(raw: &str) -> bool {
    let token = raw.trim_end_matches(['.', ',', ';', ':']);
    let Some(opening) = token.chars().next() else {
        return false;
    };
    let Some(closing) = token.chars().last() else {
        return false;
    };
    token.len() > opening.len_utf8()
        && matches!((opening, closing), ('`', '`') | ('\'', '\'') | ('"', '"'))
}

fn output_verb(word: &str) -> bool {
    matches!(
        normalized(word).as_str(),
        "write"
            | "writes"
            | "create"
            | "creates"
            | "save"
            | "saves"
            | "produce"
            | "produces"
            | "generate"
            | "generates"
            | "emit"
            | "emits"
            | "place"
            | "places"
            | "put"
            | "output"
            | "implement"
            | "implements"
            | "complete"
            | "completes"
            | "update"
            | "updates"
            | "modify"
            | "modifies"
            | "edit"
            | "edits"
            | "add"
            | "adds"
    )
}

fn negation(word: &str) -> bool {
    matches!(normalized(word).as_str(), "not" | "never" | "without" | "avoid" | "dont")
}

fn normalized(word: &str) -> String {
    word.chars().filter(char::is_ascii_alphanumeric).flat_map(char::to_lowercase).collect()
}

#[cfg(test)]
mod tests;
