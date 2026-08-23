use super::expansion;
use super::signature::{self, Mode};
use super::violation::{Violation, ViolationKind};
use crate::source::reference_lexer::{Token, TokenKind, tokenize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Context {
    PublicSafeTrait,
    UnsafeTrait,
    NonPublicTrait,
    TraitImplementation,
    Other,
}

pub(super) struct Scan {
    pub(super) executable_entry_points: usize,
    pub(super) violations: Vec<Violation>,
}

pub(super) fn scan(source: &str) -> Scan {
    let tokens = tokenize(source);
    let mut contexts = Vec::new();
    let mut violations = expansion::violations(&tokens);
    let mut executable_entry_points = 0;

    for (index, token) in tokens.iter().enumerate() {
        if punctuation_is(token, '}') {
            contexts.pop();
            continue;
        }
        if punctuation_is(token, '{') {
            contexts.push(classify_context(&tokens, index));
            continue;
        }
        if !identifier_is(token, "fn") || !is_function_declaration(&tokens, index) {
            continue;
        }

        let start = item_start(&tokens, index);
        let function = identifier(&tokens[index + 1]).unwrap_or("<unknown>").to_owned();
        let line = token.line;
        let Some(prefix) = signature::parse(&tokens[start..index]) else {
            violations.push(Violation {
                line,
                function,
                clause: None,
                kind: ViolationKind::UnparseableHeader,
            });
            continue;
        };
        let mode = prefix.mode;
        if mode == Mode::Spec {
            continue;
        }
        let attribute_clauses = verus_spec_clauses(&tokens[start..index]);
        let context = contexts.last().copied().unwrap_or(Context::Other);
        let visible = prefix.public
            || matches!(
                context,
                Context::PublicSafeTrait | Context::UnsafeTrait | Context::TraitImplementation
            );
        if mode == Mode::Exec && visible {
            executable_entry_points += 1;
        }

        let Some(header) = inspect_header(&tokens, index) else {
            if mode == Mode::Exec && visible {
                violations.push(Violation {
                    line,
                    function,
                    clause: None,
                    kind: ViolationKind::UnparseableHeader,
                });
            }
            continue;
        };
        if mode == Mode::Exec && header.opaque_return {
            violations.push(Violation {
                line,
                function: function.clone(),
                clause: None,
                kind: ViolationKind::OpaqueReturn,
            });
        }
        if prefix.unsafe_ {
            continue;
        }

        if mode == Mode::Exec
            && (header.has_clause("requires")
                || attribute_clauses.iter().any(|clause| clause == "requires"))
            && visible
        {
            violations.push(Violation {
                line,
                function: function.clone(),
                clause: Some("requires".to_owned()),
                kind: ViolationKind::ExposedRequires,
            });
        }

        if context == Context::PublicSafeTrait {
            for clause in ["ensures", "no_unwind", "opens_invariants", "decreases"] {
                if header.has_clause(clause)
                    || attribute_clauses.iter().any(|candidate| candidate == clause)
                {
                    violations.push(Violation {
                        line,
                        function: function.clone(),
                        clause: Some(clause.to_owned()),
                        kind: ViolationKind::PublicTraitContract,
                    });
                }
            }
        }
    }

    Scan { executable_entry_points, violations }
}

fn verus_spec_clauses(tokens: &[Token]) -> Vec<String> {
    let mut clauses = Vec::new();
    let mut cursor = 0;
    while cursor < tokens.len() {
        if !punctuation_is(&tokens[cursor], '#') {
            cursor += 1;
            continue;
        }
        let Some(open) =
            tokens.get(cursor + 1).filter(|token| punctuation_is(token, '[')).map(|_| cursor + 1)
        else {
            cursor += 1;
            continue;
        };
        let Some(end) = matching_group(tokens, open, '[', ']') else {
            return clauses;
        };
        let attribute = &tokens[open + 1..end.saturating_sub(1)];
        if identifier(attribute.first().unwrap_or(&tokens[open])) == Some("verus_spec") {
            for token in attribute.iter().skip(1) {
                if let Some(word) = identifier(token)
                    && matches!(
                        word,
                        "requires" | "ensures" | "no_unwind" | "opens_invariants" | "decreases"
                    )
                    && !clauses.iter().any(|clause| clause == word)
                {
                    clauses.push(word.to_owned());
                }
            }
        }
        cursor = end;
    }
    clauses
}

struct Header {
    clauses: Vec<String>,
    opaque_return: bool,
}

impl Header {
    fn has_clause(&self, expected: &str) -> bool {
        self.clauses.iter().any(|clause| clause == expected)
    }
}

fn inspect_header(tokens: &[Token], function: usize) -> Option<Header> {
    let mut cursor = function + 2;
    let mut angles = 0_usize;
    let parameters = loop {
        match punctuation(tokens.get(cursor)?) {
            Some('<') => angles += 1,
            Some('>') => angles = angles.saturating_sub(1),
            Some('(') if angles == 0 => break cursor,
            Some('{' | ';') if angles == 0 => return None,
            _ => {}
        }
        cursor += 1;
    };
    cursor = matching_group(tokens, parameters, '(', ')')?;

    let mut parentheses = 0_usize;
    let mut brackets = 0_usize;
    angles = 0;
    let mut clauses = Vec::new();
    let mut opaque_return = false;
    let mut in_contract = false;
    while cursor < tokens.len() {
        let top_level = parentheses == 0 && brackets == 0 && (in_contract || angles == 0);
        if top_level {
            if punctuation_is(&tokens[cursor], ';') {
                return Some(Header { clauses, opaque_return });
            }
            if punctuation_is(&tokens[cursor], '{') {
                if in_contract {
                    let end = matching_group(tokens, cursor, '{', '}')?;
                    if braced_contract_expression_continues(tokens, end) {
                        cursor = end;
                        continue;
                    }
                }
                return Some(Header { clauses, opaque_return });
            }
            if let Some(word) = identifier(&tokens[cursor]) {
                if matches!(
                    word,
                    "requires" | "ensures" | "no_unwind" | "opens_invariants" | "decreases"
                ) && !clauses.iter().any(|clause| clause == word)
                {
                    clauses.push(word.to_owned());
                    in_contract = true;
                }
                opaque_return |= !in_contract && word == "impl";
            }
        }
        match punctuation(&tokens[cursor]) {
            Some('(') => parentheses += 1,
            Some(')') => parentheses = parentheses.saturating_sub(1),
            Some('[') => brackets += 1,
            Some(']') => brackets = brackets.saturating_sub(1),
            Some('<') if !in_contract => angles += 1,
            Some('>') if !in_contract => angles = angles.saturating_sub(1),
            Some('}') if top_level => return None,
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn braced_contract_expression_continues(tokens: &[Token], end: usize) -> bool {
    let Some(next) = tokens.get(end) else { return false };
    if punctuation_is(next, ',') {
        return true;
    }
    if matches!(
        punctuation(next),
        Some(
            '{' | ';'
                | '.'
                | '?'
                | '['
                | '('
                | '='
                | '!'
                | '<'
                | '>'
                | '&'
                | '|'
                | '+'
                | '-'
                | '*'
                | '/'
                | '%'
        )
    ) {
        return true;
    }
    identifier(next).is_some_and(|word| {
        matches!(
            word,
            "requires" | "ensures" | "no_unwind" | "opens_invariants" | "decreases" | "as"
        )
    })
}

fn classify_context(tokens: &[Token], brace: usize) -> Context {
    let start = item_start(tokens, brace);
    match signature::container(&tokens[start..brace]) {
        signature::Container::PublicSafeTrait => Context::PublicSafeTrait,
        signature::Container::UnsafeTrait => Context::UnsafeTrait,
        signature::Container::NonPublicTrait => Context::NonPublicTrait,
        signature::Container::TraitImplementation => Context::TraitImplementation,
        signature::Container::Other => Context::Other,
    }
}

fn item_start(tokens: &[Token], index: usize) -> usize {
    let mut cursor = index;
    while cursor > 0 {
        if matches!(punctuation(&tokens[cursor - 1]), Some('{' | '}' | ';')) {
            break;
        }
        cursor -= 1;
    }
    cursor
}

fn is_function_declaration(tokens: &[Token], index: usize) -> bool {
    tokens.get(index + 1).and_then(identifier).is_some()
}

fn matching_group(tokens: &[Token], open: usize, opening: char, closing: char) -> Option<usize> {
    let mut depth = 0_usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if punctuation_is(token, opening) {
            depth += 1;
        } else if punctuation_is(token, closing) {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index + 1);
            }
        }
    }
    None
}

fn identifier_is(token: &Token, expected: &str) -> bool {
    matches!(&token.kind, TokenKind::Identifier(value, false) if value == expected)
}

fn identifier(token: &Token) -> Option<&str> {
    match &token.kind {
        TokenKind::Identifier(value, _) => Some(value),
        TokenKind::Punctuation(_) | TokenKind::StringLiteral(_) => None,
    }
}

fn punctuation_is(token: &Token, expected: char) -> bool {
    punctuation(token) == Some(expected)
}

const fn punctuation(token: &Token) -> Option<char> {
    match token.kind {
        TokenKind::Punctuation(value) => Some(value),
        TokenKind::Identifier(_, _) | TokenKind::StringLiteral(_) => None,
    }
}
