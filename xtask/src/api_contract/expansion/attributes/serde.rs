//! Closed Serde metadata grammar. No custom conversion, flattening, or deserializer hooks.

use super::{identifier, identifier_is, matching_group, punctuation_is};
use crate::source::reference_lexer::{Token, TokenKind};
use std::collections::BTreeSet;

// These predicates are ordinary, source-audited methods. Arbitrary callback paths remain
// unsupported: adding one requires reviewing its omission semantics, not only its spelling.
const OMISSION_PREDICATES: &[&str] = &[
    "Option::is_none",
    "Vec::is_empty",
    "super::InputLedger::is_empty",
    "super::TaskBrief::is_empty",
    "super::ContextSelections::is_empty",
    "super::PromptView::is_empty",
    "super::ImageAttachments::is_empty",
    "super::FileAttachments::is_empty",
    "super::ReviewLedger::is_empty",
];

pub(super) fn audited(tokens: &[Token], serialize_imported: bool) -> bool {
    if tokens.len() < 4
        || !identifier_is(&tokens[0], "serde")
        || !punctuation_is(&tokens[1], '(')
        || matching_group(tokens, 1, '(', ')') != Some(tokens.len())
    {
        return false;
    }
    let clauses = tokens[2..tokens.len() - 1].split(|token| punctuation_is(token, ','));
    let mut names = BTreeSet::new();
    for clause in clauses {
        let Some(name) = clause.first().and_then(identifier) else { return false };
        if !names.insert(name) || !audited_clause(name, clause, serialize_imported) {
            return false;
        }
    }
    !names.is_empty()
}

fn audited_clause(name: &str, tokens: &[Token], serialize_imported: bool) -> bool {
    if tokens.len() == 1 {
        return matches!(name, "default" | "deny_unknown_fields");
    }
    if tokens.len() != 3 || !punctuation_is(&tokens[1], '=') {
        return false;
    }
    let TokenKind::StringLiteral(Some(value)) = &tokens[2].kind else { return false };
    match name {
        "rename_all" => matches!(value.as_str(), "snake_case" | "kebab-case"),
        "rename" | "tag" => simple_key(value),
        "skip_serializing_if" => {
            serialize_imported && OMISSION_PREDICATES.contains(&value.as_str())
        }
        _ => false,
    }
}

fn simple_key(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && !value.as_bytes()[0].is_ascii_digit()
}
