//! Authenticated physical pages of exact canonical event envelopes.

use peritus_types::Sha256Digest;

use crate::{
    EventEnvelope, HistoryArchiveIdentity, PhysicalPageCapacity, ProtocolError, ProtocolErrorKind,
    ProtocolLimits,
    archive::{ArchivePageKind, decode_page, encode_page, is_page},
};

use super::{decode_event_envelope, encode_event_envelope};

/// One authenticated physical event-history page.
///
/// Each nested payload is an unchanged canonical `P5EV` envelope. This preserves provider event
/// identities, exact sequence facts, and legacy bytes while the outer page authenticates lineage,
/// order, and its predecessor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventArchivePage {
    identity: HistoryArchiveIdentity,
    page_index: u64,
    first_sequence: u64,
    previous_page_digest: Option<Sha256Digest>,
    envelopes: Vec<EventEnvelope>,
    encoded_envelopes: Vec<Vec<u8>>,
    digest: Sha256Digest,
    encoded_page: Vec<u8>,
}

impl EventArchivePage {
    /// Returns the exact lineage identity authenticated by this page.
    #[must_use]
    pub const fn identity(&self) -> HistoryArchiveIdentity {
        self.identity
    }

    /// Returns the zero-based physical page index.
    #[must_use]
    pub const fn page_index(&self) -> u64 {
        self.page_index
    }

    /// Returns the local sequence of the first envelope.
    #[must_use]
    pub const fn first_sequence(&self) -> u64 {
        self.first_sequence
    }

    /// Returns the authenticated predecessor page digest, absent only on a root page.
    #[must_use]
    pub const fn previous_page_digest(&self) -> Option<Sha256Digest> {
        self.previous_page_digest
    }

    /// Borrows normalized envelopes in exact local sequence order.
    #[must_use]
    pub fn envelopes(&self) -> &[EventEnvelope] {
        &self.envelopes
    }

    /// Borrows the exact canonical `P5EV` bytes for every envelope.
    #[must_use]
    pub fn encoded_envelopes(&self) -> &[Vec<u8>] {
        &self.encoded_envelopes
    }

    /// Returns the digest authenticating the complete page and predecessor binding.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Borrows the complete canonical physical-page bytes.
    #[must_use]
    pub fn encoded_page(&self) -> &[u8] {
        &self.encoded_page
    }

    /// Returns whether this page is the exact contiguous successor of `previous`.
    #[must_use]
    pub fn follows(&self, previous: &Self) -> bool {
        let next_page = previous.page_index.checked_add(1);
        let next_sequence = previous.envelopes.last().and_then(|value| value.sequence().checked_add(1));
        self.identity == previous.identity
            && Some(self.page_index) == next_page
            && Some(self.first_sequence) == next_sequence
            && self.previous_page_digest == Some(previous.digest)
    }

    /// Consumes the page and returns its normalized envelopes.
    #[must_use]
    pub fn into_envelopes(self) -> Vec<EventEnvelope> {
        self.envelopes
    }
}

/// Encodes one nonempty physical event-history page under an explicit transport capacity.
///
/// # Errors
///
/// Rejects empty, noncontiguous, or invalid envelopes and a page that does not fit `capacity`.
pub fn encode_event_archive_page(
    identity: HistoryArchiveIdentity,
    page_index: u64,
    previous_page_digest: Option<Sha256Digest>,
    envelopes: &[EventEnvelope],
    limits: ProtocolLimits,
    capacity: PhysicalPageCapacity,
) -> Result<EventArchivePage, ProtocolError> {
    let first_sequence = validate_sequences(envelopes)?;
    let mut encoded_envelopes = Vec::new();
    encoded_envelopes
        .try_reserve_exact(envelopes.len())
        .map_err(|_| invalid_page("event page allocation is unavailable"))?;
    for envelope in envelopes {
        encoded_envelopes.push(encode_event_envelope(envelope, limits)?);
    }
    let (encoded_page, digest) = encode_page(
        ArchivePageKind::Events,
        identity,
        page_index,
        first_sequence,
        previous_page_digest,
        &encoded_envelopes,
        capacity,
    )?;
    Ok(EventArchivePage {
        identity,
        page_index,
        first_sequence,
        previous_page_digest,
        envelopes: envelopes.to_vec(),
        encoded_envelopes,
        digest,
        encoded_page,
    })
}

/// Decodes and authenticates one physical event-history page.
///
/// # Errors
///
/// Rejects malformed page metadata, a broken predecessor-bound digest, noncanonical legacy event
/// bytes, sequence gaps, invalid event values, or data beyond the selected physical capacity.
pub fn decode_event_archive_page(
    bytes: &[u8],
    limits: ProtocolLimits,
    capacity: PhysicalPageCapacity,
) -> Result<EventArchivePage, ProtocolError> {
    let page = decode_page(bytes, ArchivePageKind::Events, capacity)?;
    let mut envelopes = Vec::new();
    envelopes
        .try_reserve_exact(page.payloads.len())
        .map_err(|_| invalid_page("event page allocation is unavailable"))?;
    for payload in &page.payloads {
        envelopes.push(decode_event_envelope(payload, limits)?);
    }
    let first_sequence = validate_sequences(&envelopes)?;
    if first_sequence != page.first_ordinal {
        return Err(invalid_page("event page first sequence does not match its payload"));
    }
    Ok(EventArchivePage {
        identity: page.identity,
        page_index: page.page_index,
        first_sequence,
        previous_page_digest: page.previous_page_digest,
        envelopes,
        encoded_envelopes: page.payloads,
        digest: page.digest,
        encoded_page: bytes.to_vec(),
    })
}

/// Returns whether bytes begin with the closed authenticated-page magic.
#[must_use]
pub fn is_event_archive_page(bytes: &[u8]) -> bool {
    is_page(bytes)
}

fn validate_sequences(envelopes: &[EventEnvelope]) -> Result<u64, ProtocolError> {
    let Some(first) = envelopes.first() else {
        return Err(invalid_page("event history page is empty"));
    };
    let first_sequence = first.sequence();
    for (offset, envelope) in envelopes.iter().enumerate() {
        let offset = u64::try_from(offset)
            .map_err(|_| invalid_page("event page sequence offset cannot be represented"))?;
        if first_sequence.checked_add(offset) != Some(envelope.sequence()) {
            return Err(invalid_page("event page local sequences are not contiguous"));
        }
    }
    Ok(first_sequence)
}

fn invalid_page(detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidEvent, "event_archive.page", detail)
}
