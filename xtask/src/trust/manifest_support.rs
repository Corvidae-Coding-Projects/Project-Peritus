use super::manifest_date::CalendarDate;
pub(super) use super::manifest_symbol::{
    governing_symbol, source_line_exists, validate_symbol, validate_symbol_declared_at_line,
    validate_symbol_governs_line,
};
use crate::error::Diagnostic;
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn validate_envelope(
    manifest: &Path,
    schema: &str,
    schema_version: u64,
    baseline: &str,
    expected_schema: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if schema != expected_schema || schema_version != 1 || baseline != "A1" {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!(
                "verification manifest envelope is `{schema}` v{schema_version} baseline `{baseline}`"
            ),
            format!("use exact schema `{expected_schema}`, schema_version 1, and baseline `A1`"),
        ));
    }
}

pub(super) fn validate_id(
    manifest: &Path,
    id: &str,
    prefix: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let suffix = id.strip_prefix(prefix);
    let valid = suffix.is_some_and(|digits| {
        digits.len() == 4 && digits.bytes().all(|byte| byte.is_ascii_digit()) && digits != "0000"
    });
    if !valid {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry ID `{id}` does not use nonzero `{prefix}NNNN` form"),
            "assign one stable four-digit manifest ID",
        ));
    }
    valid
}

pub(super) fn validate_unique_id(
    manifest: &Path,
    id: &str,
    seen: &mut BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !seen.insert(id.to_owned()) {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry ID `{id}` is declared more than once"),
            "retain exactly one record for each stable ID",
        ));
    }
}

pub(super) fn validate_text(
    manifest: &Path,
    id: &str,
    field: &str,
    value: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let normalized = value.trim().to_ascii_lowercase();
    let placeholder = ["n/a", "na", "none", "tbd", concat!("to", "do"), "unknown", "placeholder"];
    if value.trim().len() < 4
        || placeholder.contains(&normalized.as_str())
        || normalized.contains(concat!("to", "do"))
        || normalized.contains("placeholder")
    {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{id}` field `{field}` is blank, vague, or placeholder text"),
            "record a concrete reviewable value",
        ));
    }
}

pub(super) fn validate_issue(
    manifest: &Path,
    id: &str,
    issue: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let local = issue.strip_prefix('#').is_some_and(positive_decimal);
    let github = issue.rsplit_once("/issues/").is_some_and(|(repository, number)| {
        repository == "https://github.com/Corvidae-Coding-Projects/Project-Peritus"
            && positive_decimal(number)
    });
    if !local && !github {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{id}` has non-canonical live issue reference `{issue}`"),
            "use `#N` or the canonical Project Peritus GitHub issue URL; protected-branch review confirms liveness",
        ));
    }
}

pub(super) fn validate_review_window(
    manifest: &Path,
    id: &str,
    reviewed: &str,
    deadline: &str,
    today: CalendarDate,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let review = CalendarDate::parse(reviewed);
    let expiry = CalendarDate::parse(deadline);
    if review.is_none() || expiry.is_none() {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{id}` has a malformed review date or deadline"),
            "use real calendar dates in exact YYYY-MM-DD form",
        ));
        return;
    }
    let (review, expiry) = (review.unwrap(), expiry.unwrap());
    if review > today || expiry <= review || expiry < today {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{id}` review window is future-dated, reversed, or expired"),
            "record a completed review and a later deadline that has not expired",
        ));
    }
}

pub(super) fn version_is_pinned(version: &str) -> bool {
    let value = version.trim();
    let tokens: Vec<_> = value.split_ascii_whitespace().collect();
    is_full_commit(value)
        || matches!(tokens.as_slice(), ["commit", hash] if is_full_commit(hash))
        || exact_abi_range(&tokens)
        || exact_release(value)
        || matches!(tokens.as_slice(), ["release", release] if exact_release(release))
}

fn is_full_commit(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn exact_abi_range(tokens: &[&str]) -> bool {
    matches!(tokens, ["ABI", version] if positive_decimal(version))
        || matches!(tokens, ["ABI", first, "through", last]
            if positive_decimal(first) && positive_decimal(last) && decimal_at_most(first, last))
}

fn decimal_at_most(first: &str, last: &str) -> bool {
    let first = first.trim_start_matches('0');
    let last = last.trim_start_matches('0');
    first.len() < last.len() || (first.len() == last.len() && first <= last)
}

fn exact_release(value: &str) -> bool {
    let value = value.strip_prefix('v').unwrap_or(value);
    let components: Vec<_> = value.split('.').collect();
    components.len() >= 3
        && components.iter().all(|component| {
            !component.is_empty()
                && component.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && component.bytes().any(|byte| byte.is_ascii_digit())
        })
}

fn positive_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.bytes().any(|byte| byte != b'0')
}
