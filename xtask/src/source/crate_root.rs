use crate::error::Diagnostic;
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RootKind {
    Library,
    Binary,
}

pub(super) fn inspect_crate_root(
    relative: &Path,
    contents: &str,
    root_line_limit: usize,
    root_kind: RootKind,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let line_count = contents.lines().count();
    if line_count > root_line_limit {
        diagnostics.push(Diagnostic::at(
            relative,
            format!("crate root has {line_count} lines; composition budget is {root_line_limit}"),
            "move implementation into responsibility-named modules",
        ));
    }
    let allows_main = root_kind == RootKind::Binary;
    let tokens = tokenize(contents);
    inspect_composition(relative, &tokens, allows_main, true, diagnostics);
}

fn inspect_composition(
    relative: &Path,
    tokens: &[Token],
    allows_main: bool,
    allows_verus_wrapper: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut index = 0;
    while index < tokens.len() {
        index = skip_attributes(tokens, index);
        if index == tokens.len() {
            break;
        }
        let item_line = tokens[index].line;
        let mut cursor = skip_visibility(tokens, index);
        if allows_verus_wrapper
            && cursor == index
            && token_is(tokens, cursor, "verus")
            && token_is(tokens, cursor + 1, "!")
            && token_is(tokens, cursor + 2, "{")
        {
            let end = skip_group(tokens, cursor + 2, "{", "}");
            if end <= cursor + 3 || !token_is(tokens, end - 1, "}") {
                diagnostics.push(Diagnostic::at(
                    relative,
                    format!("line {item_line} has an unterminated Verus composition wrapper"),
                    "close the exact `verus! { ... }` wrapper around composition-only items",
                ));
                return;
            }
            inspect_composition(
                relative,
                &tokens[cursor + 3..end - 1],
                allows_main,
                false,
                diagnostics,
            );
            index = end;
            continue;
        }
        let mut qualified_function = false;
        loop {
            let qualifier = token_is(tokens, cursor, "async")
                || token_is(tokens, cursor, "const")
                || token_is(tokens, cursor, "unsafe")
                || token_is(tokens, cursor, "default")
                || (token_is(tokens, cursor, "extern") && !token_is(tokens, cursor + 1, "crate"));
            if !qualifier {
                break;
            }
            qualified_function = true;
            cursor += 1;
        }
        let allowed = token_is(tokens, cursor, "use")
            || (token_is(tokens, cursor, "extern") && token_is(tokens, cursor + 1, "crate"))
            || (token_is(tokens, cursor, "mod") && module_is_declaration(tokens, cursor))
            || (allows_main
                && !qualified_function
                && token_is(tokens, cursor, "fn")
                && token_is(tokens, cursor + 1, "main"));
        if !allowed {
            diagnostics.push(Diagnostic::at(
                relative,
                format!("line {item_line} contains implementation in a crate composition root"),
                "move the item to a responsibility-named module and re-export only intentional API",
            ));
        }
        let ends_at_semicolon = token_is(tokens, cursor, "use")
            || token_is(tokens, cursor, "extern")
            || (token_is(tokens, cursor, "mod") && allowed);
        index = if ends_at_semicolon {
            skip_to_semicolon(tokens, cursor)
        } else {
            skip_item(tokens, cursor)
        }
        .max(index + 1);
    }
}

#[derive(Debug)]
struct Token {
    text: String,
    line: usize,
}

fn tokenize(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut line = 1;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"//") {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if bytes[index..].starts_with(b"/*") {
            index = skip_block_comment(bytes, index, &mut line);
        } else if let Some(end) = raw_string_end(bytes, index) {
            line += source[index..end].matches('\n').count();
            index = end;
        } else if bytes[index] == b'"'
            || matches!(bytes.get(index..index + 2), Some(b"b\"" | b"c\""))
        {
            let quote = if bytes[index] == b'"' { index } else { index + 1 };
            index = quoted_end(bytes, quote, b'"', &mut line);
        } else if (bytes[index] == b'\'' && is_character_literal(bytes, index))
            || matches!(bytes.get(index..index + 2), Some(b"b'"))
        {
            let quote = if bytes[index] == b'\'' { index } else { index + 1 };
            index = quoted_end(bytes, quote, b'\'', &mut line);
        } else if bytes[index] == b'\n' {
            line += 1;
            index += 1;
        } else if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(Token { text: source[start..index].to_owned(), line });
        } else {
            let character = source[index..].chars().next().expect("index is in bounds");
            tokens.push(Token { text: character.to_string(), line });
            index += character.len_utf8();
        }
    }
    tokens
}

fn skip_block_comment(bytes: &[u8], mut index: usize, line: &mut usize) -> usize {
    let mut depth = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"/*") {
            depth += 1;
            index += 2;
        } else if bytes[index..].starts_with(b"*/") {
            index += 2;
            depth -= 1;
            if depth == 0 {
                break;
            }
        } else {
            *line += usize::from(bytes[index] == b'\n');
            index += 1;
        }
    }
    index
}

fn raw_string_end(bytes: &[u8], index: usize) -> Option<usize> {
    let mut cursor = index;
    if matches!(bytes.get(cursor), Some(b'b' | b'c')) {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor += 1;
    let hashes = bytes[cursor..].iter().take_while(|byte| **byte == b'#').count();
    cursor += hashes;
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    cursor += 1;
    while cursor < bytes.len() {
        let closing_hashes = bytes
            .get(cursor + 1..cursor + 1 + hashes)
            .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'));
        if bytes[cursor] == b'"' && closing_hashes {
            return Some(cursor + 1 + hashes);
        }
        cursor += 1;
    }
    Some(bytes.len())
}

fn quoted_end(bytes: &[u8], quote: usize, delimiter: u8, line: &mut usize) -> usize {
    let mut index = quote + 1;
    while index < bytes.len() {
        *line += usize::from(bytes[index] == b'\n');
        if bytes[index] == b'\\' {
            index = (index + 2).min(bytes.len());
        } else if bytes[index] == delimiter {
            return index + 1;
        } else {
            index += 1;
        }
    }
    index
}

fn is_character_literal(bytes: &[u8], quote: usize) -> bool {
    let Some(next) = bytes.get(quote + 1) else { return false };
    if *next == b'\\' {
        return bytes[quote + 2..].iter().take(8).any(|byte| *byte == b'\'');
    }
    bytes.get(quote + 2) == Some(&b'\'')
}

fn skip_attributes(tokens: &[Token], mut index: usize) -> usize {
    while token_is(tokens, index, "#") {
        index += 1;
        if token_is(tokens, index, "!") {
            index += 1;
        }
        if token_is(tokens, index, "[") {
            index = skip_group(tokens, index, "[", "]");
        }
    }
    index
}

fn skip_visibility(tokens: &[Token], mut index: usize) -> usize {
    if token_is(tokens, index, "pub") {
        index += 1;
        if token_is(tokens, index, "(") {
            index = skip_group(tokens, index, "(", ")");
        }
    }
    index
}

fn module_is_declaration(tokens: &[Token], mut index: usize) -> bool {
    while index < tokens.len() && !token_is(tokens, index, ";") {
        if token_is(tokens, index, "{") {
            return false;
        }
        index += 1;
    }
    index < tokens.len()
}

fn skip_to_semicolon(tokens: &[Token], mut index: usize) -> usize {
    while index < tokens.len() && !token_is(tokens, index, ";") {
        index += 1;
    }
    (index + 1).min(tokens.len())
}

fn skip_item(tokens: &[Token], mut index: usize) -> usize {
    let mut groups = Vec::new();
    while index < tokens.len() {
        match tokens[index].text.as_str() {
            "(" => groups.push(")"),
            "[" => groups.push("]"),
            "{" => groups.push("}"),
            ";" if groups.is_empty() => return index + 1,
            token if groups.last().is_some_and(|expected| token == *expected) => {
                groups.pop();
                if groups.is_empty() && token == "}" {
                    return index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    index
}

fn skip_group(tokens: &[Token], mut index: usize, open: &str, close: &str) -> usize {
    let mut depth = 0;
    while index < tokens.len() {
        if token_is(tokens, index, open) {
            depth += 1;
        } else if token_is(tokens, index, close) {
            depth -= 1;
            if depth == 0 {
                return index + 1;
            }
        }
        index += 1;
    }
    index
}

fn token_is(tokens: &[Token], index: usize, expected: &str) -> bool {
    tokens.get(index).is_some_and(|token| token.text == expected)
}
