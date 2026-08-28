//! Exact attribute and derive policy for formal-boundary source.

use super::{identifier, identifier_is, matching_group, punctuation_is};
use crate::source::reference_lexer::{Token, TokenKind};

use super::super::violation::{Violation, ViolationKind};

const SIMPLE: &[&str] = &[
    "allow",
    "auto",
    "cfg",
    "doc",
    "must_use",
    "no_std",
    "non_exhaustive",
    "path",
    "test",
    "trigger",
    "verifier::spinoff_prover",
    "verus_spec",
];
const DERIVES: &[&str] = &[
    "Clone",
    "Copy",
    "Debug",
    "Default",
    "Deserialize",
    "Eq",
    "Hash",
    "Ord",
    "PartialEq",
    "PartialOrd",
    "Serialize",
];

pub(super) fn inspect(
    tokens: &[Token],
    line: usize,
    deserialize_imported: bool,
    serialize_imported: bool,
    violations: &mut Vec<Violation>,
) {
    let name = attribute_name(tokens);
    let allowed = name.as_deref().is_some_and(|name| SIMPLE.contains(&name))
        || name.as_deref().is_some_and(trust_accounted)
        || audited_repr(tokens)
        || deserialize_imported && audited_serde(tokens)
        || name.as_deref() == Some("derive")
            && derive_list(tokens, deserialize_imported, serialize_imported);
    if !allowed {
        violations.push(unsupported(line, name.as_deref().unwrap_or("<malformed>")));
    }
}

pub(super) fn audited_deserialize_declaration(tokens: &[Token]) -> bool {
    audited_serde_declaration(tokens, "Deserialize")
}

pub(super) fn audited_serialize_declaration(tokens: &[Token]) -> bool {
    audited_serde_declaration(tokens, "Serialize")
}

fn audited_serde_declaration(tokens: &[Token], derive: &str) -> bool {
    tokens.len() == 4
        && identifier_is(&tokens[0], "serde")
        && punctuation_is(&tokens[1], ':')
        && punctuation_is(&tokens[2], ':')
        && identifier_is(&tokens[3], derive)
}

pub(super) fn is_expansion_name(name: &str) -> bool {
    SIMPLE.contains(&name) || DERIVES.contains(&name) || matches!(name, "derive" | "repr" | "serde")
}

pub(super) fn unsupported(line: usize, name: &str) -> Violation {
    Violation {
        line,
        function: format!("#[{name}]"),
        clause: None,
        kind: ViolationKind::UnsupportedAttribute,
    }
}

fn derive_list(tokens: &[Token], deserialize_imported: bool, serialize_imported: bool) -> bool {
    if tokens.len() < 4
        || !identifier_is(&tokens[0], "derive")
        || !punctuation_is(&tokens[1], '(')
        || matching_group(tokens, 1, '(', ')') != Some(tokens.len())
    {
        return false;
    }
    let values = &tokens[2..tokens.len() - 1];
    !values.is_empty()
        && values.iter().enumerate().all(|(index, token)| {
            if index % 2 == 0 {
                identifier(token).is_some_and(|name| {
                    DERIVES.contains(&name)
                        && (name != "Deserialize" || deserialize_imported)
                        && (name != "Serialize" || serialize_imported)
                })
            } else {
                punctuation_is(token, ',')
            }
        })
        && values.len() % 2 == 1
}

fn audited_repr(tokens: &[Token]) -> bool {
    tokens.len() == 4
        && identifier_is(&tokens[0], "repr")
        && punctuation_is(&tokens[1], '(')
        && identifier_is(&tokens[2], "u8")
        && punctuation_is(&tokens[3], ')')
}

fn audited_serde(tokens: &[Token]) -> bool {
    let default = tokens.len() == 4
        && identifier_is(&tokens[0], "serde")
        && punctuation_is(&tokens[1], '(')
        && identifier_is(&tokens[2], "default")
        && punctuation_is(&tokens[3], ')');
    let deny_unknown_fields = tokens.len() == 4
        && identifier_is(&tokens[0], "serde")
        && punctuation_is(&tokens[1], '(')
        && identifier_is(&tokens[2], "deny_unknown_fields")
        && punctuation_is(&tokens[3], ')');
    let reviewed_case = tokens.len() == 6
        && identifier_is(&tokens[0], "serde")
        && punctuation_is(&tokens[1], '(')
        && identifier_is(&tokens[2], "rename_all")
        && punctuation_is(&tokens[3], '=')
        && matches!(&tokens[4].kind, TokenKind::StringLiteral(Some(value)) if matches!(value.as_str(), "snake_case" | "kebab-case"))
        && punctuation_is(&tokens[5], ')');
    default || deny_unknown_fields || reviewed_case
}

fn attribute_name(tokens: &[Token]) -> Option<String> {
    let first = identifier(tokens.first()?)?;
    let mut name = first.to_owned();
    let mut cursor = 1;
    while tokens.get(cursor).is_some_and(|token| punctuation_is(token, ':'))
        && tokens.get(cursor + 1).is_some_and(|token| punctuation_is(token, ':'))
    {
        let segment = identifier(tokens.get(cursor + 2)?)?;
        name.push_str("::");
        name.push_str(segment);
        cursor += 3;
    }
    Some(name)
}

fn trust_accounted(name: &str) -> bool {
    matches!(
        name,
        "external"
            | "external_body"
            | "external_derive"
            | "external_fn_specification"
            | "external_trait_blanket"
            | "external_trait_extension"
            | "external_trait_private_bound"
            | "external_trait_specification"
            | "external_type_specification"
            | "verifier::assume_termination"
            | "verifier::exec_allows_no_decreases_clause"
            | "verifier::type_invariant"
            | "verus::trusted"
    )
}
