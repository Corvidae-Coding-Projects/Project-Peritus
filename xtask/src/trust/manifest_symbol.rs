#[path = "manifest_symbol/owner.rs"]
mod owner;

use super::lexer::DeclarationOwner;
use crate::api_contract::{FunctionDeclaration, function_declarations};
use crate::error::Diagnostic;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OwnedFunctionDeclaration {
    pub(super) path: String,
    pub(super) declaration: FunctionDeclaration,
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
    let candidates = owned_function_declarations(owning_crate, source, &contents, final_segment);
    let matching = candidates.iter().filter(|candidate| candidate.path == symbol).count();
    if matching != 1 {
        let declared: Vec<_> = candidates.iter().map(|candidate| candidate.path.as_str()).collect();
        let remedy = if declared.is_empty() {
            "name an actual non-local function declaration in the recorded source".to_owned()
        } else if matching > 1 {
            format!(
                "retain one unambiguous declaration for `{symbol}`; {matching} exact declarations were found"
            )
        } else {
            format!("use one exact declared symbol path: {}", declared.join(", "))
        };
        diagnostics.push(Diagnostic::at(
            manifest,
            format!(
                "entry `{id}` symbol `{symbol}` does not match exactly one source module path and associated-item owner"
            ),
            remedy,
        ));
    }
}

pub(super) fn owned_function_declarations(
    owning_crate: &str,
    source: &Path,
    contents: &str,
    name: &str,
) -> Vec<OwnedFunctionDeclaration> {
    let Some(module) = module_symbol(owning_crate, source) else { return Vec::new() };
    let parsed = function_declarations(contents, name);
    super::lexer::declaration_paths(contents, name)
        .into_iter()
        .filter_map(|(owners, ordinal)| {
            let declaration = parsed.get(ordinal).copied()?;
            if declaration.nested {
                return None;
            }
            let mut path = module.clone();
            let mut inline_modules = Vec::new();
            for owner in owners {
                match owner {
                    DeclarationOwner::Module(name) => {
                        path.push_str("::");
                        path.push_str(&name);
                        inline_modules.push(name);
                    }
                    DeclarationOwner::Trait(name) => {
                        path.push_str("::");
                        path.push_str(&name);
                    }
                    DeclarationOwner::Impl(self_type) => {
                        path = owner::resolve(
                            owning_crate,
                            source,
                            contents,
                            &inline_modules,
                            &self_type,
                        )?;
                    }
                }
            }
            path.push_str("::");
            path.push_str(name);
            Some(OwnedFunctionDeclaration { path, declaration })
        })
        .collect()
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
    Some(format!("{}::{item}", module_symbol(owning_crate, relative)?))
}

fn module_symbol(owning_crate: &str, source: &Path) -> Option<String> {
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
    Some(modules.join("::"))
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
