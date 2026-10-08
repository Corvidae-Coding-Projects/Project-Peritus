//! Iterative bounded JSON parsing with duplicate-key detection.

use std::collections::{BTreeMap, btree_map};
use std::slice;

use serde_json::Number;

#[derive(Clone, Copy)]
pub(super) struct ParseLimits {
    pub(super) max_bytes: usize,
    pub(super) max_depth: Option<usize>,
    pub(super) max_members: Option<usize>,
    pub(super) max_string_bytes: usize,
}

pub(super) struct ParsedJson {
    pub(super) canonical: Vec<u8>,
    pub(super) root_is_object: bool,
    pub(super) remote_reference: Option<String>,
}

pub(super) struct ParseError {
    path: String,
    kind: ParseErrorKind,
}

impl ParseError {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) const fn detail(&self) -> &'static str {
        match self.kind {
            ParseErrorKind::InputBytes => "JSON input exceeds its byte bound",
            ParseErrorKind::Syntax => "JSON syntax is malformed",
            ParseErrorKind::Duplicate => "JSON object contains a duplicate key",
            ParseErrorKind::Depth => "JSON depth exceeds its bound",
            ParseErrorKind::MemberOverflow => "JSON member overflow",
            ParseErrorKind::Members => "JSON member count exceeds its bound",
            ParseErrorKind::StringBytes => "JSON string exceeds its byte bound",
            ParseErrorKind::CanonicalBytes => "canonical JSON exceeds its byte bound",
        }
    }

    fn at(path: String, kind: ParseErrorKind) -> Self {
        Self { path, kind }
    }

    fn root(kind: ParseErrorKind) -> Self {
        Self::at("$".to_owned(), kind)
    }
}

#[derive(Clone, Copy)]
enum ParseErrorKind {
    InputBytes,
    Syntax,
    Duplicate,
    Depth,
    MemberOverflow,
    Members,
    StringBytes,
    CanonicalBytes,
}

enum Node {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<usize>),
    Object(BTreeMap<String, usize>),
}

#[derive(Clone)]
enum Segment {
    Root,
    Index(usize),
    Key(String),
}

struct Frame {
    segment: Segment,
    kind: FrameKind,
    after_value: bool,
}

enum FrameKind {
    Array(Vec<usize>),
    Object { values: BTreeMap<String, usize>, pending_key: Option<String> },
}

enum Started {
    Complete(usize),
    Container(Frame),
}

enum Advance {
    Close(Frame),
    Value { frame: Frame, segment: Segment },
}

enum StringError {
    Syntax,
    TooLong,
}

pub(super) fn parse(input: &str, limits: ParseLimits) -> Result<ParsedJson, ParseError> {
    if input.len() > limits.max_bytes {
        return Err(ParseError::root(ParseErrorKind::InputBytes));
    }

    let mut parser = Parser {
        input,
        bytes: input.as_bytes(),
        position: 0,
        limits,
        members: 0,
        nodes: Vec::new(),
        frames: Vec::new(),
        remote_reference: None,
    };
    parser.whitespace();
    let mut root = match parser.start_value(Segment::Root, 1)? {
        Started::Complete(node) => Some(node),
        Started::Container(frame) => {
            parser.frames.push(frame);
            None
        }
    };

    while root.is_none() {
        let frame = parser
            .frames
            .pop()
            .ok_or_else(|| ParseError::root(ParseErrorKind::Syntax))?;
        match parser.advance(frame)? {
            Advance::Close(frame) => {
                let node = parser.close(frame)?;
                if parser.frames.is_empty() {
                    root = Some(node);
                } else {
                    parser.attach(node)?;
                }
            }
            Advance::Value { frame, segment } => {
                let depth = parser.frames.len().saturating_add(2);
                parser.frames.push(frame);
                match parser.start_value(segment, depth)? {
                    Started::Complete(node) => parser.attach(node)?,
                    Started::Container(frame) => parser.frames.push(frame),
                }
            }
        }
    }

    parser.whitespace();
    if parser.position != parser.bytes.len() {
        return Err(ParseError::root(ParseErrorKind::Syntax));
    }
    let root = root.ok_or_else(|| ParseError::root(ParseErrorKind::Syntax))?;
    let root_is_object = matches!(parser.nodes.get(root), Some(Node::Object(_)));
    let canonical = canonicalize(&parser.nodes, root, limits.max_bytes, input.len())?;
    Ok(ParsedJson {
        canonical,
        root_is_object,
        remote_reference: parser.remote_reference,
    })
}

struct Parser<'a> {
    input: &'a str,
    bytes: &'a [u8],
    position: usize,
    limits: ParseLimits,
    members: usize,
    nodes: Vec<Node>,
    frames: Vec<Frame>,
    remote_reference: Option<String>,
}

impl Parser<'_> {
    fn start_value(&mut self, segment: Segment, depth: usize) -> Result<Started, ParseError> {
        if self.limits.max_depth.is_some_and(|maximum| depth > maximum) {
            return Err(ParseError::at(self.location(Some(&segment)), ParseErrorKind::Depth));
        }
        self.whitespace();
        match self.peek() {
            Some(b'{') => {
                self.position += 1;
                Ok(Started::Container(Frame {
                    segment,
                    kind: FrameKind::Object { values: BTreeMap::new(), pending_key: None },
                    after_value: false,
                }))
            }
            Some(b'[') => {
                self.position += 1;
                Ok(Started::Container(Frame {
                    segment,
                    kind: FrameKind::Array(Vec::new()),
                    after_value: false,
                }))
            }
            Some(b'"') => {
                let value = self
                    .string()
                    .map_err(|error| self.string_error(error, Some(&segment)))?;
                Ok(Started::Complete(self.push(Node::String(value))))
            }
            Some(_) => self.scalar().map(Started::Complete),
            None => Err(ParseError::root(ParseErrorKind::Syntax)),
        }
    }

    fn advance(&mut self, mut frame: Frame) -> Result<Advance, ParseError> {
        self.whitespace();
        match &mut frame.kind {
            FrameKind::Array(values) => {
                if frame.after_value {
                    match self.peek() {
                        Some(b']') => {
                            self.position += 1;
                            return Ok(Advance::Close(frame));
                        }
                        Some(b',') => {
                            self.position += 1;
                            self.whitespace();
                        }
                        _ => return Err(ParseError::root(ParseErrorKind::Syntax)),
                    }
                } else if self.peek() == Some(b']') {
                    self.position += 1;
                    return Ok(Advance::Close(frame));
                }

                self.account(&frame.segment)?;
                let segment = Segment::Index(values.len());
                frame.after_value = true;
                Ok(Advance::Value { frame, segment })
            }
            FrameKind::Object { values, pending_key } => {
                if frame.after_value {
                    match self.peek() {
                        Some(b'}') => {
                            self.position += 1;
                            return Ok(Advance::Close(frame));
                        }
                        Some(b',') => {
                            self.position += 1;
                            self.whitespace();
                        }
                        _ => return Err(ParseError::root(ParseErrorKind::Syntax)),
                    }
                } else if self.peek() == Some(b'}') {
                    self.position += 1;
                    return Ok(Advance::Close(frame));
                }

                let key = self
                    .string()
                    .map_err(|error| self.string_error(error, Some(&frame.segment)))?;
                if values.contains_key(&key) {
                    return Err(ParseError::root(ParseErrorKind::Duplicate));
                }
                self.whitespace();
                if self.peek() != Some(b':') {
                    return Err(ParseError::root(ParseErrorKind::Syntax));
                }
                self.position += 1;
                self.whitespace();
                self.account(&frame.segment)?;
                let segment = Segment::Key(key.clone());
                *pending_key = Some(key);
                frame.after_value = true;
                Ok(Advance::Value { frame, segment })
            }
        }
    }

    fn attach(&mut self, node: usize) -> Result<(), ParseError> {
        let mut remote_reference = false;
        {
            let frame = self
                .frames
                .last_mut()
                .ok_or_else(|| ParseError::root(ParseErrorKind::Syntax))?;
            match &mut frame.kind {
                FrameKind::Array(values) => values.push(node),
                FrameKind::Object { values, pending_key } => {
                    let key = pending_key
                        .take()
                        .ok_or_else(|| ParseError::root(ParseErrorKind::Syntax))?;
                    remote_reference = key == "$ref"
                        && self
                            .nodes
                            .get(node)
                            .and_then(|value| match value {
                                Node::String(value) => Some(value.as_str()),
                                _ => None,
                            })
                            .is_some_and(|reference| !reference.starts_with('#'));
                    if values.insert(key, node).is_some() {
                        return Err(ParseError::root(ParseErrorKind::Duplicate));
                    }
                }
            }
        }
        if remote_reference && self.remote_reference.is_none() {
            self.remote_reference = Some(self.location(None));
        }
        Ok(())
    }

    fn close(&mut self, frame: Frame) -> Result<usize, ParseError> {
        match frame.kind {
            FrameKind::Array(values) => Ok(self.push(Node::Array(values))),
            FrameKind::Object { values, pending_key: None } => {
                Ok(self.push(Node::Object(values)))
            }
            FrameKind::Object { pending_key: Some(_), .. } => {
                Err(ParseError::root(ParseErrorKind::Syntax))
            }
        }
    }

    fn scalar(&mut self) -> Result<usize, ParseError> {
        let start = self.position;
        while self.peek().is_some_and(|byte| {
            !matches!(byte, b',' | b']' | b'}' | b' ' | b'\n' | b'\r' | b'\t')
        }) {
            self.position += 1;
        }
        let token = &self.input[start..self.position];
        let node = match token {
            "null" => Node::Null,
            "false" => Node::Bool(false),
            "true" => Node::Bool(true),
            _ => Node::Number(
                serde_json::from_str(token)
                    .map_err(|_| ParseError::root(ParseErrorKind::Syntax))?,
            ),
        };
        Ok(self.push(node))
    }

    fn string(&mut self) -> Result<String, StringError> {
        if self.peek() != Some(b'"') {
            return Err(StringError::Syntax);
        }
        let start = self.position;
        self.position += 1;
        let mut escaped = false;
        while let Some(byte) = self.peek() {
            self.position += 1;
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                let value: String = serde_json::from_str(&self.input[start..self.position])
                    .map_err(|_| StringError::Syntax)?;
                if value.len() > self.limits.max_string_bytes {
                    return Err(StringError::TooLong);
                }
                return Ok(value);
            }
        }
        Err(StringError::Syntax)
    }

    fn string_error(&self, error: StringError, segment: Option<&Segment>) -> ParseError {
        match error {
            StringError::Syntax => ParseError::root(ParseErrorKind::Syntax),
            StringError::TooLong => {
                ParseError::at(self.location(segment), ParseErrorKind::StringBytes)
            }
        }
    }

    fn account(&mut self, current: &Segment) -> Result<(), ParseError> {
        self.members = self.members.checked_add(1).ok_or_else(|| {
            ParseError::at(self.location(Some(current)), ParseErrorKind::MemberOverflow)
        })?;
        if self.limits.max_members.is_some_and(|maximum| self.members > maximum) {
            return Err(ParseError::at(
                self.location(Some(current)),
                ParseErrorKind::Members,
            ));
        }
        Ok(())
    }

    fn push(&mut self, node: Node) -> usize {
        let index = self.nodes.len();
        self.nodes.push(node);
        index
    }

    fn location(&self, trailing: Option<&Segment>) -> String {
        let mut path = "$".to_owned();
        for frame in &self.frames {
            append_segment(&mut path, &frame.segment);
        }
        if let Some(segment) = trailing {
            append_segment(&mut path, segment);
        }
        path
    }

    fn whitespace(&mut self) {
        while self.peek().is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t')) {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }
}

fn append_segment(path: &mut String, segment: &Segment) {
    match segment {
        Segment::Root => {}
        Segment::Index(index) => {
            path.push('/');
            path.push_str(&index.to_string());
        }
        Segment::Key(key) => {
            path.push('/');
            for character in key.chars() {
                match character {
                    '~' => path.push_str("~0"),
                    '/' => path.push_str("~1"),
                    character => path.push(character),
                }
            }
        }
    }
}

enum WriteFrame<'a> {
    Value(usize),
    Array { values: slice::Iter<'a, usize>, first: bool },
    Object { values: btree_map::Iter<'a, String, usize>, first: bool },
}

fn canonicalize(
    nodes: &[Node],
    root: usize,
    max_bytes: usize,
    capacity: usize,
) -> Result<Vec<u8>, ParseError> {
    let mut output = Vec::with_capacity(capacity.min(max_bytes));
    let mut stack = vec![WriteFrame::Value(root)];
    while let Some(frame) = stack.pop() {
        match frame {
            WriteFrame::Value(index) => match nodes
                .get(index)
                .ok_or_else(|| ParseError::root(ParseErrorKind::Syntax))?
            {
                Node::Null => extend(&mut output, b"null", max_bytes)?,
                Node::Bool(false) => extend(&mut output, b"false", max_bytes)?,
                Node::Bool(true) => extend(&mut output, b"true", max_bytes)?,
                Node::Number(value) => {
                    extend(&mut output, value.to_string().as_bytes(), max_bytes)?;
                }
                Node::String(value) => write_string(value, &mut output, max_bytes)?,
                Node::Array(values) => {
                    extend(&mut output, b"[", max_bytes)?;
                    stack.push(WriteFrame::Array { values: values.iter(), first: true });
                }
                Node::Object(values) => {
                    extend(&mut output, b"{", max_bytes)?;
                    stack.push(WriteFrame::Object { values: values.iter(), first: true });
                }
            },
            WriteFrame::Array { mut values, first } => {
                if let Some(value) = values.next() {
                    if !first {
                        extend(&mut output, b",", max_bytes)?;
                    }
                    stack.push(WriteFrame::Array { values, first: false });
                    stack.push(WriteFrame::Value(*value));
                } else {
                    extend(&mut output, b"]", max_bytes)?;
                }
            }
            WriteFrame::Object { mut values, first } => {
                if let Some((key, value)) = values.next() {
                    if !first {
                        extend(&mut output, b",", max_bytes)?;
                    }
                    write_string(key, &mut output, max_bytes)?;
                    extend(&mut output, b":", max_bytes)?;
                    stack.push(WriteFrame::Object { values, first: false });
                    stack.push(WriteFrame::Value(*value));
                } else {
                    extend(&mut output, b"}", max_bytes)?;
                }
            }
        }
    }
    Ok(output)
}

fn write_string(
    value: &str,
    output: &mut Vec<u8>,
    max_bytes: usize,
) -> Result<(), ParseError> {
    let encoded = serde_json::to_string(value)
        .map_err(|_| ParseError::root(ParseErrorKind::Syntax))?;
    extend(output, encoded.as_bytes(), max_bytes)
}

fn extend(output: &mut Vec<u8>, bytes: &[u8], max_bytes: usize) -> Result<(), ParseError> {
    if bytes.len() > max_bytes.saturating_sub(output.len()) {
        return Err(ParseError::root(ParseErrorKind::CanonicalBytes));
    }
    output.extend_from_slice(bytes);
    Ok(())
}
