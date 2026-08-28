//! Stable opaque text form for exact platform credential references.

use peritus_sandbox::SecretReference;
use peritus_types::{ResourceId, Sha256Digest};

const PREFIX: &str = "peritus-secret-v1:";

/// Formats an exact reference without resolving or exposing credential material.
#[must_use]
pub fn format_credential_reference(reference: SecretReference) -> String {
    format!(
        "{PREFIX}{}:{}",
        hex(reference.resource_id().as_bytes()),
        hex(reference.version().as_bytes())
    )
}

/// Parses the stable opaque reference form.
///
/// # Errors
///
/// Rejects an unsupported scheme, malformed hex, wrong length, or zero resource identity.
pub fn parse_credential_reference(value: &str) -> Result<SecretReference, crate::SecretError> {
    let body = value
        .strip_prefix(PREFIX)
        .ok_or_else(|| crate::error::invalid("credential reference scheme is unsupported"))?;
    let (resource, version) = body
        .split_once(':')
        .ok_or_else(|| crate::error::invalid("credential reference is malformed"))?;
    if version.contains(':') {
        return Err(crate::error::invalid("credential reference is malformed"));
    }
    let resource = ResourceId::new(decode::<16>(resource)?)
        .map_err(|_| crate::error::invalid("credential resource identity is zero"))?;
    Ok(SecretReference::new(resource, Sha256Digest::new(decode::<32>(version)?)))
}

fn decode<const N: usize>(value: &str) -> Result<[u8; N], crate::SecretError> {
    if value.len() != N * 2 {
        return Err(crate::error::invalid("credential reference hex length is invalid"));
    }
    let mut bytes = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = nibble(pair[0])
            .ok_or_else(|| crate::error::invalid("credential reference is not lowercase hex"))?;
        let low = nibble(pair[1])
            .ok_or_else(|| crate::error::invalid("credential reference is not lowercase hex"))?;
        bytes[index] = (high << 4) | low;
    }
    Ok(bytes)
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    value
}

const fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_reference_round_trips() {
        let reference = SecretReference::new(
            ResourceId::new([7; 16]).expect("resource"),
            Sha256Digest::new([9; 32]),
        );
        assert_eq!(
            parse_credential_reference(&format_credential_reference(reference)).expect("parse"),
            reference
        );
    }
}
