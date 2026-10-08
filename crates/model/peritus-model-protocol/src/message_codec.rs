//! Profile-independent canonical message archives using the existing C5 content encoding.

use crate::{
    HistoryArchiveIdentity, HistoryArchiveProgress, Message, PhysicalPageCapacity, ProtocolError,
    ProtocolErrorKind, ProtocolLimits,
    archive::{ArchivePageKind, decode_page, encode_page},
};
use peritus_codec::{
    CanonicalReader, CanonicalVerifier, CanonicalWrite, CanonicalWriter, CodecLimits,
};
use peritus_types::Sha256Digest;

const MAGIC: [u8; 4] = *b"P5MS";
const VERSION: u16 = 1;

/// Encodes an ordered message sequence without a provider profile or continuation token.
///
/// This is storage, not authorization to replay its roles or provider-specific content. Hosts
/// must project archived messages through current policy and the receiving protocol.
///
/// # Errors
/// Rejects an unrepresentable collection or an invalid bounded message value. The number of
/// retained messages is deliberately independent of the selected provider request bound.
pub fn encode_messages(
    messages: &[Message],
    limits: ProtocolLimits,
) -> Result<Vec<u8>, ProtocolError> {
    for message in messages {
        message.validate_under(limits)?;
    }
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_fixed(&MAGIC).map_err(codec)?;
    writer.write_u16(VERSION).map_err(codec)?;
    writer.write_collection_len(messages.len()).map_err(codec)?;
    for message in messages {
        crate::canonical::message_value(&mut writer, message)?;
    }
    Ok(writer.into_bytes())
}

/// Decodes a version-one message archive without requiring the original provider profile.
///
/// # Errors
/// Rejects unknown versions, malformed/noncanonical data, invalid content, bounds, or trailing
/// bytes. Tool association and role visibility remain the assembling host's responsibility.
pub fn decode_messages(
    bytes: &[u8],
    limits: ProtocolLimits,
) -> Result<Vec<Message>, ProtocolError> {
    let mut reader = CanonicalReader::new(bytes, decoder_limits(bytes.len()));
    if reader.read_fixed::<4>().map_err(codec)? != MAGIC
        || reader.read_u16().map_err(codec)? != VERSION
    {
        return Err(invalid());
    }
    let messages = crate::canonical_decode::decode_archived_messages(&mut reader, limits)?;
    reader.finish().map_err(codec)?;
    if !messages_match_canonical_bytes(&messages, bytes) {
        return Err(invalid());
    }
    Ok(messages)
}

/// One authenticated physical page whose payload remains an exact legacy `P5MS` archive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageArchivePage {
    identity: HistoryArchiveIdentity,
    page_index: u64,
    first_message: u64,
    previous_page_digest: Option<Sha256Digest>,
    messages: Vec<Message>,
    legacy_archive: Vec<u8>,
    digest: Sha256Digest,
    encoded_page: Vec<u8>,
}

impl MessageArchivePage {
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

    /// Returns the zero-based ordinal of the first retained message on this page.
    #[must_use]
    pub const fn first_message(&self) -> u64 {
        self.first_message
    }

    /// Returns the authenticated predecessor page digest, absent only on a root page.
    #[must_use]
    pub const fn previous_page_digest(&self) -> Option<Sha256Digest> {
        self.previous_page_digest
    }

    /// Borrows messages in their exact retained order.
    #[must_use]
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Borrows the exact canonical version-one payload readable by legacy archive readers.
    #[must_use]
    pub fn legacy_archive(&self) -> &[u8] {
        &self.legacy_archive
    }

    /// Returns the digest authenticating the complete page and its predecessor binding.
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
        let next_message = u64::try_from(previous.messages.len())
            .ok()
            .and_then(|count| previous.first_message.checked_add(count));
        self.identity == previous.identity
            && Some(self.page_index) == next_page
            && Some(self.first_message) == next_message
            && self.previous_page_digest == Some(previous.digest)
    }

    /// Authenticates and advances a resumable page-boundary history checkpoint.
    ///
    /// # Errors
    ///
    /// Rejects a different lineage, ordinal gap, predecessor mismatch, or overflow without
    /// changing `progress`.
    pub fn advance_progress(
        &self,
        progress: &mut HistoryArchiveProgress,
    ) -> Result<(), ProtocolError> {
        progress.advance(
            self.identity,
            self.page_index,
            self.first_message,
            self.previous_page_digest,
            self.digest,
            self.messages.len(),
        )
    }

    /// Consumes the page and returns its retained messages.
    #[must_use]
    pub fn into_messages(self) -> Vec<Message> {
        self.messages
    }
}

/// Encodes one nonempty physical message-history page under an explicit transport capacity.
///
/// The nested payload is the unchanged version-one message archive, so migrations can retain and
/// expose its original bytes. Callers choose page boundaries; no total-history count is imposed.
///
/// # Errors
///
/// Rejects an empty page, ordinal overflow, invalid message content, or a page that does not fit
/// `capacity`.
pub fn encode_message_archive_page(
    identity: HistoryArchiveIdentity,
    page_index: u64,
    first_message: u64,
    previous_page_digest: Option<Sha256Digest>,
    messages: &[Message],
    limits: ProtocolLimits,
    capacity: PhysicalPageCapacity,
) -> Result<MessageArchivePage, ProtocolError> {
    if messages.is_empty() {
        return Err(invalid_page("message history page is empty"));
    }
    let count = u64::try_from(messages.len())
        .map_err(|_| invalid_page("message history count cannot be represented"))?;
    first_message
        .checked_add(count)
        .ok_or_else(|| invalid_page("message history ordinal overflow"))?;
    let legacy_archive = encode_messages(messages, limits)?;
    let payloads = vec![legacy_archive.clone()];
    let (encoded_page, digest) = encode_page(
        ArchivePageKind::Messages,
        identity,
        page_index,
        first_message,
        previous_page_digest,
        &payloads,
        capacity,
    )?;
    Ok(MessageArchivePage {
        identity,
        page_index,
        first_message,
        previous_page_digest,
        messages: messages.to_vec(),
        legacy_archive,
        digest,
        encoded_page,
    })
}

/// Decodes and authenticates one physical message-history page.
///
/// # Errors
///
/// Rejects malformed page metadata, a broken predecessor-bound digest, noncanonical legacy bytes,
/// invalid message values, or data beyond the caller-selected physical capacity.
pub fn decode_message_archive_page(
    bytes: &[u8],
    limits: ProtocolLimits,
    capacity: PhysicalPageCapacity,
) -> Result<MessageArchivePage, ProtocolError> {
    let page = decode_page(bytes, ArchivePageKind::Messages, capacity)?;
    if page.payloads.len() != 1 {
        return Err(invalid_page("message history page must contain one legacy archive"));
    }
    let mut payloads = page.payloads;
    let legacy_archive = payloads
        .pop()
        .ok_or_else(|| invalid_page("message history page is missing its legacy archive"))?;
    let messages = decode_messages(&legacy_archive, limits)?;
    if messages.is_empty() {
        return Err(invalid_page("message history page is empty"));
    }
    let count = u64::try_from(messages.len())
        .map_err(|_| invalid_page("message history count cannot be represented"))?;
    page.first_ordinal
        .checked_add(count)
        .ok_or_else(|| invalid_page("message history ordinal overflow"))?;
    Ok(MessageArchivePage {
        identity: page.identity,
        page_index: page.page_index,
        first_message: page.first_ordinal,
        previous_page_digest: page.previous_page_digest,
        messages,
        legacy_archive,
        digest: page.digest,
        encoded_page: bytes.to_vec(),
    })
}

/// Decodes one exact page and advances `progress` only after complete authentication and parity.
///
/// Owners may stop between calls and retain `progress` as their cooperative cancellation
/// checkpoint; this operation imposes no cumulative page or history ceiling.
///
/// # Errors
///
/// Rejects every [`decode_message_archive_page`] failure or a lineage/ordinal discontinuity while
/// leaving `progress` unchanged.
pub fn decode_next_message_archive_page(
    bytes: &[u8],
    limits: ProtocolLimits,
    capacity: PhysicalPageCapacity,
    progress: &mut HistoryArchiveProgress,
) -> Result<MessageArchivePage, ProtocolError> {
    let page = decode_message_archive_page(bytes, limits, capacity)?;
    page.advance_progress(progress)?;
    Ok(page)
}

fn messages_match_canonical_bytes(messages: &[Message], bytes: &[u8]) -> bool {
    let mut verifier = CanonicalVerifier::new(bytes);
    if verifier.write_fixed(&MAGIC).is_err()
        || verifier.write_u16(VERSION).is_err()
        || verifier.write_collection_len(messages.len()).is_err()
    {
        return false;
    }
    for message in messages {
        if crate::canonical::message_value(&mut verifier, message).is_err() {
            return false;
        }
    }
    verifier.finish().is_ok()
}

const fn decoder_limits(encoded_bytes: usize) -> CodecLimits {
    CodecLimits::new(
        encoded_bytes,
        encoded_bytes,
        usize::MAX,
        encoded_bytes,
        encoded_bytes,
        CodecLimits::UNLIMITED_NESTING,
    )
}
fn codec(_: peritus_codec::CodecError) -> ProtocolError {
    ProtocolError::at(
        ProtocolErrorKind::InvalidContent,
        "message_archive",
        "canonical message bytes are malformed or incomplete",
    )
}
fn invalid() -> ProtocolError {
    ProtocolError::at(
        ProtocolErrorKind::InvalidContent,
        "message_archive",
        "invalid version, bounds, or canonical representation",
    )
}

fn invalid_page(detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidContent, "message_archive.page", detail)
}
