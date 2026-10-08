//! Authenticated physical pages for retained C5 history.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::{ProtocolError, ProtocolErrorKind};

pub(crate) const PAGE_MAGIC: [u8; 4] = *b"P5PG";
pub(crate) const PAGE_SCHEMA_VERSION: u16 = 1;
const PAGE_DIGEST_DOMAIN: &[u8] = b"peritus-c5-history-page-v1\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArchivePageKind {
    Messages,
    Events,
}

impl ArchivePageKind {
    const fn tag(self) -> u8 {
        match self {
            Self::Messages => 1,
            Self::Events => 2,
        }
    }
}

/// Exact thread, provider-profile, and run binding carried by every retained-history page.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HistoryArchiveIdentity {
    thread_id: [u8; 16],
    profile_id: [u8; 16],
    run_id: [u8; 16],
}

impl HistoryArchiveIdentity {
    /// Creates a nonzero identity binding for one history lineage.
    ///
    /// # Errors
    ///
    /// Rejects a reserved all-zero component.
    pub fn new(
        thread_id: [u8; 16],
        profile_id: [u8; 16],
        run_id: [u8; 16],
    ) -> Result<Self, ProtocolError> {
        if [thread_id, profile_id, run_id]
            .iter()
            .any(|value| value.iter().all(|byte| *byte == 0))
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidIdentity,
                "history_archive.identity",
                "thread, profile, and run identities must be nonzero",
            ));
        }
        Ok(Self { thread_id, profile_id, run_id })
    }

    /// Returns the exact thread identity bytes.
    #[must_use]
    pub const fn thread_id(self) -> [u8; 16] {
        self.thread_id
    }

    /// Returns the exact provider-profile identity bytes.
    #[must_use]
    pub const fn profile_id(self) -> [u8; 16] {
        self.profile_id
    }

    /// Returns the exact run identity bytes.
    #[must_use]
    pub const fn run_id(self) -> [u8; 16] {
        self.run_id
    }
}

/// Caller-selected capacity for one physical history page.
///
/// This bounds one transport or storage operation. It is never a cumulative history allowance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalPageCapacity {
    max_encoded_bytes: usize,
}

impl PhysicalPageCapacity {
    /// Creates a positive physical-page capacity without applying a compiled production ceiling.
    ///
    /// # Errors
    ///
    /// Rejects zero, which cannot carry a page.
    pub fn new(max_encoded_bytes: usize) -> Result<Self, ProtocolError> {
        if max_encoded_bytes == 0 {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidLimit,
                "history_archive.page_capacity",
                "physical page capacity must be positive",
            ));
        }
        Ok(Self { max_encoded_bytes })
    }

    /// Returns the maximum bytes admitted for one encoded physical page.
    #[must_use]
    pub const fn max_encoded_bytes(self) -> usize {
        self.max_encoded_bytes
    }
}

pub(crate) struct DecodedArchivePage {
    pub(crate) identity: HistoryArchiveIdentity,
    pub(crate) page_index: u64,
    pub(crate) first_ordinal: u64,
    pub(crate) previous_page_digest: Option<Sha256Digest>,
    pub(crate) payloads: Vec<Vec<u8>>,
    pub(crate) digest: Sha256Digest,
}

pub(crate) fn encode_page(
    kind: ArchivePageKind,
    identity: HistoryArchiveIdentity,
    page_index: u64,
    first_ordinal: u64,
    previous_page_digest: Option<Sha256Digest>,
    payloads: &[Vec<u8>],
    capacity: PhysicalPageCapacity,
) -> Result<(Vec<u8>, Sha256Digest), ProtocolError> {
    validate_position(page_index, previous_page_digest, payloads)?;
    let mut writer = CanonicalWriter::new(page_codec_limits(capacity));
    writer.write_fixed(&PAGE_MAGIC).map_err(page_capacity)?;
    writer.write_u16(PAGE_SCHEMA_VERSION).map_err(page_capacity)?;
    writer.write_u8(kind.tag()).map_err(page_capacity)?;
    writer.write_fixed(&identity.thread_id).map_err(page_capacity)?;
    writer.write_fixed(&identity.profile_id).map_err(page_capacity)?;
    writer.write_fixed(&identity.run_id).map_err(page_capacity)?;
    writer.write_u64(page_index).map_err(page_capacity)?;
    writer.write_u64(first_ordinal).map_err(page_capacity)?;
    writer.write_option_tag(previous_page_digest.is_some()).map_err(page_capacity)?;
    if let Some(previous) = previous_page_digest {
        writer.write_fixed(previous.as_bytes()).map_err(page_capacity)?;
    }
    writer.write_collection_len(payloads.len()).map_err(page_capacity)?;
    for payload in payloads {
        if payload.is_empty() {
            return Err(invalid_page("history page contains an empty legacy payload"));
        }
        writer.write_bytes(payload).map_err(page_capacity)?;
    }
    let digest = page_digest(writer.as_slice());
    writer.write_fixed(digest.as_bytes()).map_err(page_capacity)?;
    Ok((writer.into_bytes(), digest))
}

pub(crate) fn decode_page(
    bytes: &[u8],
    expected_kind: ArchivePageKind,
    capacity: PhysicalPageCapacity,
) -> Result<DecodedArchivePage, ProtocolError> {
    if bytes.len() > capacity.max_encoded_bytes {
        return Err(page_capacity_error());
    }
    let mut reader = CanonicalReader::new(bytes, page_codec_limits(capacity));
    if reader.read_fixed::<4>().map_err(malformed_page)? != PAGE_MAGIC {
        return Err(invalid_page("history page magic is invalid"));
    }
    if reader.read_u16().map_err(malformed_page)? != PAGE_SCHEMA_VERSION {
        return Err(ProtocolError::at(
            ProtocolErrorKind::UnsupportedVersion,
            "history_archive.schema_version",
            "history page schema version is unsupported",
        ));
    }
    if reader.read_u8().map_err(malformed_page)? != expected_kind.tag() {
        return Err(invalid_page("history page kind is unknown or does not match"));
    }
    let identity = HistoryArchiveIdentity::new(
        reader.read_fixed::<16>().map_err(malformed_page)?,
        reader.read_fixed::<16>().map_err(malformed_page)?,
        reader.read_fixed::<16>().map_err(malformed_page)?,
    )?;
    let page_index = reader.read_u64().map_err(malformed_page)?;
    let first_ordinal = reader.read_u64().map_err(malformed_page)?;
    let previous_page_digest = if reader.read_option_tag().map_err(malformed_page)? {
        Some(Sha256Digest::new(reader.read_fixed::<32>().map_err(malformed_page)?))
    } else {
        None
    };
    let count = reader.read_collection_len(4).map_err(malformed_page)?;
    let mut payloads = reader.reserve_collection(count).map_err(malformed_page)?;
    for _ in 0..count {
        let payload = reader.read_bytes_owned().map_err(malformed_page)?;
        if payload.is_empty() {
            return Err(invalid_page("history page contains an empty legacy payload"));
        }
        payloads.push(payload);
    }
    let digest = Sha256Digest::new(reader.read_fixed::<32>().map_err(malformed_page)?);
    reader.finish().map_err(malformed_page)?;
    validate_position(page_index, previous_page_digest, &payloads)?;
    let digest_offset = bytes
        .len()
        .checked_sub(32)
        .ok_or_else(|| invalid_page("history page is shorter than its digest"))?;
    if page_digest(&bytes[..digest_offset]) != digest {
        return Err(invalid_page("history page digest does not authenticate its bytes"));
    }
    let (canonical, canonical_digest) = encode_page(
        expected_kind,
        identity,
        page_index,
        first_ordinal,
        previous_page_digest,
        &payloads,
        capacity,
    )?;
    if canonical_digest != digest || canonical.as_slice() != bytes {
        return Err(invalid_page("history page is not in canonical form"));
    }
    Ok(DecodedArchivePage {
        identity,
        page_index,
        first_ordinal,
        previous_page_digest,
        payloads,
        digest,
    })
}

#[must_use]
pub(crate) fn is_page(bytes: &[u8]) -> bool {
    bytes.starts_with(&PAGE_MAGIC)
}

fn validate_position(
    page_index: u64,
    previous_page_digest: Option<Sha256Digest>,
    payloads: &[Vec<u8>],
) -> Result<(), ProtocolError> {
    if payloads.is_empty() {
        return Err(invalid_page("history page must contain at least one legacy payload"));
    }
    if (page_index == 0) != previous_page_digest.is_none() {
        return Err(invalid_page(
            "history page index and predecessor presence disagree",
        ));
    }
    Ok(())
}

fn page_digest(bytes: &[u8]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(PAGE_DIGEST_DOMAIN);
    hasher.update(bytes);
    Sha256Digest::new(hasher.finalize().into())
}

const fn page_codec_limits(capacity: PhysicalPageCapacity) -> CodecLimits {
    CodecLimits::new(
        capacity.max_encoded_bytes,
        capacity.max_encoded_bytes,
        usize::MAX,
        capacity.max_encoded_bytes,
        capacity.max_encoded_bytes,
        CodecLimits::UNLIMITED_NESTING,
    )
}

fn page_capacity(_: peritus_codec::CodecError) -> ProtocolError {
    page_capacity_error()
}

fn page_capacity_error() -> ProtocolError {
    ProtocolError::at(
        ProtocolErrorKind::InvalidLimit,
        "history_archive.page_capacity",
        "canonical history page exceeds the caller-selected physical capacity",
    )
}

fn malformed_page(_: peritus_codec::CodecError) -> ProtocolError {
    invalid_page("canonical history page is malformed or incomplete")
}

fn invalid_page(detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidContent, "history_archive.page", detail)
}
