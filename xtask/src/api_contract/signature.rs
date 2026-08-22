use crate::source::reference_lexer::{Token, TokenKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Mode {
    Exec,
    Proof,
    Spec,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Prefix {
    pub(super) mode: Mode,
    pub(super) public: bool,
    pub(super) unsafe_: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Container {
    PublicSafeTrait,
    UnsafeTrait,
    NonPublicTrait,
    TraitImplementation,
    Other,
}

/// Parses the exact modifier order accepted by the pinned Verus `Signature` grammar.
pub(super) fn parse(tokens: &[Token]) -> Option<Prefix> {
    let mut cursor = skip_attributes(tokens, 0)?;
    let (next, public) = visibility(tokens, cursor)?;
    cursor = next;

    cursor = publish(tokens, cursor)?;
    consume(tokens, &mut cursor, "const");
    consume(tokens, &mut cursor, "async");
    let unsafe_ = consume(tokens, &mut cursor, "unsafe");
    cursor = abi(tokens, cursor)?;
    consume(tokens, &mut cursor, "broadcast");
    let (next, mode) = mode(tokens, cursor)?;
    cursor = next;

    (cursor == tokens.len()).then_some(Prefix { mode, public, unsafe_ })
}

pub(super) fn container(tokens: &[Token]) -> Container {
    let Some(mut cursor) = skip_attributes(tokens, 0) else { return Container::Other };
    let Some((next, public)) = visibility(tokens, cursor) else { return Container::Other };
    cursor = next;
    let unsafe_ = consume(tokens, &mut cursor, "unsafe");
    consume(tokens, &mut cursor, "auto");
    if identifier_is(tokens, cursor, "trait") {
        return match (public, unsafe_) {
            (true, false) => Container::PublicSafeTrait,
            (_, true) => Container::UnsafeTrait,
            (false, false) => Container::NonPublicTrait,
        };
    }

    cursor = skip_attributes(tokens, 0).unwrap_or(0);
    consume(tokens, &mut cursor, "default");
    consume(tokens, &mut cursor, "unsafe");
    if identifier_is(tokens, cursor, "impl") && has_top_level_for(&tokens[cursor + 1..]) {
        Container::TraitImplementation
    } else {
        Container::Other
    }
}

fn skip_attributes(tokens: &[Token], mut cursor: usize) -> Option<usize> {
    while punctuation_is(tokens, cursor, '#') {
        cursor += 1;
        if punctuation_is(tokens, cursor, '!') {
            cursor += 1;
        }
        if !punctuation_is(tokens, cursor, '[') {
            return None;
        }
        cursor = matching_group(tokens, cursor, '[', ']')?;
    }
    Some(cursor)
}

fn visibility(tokens: &[Token], mut cursor: usize) -> Option<(usize, bool)> {
    if !identifier_is(tokens, cursor, "pub") {
        return Some((cursor, false));
    }
    cursor += 1;
    if punctuation_is(tokens, cursor, '(') {
        cursor = matching_group(tokens, cursor, '(', ')')?;
    }
    Some((cursor, true))
}

fn publish(tokens: &[Token], mut cursor: usize) -> Option<usize> {
    if identifier_is(tokens, cursor, "closed") || identifier_is(tokens, cursor, "uninterp") {
        return Some(cursor + 1);
    }
    if identifier_is(tokens, cursor, "open") {
        cursor += 1;
        if punctuation_is(tokens, cursor, '(') {
            cursor = matching_group(tokens, cursor, '(', ')')?;
        }
    }
    Some(cursor)
}

fn abi(tokens: &[Token], mut cursor: usize) -> Option<usize> {
    if !identifier_is(tokens, cursor, "extern") {
        return Some(cursor);
    }
    cursor += 1;
    if let Some(Token { kind: TokenKind::StringLiteral(value), .. }) = tokens.get(cursor) {
        value.as_ref()?;
        cursor += 1;
    }
    Some(cursor)
}

fn mode(tokens: &[Token], mut cursor: usize) -> Option<(usize, Mode)> {
    let mode = if identifier_is(tokens, cursor, "spec") {
        cursor += 1;
        if punctuation_is(tokens, cursor, '(') {
            let end = matching_group(tokens, cursor, '(', ')')?;
            if end != cursor + 3 || !identifier_is(tokens, cursor + 1, "checked") {
                return None;
            }
            cursor = end;
        }
        Mode::Spec
    } else if identifier_is(tokens, cursor, "proof") || identifier_is(tokens, cursor, "axiom") {
        cursor += 1;
        Mode::Proof
    } else if identifier_is(tokens, cursor, "exec") {
        cursor += 1;
        Mode::Exec
    } else {
        Mode::Exec
    };
    Some((cursor, mode))
}

fn has_top_level_for(tokens: &[Token]) -> bool {
    let mut angles = 0_usize;
    let mut parentheses = 0_usize;
    let mut brackets = 0_usize;
    for token in tokens {
        let top_level = angles == 0 && parentheses == 0 && brackets == 0;
        if top_level && identifier(token) == Some("where") {
            return false;
        }
        if top_level && identifier(token) == Some("for") {
            return true;
        }
        match punctuation(token) {
            Some('<') => angles += 1,
            Some('>') => angles = angles.saturating_sub(1),
            Some('(') => parentheses += 1,
            Some(')') => parentheses = parentheses.saturating_sub(1),
            Some('[') => brackets += 1,
            Some(']') => brackets = brackets.saturating_sub(1),
            _ => {}
        }
    }
    false
}

fn consume(tokens: &[Token], cursor: &mut usize, expected: &str) -> bool {
    let present = identifier_is(tokens, *cursor, expected);
    *cursor += usize::from(present);
    present
}

fn matching_group(tokens: &[Token], open: usize, opening: char, closing: char) -> Option<usize> {
    let mut depth = 0_usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if punctuation(token) == Some(opening) {
            depth += 1;
        } else if punctuation(token) == Some(closing) {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index + 1);
            }
        }
    }
    None
}

fn identifier_is(tokens: &[Token], index: usize, expected: &str) -> bool {
    tokens.get(index).and_then(identifier) == Some(expected)
}

fn identifier(token: &Token) -> Option<&str> {
    match &token.kind {
        TokenKind::Identifier(value, false) => Some(value),
        TokenKind::Identifier(_, true)
        | TokenKind::Punctuation(_)
        | TokenKind::StringLiteral(_) => None,
    }
}

fn punctuation_is(tokens: &[Token], index: usize, expected: char) -> bool {
    tokens.get(index).and_then(punctuation) == Some(expected)
}

const fn punctuation(token: &Token) -> Option<char> {
    match token.kind {
        TokenKind::Punctuation(value) => Some(value),
        TokenKind::Identifier(_, _) | TokenKind::StringLiteral(_) => None,
    }
}
