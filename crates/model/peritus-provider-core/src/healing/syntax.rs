//! Conservative byte scanner: strings are opaque and only complete containers are extracted.

use std::borrow::Cow;

pub(super) fn extract(input: &str, allow_plain_prelude: bool) -> Option<Cow<'_, str>> {
    let text = input.trim();
    if let Some(fenced) = text.strip_prefix("```json").or_else(|| text.strip_prefix("```")) {
        return fenced.strip_suffix("```").map(|value| Cow::Borrowed(value.trim()));
    }
    let start = text.find(['{', '['])?;
    let bytes = text.as_bytes();
    let mut depth = 0_usize;
    let mut cursor = start;
    let end = loop {
        let byte = *bytes.get(cursor)?;
        match byte {
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
                    break cursor + 1;
                }
            }
            _ => {}
        }
        cursor += 1;
    };
    let mut prelude = &text[..start];
    let tail = text[end..].trim();
    if !tail.is_empty() {
        // DeepSeek sometimes closes the envelope after `content` and then appends the intended
        // empty tool-call field as if the object were still open. Accept only that exact fragment;
        // the canonical duplicate-key and envelope checks still run on the reconstructed object.
        if allow_plain_prelude
            && tail == ",\"tool_calls\":[]}"
            && text[start..end].ends_with('}')
            && valid_prelude(prelude, allow_plain_prelude, start)
        {
            let mut joined = String::with_capacity(text.len());
            joined.push_str(&text[start..end - 1]);
            joined.push_str(tail);
            return Some(Cow::Owned(joined));
        }
        // One object wrapped in a Markdown fence that follows the prelude is formatting, not a
        // second candidate. The fence must open immediately before the object and close the
        // payload, so the object stays the sole structured candidate.
        if tail != "```" {
            return None;
        }
        prelude = prelude
            .trim_end()
            .strip_suffix("```json")
            .or_else(|| prelude.trim_end().strip_suffix("```"))?;
    }
    if !valid_prelude(prelude, allow_plain_prelude, start) {
        return None;
    }
    Some(Cow::Borrowed(&text[start..end]))
}

fn valid_prelude(prelude: &str, allow_plain_prelude: bool, start: usize) -> bool {
    // A second structured candidate is ambiguous, not explanatory prose. Inline code spans and
    // quoted prose are ordinary text; the private envelope may also carry one final fenced object.
    let separated = prelude.trim_end_matches([' ', '\t']);
    let plain_prelude =
        allow_plain_prelude && (separated.ends_with("\n\n") || separated.ends_with("\r\n\r\n"));
    !prelude.contains(['[', ']', '{', '}'])
        && !prelude.contains("```")
        && (allow_plain_prelude || !prelude.contains(['`', '"']))
        && (start == 0 || prelude.trim_end().ends_with(':') || plain_prelude)
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
            output.push(b'"');
            let mut string = cursor + 1;
            while string < end {
                match bytes[string] {
                    b'\\' => {
                        output.push(b'\\');
                        output.push(*bytes.get(string + 1)?);
                        string += 2;
                    }
                    b'\n' => {
                        output.extend_from_slice(b"\\n");
                        string += 1;
                    }
                    b'\r' => {
                        output.extend_from_slice(b"\\r");
                        string += 1;
                    }
                    b'\t' => {
                        output.extend_from_slice(b"\\t");
                        string += 1;
                    }
                    b'\x08' => {
                        output.extend_from_slice(b"\\b");
                        string += 1;
                    }
                    b'\x0c' => {
                        output.extend_from_slice(b"\\f");
                        string += 1;
                    }
                    control @ 0x00..=0x1f => {
                        const HEX: &[u8; 16] = b"0123456789abcdef";
                        output.extend_from_slice(b"\\u00");
                        output.push(HEX[usize::from(control >> 4)]);
                        output.push(HEX[usize::from(control & 0x0f)]);
                        string += 1;
                    }
                    _ => {
                        output.push(bytes[string]);
                        string += 1;
                    }
                }
            }
            output.push(b'"');
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
