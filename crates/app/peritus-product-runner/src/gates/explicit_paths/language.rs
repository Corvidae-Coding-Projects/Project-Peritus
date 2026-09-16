//! Clause and naming cues for literal path requirements.

use super::{path_token, prose_abbreviation};

pub(super) fn output_context(words: &[&str]) -> bool {
    // An output verb authorizes paths in its clause, not a later verification instruction.
    let start = words.len().saturating_sub(12).max(clause_start(words));
    let context = &words[start..];
    let Some(trigger) = context.iter().rposition(|word| output_verb(word)) else {
        return false;
    };
    let negation_start = trigger.saturating_sub(2);
    if context[negation_start..trigger].iter().any(|word| negation(word)) {
        return false;
    }
    let trailing = &context[trigger + 1..];
    // A neighboring path identifies an existing location anchor, not another output.
    let neighboring_anchor = trailing
        .last()
        .is_some_and(|word| matches!(normalized(word).as_str(), "beside" | "alongside"))
        || trailing.len() >= 2
            && normalized(trailing[trailing.len() - 1]) == "to"
            && matches!(normalized(trailing[trailing.len() - 2]).as_str(), "next" | "adjacent");
    if neighboring_anchor || trailing.last().is_some_and(|word| normalized(word) == "from") {
        return false;
    }
    !ambiguous_addition_verb(context[trigger])
        || trailing.len() <= 3
        || trailing.iter().any(|word| path_noun(word))
}

pub(super) fn path_noun_context(words: &[&str]) -> bool {
    let start = words.len().saturating_sub(6);
    words[start..].iter().any(|word| path_noun(word))
}

pub(super) fn path_context(words: &[&str]) -> bool {
    let Some((last, preceding)) = words.split_last() else {
        return false;
    };
    // A content modifier after an output's name cannot inherit that name's path cue.
    // Determiners and naming connectors still permit ordinary explicit path clauses.
    let cue = if matches!(normalized(last).as_str(), "a" | "an" | "the") {
        preceding.last().copied().unwrap_or(last)
    } else {
        last
    };
    output_verb(cue)
        || path_noun(cue)
        || matches!(normalized(cue).as_str(), "at" | "in" | "into" | "to" | "under")
        || matches!(normalized(last).as_str(), "named" | "called")
            && preceding.iter().rev().take(2).any(|word| path_noun(word))
}

pub(super) fn conditional_clause(words: &[&str]) -> bool {
    words[clause_start(words)..]
        .iter()
        .any(|word| matches!(normalized(word).as_str(), "if" | "unless"))
}

pub(super) fn clause_start(words: &[&str]) -> usize {
    words
        .iter()
        .rposition(|word| word.ends_with(['.', '?', '!', ';']) && !prose_abbreviation(word))
        .map_or(0, |index| index + 1)
}

pub(super) fn path_noun(word: &str) -> bool {
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
}

pub(super) fn ambiguous_addition_verb(word: &str) -> bool {
    matches!(normalized(word).as_str(), "add" | "adds")
}

pub(super) fn explicitly_delimited(raw: &str) -> bool {
    let token = path_token(raw).trim_end_matches(['.', ',', ';', ':']);
    let Some(opening) = token.chars().next() else {
        return false;
    };
    let Some(closing) = token.chars().last() else {
        return false;
    };
    token.len() > opening.len_utf8()
        && matches!((opening, closing), ('`', '`') | ('\'', '\'') | ('"', '"'))
}

pub(super) fn output_verb(word: &str) -> bool {
    word.split('/').all(|part| {
        matches!(
            normalized(part).as_str(),
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
    })
}

pub(super) fn negation(word: &str) -> bool {
    matches!(normalized(word).as_str(), "not" | "never" | "without" | "avoid" | "dont")
}

pub(super) fn normalized(word: &str) -> String {
    word.chars().filter(char::is_ascii_alphanumeric).flat_map(char::to_lowercase).collect()
}
