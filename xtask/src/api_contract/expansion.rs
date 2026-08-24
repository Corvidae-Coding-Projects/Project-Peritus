//! Fail-closed source-expansion policy for formal-boundary code.

use super::violation::{Violation, ViolationKind};
use crate::source::reference_lexer::{Token, TokenKind};

const SIMPLE_ATTRIBUTES: &[&str] = &[
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
const BUILTIN_DERIVES: &[&str] =
    &["Clone", "Copy", "Debug", "Default", "Eq", "Hash", "Ord", "PartialEq", "PartialOrd"];
const MODELED_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "format",
    "include",
    "matches",
    "panic",
    "params",
    "proof",
    "vec",
    "verus",
    "write",
];
const FORBIDDEN_EXPANSION_NAMES: &[&str] = &["state_machine", "tokenized_state_machine"];

pub(super) fn violations(tokens: &[Token]) -> Vec<Violation> {
    let local_modules = local_module_names(tokens);
    let params_imported = tokens
        .iter()
        .enumerate()
        .any(|(index, token)| identifier_is(token, "use") && audited_params_import(tokens, index));
    let mut violations = Vec::new();
    let mut cursor = 0;
    while cursor < tokens.len() {
        if identifier_is(&tokens[cursor], "use") {
            inspect_use(tokens, cursor, &local_modules, &mut violations);
        }
        if punctuation_is(&tokens[cursor], '#') {
            let mut open = cursor + 1;
            if tokens.get(open).is_some_and(|token| punctuation_is(token, '!')) {
                open += 1;
            }
            if tokens.get(open).is_some_and(|token| punctuation_is(token, '[')) {
                let Some(end) = matching_group(tokens, open, '[', ']') else {
                    violations.push(unsupported_attribute(tokens[cursor].line, "<malformed>"));
                    break;
                };
                inspect_attribute(
                    &tokens[open + 1..end.saturating_sub(1)],
                    tokens[cursor].line,
                    &mut violations,
                );
                cursor = end;
                continue;
            }
        }
        inspect_macro(tokens, cursor, params_imported, &mut violations);
        cursor += 1;
    }
    violations
}

fn local_module_names(tokens: &[Token]) -> Vec<&str> {
    tokens
        .windows(3)
        .filter(|window| {
            identifier_is(&window[0], "mod") && matches!(punctuation(&window[2]), Some(';' | '{'))
        })
        .filter_map(|window| identifier(&window[1]))
        .collect()
}

fn inspect_attribute(tokens: &[Token], line: usize, violations: &mut Vec<Violation>) {
    let name = attribute_name(tokens);
    let allowed = name.as_deref().is_some_and(|name| SIMPLE_ATTRIBUTES.contains(&name))
        || name.as_deref().is_some_and(trust_accounted_attribute)
        || name.as_deref() == Some("derive") && builtin_derive_list(tokens);
    if !allowed {
        violations.push(unsupported_attribute(line, name.as_deref().unwrap_or("<malformed>")));
    }
}

fn builtin_derive_list(tokens: &[Token]) -> bool {
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
                identifier(token).is_some_and(|name| BUILTIN_DERIVES.contains(&name))
            } else {
                punctuation_is(token, ',')
            }
        })
        && values.len() % 2 == 1
}

fn inspect_macro(
    tokens: &[Token],
    cursor: usize,
    params_imported: bool,
    violations: &mut Vec<Violation>,
) {
    let TokenKind::Identifier(name, raw) = &tokens[cursor].kind else { return };
    if !tokens.get(cursor + 1).is_some_and(|token| punctuation_is(token, '!'))
        || !tokens
            .get(cursor + 2)
            .is_some_and(|token| matches!(punctuation(token), Some('(' | '[' | '{')))
    {
        return;
    }
    if !raw && rust_keyword(name) {
        return;
    }
    let qualified = cursor >= 2
        && punctuation_is(&tokens[cursor - 2], ':')
        && punctuation_is(&tokens[cursor - 1], ':');
    if *raw
        || qualified
        || !MODELED_MACROS.contains(&name.as_str())
        || name == "params" && !params_imported
    {
        violations.push(Violation {
            line: tokens[cursor].line,
            function: if qualified { format!("qualified::{name}!") } else { format!("{name}!") },
            clause: None,
            kind: ViolationKind::UnsupportedMacro,
        });
    }
}

fn rust_keyword(name: &str) -> bool {
    matches!(
        name,
        "Self"
            | "abstract"
            | "as"
            | "async"
            | "await"
            | "become"
            | "box"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "final"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "override"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}

fn inspect_use(
    tokens: &[Token],
    start: usize,
    local_modules: &[&str],
    violations: &mut Vec<Violation>,
) {
    let end = tokens[start..]
        .iter()
        .position(|token| punctuation_is(token, ';'))
        .map_or(tokens.len(), |offset| start + offset);
    let declaration = &tokens[start + 1..end];
    let first = declaration.iter().find_map(identifier);
    let trusted_namespace =
        matches!(first, Some("core" | "std" | "alloc" | "vstd" | "crate" | "self" | "super"))
            || first.is_some_and(|name| local_modules.contains(&name));
    let glob = declaration.iter().any(|token| punctuation_is(token, '*'));
    let expansion_alias = declaration.windows(2).any(|pair| {
        identifier_is(&pair[0], "as") && identifier(&pair[1]).is_some_and(is_expansion_name)
    });
    let expansion_names = imported_expansion_names(declaration);
    let imports_only_audited_params =
        expansion_names == ["params"] && audited_params_declaration(declaration);
    let imports_modeled_macro = expansion_names
        .iter()
        .any(|name| MODELED_MACROS.contains(name) || FORBIDDEN_EXPANSION_NAMES.contains(name));
    let unaudited_expansion_import = !imports_only_audited_params
        && (imports_modeled_macro || !trusted_namespace && !expansion_names.is_empty());
    if expansion_alias || unaudited_expansion_import || !trusted_namespace && glob {
        violations.push(Violation {
            line: tokens[start].line,
            function: "external expansion import".to_owned(),
            clause: None,
            kind: ViolationKind::UnsupportedMacro,
        });
    }
}

fn imported_expansion_names(tokens: &[Token]) -> Vec<&str> {
    tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            let name = identifier(token)?;
            let is_path_segment =
                tokens.get(index + 1).is_some_and(|token| punctuation_is(token, ':'))
                    && tokens.get(index + 2).is_some_and(|token| punctuation_is(token, ':'));
            (!is_path_segment && is_expansion_name(name)).then_some(name)
        })
        .collect()
}

fn audited_params_import(tokens: &[Token], start: usize) -> bool {
    let end = tokens[start..]
        .iter()
        .position(|token| punctuation_is(token, ';'))
        .map_or(tokens.len(), |offset| start + offset);
    audited_params_declaration(&tokens[start + 1..end])
}

fn audited_params_declaration(tokens: &[Token]) -> bool {
    if tokens.len() < 4
        || !identifier_is(&tokens[0], "rusqlite")
        || !punctuation_is(&tokens[1], ':')
        || !punctuation_is(&tokens[2], ':')
        || tokens.iter().any(|token| identifier_is(token, "as"))
        || tokens.iter().any(|token| punctuation_is(token, '*'))
    {
        return false;
    }

    let imports = &tokens[3..];
    if imports.len() == 1 {
        return identifier_is(&imports[0], "params");
    }
    if !punctuation_is(&imports[0], '{')
        || matching_group(imports, 0, '{', '}') != Some(imports.len())
    {
        return false;
    }

    let mut depth = 0_usize;
    let mut segment_start = 1;
    for index in 1..imports.len() - 1 {
        match punctuation(&imports[index]) {
            Some('{' | '(' | '[') => depth += 1,
            Some('}' | ')' | ']') => depth = depth.saturating_sub(1),
            Some(',') if depth == 0 => {
                if imports[segment_start..index].len() == 1
                    && identifier_is(&imports[segment_start], "params")
                {
                    return true;
                }
                segment_start = index + 1;
            }
            _ => {}
        }
    }
    imports[segment_start..imports.len() - 1].len() == 1
        && identifier_is(&imports[segment_start], "params")
}

fn is_expansion_name(name: &str) -> bool {
    SIMPLE_ATTRIBUTES.contains(&name)
        || MODELED_MACROS.contains(&name)
        || FORBIDDEN_EXPANSION_NAMES.contains(&name)
        || BUILTIN_DERIVES.contains(&name)
        || name == "derive"
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

fn trust_accounted_attribute(name: &str) -> bool {
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

fn unsupported_attribute(line: usize, name: &str) -> Violation {
    Violation {
        line,
        function: format!("#[{name}]"),
        clause: None,
        kind: ViolationKind::UnsupportedAttribute,
    }
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
    identifier(token) == Some(expected)
}

fn identifier(token: &Token) -> Option<&str> {
    match &token.kind {
        TokenKind::Identifier(value, false) => Some(value),
        TokenKind::Identifier(_, true)
        | TokenKind::Punctuation(_)
        | TokenKind::StringLiteral(_) => None,
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
