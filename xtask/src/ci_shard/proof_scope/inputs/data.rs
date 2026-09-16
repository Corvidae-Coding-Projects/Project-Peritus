//! Static data compiled by direct Rust include macros, without interpreting data as Rust.

mod context;

use crate::error::XtaskError;
use crate::model::CargoMetadata;
use crate::source::reference_lexer::{Token, TokenKind, tokenize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub(super) fn discover(
    root: &Path,
    rust_sources: &[PathBuf],
    cargo: &CargoMetadata,
) -> Result<BTreeSet<PathBuf>, XtaskError> {
    let mut paths = BTreeSet::new();
    let mut context_checked = false;
    for source in rust_sources {
        let contents = fs::read_to_string(source)
            .map_err(|error| XtaskError::io("read data include source", source, error))?;
        let manifest = manifest_directory(source, cargo);
        let tokens = tokenize(&contents);
        let mut cursor = 0;
        while cursor < tokens.len() {
            if is_data_name(&tokens[cursor]) {
                let line = tokens[cursor].line;
                if cursor > 1
                    && punctuation(&tokens, cursor - 1, ':')
                    && punctuation(&tokens, cursor - 2, ':')
                {
                    return Err(invalid(source, line, "qualified data macros are unsupported"));
                }
                cursor += 1;
                let close = macro_open(&tokens, &mut cursor).ok_or_else(|| {
                    invalid(source, line, "data include names must be direct macro invocations")
                })?;
                let mut uses_manifest = false;
                let value = operand(&tokens, &mut cursor, manifest, &mut uses_manifest, 0).ok_or_else(|| {
                    invalid(source, line, "unsupported data operand; use a string literal or concat! of literals and env!(\"CARGO_MANIFEST_DIR\")")
                })?;
                if punctuation(&tokens, cursor, ',') {
                    cursor += 1;
                }
                if !punctuation(&tokens, cursor, close) {
                    return Err(invalid(
                        source,
                        line,
                        "data include must have one complete operand",
                    ));
                }
                cursor += 1;
                if uses_manifest && !context_checked {
                    context::check(root, rust_sources, cargo)?;
                    context_checked = true;
                }
                paths.insert(
                    resolve(root, source, &value)
                        .map_err(|message| invalid(source, line, &message))?,
                );
            } else {
                cursor += 1;
            }
        }
    }
    Ok(paths)
}

fn manifest_directory<'a>(source: &Path, cargo: &'a CargoMetadata) -> Option<&'a Path> {
    cargo
        .packages
        .iter()
        .filter(|package| cargo.workspace_members.contains(&package.id))
        .filter_map(|package| package.manifest_path.parent())
        .filter(|directory| source.starts_with(directory))
        .max_by_key(|directory| directory.components().count())
}

fn operand(
    tokens: &[Token],
    cursor: &mut usize,
    manifest: Option<&Path>,
    uses_manifest: &mut bool,
    depth: u8,
) -> Option<String> {
    // Bound parser recursion independently of the compiler's macro expansion limits.
    if depth == 32 {
        return None;
    }
    match &tokens.get(*cursor)?.kind {
        TokenKind::StringLiteral(Some(value)) => {
            *cursor += 1;
            Some(value.clone())
        }
        TokenKind::Identifier(name, _) if name == "concat" => {
            *cursor += 1;
            let close = macro_open(tokens, cursor)?;
            let mut value = String::new();
            while !punctuation(tokens, *cursor, close) {
                value.push_str(&operand(tokens, cursor, manifest, uses_manifest, depth + 1)?);
                if punctuation(tokens, *cursor, ',') {
                    *cursor += 1;
                } else if !punctuation(tokens, *cursor, close) {
                    return None;
                }
            }
            *cursor += 1;
            Some(value)
        }
        TokenKind::Identifier(name, _) if name == "env" => {
            *cursor += 1;
            let close = macro_open(tokens, cursor)?;
            if !matches!(
                &tokens.get(*cursor)?.kind,
                TokenKind::StringLiteral(Some(name)) if name == "CARGO_MANIFEST_DIR"
            ) {
                return None;
            }
            *cursor += 1;
            if punctuation(tokens, *cursor, ',') {
                *cursor += 1;
            }
            if !punctuation(tokens, *cursor, close) {
                return None;
            }
            *cursor += 1;
            *uses_manifest = true;
            Some(manifest?.to_str()?.replace(std::path::MAIN_SEPARATOR, "/"))
        }
        _ => None,
    }
}

fn macro_open(tokens: &[Token], cursor: &mut usize) -> Option<char> {
    if !punctuation(tokens, *cursor, '!') {
        return None;
    }
    let close = match tokens.get(*cursor + 1)?.kind {
        TokenKind::Punctuation('(') => ')',
        TokenKind::Punctuation('[') => ']',
        TokenKind::Punctuation('{') => '}',
        _ => return None,
    };
    *cursor += 2;
    Some(close)
}

fn is_data_name(token: &Token) -> bool {
    matches!(&token.kind, TokenKind::Identifier(name, _) if matches!(name.as_str(), "include_str" | "include_bytes"))
}

fn punctuation(tokens: &[Token], cursor: usize, expected: char) -> bool {
    matches!(tokens.get(cursor).map(|token| &token.kind), Some(TokenKind::Punctuation(value)) if *value == expected)
}

fn resolve(root: &Path, source: &Path, value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.contains(['\\', '\0']) {
        return Err("data path must be a nonempty portable path".to_owned());
    }
    let declared = Path::new(value);
    let path = if declared.is_absolute() {
        declared.to_path_buf()
    } else {
        source.parent().ok_or("data source has no parent directory")?.join(declared)
    };
    let relative = path.strip_prefix(root).map_err(|_| "data input is outside the workspace")?;
    let mut current = root.to_path_buf();
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if current == root {
                    return Err("data input is outside the workspace".to_owned());
                }
                current.pop();
            }
            Component::Normal(name) => {
                current.push(name);
                let metadata = current.symlink_metadata().map_err(|error| {
                    format!("cannot inspect data input {}: {error}", current.display())
                })?;
                if metadata.file_type().is_symlink() {
                    return Err("data input passes through a symbolic link".to_owned());
                }
                if components.peek().is_some() && !metadata.is_dir() {
                    return Err("data input traverses a non-directory component".to_owned());
                }
            }
            _ => return Err("data input path is not workspace-relative".to_owned()),
        }
    }
    let metadata = current
        .symlink_metadata()
        .map_err(|error| format!("cannot inspect data input {}: {error}", current.display()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("data input is not a regular file".to_owned());
    }
    Ok(current)
}

fn invalid(source: &Path, line: usize, message: &str) -> XtaskError {
    XtaskError::metadata(format!("data include {}:{line}: {message}", source.display()))
}

#[cfg(test)]
mod tests;
