use super::construct::Construct;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Occurrence {
    pub(super) construct: Construct,
    pub(super) line: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TokenKind<'source> {
    Identifier(&'source str),
    Punctuation(u8),
    AllowInlineAir,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Token<'source> {
    kind: TokenKind<'source>,
    line: usize,
}

pub(super) fn scan(source: &str) -> Vec<Occurrence> {
    let tokens = tokenize(source);
    let mut occurrences = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        let construct = match token.kind {
            TokenKind::Identifier(identifier) => identifier_construct(identifier),
            TokenKind::AllowInlineAir => Some(Construct::AllowInlineAir),
            TokenKind::Punctuation(_) => None,
        };
        if let Some(construct) = construct {
            occurrences.push(Occurrence { construct, line: token.line });
        }

        if let Some(construct) = attribute_construct(&tokens, index) {
            occurrences.push(Occurrence { construct, line: token.line });
        }
    }

    occurrences
}

fn identifier_construct(identifier: &str) -> Option<Construct> {
    match identifier {
        "assume" => Some(Construct::Assume),
        "admit" => Some(Construct::Admit),
        "axiom" => Some(Construct::Axiom),
        "assume_specification" => Some(Construct::AssumeSpecification),
        "exec_spec_unverified" => Some(Construct::ExecSpecUnverified),
        "inline_air_stmt" => Some(Construct::InlineAirStatement),
        concat!("allow_", "inline_air") => Some(Construct::AllowInlineAir),
        _ => None,
    }
}

fn attribute_construct(tokens: &[Token<'_>], index: usize) -> Option<Construct> {
    let TokenKind::Identifier(identifier) = tokens[index].kind else { return None };
    if !inside_attribute(tokens, index) {
        return None;
    }
    match identifier {
        "external" => Some(Construct::External),
        "external_body" => Some(Construct::ExternalBody),
        "external_fn_specification" => Some(Construct::ExternalFunctionSpecification),
        "external_type_specification" => Some(Construct::ExternalTypeSpecification),
        "external_trait_specification" => Some(Construct::ExternalTraitSpecification),
        "external_trait_extension" => Some(Construct::ExternalTraitExtension),
        "external_trait_private_bound" => Some(Construct::ExternalTraitPrivateBound),
        "external_derive" => Some(Construct::ExternalDerive),
        "external_trait_blanket" => Some(Construct::ExternalTraitBlanket),
        "trusted" if path_prefix_is(tokens, index, "verus") => Some(Construct::Trusted),
        "assume_termination" if path_prefix_is(tokens, index, "verifier") => {
            Some(Construct::AssumeTermination)
        }
        "exec_allows_no_decreases_clause" if path_prefix_is(tokens, index, "verifier") => {
            Some(Construct::ExecAllowsNoDecreases)
        }
        _ => None,
    }
}

fn path_prefix_is(tokens: &[Token<'_>], index: usize, expected: &str) -> bool {
    index >= 3
        && matches!(tokens[index - 3].kind, TokenKind::Identifier(prefix) if prefix == expected)
        && matches!(tokens[index - 2].kind, TokenKind::Punctuation(b':'))
        && matches!(tokens[index - 1].kind, TokenKind::Punctuation(b':'))
}

fn inside_attribute(tokens: &[Token<'_>], index: usize) -> bool {
    let mut depth = 0;
    for cursor in (0..index).rev() {
        match tokens[cursor].kind {
            TokenKind::Punctuation(b']') => depth += 1,
            TokenKind::Punctuation(b'[') if depth == 0 => {
                let hash = cursor.checked_sub(1).filter(|position| {
                    matches!(tokens[*position].kind, TokenKind::Punctuation(b'#'))
                });
                let inner_hash = cursor.checked_sub(2).filter(|position| {
                    matches!(tokens[*position].kind, TokenKind::Punctuation(b'#'))
                        && matches!(tokens[cursor - 1].kind, TokenKind::Punctuation(b'!'))
                });
                return hash.is_some() || inner_hash.is_some();
            }
            TokenKind::Punctuation(b'[') => depth -= 1,
            _ => {}
        }
    }
    false
}

fn tokenize(source: &str) -> Vec<Token<'_>> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut line = 1;

    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                line += 1;
                index += 1;
            }
            byte if byte.is_ascii_whitespace() => index += 1,
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = skip_line_comment(bytes, index + 2);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                (index, line) = skip_block_comment(bytes, index + 2, line);
            }
            _ => {
                if let Some((content_start, content_end, end)) = raw_literal(bytes, index) {
                    push_literal_token(source, content_start, content_end, line, &mut tokens);
                    line += count_newlines(&bytes[index..end]);
                    index = end;
                } else if let Some((content_start, content_end, end)) = quoted_string(bytes, index)
                {
                    push_literal_token(source, content_start, content_end, line, &mut tokens);
                    line += count_newlines(&bytes[index..end]);
                    index = end;
                } else if let Some(end) = character_literal(bytes, index) {
                    line += count_newlines(&bytes[index..end]);
                    index = end;
                } else if let Some(end) = lifetime(source, index) {
                    index = end;
                } else if let Some((identifier, end)) = identifier(source, index) {
                    tokens.push(Token { kind: TokenKind::Identifier(identifier), line });
                    index = end;
                } else {
                    tokens.push(Token { kind: TokenKind::Punctuation(bytes[index]), line });
                    index += source[index..].chars().next().map_or(1, char::len_utf8);
                }
            }
        }
    }

    tokens
}

fn push_literal_token<'source>(
    source: &'source str,
    content_start: usize,
    content_end: usize,
    line: usize,
    tokens: &mut Vec<Token<'source>>,
) {
    let option = concat!("--allow", "-inline-air");
    if &source[content_start..content_end] == option {
        tokens.push(Token { kind: TokenKind::AllowInlineAir, line });
    }
}

fn skip_line_comment(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

fn skip_block_comment(bytes: &[u8], mut index: usize, mut line: usize) -> (usize, usize) {
    let mut depth = 1_usize;
    while index < bytes.len() && depth > 0 {
        if bytes[index] == b'\n' {
            line += 1;
            index += 1;
        } else if bytes[index..].starts_with(b"/*") {
            depth += 1;
            index += 2;
        } else if bytes[index..].starts_with(b"*/") {
            depth -= 1;
            index += 2;
        } else {
            index += 1;
        }
    }
    (index, line)
}

fn raw_literal(bytes: &[u8], index: usize) -> Option<(usize, usize, usize)> {
    let raw_marker = match bytes.get(index..index + 2) {
        Some(b"br" | b"cr") => index + 2,
        _ if bytes.get(index) == Some(&b'r') => index + 1,
        _ => return None,
    };
    let mut quote = raw_marker;
    while bytes.get(quote) == Some(&b'#') {
        quote += 1;
    }
    if bytes.get(quote) != Some(&b'"') {
        return None;
    }

    let hashes = quote - raw_marker;
    let content_start = quote + 1;
    let mut cursor = content_start;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"'
            && bytes.get(cursor + 1..cursor + 1 + hashes) == Some(&bytes[raw_marker..quote])
        {
            return Some((content_start, cursor, cursor + 1 + hashes));
        }
        cursor += 1;
    }
    Some((content_start, bytes.len(), bytes.len()))
}

fn quoted_string(bytes: &[u8], index: usize) -> Option<(usize, usize, usize)> {
    let quote = match bytes.get(index..index + 2) {
        Some([b'b' | b'c', b'"']) => index + 1,
        _ if bytes.get(index) == Some(&b'"') => index,
        _ => return None,
    };
    let content_start = quote + 1;
    let end = skip_escaped_literal(bytes, content_start, b'"');
    let content_end = if bytes.get(end.saturating_sub(1)) == Some(&b'"') { end - 1 } else { end };
    Some((content_start, content_end, end))
}

fn character_literal(bytes: &[u8], index: usize) -> Option<usize> {
    let quote = if bytes.get(index..index + 2) == Some(b"b'") {
        index + 1
    } else if bytes.get(index) == Some(&b'\'') {
        index
    } else {
        return None;
    };
    let mut cursor = quote + 1;
    if bytes.get(cursor) == Some(&b'\\') {
        cursor = skip_escape(bytes, cursor + 1);
    } else {
        cursor += utf8_character_width(*bytes.get(cursor)?);
    }
    (bytes.get(cursor) == Some(&b'\'')).then_some(cursor + 1)
}

fn lifetime(source: &str, index: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(index) != Some(&b'\'') {
        return None;
    }
    identifier(source, index + 1).map(|(_, end)| end)
}

fn skip_escaped_literal(bytes: &[u8], mut index: usize, delimiter: u8) -> usize {
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = skip_escape(bytes, index + 1);
        } else if bytes[index] == delimiter {
            return index + 1;
        } else {
            index += 1;
        }
    }
    bytes.len()
}

fn skip_escape(bytes: &[u8], index: usize) -> usize {
    if bytes.get(index) == Some(&b'u') && bytes.get(index + 1) == Some(&b'{') {
        let mut cursor = index + 2;
        while cursor < bytes.len() && bytes[cursor] != b'}' {
            cursor += 1;
        }
        return (cursor + 1).min(bytes.len());
    }
    (index + 1).min(bytes.len())
}

fn identifier(source: &str, index: usize) -> Option<(&str, usize)> {
    let bytes = source.as_bytes();
    let start = if bytes.get(index..index + 2) == Some(b"r#")
        && source
            .get(index + 2..)
            .and_then(|tail| tail.chars().next())
            .is_some_and(identifier_start)
    {
        index + 2
    } else {
        index
    };
    let mut characters = source.get(start..)?.char_indices();
    let (_, first) = characters.next()?;
    if !identifier_start(first) {
        return None;
    }
    let mut end = start + first.len_utf8();
    for (offset, character) in characters {
        if !identifier_continue(character) {
            break;
        }
        end = start + offset + character.len_utf8();
    }
    Some((&source[start..end], end))
}

fn identifier_start(character: char) -> bool {
    character == '_' || character.is_alphabetic() || !character.is_ascii()
}

fn identifier_continue(character: char) -> bool {
    identifier_start(character) || character.is_ascii_digit()
}

const fn utf8_character_width(first_byte: u8) -> usize {
    match first_byte {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

fn count_newlines(bytes: &[u8]) -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            count += 1;
        }
        index += 1;
    }
    count
}
