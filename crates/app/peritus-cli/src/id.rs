use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::error::CliError;

static NONCE: AtomicU64 = AtomicU64::new(1);

pub fn parse_hex_id(value: &str, name: &str) -> Result<[u8; 16], CliError> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CliError::usage(format!("{name} must be exactly 32 hexadecimal characters")));
    }
    let mut bytes = [0_u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0]);
        let low = hex_digit(pair[1]);
        bytes[index] = (high << 4) | low;
    }
    if bytes == [0; 16] {
        return Err(CliError::usage(format!("{name} cannot be the all-zero identifier")));
    }
    Ok(bytes)
}

pub fn generated_id(domain: &[u8]) -> [u8; 16] {
    let timestamp =
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(1_u128, |duration| duration.as_nanos());
    let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/cli-id/v1\0");
    hasher.update(domain);
    hasher.update(std::process::id().to_be_bytes());
    hasher.update(timestamp.to_be_bytes());
    hasher.update(nonce.to_be_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    if bytes == [0; 16] {
        bytes[15] = 1;
    }
    bytes
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

const fn hex_digit(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}
