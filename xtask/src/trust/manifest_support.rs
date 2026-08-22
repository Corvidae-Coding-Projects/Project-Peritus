use super::manifest_date::CalendarDate;
use crate::error::Diagnostic;
use std::collections::BTreeSet;
use std::fs;
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

pub(super) fn validate_symbol(
    manifest: &Path,
    id: &str,
    owning_crate: &str,
    source: Option<&Path>,
    symbol: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let segments: Vec<_> = symbol.split("::").collect();
    let expected_root = owning_crate.replace('-', "_");
    let valid = segments.len() >= 2
        && segments.first() == Some(&expected_root.as_str())
        && segments.iter().all(|segment| valid_identifier(segment));
    if !valid {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{id}` symbol `{symbol}` is not a fully qualified owned symbol"),
            format!("use a Rust path beginning with `{expected_root}::`"),
        ));
        return;
    }
    let Some(source) = source else { return };
    let final_segment = segments.last().expect("validated symbol has segments");
    let contents = fs::read_to_string(source).unwrap_or_default();
    if !contains_identifier(&contents, final_segment) {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{id}` symbol `{symbol}` is not lexically present in its source file"),
            "point at the exact current item rather than a stale or broad symbol",
        ));
    } else if expected_symbol_for_file(owning_crate, source, final_segment).as_deref()
        != Some(symbol)
    {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{id}` symbol `{symbol}` does not match its source module path"),
            "use the exact crate/module/item path derived from the owning source file",
        ));
    }
}

pub(super) fn source_line_exists(path: &Path, line: u64) -> bool {
    line > 0
        && usize::try_from(line).ok().is_some_and(|line| {
            fs::read_to_string(path).is_ok_and(|text| text.lines().count() >= line)
        })
}

pub(super) fn validate_symbol_governs_line(
    manifest: &Path,
    id: &str,
    source: Option<&Path>,
    line: u64,
    symbol: &str,
    construct: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(source) = source else { return };
    let Ok(index) = usize::try_from(line.saturating_sub(1)) else { return };
    let Ok(contents) = fs::read_to_string(source) else { return };
    let lines: Vec<_> = contents.lines().collect();
    let Some(name) = symbol.rsplit("::").next() else { return };
    let same_line_target = construct == "assume_specification"
        && lines.get(index).is_some_and(|text| contains_identifier(text, name));
    let next_declaration =
        lines.iter().enumerate().skip(index).take(12).find_map(|(_, text)| declaration_name(text));
    let enclosing_declaration =
        lines[..index.min(lines.len())].iter().rev().find_map(|text| declaration_name(text));
    if !same_line_target
        && next_declaration.as_deref() != Some(name)
        && enclosing_declaration.as_deref() != Some(name)
    {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{id}` symbol `{symbol}` does not govern its recorded construct line"),
            "name the nearest enclosing item or the item immediately following the trusted attribute",
        ));
    }
}

pub(super) fn validate_symbol_declared_at_line(
    manifest: &Path,
    id: &str,
    source: Option<&Path>,
    line: u64,
    symbol: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(source) = source else { return };
    let Ok(index) = usize::try_from(line.saturating_sub(1)) else { return };
    let Ok(contents) = fs::read_to_string(source) else { return };
    let lines: Vec<_> = contents.lines().collect();
    let name = symbol.rsplit("::").next();
    if lines.get(index).and_then(|text| declaration_name(text)).as_deref() != name {
        diagnostics.push(Diagnostic::at(
            manifest,
            format!("entry `{id}` source line does not begin its declared symbol `{symbol}`"),
            "record the exact line containing the governed item declaration",
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

pub(super) fn governing_symbol(
    owning_crate: &str,
    relative: &Path,
    contents: &str,
    line: u64,
    construct: &str,
) -> Option<String> {
    let index = usize::try_from(line.checked_sub(1)?).ok()?;
    let lines: Vec<_> = contents.lines().collect();
    let same_line = (construct == "assume_specification")
        .then(|| lines.get(index).and_then(|text| declaration_name(text)))
        .flatten();
    let item = same_line
        .or_else(|| lines.iter().skip(index).take(12).find_map(|text| declaration_name(text)))
        .or_else(|| lines.get(..index)?.iter().rev().find_map(|text| declaration_name(text)))?;
    let source_root = Path::new("crates/foundation/peritus-tcb/src");
    let module_file = relative.strip_prefix(source_root).ok()?;
    let mut segments = vec![owning_crate.replace('-', "_")];
    let mut modules: Vec<_> = module_file
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(str::to_owned)
        .collect();
    if let Some(file) = modules.pop() {
        let stem = file.strip_suffix(".rs")?;
        if stem != "lib" && stem != "mod" {
            modules.push(stem.to_owned());
        }
    }
    segments.extend(modules);
    segments.push(item);
    Some(segments.join("::"))
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

fn valid_identifier(value: &str) -> bool {
    let value = value.strip_prefix("r#").unwrap_or(value);
    value.as_bytes().split_first().is_some_and(|(first, rest)| {
        (first.is_ascii_alphabetic() || *first == b'_')
            && rest.iter().all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    })
}

fn contains_identifier(source: &str, expected: &str) -> bool {
    source.match_indices(expected).any(|(start, _)| {
        let end = start + expected.len();
        source[..start]
            .chars()
            .next_back()
            .is_none_or(|character| !character.is_alphanumeric() && character != '_')
            && source[end..]
                .chars()
                .next()
                .is_none_or(|character| !character.is_alphanumeric() && character != '_')
    })
}

fn declaration_name(line: &str) -> Option<String> {
    let tokens: Vec<_> = line
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| !token.is_empty())
        .collect();
    tokens.windows(2).find_map(|pair| {
        ["fn", "struct", "enum", "union", "trait", "type", "const", "static", "mod", "impl"]
            .contains(&pair[0])
            .then(|| pair[1].to_owned())
    })
}

fn expected_symbol_for_file(owning_crate: &str, source: &Path, item: &str) -> Option<String> {
    let components: Vec<_> = source.components().collect();
    let boundary = components
        .iter()
        .rposition(|component| matches!(component.as_os_str().to_str(), Some("src" | "tests")))?;
    let boundary_name = components[boundary].as_os_str().to_str()?;
    let mut modules = vec![owning_crate.replace('-', "_")];
    if boundary_name == "tests" {
        modules.push("tests".to_owned());
    }
    let mut tail: Vec<_> = components[boundary + 1..]
        .iter()
        .filter_map(|component| component.as_os_str().to_str())
        .map(str::to_owned)
        .collect();
    let file = tail.pop()?;
    let stem = file.strip_suffix(".rs")?;
    modules.extend(tail);
    if !matches!(stem, "lib" | "main" | "mod") {
        modules.push(stem.to_owned());
    }
    modules.push(item.to_owned());
    Some(modules.join("::"))
}
