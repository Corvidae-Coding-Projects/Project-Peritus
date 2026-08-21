//! Lexical compilation-source policy.
//!
//! A0 permits direct literal `include!` and `#[path]` declarations, reserves every other code
//! token named `include`, and rejects local `macro_rules!`. Local macro expansion can otherwise
//! synthesize compilation inputs that a pre-expansion source walk cannot enumerate soundly.

use super::reference_lexer::{Token, TokenKind, tokenize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReferenceKind {
    Include,
    PathAttribute,
}

impl ReferenceKind {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Include => "include!",
            Self::PathAttribute => "#[path]",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct SourceReference {
    pub(super) kind: ReferenceKind,
    pub(super) line: usize,
    pub(super) path: Option<PathBuf>,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct IncludeImport {
    pub(super) line: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct ReservedInclude {
    pub(super) line: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct MacroRulesDefinition {
    pub(super) line: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Scan {
    pub(super) references: Vec<SourceReference>,
    pub(super) include_imports: Vec<IncludeImport>,
    pub(super) reserved_includes: Vec<ReservedInclude>,
    pub(super) macro_rules_definitions: Vec<MacroRulesDefinition>,
}

pub(super) fn scan(source: &str) -> Scan {
    let tokens = tokenize(source);
    let mut references = Vec::new();
    let mut include_imports = Vec::new();
    let mut reserved_includes = Vec::new();
    let mut macro_rules_definitions = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if keyword_is(&tokens, index, "use") {
            let end = statement_end(&tokens, index + 1);
            scan_include_imports(&tokens[index + 1..end], &mut include_imports);
            index = end.saturating_add(1);
            continue;
        }
        if identifier_is(&tokens, index, "macro_rules") && punctuation_is(&tokens, index + 1, '!') {
            macro_rules_definitions.push(MacroRulesDefinition { line: tokens[index].line });
            index += 2;
            continue;
        }
        if identifier_is(&tokens, index, "include")
            && punctuation_is(&tokens, index + 1, '!')
            && opening_delimiter(&tokens, index + 2).is_some()
        {
            let delimiter = opening_delimiter(&tokens, index + 2).expect("delimiter was checked");
            let end = matching_delimiter(&tokens, index + 2, delimiter);
            references.push(SourceReference {
                kind: ReferenceKind::Include,
                line: tokens[index].line,
                path: single_string(&tokens[index + 3..end]).map(PathBuf::from),
            });
            index = end.saturating_add(1);
            continue;
        }
        if identifier_is(&tokens, index, "include") {
            reserved_includes.push(ReservedInclude { line: tokens[index].line });
            index += 1;
            continue;
        }
        if punctuation_is(&tokens, index, '#') {
            let mut attribute_start = index + 1;
            if punctuation_is(&tokens, attribute_start, '!') {
                attribute_start += 1;
            }
            if punctuation_is(&tokens, attribute_start, '[') {
                let end = matching_delimiter(&tokens, attribute_start, ('[', ']'));
                scan_attribute(&tokens[attribute_start + 1..end], &mut references);
                index = end.saturating_add(1);
                continue;
            }
        }
        index += 1;
    }
    Scan { references, include_imports, reserved_includes, macro_rules_definitions }
}

fn statement_end(tokens: &[Token], start: usize) -> usize {
    tokens[start..]
        .iter()
        .position(|token| matches!(token.kind, TokenKind::Punctuation(';')))
        .map_or(tokens.len(), |offset| start + offset)
}

fn scan_include_imports(tokens: &[Token], include_imports: &mut Vec<IncludeImport>) {
    for (index, token) in tokens.iter().enumerate() {
        if identifier_is(tokens, index, "include") {
            include_imports.push(IncludeImport { line: token.line });
        }
    }
}

fn scan_attribute(tokens: &[Token], references: &mut Vec<SourceReference>) {
    if identifier_is(tokens, 0, "path") {
        references.push(path_reference(tokens, 0));
    } else if identifier_is(tokens, 0, "cfg_attr") {
        for index in 0..tokens.len() {
            if identifier_is(tokens, index, "path") && punctuation_is(tokens, index + 1, '=') {
                references.push(path_reference(tokens, index));
            }
        }
    }
}

fn path_reference(tokens: &[Token], index: usize) -> SourceReference {
    let path = match tokens.get(index + 2).map(|token| &token.kind) {
        Some(TokenKind::StringLiteral(Some(value))) => Some(PathBuf::from(value)),
        _ => None,
    };
    SourceReference {
        kind: ReferenceKind::PathAttribute,
        line: tokens.get(index).map_or(1, |token| token.line),
        path,
    }
}

fn single_string(tokens: &[Token]) -> Option<String> {
    match tokens {
        [Token { kind: TokenKind::StringLiteral(Some(value)), .. }] => Some(value.clone()),
        _ => None,
    }
}

fn matching_delimiter(tokens: &[Token], start: usize, delimiter: (char, char)) -> usize {
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if matches!(token.kind, TokenKind::Punctuation(character) if character == delimiter.0) {
            depth += 1;
        } else if matches!(token.kind, TokenKind::Punctuation(character) if character == delimiter.1)
        {
            depth -= 1;
            if depth == 0 {
                return index;
            }
        }
    }
    tokens.len()
}

fn opening_delimiter(tokens: &[Token], index: usize) -> Option<(char, char)> {
    match tokens.get(index).map(|token| &token.kind) {
        Some(TokenKind::Punctuation('(')) => Some(('(', ')')),
        Some(TokenKind::Punctuation('[')) => Some(('[', ']')),
        Some(TokenKind::Punctuation('{')) => Some(('{', '}')),
        _ => None,
    }
}

fn identifier_is(tokens: &[Token], index: usize, expected: &str) -> bool {
    matches!(
        tokens.get(index).map(|token| &token.kind),
        Some(TokenKind::Identifier(identifier, _)) if identifier == expected
    )
}

fn keyword_is(tokens: &[Token], index: usize, expected: &str) -> bool {
    matches!(
        tokens.get(index).map(|token| &token.kind),
        Some(TokenKind::Identifier(identifier, false)) if identifier == expected
    )
}

fn punctuation_is(tokens: &[Token], index: usize, expected: char) -> bool {
    matches!(
        tokens.get(index).map(|token| &token.kind),
        Some(TokenKind::Punctuation(character)) if *character == expected
    )
}
