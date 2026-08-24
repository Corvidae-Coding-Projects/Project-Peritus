//! Recursive duplicate-key detection before map materialization.

use std::collections::BTreeSet;

pub(super) fn contains(input: &str) -> bool {
    let mut cursor = Cursor { input: input.as_bytes(), position: 0, duplicate: false };
    cursor.value();
    cursor.duplicate
}

struct Cursor<'a> {
    input: &'a [u8],
    position: usize,
    duplicate: bool,
}

impl Cursor<'_> {
    fn value(&mut self) {
        self.whitespace();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => {
                let _ = self.string_end();
            }
            Some(_) => self.scalar(),
            None => {}
        }
        self.whitespace();
    }

    fn object(&mut self) {
        self.position += 1;
        self.whitespace();
        let mut keys = BTreeSet::new();
        while self.peek().is_some() && self.peek() != Some(b'}') {
            let start = self.position;
            let Some(end) = self.string_end() else {
                return;
            };
            let key = std::str::from_utf8(&self.input[start..end])
                .ok()
                .and_then(|encoded| serde_json::from_str::<String>(encoded).ok());
            if key.is_some_and(|key| !keys.insert(key)) {
                self.duplicate = true;
            }
            self.whitespace();
            if self.peek() == Some(b':') {
                self.position += 1;
            }
            self.value();
            if self.peek() == Some(b',') {
                self.position += 1;
                self.whitespace();
            } else {
                break;
            }
        }
        if self.peek() == Some(b'}') {
            self.position += 1;
        }
    }

    fn array(&mut self) {
        self.position += 1;
        self.whitespace();
        while self.peek().is_some() && self.peek() != Some(b']') {
            self.value();
            if self.peek() == Some(b',') {
                self.position += 1;
                self.whitespace();
            } else {
                break;
            }
        }
        if self.peek() == Some(b']') {
            self.position += 1;
        }
    }

    fn string_end(&mut self) -> Option<usize> {
        if self.peek() != Some(b'"') {
            return None;
        }
        self.position += 1;
        let mut escaped = false;
        while let Some(byte) = self.peek() {
            self.position += 1;
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                return Some(self.position);
            }
        }
        None
    }

    fn scalar(&mut self) {
        while let Some(byte) = self.peek() {
            if matches!(byte, b',' | b']' | b'}' | b' ' | b'\n' | b'\r' | b'\t') {
                break;
            }
            self.position += 1;
        }
    }

    fn whitespace(&mut self) {
        while self.peek().is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t')) {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }
}
