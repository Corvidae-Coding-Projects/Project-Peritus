//! Closed model for the pinned standard library's `thread_local!` expansion.

use super::{identifier, identifier_is, matching_group, punctuation_is};
use crate::source::reference_lexer::Token;

pub(super) fn audited_std_macro(tokens: &[Token], cursor: usize, local_modules: &[&str]) -> bool {
    (cursor == 3 || !punctuation_is(&tokens[cursor - 4], ':'))
        && !local_modules.contains(&"std")
        && !std_namespace_is_imported(tokens)
        && audited_declarations(tokens, cursor)
}

fn audited_declarations(tokens: &[Token], cursor: usize) -> bool {
    let open = cursor + 2;
    if !tokens.get(open).is_some_and(|token| punctuation_is(token, '{')) {
        return false;
    }
    let Some(end) = matching_group(tokens, open, '{', '}') else { return false };
    let body = &tokens[open + 1..end - 1];
    let mut cursor = 0;
    let mut declarations = 0;
    while cursor < body.len() {
        let Some(next) = declaration_end(body, cursor) else { return false };
        cursor = next;
        declarations += 1;
    }
    declarations > 0
}

fn declaration_end(tokens: &[Token], start: usize) -> Option<usize> {
    if !tokens.get(start).is_some_and(|token| identifier_is(token, "static")) {
        return None;
    }
    let name = tokens.get(start + 1).and_then(identifier)?;
    if name == "_" || super::super::rust_keyword(name) {
        return None;
    }
    if !tokens.get(start + 2).is_some_and(|token| punctuation_is(token, ':')) {
        return None;
    }

    let assignment = type_assignment(tokens, start + 3)?;
    if assignment == start + 3
        || !tokens.get(assignment + 1).is_some_and(|token| identifier_is(token, "const"))
        || !tokens.get(assignment + 2).is_some_and(|token| punctuation_is(token, '{'))
    {
        return None;
    }
    let end = matching_group(tokens, assignment + 2, '{', '}')?;
    tokens.get(end).filter(|token| punctuation_is(token, ';')).map(|_| end + 1)
}

fn type_assignment(tokens: &[Token], start: usize) -> Option<usize> {
    let mut groups = Vec::new();
    let mut angles = 0_usize;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        match super::super::punctuation(token) {
            Some(open @ ('(' | '[' | '{')) => groups.push(open),
            Some(')') if groups.pop() != Some('(') => return None,
            Some(']') if groups.pop() != Some('[') => return None,
            Some('}') if groups.pop() != Some('{') => return None,
            Some('<') if !groups.contains(&'{') => angles += 1,
            Some('>') if !groups.contains(&'{') => angles = angles.saturating_sub(1),
            Some('=') if groups.is_empty() && angles == 0 => return Some(index),
            Some(';') if groups.is_empty() && angles == 0 => return None,
            _ => {}
        }
    }
    None
}

fn std_namespace_is_imported(tokens: &[Token]) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        if identifier_is(token, "use") {
            let end = declaration_end_at_semicolon(tokens, index + 1);
            return use_binds_std(&tokens[index + 1..end]);
        }
        if identifier_is(token, "extern")
            && tokens.get(index + 1).is_some_and(|token| identifier_is(token, "crate"))
        {
            let end = declaration_end_at_semicolon(tokens, index + 2);
            let declaration = &tokens[index + 2..end];
            return declaration
                .windows(2)
                .any(|pair| identifier_is(&pair[0], "as") && identifier_is(&pair[1], "std"));
        }
        false
    })
}

fn declaration_end_at_semicolon(tokens: &[Token], start: usize) -> usize {
    tokens[start..]
        .iter()
        .position(|token| punctuation_is(token, ';'))
        .map_or(tokens.len(), |offset| start + offset)
}

fn use_binds_std(tokens: &[Token]) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        if !identifier_is(token, "std") {
            return false;
        }
        if index > 0 && identifier_is(&tokens[index - 1], "as") {
            return true;
        }
        let is_path_segment = tokens.get(index + 1).is_some_and(|token| punctuation_is(token, ':'))
            && tokens.get(index + 2).is_some_and(|token| punctuation_is(token, ':'));
        let is_renamed = tokens.get(index + 1).is_some_and(|token| identifier_is(token, "as"));
        !is_path_segment && !is_renamed
    })
}
