//! Exact attribute and derive policy for formal-boundary source.

use super::{identifier, identifier_is, matching_group, punctuation_is};
use crate::source::reference_lexer::{Token, TokenKind};

use super::super::violation::{Violation, ViolationKind};

#[path = "attributes/serde.rs"]
mod serde;

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
    tokio_namespace: bool,
    violations: &mut Vec<Violation>,
) {
    let name = attribute_name(tokens);
    let allowed = name.as_deref().is_some_and(|name| SIMPLE.contains(&name))
        || name.as_deref().is_some_and(trust_accounted)
        || audited_direct_verification(tokens)
        || audited_ghost_documentation(tokens)
        || audited_repr(tokens)
        || deserialize_imported && serde::audited(tokens, serialize_imported)
        || name.as_deref() == Some("default") && tokens.len() == 1
        || name.as_deref() == Some("ignore")
            && tokens.len() == 3
            && punctuation_is(&tokens[1], '=')
            && matches!(&tokens[2].kind, TokenKind::StringLiteral(Some(reason)) if !reason.trim().is_empty())
        || tokio_namespace && name.as_deref() == Some("tokio::test") && tokens.len() == 4
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
    if tokens.len() < 4
        || !identifier_is(&tokens[0], "serde")
        || !punctuation_is(&tokens[1], ':')
        || !punctuation_is(&tokens[2], ':')
    {
        return false;
    }
    if tokens.len() == 4 {
        return identifier_is(&tokens[3], derive);
    }
    let imports = &tokens[3..];
    if !punctuation_is(&imports[0], '{')
        || matching_group(imports, 0, '{', '}') != Some(imports.len())
    {
        return false;
    }
    let names = &imports[1..imports.len() - 1];
    !names.is_empty()
        && names.iter().enumerate().all(|(index, token)| {
            if index % 2 == 0 {
                matches!(identifier(token), Some("Deserialize" | "Serialize"))
            } else {
                punctuation_is(token, ',')
            }
        })
        && names.iter().any(|token| identifier_is(token, derive))
}

pub(super) fn is_expansion_name(name: &str) -> bool {
    SIMPLE.contains(&name)
        || DERIVES.contains(&name)
        || matches!(name, "derive" | "repr" | "serde" | "default" | "ignore" | "cfg_attr")
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

// The pinned Verus toolchain uses this marker to verify ordinary Rust declarations
// without routing them through `verus!` and synthesizing undocumented enum projections.
fn audited_direct_verification(tokens: &[Token]) -> bool {
    tokens.len() == 9
        && identifier_is(&tokens[0], "cfg_attr")
        && punctuation_is(&tokens[1], '(')
        && identifier_is(&tokens[2], "verus_keep_ghost")
        && punctuation_is(&tokens[3], ',')
        && identifier_is(&tokens[4], "verifier")
        && punctuation_is(&tokens[5], ':')
        && punctuation_is(&tokens[6], ':')
        && identifier_is(&tokens[7], "verify")
        && punctuation_is(&tokens[8], ')')
}

// The pinned Verus enum expansion synthesizes undocumented ghost projection methods.
// Permit only documentation lint metadata in that expansion; neither ordinary Rust
// documentation checks nor executable attributes/contracts can change through this form.
fn audited_ghost_documentation(tokens: &[Token]) -> bool {
    if tokens.len() < 13
        || !identifier_is(&tokens[0], "cfg_attr")
        || !punctuation_is(&tokens[1], '(')
        || matching_group(tokens, 1, '(', ')') != Some(tokens.len())
    {
        return false;
    }
    let arguments = without_trailing_comma(&tokens[2..tokens.len() - 1]);
    if arguments.len() < 3
        || !identifier_is(&arguments[0], "verus_keep_ghost")
        || !punctuation_is(&arguments[1], ',')
    {
        return false;
    }
    let attribute = &arguments[2..];
    if attribute.len() < 4
        || !identifier_is(&attribute[0], "allow")
        || !punctuation_is(&attribute[1], '(')
        || matching_group(attribute, 1, '(', ')') != Some(attribute.len())
    {
        return false;
    }
    let lint = without_trailing_comma(&attribute[2..attribute.len() - 1]);
    lint.len() == 5
        && identifier_is(&lint[0], "missing_docs")
        && punctuation_is(&lint[1], ',')
        && identifier_is(&lint[2], "reason")
        && punctuation_is(&lint[3], '=')
        && matches!(&lint[4].kind, TokenKind::StringLiteral(Some(reason)) if !reason.trim().is_empty())
}

fn without_trailing_comma(tokens: &[Token]) -> &[Token] {
    if tokens.last().is_some_and(|token| punctuation_is(token, ',')) {
        &tokens[..tokens.len() - 1]
    } else {
        tokens
    }
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
