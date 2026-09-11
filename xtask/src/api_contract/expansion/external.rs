//! Audited expression-only expansion boundaries of the pinned Serde JSON and Tokio versions.
//! Payload tokens are never skipped: nested functions, contracts, and macros remain scanned.

use super::{identifier, identifier_is, matching_group, punctuation_is};
use crate::source::reference_lexer::Token;

pub(super) fn is_expansion_name(name: &str) -> bool {
    // `pin!` and `select!` are accepted only when fully qualified, so an ordinary function
    // import named `select` cannot shadow either modeled expansion and needs no exception.
    matches!(name, "json" | "serde_json" | "tokio")
}

pub(super) fn audited_macro(tokens: &[Token], cursor: usize, local_modules: &[&str]) -> bool {
    let Some(name) = identifier(&tokens[cursor]) else { return false };
    let qualified = cursor >= 2
        && punctuation_is(&tokens[cursor - 1], ':')
        && punctuation_is(&tokens[cursor - 2], ':');
    if qualified {
        let Some(namespace) = cursor.checked_sub(3).and_then(|index| identifier(&tokens[index]))
        else {
            return false;
        };
        // No re-exported/nested paths or local modules masquerading as a pinned dependency.
        if cursor >= 4 && punctuation_is(&tokens[cursor - 4], ':')
            || local_modules.contains(&namespace)
        {
            return false;
        }
        return match (namespace, name) {
            ("serde_json", "json") | ("tokio", "select") => true,
            ("tokio", "pin") => single_pin_identifier(tokens, cursor),
            _ => false,
        };
    }
    name == "json"
        && !local_modules.contains(&"serde_json")
        && tokens.iter().enumerate().any(|(index, token)| {
            if !identifier_is(token, "use") {
                return false;
            }
            let end = tokens[index..]
                .iter()
                .position(|token| punctuation_is(token, ';'))
                .map_or(tokens.len(), |offset| index + offset);
            audited_json_declaration(&tokens[index + 1..end])
        })
}

pub(super) fn audited_json_declaration(tokens: &[Token]) -> bool {
    tokens.len() == 4
        && identifier_is(&tokens[0], "serde_json")
        && punctuation_is(&tokens[1], ':')
        && punctuation_is(&tokens[2], ':')
        && identifier_is(&tokens[3], "json")
}

fn single_pin_identifier(tokens: &[Token], cursor: usize) -> bool {
    let open = cursor + 2;
    punctuation_is(&tokens[open], '(')
        && matching_group(tokens, open, '(', ')') == Some(open + 3)
        && identifier(&tokens[open + 1]).is_some()
}
