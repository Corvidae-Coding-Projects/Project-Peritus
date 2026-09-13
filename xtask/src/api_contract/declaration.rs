use super::scanner;
use super::signature::{self, Mode};
use crate::source::reference_lexer::{Token, TokenKind, tokenize};
use std::collections::BTreeSet;

/// A structurally parsed Rust/Verus function declaration relevant to manifest evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FunctionDeclaration {
    /// Source line containing the declaration's `fn` token.
    pub(crate) line: usize,
    /// Verus function mode parsed with the pinned signature grammar.
    pub(crate) mode: Option<Mode>,
    /// Whether the declaration is lexically enclosed by `verus! { ... }`.
    pub(crate) in_verus: bool,
    /// Whether a containing function makes this a function-local declaration.
    pub(crate) nested: bool,
    /// The structurally recognized conditional-compilation envelope.
    pub(crate) configuration: Configuration,
    /// The exact Cargo-test attributes on the declaration.
    pub(crate) cargo_test: CargoTest,
}

/// Cargo-test attributes relevant to the canonical evidence command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CargoTest {
    /// The declaration is not an exact `#[test]`.
    None,
    /// The declaration is an exact non-ignored `#[test]`.
    Runnable,
    /// The declaration is an exact `#[test]` but also has `#[ignore]`.
    Ignored,
}

/// Conditional-compilation status relevant to exact evidence commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Configuration {
    /// No enclosing or declaration-local conditional compilation.
    Unconditional,
    /// Only exact `#[cfg(test)]`, which the canonical Cargo test command enables.
    TestOnly,
    /// Any other, mixed, attributed, or malformed configuration.
    Other,
}

impl Configuration {
    const fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Other, _) | (_, Self::Other) => Self::Other,
            (Self::TestOnly, _) | (_, Self::TestOnly) => Self::TestOnly,
            (Self::Unconditional, Self::Unconditional) => Self::Unconditional,
        }
    }
}

/// Finds every function declaration named `name` without treating comments or literals as code.
pub(crate) fn function_declarations(source: &str, name: &str) -> Vec<FunctionDeclaration> {
    let tokens = tokenize(source);
    let function_bodies: BTreeSet<_> = tokens
        .iter()
        .enumerate()
        .filter(|(index, token)| {
            identifier_is(token, "fn") && declaration_name(&tokens, *index).is_some()
        })
        .filter_map(|(index, _)| scanner::function_body(&tokens, index))
        .collect();
    let mut scopes = Vec::new();
    let mut declarations = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        if punctuation_is(token, '}') {
            scopes.pop();
            continue;
        }
        if punctuation_is(token, '{') {
            let kind = if function_bodies.contains(&index) {
                ScopeKind::Function
            } else if verus_macro_body(&tokens, index) {
                ScopeKind::Verus
            } else {
                ScopeKind::Other
            };
            let start = item_start(&tokens, index);
            let configuration = configuration(&tokens[start..index]);
            scopes.push(Scope { kind, configuration });
            continue;
        }
        if !identifier_is(token, "fn") || declaration_name(&tokens, index) != Some(name) {
            continue;
        }

        let start = item_start(&tokens, index);
        let prefix = &tokens[start..index];
        let configuration = scopes
            .iter()
            .fold(configuration(prefix), |current, scope| current.combine(scope.configuration));
        let cargo_test =
            match (has_attribute(prefix, &["test"]), has_attribute(prefix, &["ignore"])) {
                (true, false) => CargoTest::Runnable,
                (true, true) => CargoTest::Ignored,
                (false, _) => CargoTest::None,
            };
        declarations.push(FunctionDeclaration {
            line: token.line,
            mode: signature::parse(prefix).map(|parsed| parsed.mode),
            in_verus: scopes.iter().any(|scope| scope.kind == ScopeKind::Verus),
            nested: scopes.iter().any(|scope| scope.kind == ScopeKind::Function),
            configuration,
            cargo_test,
        });
    }
    declarations
}

#[derive(Clone, Copy)]
struct Scope {
    kind: ScopeKind,
    configuration: Configuration,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ScopeKind {
    Function,
    Verus,
    Other,
}

fn verus_macro_body(tokens: &[Token], brace: usize) -> bool {
    brace >= 2
        && punctuation_is(&tokens[brace - 1], '!')
        && identifier_is(&tokens[brace - 2], "verus")
}

fn has_attribute(tokens: &[Token], names: &[&str]) -> bool {
    let mut cursor = 0;
    while cursor < tokens.len() {
        if !punctuation_is(&tokens[cursor], '#') {
            cursor += 1;
            continue;
        }
        let mut open = cursor + 1;
        if tokens.get(open).is_some_and(|token| punctuation_is(token, '!')) {
            open += 1;
        }
        if !tokens.get(open).is_some_and(|token| punctuation_is(token, '[')) {
            cursor += 1;
            continue;
        }
        let Some(end) = matching_group(tokens, open, '[', ']') else { return true };
        if tokens.get(open + 1).and_then(identifier).is_some_and(|name| names.contains(&name)) {
            return true;
        }
        cursor = end;
    }
    false
}

fn configuration(tokens: &[Token]) -> Configuration {
    let mut result = Configuration::Unconditional;
    let mut cursor = 0;
    while cursor < tokens.len() {
        if !punctuation_is(&tokens[cursor], '#') {
            cursor += 1;
            continue;
        }
        let mut open = cursor + 1;
        if tokens.get(open).is_some_and(|token| punctuation_is(token, '!')) {
            open += 1;
        }
        if !tokens.get(open).is_some_and(|token| punctuation_is(token, '[')) {
            return Configuration::Other;
        }
        let Some(end) = matching_group(tokens, open, '[', ']') else {
            return Configuration::Other;
        };
        let attribute = &tokens[open + 1..end - 1];
        match attribute.first().and_then(identifier) {
            Some("cfg") if exact_cfg_test(attribute) => {
                result = result.combine(Configuration::TestOnly);
            }
            Some("cfg" | "cfg_attr") => return Configuration::Other,
            _ => {}
        }
        cursor = end;
    }
    result
}

fn exact_cfg_test(attribute: &[Token]) -> bool {
    attribute.len() == 4
        && identifier_is(&attribute[0], "cfg")
        && punctuation_is(&attribute[1], '(')
        && identifier_is(&attribute[2], "test")
        && punctuation_is(&attribute[3], ')')
}

fn item_start(tokens: &[Token], index: usize) -> usize {
    let mut cursor = index;
    while cursor > 0 {
        if matches!(tokens[cursor - 1].kind, TokenKind::Punctuation('{' | '}' | ';')) {
            break;
        }
        cursor -= 1;
    }
    cursor
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

fn declaration_name(tokens: &[Token], function: usize) -> Option<&str> {
    tokens.get(function + 1).and_then(identifier)
}

fn identifier_is(token: &Token, expected: &str) -> bool {
    identifier(token) == Some(expected)
}

fn identifier(token: &Token) -> Option<&str> {
    match &token.kind {
        TokenKind::Identifier(value, _) => Some(value),
        TokenKind::Punctuation(_) | TokenKind::StringLiteral(_) => None,
    }
}

const fn punctuation_is(token: &Token, expected: char) -> bool {
    matches!(token.kind, TokenKind::Punctuation(value) if value == expected)
}
