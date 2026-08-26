//! Exact registered B3 event frames retained for lossless redelivery.

use peritus_codec::{CodecLimits, decode_frame, sha256};
use peritus_protocol::schema::{FAMILIES, MessageRole};
use peritus_types::Sha256Digest;

use super::{SubscriptionError, SubscriptionErrorKind, error::reject};

/// A checked complete B3 event frame retained without reserialization.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RegisteredEventFrame {
    bytes: Vec<u8>,
    family: u16,
    schema_version: u16,
    digest: Sha256Digest,
}

impl RegisteredEventFrame {
    /// Validates and owns one complete canonical frame whose B3 registry role is `Event`.
    ///
    /// # Errors
    ///
    /// Returns a typed input error for malformed framing, an unregistered family/version, or a
    /// registered family whose semantic role is not `Event`.
    pub fn new(bytes: Vec<u8>, limits: CodecLimits) -> Result<Self, SubscriptionError> {
        let decoded = decode_frame(&bytes, limits).map_err(|_| {
            reject(
                SubscriptionErrorKind::InvalidInput,
                "event bytes are not one complete canonical B3 frame",
            )
        })?;
        let header = decoded.header();
        let registered =
            FAMILIES.iter().find(|family| family.tag == header.family()).ok_or_else(|| {
                reject(
                    SubscriptionErrorKind::InvalidInput,
                    "event frame family is not registered by B3",
                )
            })?;
        if registered.schema_version != header.schema_version()
            || registered.role() != MessageRole::Event
        {
            return Err(reject(
                SubscriptionErrorKind::InvalidInput,
                "frame is not a current registered B3 event",
            ));
        }
        let digest = sha256(&bytes);
        Ok(Self { bytes, family: header.family(), schema_version: header.schema_version(), digest })
    }

    /// Borrows the exact complete frame bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the checked B3 family tag.
    #[must_use]
    pub const fn family(&self) -> u16 {
        self.family
    }

    /// Returns the checked B3 schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    /// Returns SHA-256 over the exact complete frame bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}
