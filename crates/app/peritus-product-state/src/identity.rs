//! Stable non-secret installation identity.

use serde::Deserialize;
use serde::Serialize;

use crate::ProductStateError;

/// Non-secret identities generated once for a local Peritus installation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallIdentity {
    store_id: String,
    actor_id: String,
}

impl InstallIdentity {
    /// Creates an identity from exact nonzero 128-bit values.
    ///
    /// # Errors
    ///
    /// Returns [`ProductStateError::InvalidIdentity`] if either value is zero.
    pub fn new(store_id: [u8; 16], actor_id: [u8; 16]) -> Result<Self, ProductStateError> {
        if store_id == [0; 16] {
            return Err(ProductStateError::InvalidIdentity("store identity"));
        }
        if actor_id == [0; 16] {
            return Err(ProductStateError::InvalidIdentity("actor identity"));
        }
        Ok(Self { store_id: encode_hex(store_id), actor_id: encode_hex(actor_id) })
    }

    /// Parses the durable lowercase hexadecimal representation.
    ///
    /// # Errors
    ///
    /// Returns [`ProductStateError::InvalidIdentity`] for malformed or zero values.
    pub fn parse(store_id: &str, actor_id: &str) -> Result<Self, ProductStateError> {
        let store = decode_hex(store_id, "store identity")?;
        let actor = decode_hex(actor_id, "actor identity")?;
        Self::new(store, actor)
    }

    /// Borrows the stable daemon store identity.
    #[must_use]
    pub fn store_id(&self) -> &str {
        &self.store_id
    }

    /// Borrows the stable local human actor identity.
    #[must_use]
    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }

    pub(crate) fn validate(&self) -> Result<(), ProductStateError> {
        Self::parse(&self.store_id, &self.actor_id).map(|_| ())
    }
}

fn encode_hex(bytes: [u8; 16]) -> String {
    let mut encoded = String::with_capacity(32);
    for byte in bytes {
        use core::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing into String cannot fail");
    }
    encoded
}

fn decode_hex(value: &str, field: &'static str) -> Result<[u8; 16], ProductStateError> {
    if value.len() != 32
        || !value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(ProductStateError::InvalidIdentity(field));
    }
    let mut decoded = [0_u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        decoded[index] = nibble(pair[0]) << 4 | nibble(pair[1]);
    }
    if decoded == [0; 16] {
        return Err(ProductStateError::InvalidIdentity(field));
    }
    Ok(decoded)
}

const fn nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => 0,
    }
}
