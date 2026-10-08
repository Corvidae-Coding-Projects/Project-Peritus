//! Single-pass local proposal decoding with disk-backed unbounded path tokens.

use super::{Kind, Operation, Status, Update, Validity};
use peritus_context::working::WorkingLimits;
use sha2::{Digest as _, Sha256};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
};

const FIELD_BYTES: usize = 32;
const ENUM_BYTES: usize = 32;
// The public schema admits at most 100 Unicode scalar values per source handle. Four UTF-8
// bytes per scalar is therefore the exact largest decoded allocation needed before `sequence`
// applies the narrower current-scope handle grammar.
const SOURCE_CHARACTERS: usize = 100;
const SOURCE_BYTES: usize = SOURCE_CHARACTERS * 4;
const EXPLICIT_ID_BYTES: usize = 38;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DecodeError {
    Invalid,
    Read,
    Authenticate,
    Backing,
}

type Result<T> = std::result::Result<T, DecodeError>;

pub(super) fn decode(reader: impl Read, limits: WorkingLimits) -> Result<Update> {
    Decoder::new(reader, limits)?.decode()
}

struct Decoder<R> {
    input: Input<R>,
    arena: Arena,
    limits: WorkingLimits,
}

impl<R: Read> Decoder<R> {
    fn new(reader: R, limits: WorkingLimits) -> Result<Self> {
        Ok(Self {
            input: Input::new(reader),
            arena: Arena::new()?,
            limits,
        })
    }

    fn decode(mut self) -> Result<Update> {
        self.input.expect(b'{')?;
        let mut base_revision = None;
        let mut operations = None;
        if !self.input.take_if(b'}')? {
            loop {
                let field = self.input.bounded_string(FIELD_BYTES, None)?;
                self.input.expect(b':')?;
                match field.as_str() {
                    "base_revision" => {
                        if base_revision.is_some() {
                            return Err(DecodeError::Invalid);
                        }
                        base_revision = Some(self.input.unsigned()?);
                    }
                    "operations" => {
                        if operations.is_some() {
                            return Err(DecodeError::Invalid);
                        }
                        operations = Some(self.operations()?);
                    }
                    _ => return Err(DecodeError::Invalid),
                }
                if self.input.take_if(b'}')? {
                    break;
                }
                self.input.expect(b',')?;
            }
        }
        let base_revision = base_revision.ok_or(DecodeError::Invalid)?;
        let decoded = operations.ok_or(DecodeError::Invalid)?;
        if decoded.is_empty() {
            return Err(DecodeError::Invalid);
        }
        // No unbounded path token reaches owned state until the complete JSON document and the
        // captured proposal's digest/size have both reached authenticated EOF.
        self.input.authenticate_end()?;
        let mut arena = self.arena.finish()?;
        let operations = decoded
            .into_iter()
            .map(|operation| operation.materialize(&mut arena))
            .collect::<Result<Vec<_>>>()?;
        Ok(Update { base_revision, operations })
    }

    fn operations(&mut self) -> Result<Vec<DecodedOperation>> {
        self.input.expect(b'[')?;
        let mut operations = Vec::new();
        if self.input.take_if(b']')? {
            return Ok(operations);
        }
        loop {
            operations.push(self.operation()?);
            if self.input.take_if(b']')? {
                return Ok(operations);
            }
            self.input.expect(b',')?;
        }
    }

    fn operation(&mut self) -> Result<DecodedOperation> {
        self.input.expect(b'{')?;
        let mut id = None;
        let mut kind = None;
        let mut text = None;
        let mut supports = None;
        let mut contradicts = None;
        let mut depends_on = None;
        let mut status = None;
        let mut validity = None;
        let mut files = None;
        let mut supersedes_seen = false;
        let mut supersedes = None;
        if !self.input.take_if(b'}')? {
            loop {
                let field = self.input.bounded_string(FIELD_BYTES, None)?;
                self.input.expect(b':')?;
                match field.as_str() {
                    "id" => {
                        require_absent(&id)?;
                        id = Some(self.input.label()?);
                    }
                    "kind" => {
                        require_absent(&kind)?;
                        kind = Some(self.kind()?);
                    }
                    "text" => {
                        require_absent(&text)?;
                        let value = self
                            .input
                            .bounded_string(self.limits.entry_bytes(), None)?;
                        if value.is_empty() {
                            return Err(DecodeError::Invalid);
                        }
                        text = Some(value);
                    }
                    "supports" => {
                        require_absent(&supports)?;
                        supports = Some(self.source_list()?);
                    }
                    "contradicts" => {
                        require_absent(&contradicts)?;
                        contradicts = Some(self.source_list()?);
                    }
                    "depends_on" => {
                        require_absent(&depends_on)?;
                        depends_on = Some(self.label_list()?);
                    }
                    "status" => {
                        require_absent(&status)?;
                        status = Some(self.status()?);
                    }
                    "validity" => {
                        require_absent(&validity)?;
                        validity = Some(self.validity()?);
                    }
                    "files" => {
                        require_absent(&files)?;
                        files = Some(self.file_list()?);
                    }
                    "supersedes" => {
                        if supersedes_seen {
                            return Err(DecodeError::Invalid);
                        }
                        supersedes_seen = true;
                        supersedes = if self.input.next_is(b'n')? {
                            self.input.null()?;
                            None
                        } else {
                            Some(self.input.label()?)
                        };
                    }
                    _ => return Err(DecodeError::Invalid),
                }
                if self.input.take_if(b'}')? {
                    break;
                }
                self.input.expect(b',')?;
            }
        }
        Ok(DecodedOperation {
            id: id.ok_or(DecodeError::Invalid)?,
            kind: kind.ok_or(DecodeError::Invalid)?,
            text: text.ok_or(DecodeError::Invalid)?,
            supports: supports.ok_or(DecodeError::Invalid)?,
            contradicts: contradicts.ok_or(DecodeError::Invalid)?,
            depends_on: depends_on.ok_or(DecodeError::Invalid)?,
            status: status.ok_or(DecodeError::Invalid)?,
            validity: validity.ok_or(DecodeError::Invalid)?,
            files: files.ok_or(DecodeError::Invalid)?,
            supersedes,
        })
    }

    fn kind(&mut self) -> Result<Kind> {
        match self.input.bounded_string(ENUM_BYTES, None)?.as_str() {
            "observation" => Ok(Kind::Observation),
            "assertion" => Ok(Kind::Assertion),
            "hypothesis" => Ok(Kind::Hypothesis),
            "decision" => Ok(Kind::Decision),
            "failed_approach" => Ok(Kind::FailedApproach),
            "plan" => Ok(Kind::Plan),
            "glossary" => Ok(Kind::Glossary),
            _ => Err(DecodeError::Invalid),
        }
    }

    fn status(&mut self) -> Result<Status> {
        match self.input.bounded_string(ENUM_BYTES, None)?.as_str() {
            "open" => Ok(Status::Open),
            "contradicted" => Ok(Status::Contradicted),
            "resolved" => Ok(Status::Resolved),
            _ => Err(DecodeError::Invalid),
        }
    }

    fn validity(&mut self) -> Result<Validity> {
        match self.input.bounded_string(ENUM_BYTES, None)?.as_str() {
            "candidate" => Ok(Validity::Candidate),
            "files" => Ok(Validity::Files),
            "conversation" => Ok(Validity::Conversation),
            "task" => Ok(Validity::Task),
            _ => Err(DecodeError::Invalid),
        }
    }

    fn source_list(&mut self) -> Result<Vec<String>> {
        self.input.expect(b'[')?;
        let mut values = Vec::new();
        if self.input.take_if(b']')? {
            return Ok(values);
        }
        loop {
            values.push(
                self.input
                    .bounded_string(SOURCE_BYTES, Some(SOURCE_CHARACTERS))?,
            );
            if self.input.take_if(b']')? {
                return Ok(values);
            }
            self.input.expect(b',')?;
        }
    }

    fn label_list(&mut self) -> Result<Vec<String>> {
        self.input.expect(b'[')?;
        let mut values = Vec::new();
        if self.input.take_if(b']')? {
            return Ok(values);
        }
        loop {
            values.push(self.input.label()?);
            if self.input.take_if(b']')? {
                return Ok(values);
            }
            self.input.expect(b',')?;
        }
    }

    fn file_list(&mut self) -> Result<Vec<BackedString>> {
        self.input.expect(b'[')?;
        let mut values = Vec::new();
        if self.input.take_if(b']')? {
            return Ok(values);
        }
        loop {
            let value = self.arena.string(&mut self.input)?;
            if value.bytes == 0 {
                return Err(DecodeError::Invalid);
            }
            values.push(value);
            if self.input.take_if(b']')? {
                return Ok(values);
            }
            self.input.expect(b',')?;
        }
    }
}

fn require_absent<T>(slot: &Option<T>) -> Result<()> {
    if slot.is_some() {
        return Err(DecodeError::Invalid);
    }
    Ok(())
}

struct DecodedOperation {
    id: String,
    kind: Kind,
    text: String,
    supports: Vec<String>,
    contradicts: Vec<String>,
    depends_on: Vec<String>,
    status: Status,
    validity: Validity,
    files: Vec<BackedString>,
    supersedes: Option<String>,
}

impl DecodedOperation {
    fn materialize(self, arena: &mut File) -> Result<Operation> {
        let files = self
            .files
            .into_iter()
            .map(|field| field.materialize(arena))
            .collect::<Result<Vec<_>>>()?;
        Ok(Operation {
            id: self.id,
            kind: self.kind,
            text: self.text,
            supports: self.supports,
            contradicts: self.contradicts,
            depends_on: self.depends_on,
            status: self.status,
            validity: self.validity,
            files,
            supersedes: self.supersedes,
        })
    }
}

#[derive(Clone, Copy)]
struct BackedString {
    offset: u64,
    bytes: u64,
}

impl BackedString {
    fn materialize(self, arena: &mut File) -> Result<String> {
        arena.seek(SeekFrom::Start(self.offset)).map_err(|_| DecodeError::Backing)?;
        let capacity = usize::try_from(self.bytes).map_err(|_| DecodeError::Backing)?;
        let mut value = String::new();
        // Accepted WorkspacePath values intentionally have no application byte ceiling. Keep
        // allocation failure recoverable without turning the old 4 KiB legacy classifier into a
        // new admission quota. This is an accepted-state allocation, not a proved RSS window.
        value.try_reserve_exact(capacity).map_err(|_| DecodeError::Backing)?;
        let mut selected = arena.take(self.bytes);
        selected.read_to_string(&mut value).map_err(|_| DecodeError::Backing)?;
        if value.len() != capacity {
            return Err(DecodeError::Backing);
        }
        Ok(value)
    }
}

struct Arena {
    file: BufWriter<File>,
    bytes: u64,
}

impl Arena {
    fn new() -> Result<Self> {
        let file = tempfile::tempfile().map_err(|_| DecodeError::Backing)?;
        Ok(Self { file: BufWriter::new(file), bytes: 0 })
    }

    fn string<R: Read>(&mut self, input: &mut Input<R>) -> Result<BackedString> {
        let offset = self.bytes;
        let mut sink = ArenaSink { arena: self };
        input.string(&mut sink)?;
        Ok(BackedString { offset, bytes: sink.arena.bytes - offset })
    }

    fn finish(mut self) -> Result<File> {
        self.file.flush().map_err(|_| DecodeError::Backing)?;
        self.file.into_inner().map_err(|_| DecodeError::Backing)
    }
}

trait StringSink {
    fn push(&mut self, character: char, encoded: &[u8]) -> Result<()>;
}

struct ArenaSink<'a> {
    arena: &'a mut Arena,
}

impl StringSink for ArenaSink<'_> {
    fn push(&mut self, _character: char, encoded: &[u8]) -> Result<()> {
        self.arena.file.write_all(encoded).map_err(|_| DecodeError::Backing)?;
        self.arena.bytes = self
            .arena
            .bytes
            .checked_add(u64::try_from(encoded.len()).map_err(|_| DecodeError::Backing)?)
            .ok_or(DecodeError::Backing)?;
        Ok(())
    }
}

struct BoundedSink {
    value: String,
    maximum_bytes: usize,
    maximum_characters: Option<usize>,
    characters: usize,
}

impl BoundedSink {
    fn new(maximum_bytes: usize, maximum_characters: Option<usize>) -> Self {
        Self {
            value: String::with_capacity(maximum_bytes),
            maximum_bytes,
            maximum_characters,
            characters: 0,
        }
    }
}

impl StringSink for BoundedSink {
    fn push(&mut self, character: char, encoded: &[u8]) -> Result<()> {
        let bytes = self
            .value
            .len()
            .checked_add(encoded.len())
            .ok_or(DecodeError::Invalid)?;
        let characters = self.characters.checked_add(1).ok_or(DecodeError::Invalid)?;
        if bytes > self.maximum_bytes
            || self
                .maximum_characters
                .is_some_and(|maximum| characters > maximum)
        {
            return Err(DecodeError::Invalid);
        }
        self.value.push(character);
        self.characters = characters;
        Ok(())
    }
}

struct LabelSink {
    hash: Sha256,
    prefix: [u8; EXPLICIT_ID_BYTES],
    prefix_bytes: usize,
    bytes: u64,
}

impl LabelSink {
    fn new() -> Self {
        let mut hash = Sha256::new();
        hash.update(b"agent-entry:");
        Self { hash, prefix: [0; EXPLICIT_ID_BYTES], prefix_bytes: 0, bytes: 0 }
    }

    fn finish(self) -> Result<String> {
        if self.bytes == 0 {
            return Err(DecodeError::Invalid);
        }
        if self.prefix_bytes >= b"entry:".len()
            && &self.prefix[..b"entry:".len()] == b"entry:"
        {
            if self.bytes != 38
                || !self.prefix[b"entry:".len()..]
                    .iter()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
                || self.prefix[b"entry:".len()..]
                    .iter()
                    .all(|byte| *byte == b'0')
            {
                return Err(DecodeError::Invalid);
            }
            return String::from_utf8(self.prefix.to_vec()).map_err(|_| DecodeError::Invalid);
        }
        let digest: [u8; 32] = self.hash.finalize().into();
        let mut id = [0_u8; 16];
        id.copy_from_slice(&digest[..16]);
        id[0] |= 1;
        Ok(format!("entry:{}", hexadecimal(&id)))
    }
}

impl StringSink for LabelSink {
    fn push(&mut self, character: char, encoded: &[u8]) -> Result<()> {
        if character.is_control() {
            return Err(DecodeError::Invalid);
        }
        let remaining = EXPLICIT_ID_BYTES.saturating_sub(self.prefix_bytes);
        let copied = remaining.min(encoded.len());
        self.prefix[self.prefix_bytes..self.prefix_bytes + copied]
            .copy_from_slice(&encoded[..copied]);
        self.prefix_bytes += copied;
        self.hash.update(encoded);
        self.bytes = self
            .bytes
            .checked_add(u64::try_from(encoded.len()).map_err(|_| DecodeError::Invalid)?)
            .ok_or(DecodeError::Invalid)?;
        Ok(())
    }
}

fn hexadecimal(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

struct Input<R> {
    reader: BufReader<R>,
    lookahead: Option<u8>,
}

impl<R: Read> Input<R> {
    fn new(reader: R) -> Self {
        Self { reader: BufReader::new(reader), lookahead: None }
    }

    fn next_raw(&mut self) -> Result<Option<u8>> {
        if self.lookahead.is_some() {
            return Ok(self.lookahead.take());
        }
        let mut byte = [0_u8; 1];
        match self.reader.read(&mut byte) {
            Ok(0) => Ok(None),
            Ok(1) => Ok(Some(byte[0])),
            Ok(_) => unreachable!("one-byte read returned more than one byte"),
            Err(_) => Err(DecodeError::Read),
        }
    }

    fn peek_raw(&mut self) -> Result<Option<u8>> {
        if self.lookahead.is_none() {
            self.lookahead = self.next_raw()?;
        }
        Ok(self.lookahead)
    }

    fn skip_space(&mut self) -> Result<()> {
        while self.peek_raw()?.is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t')) {
            self.lookahead = None;
        }
        Ok(())
    }

    fn next(&mut self) -> Result<Option<u8>> {
        self.skip_space()?;
        self.next_raw()
    }

    fn expect(&mut self, expected: u8) -> Result<()> {
        if self.next()? != Some(expected) {
            return Err(DecodeError::Invalid);
        }
        Ok(())
    }

    fn take_if(&mut self, expected: u8) -> Result<bool> {
        self.skip_space()?;
        if self.peek_raw()? == Some(expected) {
            self.lookahead = None;
            return Ok(true);
        }
        Ok(false)
    }

    fn next_is(&mut self, expected: u8) -> Result<bool> {
        self.skip_space()?;
        Ok(self.peek_raw()? == Some(expected))
    }

    fn authenticate_end(&mut self) -> Result<()> {
        match self.skip_space().and_then(|()| self.next_raw()) {
            Ok(None) => Ok(()),
            Ok(Some(_)) => Err(DecodeError::Invalid),
            Err(DecodeError::Read) => Err(DecodeError::Authenticate),
            Err(error) => Err(error),
        }
    }

    fn unsigned(&mut self) -> Result<u64> {
        let first = self.next()?.ok_or(DecodeError::Invalid)?;
        let mut value = match first {
            b'0' => 0,
            b'1'..=b'9' => u64::from(first - b'0'),
            _ => return Err(DecodeError::Invalid),
        };
        if first == b'0' && self.peek_raw()?.is_some_and(|byte| byte.is_ascii_digit()) {
            return Err(DecodeError::Invalid);
        }
        while let Some(byte @ b'0'..=b'9') = self.peek_raw()? {
            self.lookahead = None;
            value = value
                .checked_mul(10)
                .and_then(|value| value.checked_add(u64::from(byte - b'0')))
                .ok_or(DecodeError::Invalid)?;
        }
        if self.peek_raw()?.is_some_and(|byte| matches!(byte, b'.' | b'e' | b'E')) {
            return Err(DecodeError::Invalid);
        }
        Ok(value)
    }

    fn null(&mut self) -> Result<()> {
        for expected in b"null" {
            if self.next_raw()? != Some(*expected) {
                return Err(DecodeError::Invalid);
            }
        }
        Ok(())
    }

    fn bounded_string(
        &mut self,
        maximum_bytes: usize,
        maximum_characters: Option<usize>,
    ) -> Result<String> {
        let mut sink = BoundedSink::new(maximum_bytes, maximum_characters);
        self.string(&mut sink)?;
        Ok(sink.value)
    }

    fn label(&mut self) -> Result<String> {
        let mut sink = LabelSink::new();
        self.string(&mut sink)?;
        sink.finish()
    }

    fn string(&mut self, sink: &mut impl StringSink) -> Result<()> {
        self.expect(b'"')?;
        loop {
            let byte = self.next_raw()?.ok_or(DecodeError::Invalid)?;
            match byte {
                b'"' => return Ok(()),
                b'\\' => self.escape(sink)?,
                0x00..=0x1f => return Err(DecodeError::Invalid),
                0x20..=0x7f => {
                    let encoded = [byte];
                    sink.push(char::from(byte), &encoded)?;
                }
                _ => {
                    let (character, encoded, bytes) = self.utf8(byte)?;
                    sink.push(character, &encoded[..bytes])?;
                }
            }
        }
    }

    fn escape(&mut self, sink: &mut impl StringSink) -> Result<()> {
        let escaped = self.next_raw()?.ok_or(DecodeError::Invalid)?;
        let character = match escaped {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{0008}',
            b'f' => '\u{000c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => self.unicode_escape()?,
            _ => return Err(DecodeError::Invalid),
        };
        let mut encoded = [0_u8; 4];
        let encoded = character.encode_utf8(&mut encoded).as_bytes();
        sink.push(character, encoded)
    }

    fn unicode_escape(&mut self) -> Result<char> {
        let high = self.hex_quad()?;
        let scalar = if (0xd800..=0xdbff).contains(&high) {
            if self.next_raw()? != Some(b'\\') || self.next_raw()? != Some(b'u') {
                return Err(DecodeError::Invalid);
            }
            let low = self.hex_quad()?;
            if !(0xdc00..=0xdfff).contains(&low) {
                return Err(DecodeError::Invalid);
            }
            0x1_0000 + ((u32::from(high) - 0xd800) << 10) + (u32::from(low) - 0xdc00)
        } else if (0xdc00..=0xdfff).contains(&high) {
            return Err(DecodeError::Invalid);
        } else {
            u32::from(high)
        };
        char::from_u32(scalar).ok_or(DecodeError::Invalid)
    }

    fn hex_quad(&mut self) -> Result<u16> {
        let mut value = 0_u16;
        for _ in 0..4 {
            let byte = self.next_raw()?.ok_or(DecodeError::Invalid)?;
            let digit = match byte {
                b'0'..=b'9' => u16::from(byte - b'0'),
                b'a'..=b'f' => u16::from(byte - b'a') + 10,
                b'A'..=b'F' => u16::from(byte - b'A') + 10,
                _ => return Err(DecodeError::Invalid),
            };
            value = (value << 4) | digit;
        }
        Ok(value)
    }

    fn utf8(&mut self, first: u8) -> Result<(char, [u8; 4], usize)> {
        let width = match first {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => return Err(DecodeError::Invalid),
        };
        let mut encoded = [0_u8; 4];
        encoded[0] = first;
        for byte in &mut encoded[1..width] {
            *byte = self.next_raw()?.ok_or(DecodeError::Invalid)?;
            if !matches!(*byte, 0x80..=0xbf) {
                return Err(DecodeError::Invalid);
            }
        }
        let text = std::str::from_utf8(&encoded[..width]).map_err(|_| DecodeError::Invalid)?;
        let mut characters = text.chars();
        let character = characters.next().ok_or(DecodeError::Invalid)?;
        if characters.next().is_some() || character.len_utf8() != width {
            return Err(DecodeError::Invalid);
        }
        Ok((character, encoded, width))
    }
}
