use std::io::{self, BufRead as _, BufReader, Read as _};

use super::super::cancellation::{CancellableReader, GateCancellation};
use super::CsvContract;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Encoding {
    Utf8,
    Latin1,
    Utf16Le,
    Utf16Be,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CsvSummary {
    pub(super) records: usize,
    pub(super) fields: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FieldState {
    Start,
    Unquoted,
    Quoted,
    AfterQuote,
}

struct CsvCharacters<R> {
    reader: BufReader<CancellableReader<R>>,
    encoding: Encoding,
    peeked: Option<char>,
    bom_check: bool,
}

impl<R: io::Read> CsvCharacters<R> {
    fn new(inner: R, encoding: Encoding, cancellation: &GateCancellation) -> Result<Self, String> {
        let mut reader = BufReader::new(cancellation.reader(inner));
        let available = reader.fill_buf().map_err(|error| format!("read file: {error}"))?;
        if available.starts_with(&[0, 0, 0xfe, 0xff]) || available.starts_with(&[0xff, 0xfe, 0, 0])
        {
            return Err("UTF-32 is not a supported declared CSV encoding".to_owned());
        }
        let (declared_bom, width) = if available.starts_with(&[0xef, 0xbb, 0xbf]) {
            (Some(Encoding::Utf8), 3)
        } else if available.starts_with(&[0xff, 0xfe]) {
            (Some(Encoding::Utf16Le), 2)
        } else if available.starts_with(&[0xfe, 0xff]) {
            (Some(Encoding::Utf16Be), 2)
        } else {
            (None, 0)
        };
        if declared_bom.is_some_and(|bom| bom != encoding) {
            return Err("byte-order mark conflicts with the declared encoding".to_owned());
        }
        reader.consume(width);
        Ok(Self { reader, encoding, peeked: None, bom_check: false })
    }

    fn read_byte(&mut self) -> io::Result<Option<u8>> {
        let mut byte = [0];
        match self.reader.read(&mut byte)? {
            0 => Ok(None),
            _ => Ok(Some(byte[0])),
        }
    }

    fn read_utf16_unit(&mut self, first: u8) -> io::Result<u16> {
        let mut pair = [first, 0];
        self.reader.read_exact(&mut pair[1..])?;
        Ok(match self.encoding {
            Encoding::Utf16Le => u16::from_le_bytes(pair),
            Encoding::Utf16Be => u16::from_be_bytes(pair),
            _ => unreachable!("UTF-16 unit read for UTF-16 encoding"),
        })
    }

    fn read_character(&mut self) -> io::Result<Option<char>> {
        let Some(first) = self.read_byte()? else { return Ok(None) };
        let character = match self.encoding {
            Encoding::Utf8 => {
                let width = match first {
                    0x00..=0x7f => 1,
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid UTF-8 lead byte",
                        ));
                    }
                };
                let mut bytes = [0; 4];
                bytes[0] = first;
                self.reader.read_exact(&mut bytes[1..width])?;
                std::str::from_utf8(&bytes[..width])
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
                    .chars()
                    .next()
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "empty UTF-8 character")
                    })?
            }
            Encoding::Latin1 => char::from(first),
            Encoding::Utf16Le | Encoding::Utf16Be => {
                let high = self.read_utf16_unit(first)?;
                let scalar = if (0xd800..=0xdbff).contains(&high) {
                    let low_first = self.read_byte()?.ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "truncated UTF-16 surrogate pair",
                        )
                    })?;
                    let low = self.read_utf16_unit(low_first)?;
                    if !(0xdc00..=0xdfff).contains(&low) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid UTF-16 surrogate pair",
                        ));
                    }
                    0x10000 + (((u32::from(high) - 0xd800) << 10) | (u32::from(low) - 0xdc00))
                } else {
                    u32::from(high)
                };
                char::from_u32(scalar).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid UTF-16 character")
                })?
            }
        };
        if !self.bom_check {
            self.bom_check = true;
            if character == '\u{feff}' {
                return self.read_character();
            }
        }
        Ok(Some(character))
    }

    fn next(&mut self) -> Result<Option<char>, String> {
        if self.peeked.is_some() {
            return Ok(self.peeked.take());
        }
        self.read_character().map_err(|error| format!("decode CSV: {error}"))
    }

    fn peek(&mut self) -> Result<Option<char>, String> {
        if self.peeked.is_none() {
            self.peeked = self.read_character().map_err(|error| format!("decode CSV: {error}"))?;
        }
        Ok(self.peeked)
    }
}

pub(super) struct CsvParser<R> {
    characters: CsvCharacters<R>,
    delimiter: char,
    rectangular: bool,
    state: FieldState,
    record: usize,
    fields: usize,
    first_fields: Option<usize>,
    completed_records: usize,
    record_started: bool,
}

impl<R: io::Read> CsvParser<R> {
    pub(super) fn new(
        reader: R,
        contract: CsvContract,
        cancellation: &GateCancellation,
    ) -> Result<Self, String> {
        Ok(Self {
            characters: CsvCharacters::new(reader, contract.encoding, cancellation)?,
            delimiter: contract.delimiter,
            rectangular: contract.rectangular,
            state: FieldState::Start,
            record: 1,
            fields: 1,
            first_fields: None,
            completed_records: 0,
            record_started: false,
        })
    }

    pub(super) fn parse(mut self) -> Result<CsvSummary, String> {
        while let Some(character) = self.characters.next()? {
            match self.state {
                FieldState::Start => self.consume_start(character)?,
                FieldState::Unquoted => self.consume_unquoted(character)?,
                FieldState::Quoted => self.consume_quoted(character)?,
                FieldState::AfterQuote => self.consume_after_quote(character)?,
            }
        }
        if self.state == FieldState::Quoted {
            return Err(format!("record {} has an unterminated quoted field", self.record));
        }
        if self.record_started {
            self.finish_record()?;
        }
        Ok(CsvSummary { records: self.completed_records, fields: self.first_fields.unwrap_or(0) })
    }

    fn consume_start(&mut self, character: char) -> Result<(), String> {
        match character {
            value if value == self.delimiter => {
                self.fields =
                    self.fields.checked_add(1).ok_or_else(|| "field count overflow".to_owned())?;
                self.record_started = true;
            }
            '"' => {
                self.state = FieldState::Quoted;
                self.record_started = true;
            }
            '\r' | '\n' => self.finish_line(self.record_started)?,
            _ => self.start_unquoted(),
        }
        Ok(())
    }

    const fn start_unquoted(&mut self) {
        self.state = FieldState::Unquoted;
        self.record_started = true;
    }

    fn consume_unquoted(&mut self, character: char) -> Result<(), String> {
        match character {
            value if value == self.delimiter => {
                self.fields =
                    self.fields.checked_add(1).ok_or_else(|| "field count overflow".to_owned())?;
                self.state = FieldState::Start;
            }
            '\r' | '\n' => self.finish_line(true)?,
            '"' => {
                return Err(format!(
                    "record {} contains a quote inside an unquoted field",
                    self.record
                ));
            }
            _ => {}
        }
        Ok(())
    }

    fn consume_quoted(&mut self, character: char) -> Result<(), String> {
        if character == '"' {
            if self.characters.peek()? == Some('"') {
                let _ = self.characters.next()?;
            } else {
                self.state = FieldState::AfterQuote;
            }
        }
        Ok(())
    }

    fn consume_after_quote(&mut self, character: char) -> Result<(), String> {
        match character {
            value if value == self.delimiter => {
                self.fields =
                    self.fields.checked_add(1).ok_or_else(|| "field count overflow".to_owned())?;
                self.state = FieldState::Start;
            }
            '\r' | '\n' => self.finish_line(true)?,
            _ => return Err(format!("record {} contains data after a closing quote", self.record)),
        }
        Ok(())
    }

    fn finish_line(&mut self, has_record: bool) -> Result<(), String> {
        if has_record {
            self.finish_record()?;
        }
        if self.characters.peek()? == Some('\n') {
            let _ = self.characters.next()?;
        }
        self.record =
            self.record.checked_add(1).ok_or_else(|| "record number overflow".to_owned())?;
        self.fields = 1;
        self.state = FieldState::Start;
        self.record_started = false;
        Ok(())
    }

    fn finish_record(&mut self) -> Result<(), String> {
        if self.rectangular {
            if let Some(expected) = self.first_fields {
                if self.fields != expected {
                    return Err(format!(
                        "record {} has {} fields; earlier record has {expected}",
                        self.record, self.fields
                    ));
                }
            } else {
                self.first_fields = Some(self.fields);
            }
        } else if self.first_fields.is_none() {
            self.first_fields = Some(self.fields);
        }
        self.completed_records = self
            .completed_records
            .checked_add(1)
            .ok_or_else(|| "record count overflow".to_owned())?;
        Ok(())
    }
}
