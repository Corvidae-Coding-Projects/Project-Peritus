//! Narrow allowlist for Cargo-owned compile-time paths used by hermetic integration tests.

use crate::source::reference_lexer::{Token, TokenKind};

const AUDITED_CARGO_ENV: &[&str] = &[
    "CARGO_MANIFEST_DIR",
    "CARGO_BIN_EXE_peritus-anthropic-claude-fake",
    "CARGO_BIN_EXE_peritus-openai-codex-fake",
];

pub(super) fn audited_cargo_env(tokens: &[Token], cursor: usize) -> bool {
    let Some(Token { kind: TokenKind::StringLiteral(Some(name)), .. }) = tokens.get(cursor + 3)
    else {
        return false;
    };
    punctuation_is(&tokens[cursor + 2], '(')
        && tokens.get(cursor + 4).is_some_and(|token| punctuation_is(token, ')'))
        && AUDITED_CARGO_ENV.contains(&name.as_str())
}

const fn punctuation_is(token: &Token, expected: char) -> bool {
    matches!(token.kind, TokenKind::Punctuation(value) if value == expected)
}
