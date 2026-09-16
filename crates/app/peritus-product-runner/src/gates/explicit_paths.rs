//! Deterministic reconciliation of literal task paths with the candidate workspace.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

use peritus_gates::GateExecutionRecord;

mod alternatives;
mod extraction;
mod language;
mod removals;

use extraction::extract;
use language::{
    clause_start, conditional_clause, explicitly_delimited, negation, normalized, output_context,
    output_verb, path_context, path_noun_context,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct PathMention {
    relative: PathBuf,
    required_output: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PathRequirements {
    mentions: Vec<PathMention>,
    alternatives: Vec<Vec<PathBuf>>,
    removals: Vec<removals::Removal>,
}

pub(super) fn run(root: &Path, transcript: &str, changed_paths: &[PathBuf]) -> GateExecutionRecord {
    let requirements = extract(root, transcript);
    let mut checked = Vec::new();
    let mut failures = Vec::new();
    let mut presence = BTreeMap::new();
    for mention in &requirements.mentions {
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
        presence.insert(mention.relative.clone(), present);
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
    }
    let mut required_missing = requirements
        .mentions
        .iter()
        .filter(|mention| {
            mention.required_output && !presence.get(&mention.relative).copied().unwrap_or(false)
        })
        .map(|mention| mention.relative.clone())
        .collect::<BTreeSet<_>>();
    for alternatives in &requirements.alternatives {
        let states = alternatives
            .iter()
            .map(|path| {
                let present = presence.get(path).copied().unwrap_or(false);
                format!("{}: {}", path.display(), if present { "present" } else { "missing" })
            })
            .collect::<Vec<_>>();
        let satisfied =
            alternatives.iter().any(|path| presence.get(path).copied().unwrap_or(false));
        checked.push(format!("  one of [{}]", states.join(", ")));
        if !satisfied {
            for path in alternatives {
                required_missing.insert(path.clone());
            }
            failures.push(format!(
                "at least one alternative explicit output path is required: {}",
                alternatives
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" or "),
            ));
        }
    }
    for relative in required_missing {
        let expected_name = relative.file_name();
        for candidate in changed_paths.iter().filter(|candidate| {
            candidate.as_path() != relative && candidate.file_name() == expected_name
        }) {
            failures.push(format!(
                "candidate {} has the requested basename but not the explicit path {}",
                candidate.display(),
                relative.display(),
            ));
        }
    }

    removals::check_all(root, &requirements.removals, &mut checked, &mut failures);

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

fn list_item_path_index(words: &[&str]) -> Option<usize> {
    let marker = *words.first()?;
    let numbered = marker
        .strip_suffix('.')
        .is_some_and(|number| !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit()));
    (words.len() > 1 && (matches!(marker, "-" | "*" | "+") || numbered)).then_some(1)
}

fn output_list_directory(root: &Path, words: &[&str]) -> Option<PathBuf> {
    words.iter().enumerate().rev().find_map(|(index, word)| {
        let word = path_token(word);
        (trim_delimiters(word).ends_with('/') && path_context(&words[..index]))
            .then(|| parse_path(root, word, true, false, true))
            .flatten()
    })
}

pub(super) fn required_outputs(root: &Path, transcript: &str) -> Vec<PathBuf> {
    extract(root, transcript)
        .mentions
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
    relative_path_context: bool,
) -> Option<PathBuf> {
    let raw = path_token(raw);
    if prose_abbreviation(raw) || unresolved_placeholder(raw) {
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
        if token.contains('/')
            && !token.contains('.')
            && !explicitly_delimited(raw)
            && !relative_path_context
        {
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
    let relative = relative
        .components()
        .filter(|component| *component != Component::CurDir)
        .collect::<PathBuf>();
    (!relative.as_os_str().is_empty()).then_some(relative)
}

fn unresolved_placeholder(raw: &str) -> bool {
    raw.contains(['{', '}', '<', '>'])
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

fn descriptive_extension(raw: &str, next: Option<&str>) -> bool {
    if explicitly_delimited(raw) || next.is_none_or(|word| normalized(word) != "files") {
        return false;
    }
    let token = trim_delimiters(raw);
    let Some(extension) = token.strip_prefix('.') else {
        return false;
    };
    !extension.is_empty()
        && extension.chars().all(|character| character.is_ascii_alphanumeric() || character == '.')
}

fn trim_delimiters(raw: &str) -> &str {
    raw.trim_matches(|character: char| {
        matches!(
            character,
            '`' | '\'' | '"' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';' | ':'
        )
    })
}

fn path_token(raw: &str) -> &str {
    let Some(opening) = raw.chars().next().filter(|ch| matches!(ch, '`' | '\'' | '"')) else {
        return raw;
    };
    let after_opening = &raw[opening.len_utf8()..];
    let Some(closing) = after_opening.find(opening).filter(|index| *index > 0) else {
        return raw;
    };
    let end = opening.len_utf8() + closing + opening.len_utf8();
    // A prose dash after a closed literal is not part of its path. Dashes inside the
    // delimiters, unclosed literals, and concatenated path suffixes keep their bytes.
    if raw[end..].starts_with(['—', '–']) { &raw[..end] } else { raw }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "explicit_paths/removal_tests.rs"]
mod removal_tests;

#[cfg(test)]
#[path = "explicit_paths/scoped_list_tests.rs"]
mod scoped_list_tests;

#[cfg(test)]
#[path = "explicit_paths/context_tests.rs"]
mod context_tests;
