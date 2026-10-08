//! Small authenticated owner protocol; each request has an independent TCP connection.

use crate::error::{Result, problem};
use std::io::{ErrorKind, Read, Write};

pub(super) const MAGIC: &[u8; 8] = b"PRTSPTY1";
pub(super) const TOKEN_BYTES: usize = 32;
pub(super) const ACK: u8 = 1;
pub(super) const UNKNOWN: u8 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Operation {
    Input = 1,
    Resize = 2,
    Terminate = 3,
    Interrupt = 4,
}

impl Operation {
    pub(super) fn decode(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::Input),
            2 => Ok(Self::Resize),
            3 => Ok(Self::Terminate),
            4 => Ok(Self::Interrupt),
            _ => Err(problem("Unknown console owner operation")),
        }
    }
}

pub(super) fn header(operation: Operation, token: &str) -> Result<Vec<u8>> {
    if token.len() != TOKEN_BYTES || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(problem("Console owner token is invalid"));
    }
    let mut bytes = Vec::with_capacity(MAGIC.len() + 1 + TOKEN_BYTES);
    bytes.extend_from_slice(MAGIC);
    bytes.push(operation as u8);
    bytes.extend_from_slice(token.as_bytes());
    Ok(bytes)
}

pub(super) fn input_header(header: &[u8], input: &str, digest: &str) -> Result<Vec<u8>> {
    if !super::receipt::valid_input(input) || !super::receipt::valid_digest(digest) {
        return Err(problem("Console input identity or digest is invalid"));
    }
    let mut bytes = Vec::with_capacity(header.len() + 32 + 64);
    bytes.extend_from_slice(header);
    bytes.extend_from_slice(input.as_bytes());
    bytes.extend_from_slice(digest.as_bytes());
    Ok(bytes)
}

pub(super) fn read_input_identity(
    reader: &mut impl Read,
    cancelled: impl Fn() -> bool,
) -> Result<(String, String)> {
    let mut bytes = [0_u8; 32 + 64];
    read_fully(reader, &mut bytes, cancelled)?;
    let input = std::str::from_utf8(&bytes[..32]).map_err(problem)?.to_owned();
    let digest = std::str::from_utf8(&bytes[32..]).map_err(problem)?.to_owned();
    if !super::receipt::valid_input(&input) || !super::receipt::valid_digest(&digest) {
        return Err(problem("Console input identity or digest is invalid"));
    }
    Ok((input, digest))
}

pub(super) fn read_header(
    reader: &mut impl Read,
    expected_token: &str,
    cancelled: impl Fn() -> bool,
) -> Result<Operation> {
    let mut header = [0_u8; MAGIC.len() + 1 + TOKEN_BYTES];
    read_fully(reader, &mut header, cancelled)?;
    if &header[..MAGIC.len()] != MAGIC
        || &header[MAGIC.len() + 1..] != expected_token.as_bytes()
    {
        return Err(problem("Console owner authentication failed"));
    }
    Operation::decode(header[MAGIC.len()])
}

pub(super) fn read_fully(
    reader: &mut impl Read,
    bytes: &mut [u8],
    cancelled: impl Fn() -> bool,
) -> Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        match reader.read(&mut bytes[offset..]) {
            Ok(0) => return Err(problem("Console owner request ended before its frame was complete")),
            Ok(count) => offset += count,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if cancelled() {
                    return Err(problem("Console ended while a control frame was in flight"));
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(super) fn respond(stream: &mut impl Write, status: u8) -> Result<()> {
    stream.write_all(&[status])?;
    stream.flush()?;
    Ok(())
}
