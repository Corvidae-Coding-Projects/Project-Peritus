//! Conservative byte scanner: strings are opaque and only complete containers are extracted.

pub(super) fn extract(input: &str) -> Option<&str> {
    let text = input.trim();
    if let Some(fenced) = text.strip_prefix("```json").or_else(|| text.strip_prefix("```")) {
        return fenced.strip_suffix("```").map(str::trim);
    }
    let start = text.find(['{', '['])?;
    // A second structured candidate or markdown fence is ambiguous, not explanatory prose.
    if text[..start].contains(['[', ']', '{', '}', '`', '"'])
        || start > 0 && !text[..start].trim_end().ends_with(':')
    {
        return None;
    }
    let bytes = text.as_bytes();
    let mut depth = 0_usize;
    let mut cursor = start;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => cursor = string_end(bytes, cursor)?,
            b'{' | b'[' => {
                depth += 1;
                if depth > 64 {
                    return None;
                }
            }
            b'}' | b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let end = cursor + 1;
                    if !text[end..].trim().is_empty() {
                        return None;
                    }
                    return Some(&text[start..end]);
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

pub(super) fn normalize(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut cursor = 0;
    let mut previous = None;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte == b'"' {
            let end = string_end(bytes, cursor)?;
            output.extend_from_slice(&bytes[cursor..=end]);
            previous = Some(b'"');
            cursor = end + 1;
            continue;
        }
        if matches!(previous, Some(b'{' | b',')) && (byte.is_ascii_alphabetic() || byte == b'_') {
            let start = cursor;
            while bytes.get(cursor).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_') {
                cursor += 1;
            }
            let next = skip_space(bytes, cursor);
            if bytes.get(next) == Some(&b':') {
                output.push(b'"');
                output.extend_from_slice(&bytes[start..cursor]);
                output.push(b'"');
                previous = Some(b'"');
            } else {
                // Array literals such as true/false/null are values, not unquoted keys.
                output.extend_from_slice(&bytes[start..cursor]);
                previous = bytes.get(cursor - 1).copied();
            }
            continue;
        }
        if byte == b','
            && matches!(bytes.get(skip_space(bytes, cursor + 1)), Some(b'}' | b']'))
            && previous.is_some_and(|b| !matches!(b, b'{' | b'[' | b',' | b':'))
        {
            cursor += 1;
            continue;
        }
        output.push(byte);
        if !byte.is_ascii_whitespace() {
            previous = Some(byte);
        }
        cursor += 1;
    }
    String::from_utf8(output).ok()
}

fn skip_space(bytes: &[u8], mut cursor: usize) -> usize {
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    cursor
}

fn string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => cursor += 2,
            b'"' => return Some(cursor),
            _ => cursor += 1,
        }
    }
    None
}
