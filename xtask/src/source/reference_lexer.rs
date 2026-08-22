#[derive(Debug, Eq, PartialEq)]
pub(crate) enum TokenKind {
    Identifier(String, bool),
    Punctuation(char),
    StringLiteral(Option<String>),
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) line: usize,
}

pub(crate) fn tokenize(source: &str) -> Vec<Token> {
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
                if let Some((value, end)) = raw_string(source, index) {
                    tokens.push(Token { kind: TokenKind::StringLiteral(value), line });
                    line += source[index..end].matches('\n').count();
                    index = end;
                } else if bytes[index] == b'"' {
                    let (value, end) = quoted_string(source, index);
                    tokens.push(Token { kind: TokenKind::StringLiteral(value), line });
                    line += source[index..end].matches('\n').count();
                    index = end;
                } else if let Some(end) = character_or_prefixed_literal(source, index) {
                    line += source[index..end].matches('\n').count();
                    index = end;
                } else if let Some((identifier, raw, end)) = identifier(source, index) {
                    tokens.push(Token {
                        kind: TokenKind::Identifier(identifier.to_owned(), raw),
                        line,
                    });
                    index = end;
                } else {
                    let character = source[index..].chars().next().expect("index is in bounds");
                    tokens.push(Token { kind: TokenKind::Punctuation(character), line });
                    index += character.len_utf8();
                }
            }
        }
    }
    tokens
}

fn raw_string(source: &str, index: usize) -> Option<(Option<String>, usize)> {
    let bytes = source.as_bytes();
    let (raw_marker, is_path_string) = match bytes.get(index..index + 2) {
        Some(b"br" | b"cr") => (index + 2, false),
        _ if bytes.get(index) == Some(&b'r') => (index + 1, true),
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
            let value = is_path_string.then(|| source[content_start..cursor].to_owned());
            return Some((value, cursor + 1 + hashes));
        }
        cursor += 1;
    }
    Some((None, bytes.len()))
}

fn quoted_string(source: &str, start: usize) -> (Option<String>, usize) {
    let bytes = source.as_bytes();
    let mut value = String::new();
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'"' {
            return (Some(value), index + 1);
        }
        if bytes[index] == b'\\' {
            let Some((character, end)) = escaped_character(source, index + 1) else {
                return (None, skip_quoted(bytes, index + 1));
            };
            if let Some(character) = character {
                value.push(character);
            }
            index = end;
        } else {
            let character = source[index..].chars().next().expect("index is in bounds");
            value.push(character);
            index += character.len_utf8();
        }
    }
    (None, bytes.len())
}

fn escaped_character(source: &str, index: usize) -> Option<(Option<char>, usize)> {
    let bytes = source.as_bytes();
    let simple = match bytes.get(index)? {
        b'\\' => Some('\\'),
        b'"' => Some('"'),
        b'n' => Some('\n'),
        b'r' => Some('\r'),
        b't' => Some('\t'),
        b'0' => Some('\0'),
        b'\n' => {
            let mut end = index + 1;
            while bytes.get(end).is_some_and(u8::is_ascii_whitespace) {
                end += 1;
            }
            return Some((None, end));
        }
        b'x' => {
            let digits = source.get(index + 1..index + 3)?;
            let value = u8::from_str_radix(digits, 16).ok()?;
            return Some((Some(char::from(value)), index + 3));
        }
        b'u' if bytes.get(index + 1) == Some(&b'{') => {
            let close = source.get(index + 2..)?.find('}')? + index + 2;
            let digits: String =
                source[index + 2..close].chars().filter(|character| *character != '_').collect();
            let value = u32::from_str_radix(&digits, 16).ok()?;
            return char::from_u32(value).map(|character| (Some(character), close + 1));
        }
        _ => return None,
    };
    Some((simple, index + 1))
}

fn character_or_prefixed_literal(source: &str, index: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(index..index + 2) == Some(b"b\"") || bytes.get(index..index + 2) == Some(b"c\"") {
        return Some(skip_quoted(bytes, index + 2));
    }
    let quote = if bytes.get(index..index + 2) == Some(b"b'") {
        index + 1
    } else if bytes.get(index) == Some(&b'\'') {
        index
    } else {
        return None;
    };
    character_literal_end(bytes, quote).or_else(|| {
        (quote == index).then(|| identifier(source, index + 1).map(|(_, _, end)| end)).flatten()
    })
}

fn character_literal_end(bytes: &[u8], quote: usize) -> Option<usize> {
    let mut cursor = quote + 1;
    if bytes.get(cursor) == Some(&b'\\') {
        cursor = (cursor + 2).min(bytes.len());
    } else {
        let width = source_character_width(*bytes.get(cursor)?);
        cursor = cursor.checked_add(width)?;
    }
    (bytes.get(cursor) == Some(&b'\'')).then_some(cursor + 1)
}

fn skip_quoted(bytes: &[u8], mut index: usize) -> usize {
    let delimiter = if bytes.get(index.saturating_sub(1)) == Some(&b'\'') { b'\'' } else { b'"' };
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = (index + 2).min(bytes.len());
        } else if bytes[index] == delimiter {
            return index + 1;
        } else {
            index += 1;
        }
    }
    bytes.len()
}

fn skip_line_comment(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

fn skip_block_comment(bytes: &[u8], mut index: usize, mut line: usize) -> (usize, usize) {
    let mut depth = 1;
    while index < bytes.len() && depth > 0 {
        if bytes[index..].starts_with(b"/*") {
            depth += 1;
            index += 2;
        } else if bytes[index..].starts_with(b"*/") {
            depth -= 1;
            index += 2;
        } else {
            line += usize::from(bytes[index] == b'\n');
            index += 1;
        }
    }
    (index, line)
}

fn identifier(source: &str, index: usize) -> Option<(&str, bool, usize)> {
    let bytes = source.as_bytes();
    let raw = bytes.get(index..index + 2) == Some(b"r#");
    let start = if raw { index + 2 } else { index };
    let mut characters = source.get(start..)?.char_indices();
    let (_, first) = characters.next()?;
    if first != '_' && !first.is_alphabetic() {
        return None;
    }
    let mut end = start + first.len_utf8();
    for (offset, character) in characters {
        if character != '_' && !character.is_alphanumeric() {
            break;
        }
        end = start + offset + character.len_utf8();
    }
    Some((&source[start..end], raw, end))
}

const fn source_character_width(first_byte: u8) -> usize {
    match first_byte {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}
