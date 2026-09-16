//! Chronological reduction of literal output and removal instructions.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use super::{
    PathMention, PathRequirements, alternatives, conditional_clause, descriptive_extension,
    explicitly_delimited, list_item_path_index, normalized, output_context, output_list_directory,
    parse_path, path_context, path_noun_context, removals, trim_delimiters,
};

pub(super) fn extract(root: &Path, transcript: &str) -> PathRequirements {
    let mut paths = BTreeMap::<PathBuf, bool>::new();
    let mut alternatives = Vec::new();
    let mut removals = Vec::<removals::Removal>::new();
    let mut output_list = false;
    let mut output_directory = None;
    let mut output_list_indentation = None;
    for line in transcript.lines() {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let list_item = list_item_path_index(&words);
        let listed_output = if output_list && list_item.is_some() {
            let indentation = line.len() - line.trim_start().len();
            let declaration_level = *output_list_indentation.get_or_insert(indentation);
            match indentation.cmp(&declaration_level) {
                Ordering::Equal => list_item,
                Ordering::Less => {
                    output_list = false;
                    output_directory = None;
                    output_list_indentation = None;
                    None
                }
                Ordering::Greater => None,
            }
        } else {
            None
        };
        if !words.is_empty() && list_item.is_none() {
            // Format/schema bullets describe an artifact's contents, not sibling files.
            // Explicit file instructions within those bullets still use their own path cues.
            output_list = line.trim_end().ends_with(':')
                && output_context(&words)
                && !words
                    .last()
                    .is_some_and(|word| matches!(normalized(word).as_str(), "format" | "schema"))
                && !conditional_clause(&words);
            output_directory = output_list.then(|| output_list_directory(root, &words)).flatten();
            output_list_indentation = None;
        }
        let mut line_mentions = Vec::new();
        let mut line_removals = BTreeMap::new();
        for (index, word) in words.iter().enumerate() {
            if descriptive_extension(word, words.get(index + 1).copied()) {
                continue;
            }
            // Only the leading list path inherits the declaration, not inputs in its description.
            let listed_path = listed_output == Some(index);
            let required_output = (listed_path || output_context(&words[..index]))
                && !conditional_clause(&words[..index]);
            let quoted_bare_name = required_output
                && (listed_path || path_noun_context(&words[..index]))
                && explicitly_delimited(word);
            let removal = removals::directive(&words, index);
            let relative_path_context = path_context(&words[..index]) || removal.is_some();
            let Some(mut relative) = parse_path(
                root,
                word,
                required_output || removal.is_some(),
                quoted_bare_name || removal.is_some() && explicitly_delimited(word),
                relative_path_context,
            ) else {
                continue;
            };
            if listed_path
                && !Path::new(trim_delimiters(word)).is_absolute()
                && let Some(directory) = &output_directory
                && !relative.starts_with(directory)
            {
                relative = directory.join(relative);
            }
            if let Some(contents_only) = removal {
                line_removals.insert(index, contents_only);
            }
            line_mentions.push((index, relative, required_output && removal.is_none()));
        }
        let line_alternatives = alternatives::groups(&words, &line_mentions);
        let alternative_paths =
            line_alternatives.iter().flatten().cloned().collect::<BTreeSet<_>>();
        alternatives.extend(line_alternatives);
        for (index, relative, required_output) in line_mentions {
            if let Some(contents_only) = line_removals.get(&index).copied() {
                let removal = removals::Removal::new(relative.clone(), contents_only);
                apply_removal(removal, &mut paths, &mut alternatives, &mut removals);
            } else if required_output {
                removals.retain_mut(|removal| removal.retain_output(&relative));
            }
            let individually_required = required_output && !alternative_paths.contains(&relative);
            paths
                .entry(relative)
                .and_modify(|required| *required |= individually_required)
                .or_insert(individually_required);
        }
    }
    alternatives.sort();
    alternatives.dedup();
    PathRequirements {
        mentions: paths
            .into_iter()
            .map(|(relative, required_output)| PathMention { relative, required_output })
            .collect(),
        alternatives,
        removals,
    }
}

fn apply_removal(
    removal: removals::Removal,
    paths: &mut BTreeMap<PathBuf, bool>,
    alternatives: &mut Vec<Vec<PathBuf>>,
    removals: &mut Vec<removals::Removal>,
) {
    for (path, required) in paths {
        if removal.covers(path) {
            *required = false;
        }
    }
    for group in alternatives.iter_mut() {
        group.retain(|path| !removal.covers(path));
    }
    alternatives.retain(|group| !group.is_empty());
    removals.retain(|previous| previous.relative != removal.relative);
    removals.push(removal);
}
