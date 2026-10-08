//! Versioned source-backed obligation pages for conversations larger than one allocation.

use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

const PAGE_DOMAIN: &[u8] = b"peritus-source-obligation-page-v2\0";
const ROOT_DOMAIN: &[u8] = b"peritus-source-obligation-ledger-v2\0";
const OCCURRENCE_DOMAIN: &[u8] = b"peritus-product-obligation-occurrence-v3\0";

/// Stable rejection reason for a malformed source-backed obligation record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceLedgerError {
    /// A zero page identity or empty span was supplied.
    InvalidField,
    /// A fixed-width canonical record had the wrong length or domain.
    InvalidEncoding,
    /// A retained digest did not match the canonical record.
    DigestMismatch,
}

impl core::fmt::Display for SourceLedgerError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "source-backed obligation rejected: {self:?}")
    }
}

impl std::error::Error for SourceLedgerError {}

/// One physical source page represented only by exact immutable provenance.
///
/// Clause bytes stay in the host's durable source store. The obligation ledger retains their
/// digest, exact source span, stable occurrence identity, and page-chain position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceObligationPage {
    page_index: u64,
    source_ordinal: u64,
    source_digest: Sha256Digest,
    conversation_revision: u64,
    byte_start: u64,
    byte_end: u64,
    clause_digest: Sha256Digest,
    requirement_id: RequirementId,
    previous_page_digest: Sha256Digest,
    digest: Sha256Digest,
}

impl SourceObligationPage {
    /// Binds one nonempty exact source slice into the V2 page chain.
    pub fn new(
        page_index: u64,
        source_ordinal: u64,
        source_digest: Sha256Digest,
        conversation_revision: u64,
        byte_start: u64,
        exact: &[u8],
        previous_page_digest: Sha256Digest,
    ) -> Result<Self, SourceLedgerError> {
        let length = u64::try_from(exact.len()).map_err(|_| SourceLedgerError::InvalidField)?;
        let byte_end = byte_start
            .checked_add(length)
            .ok_or(SourceLedgerError::InvalidField)?;
        if exact.is_empty() {
            return Err(SourceLedgerError::InvalidField);
        }
        let clause_digest = digest(exact);
        Self::from_parts(
            page_index,
            source_ordinal,
            source_digest,
            conversation_revision,
            byte_start,
            byte_end,
            clause_digest,
            occurrence_id(
                source_ordinal,
                source_digest,
                conversation_revision,
                byte_start,
                byte_end,
                clause_digest,
            ),
            previous_page_digest,
            None,
        )
    }

    /// Restores and verifies one persisted fixed-width page record.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, SourceLedgerError> {
        const FIELDS: usize = 8 + 8 + 32 + 8 + 8 + 8 + 32 + 32 + 32;
        if bytes.len() != PAGE_DOMAIN.len() + FIELDS || !bytes.starts_with(PAGE_DOMAIN) {
            return Err(SourceLedgerError::InvalidEncoding);
        }
        let mut offset = PAGE_DOMAIN.len();
        let page_index = read_u64(bytes, &mut offset)?;
        let source_ordinal = read_u64(bytes, &mut offset)?;
        let source_digest = read_digest(bytes, &mut offset)?;
        let conversation_revision = read_u64(bytes, &mut offset)?;
        let byte_start = read_u64(bytes, &mut offset)?;
        let byte_end = read_u64(bytes, &mut offset)?;
        let clause_digest = read_digest(bytes, &mut offset)?;
        let requirement_id = RequirementId::new(read_digest(bytes, &mut offset)?);
        let previous_page_digest = read_digest(bytes, &mut offset)?;
        Self::from_parts(
            page_index,
            source_ordinal,
            source_digest,
            conversation_revision,
            byte_start,
            byte_end,
            clause_digest,
            requirement_id,
            previous_page_digest,
            None,
        )
    }

    #[allow(clippy::too_many_arguments, reason = "the persisted page binds every provenance field")]
    fn from_parts(
        page_index: u64,
        source_ordinal: u64,
        source_digest: Sha256Digest,
        conversation_revision: u64,
        byte_start: u64,
        byte_end: u64,
        clause_digest: Sha256Digest,
        requirement_id: RequirementId,
        previous_page_digest: Sha256Digest,
        expected_digest: Option<Sha256Digest>,
    ) -> Result<Self, SourceLedgerError> {
        if page_index == 0
            || source_ordinal == 0
            || byte_start >= byte_end
        {
            return Err(SourceLedgerError::InvalidField);
        }
        let expected_id = occurrence_id(
            source_ordinal,
            source_digest,
            conversation_revision,
            byte_start,
            byte_end,
            clause_digest,
        );
        if requirement_id != expected_id {
            return Err(SourceLedgerError::DigestMismatch);
        }
        let mut page = Self {
            page_index,
            source_ordinal,
            source_digest,
            conversation_revision,
            byte_start,
            byte_end,
            clause_digest,
            requirement_id,
            previous_page_digest,
            digest: Sha256Digest::new([0; 32]),
        };
        page.digest = digest(&page.canonical_bytes());
        if expected_digest.is_some_and(|expected| expected != page.digest) {
            return Err(SourceLedgerError::DigestMismatch);
        }
        Ok(page)
    }

    /// Restores a page plus its separately persisted digest.
    pub fn restore(
        canonical: &[u8],
        expected_digest: Sha256Digest,
    ) -> Result<Self, SourceLedgerError> {
        let page = Self::from_canonical_bytes(canonical)?;
        Self::from_parts(
            page.page_index,
            page.source_ordinal,
            page.source_digest,
            page.conversation_revision,
            page.byte_start,
            page.byte_end,
            page.clause_digest,
            page.requirement_id,
            page.previous_page_digest,
            Some(expected_digest),
        )
    }

    /// Exact canonical bytes. V1 ledger bytes remain unchanged in `canonical::encode`.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(PAGE_DOMAIN.len() + 168);
        bytes.extend_from_slice(PAGE_DOMAIN);
        bytes.extend_from_slice(&self.page_index.to_be_bytes());
        bytes.extend_from_slice(&self.source_ordinal.to_be_bytes());
        bytes.extend_from_slice(self.source_digest.as_bytes());
        bytes.extend_from_slice(&self.conversation_revision.to_be_bytes());
        bytes.extend_from_slice(&self.byte_start.to_be_bytes());
        bytes.extend_from_slice(&self.byte_end.to_be_bytes());
        bytes.extend_from_slice(self.clause_digest.as_bytes());
        bytes.extend_from_slice(self.requirement_id.digest().as_bytes());
        bytes.extend_from_slice(self.previous_page_digest.as_bytes());
        bytes
    }

    #[must_use]
    pub const fn page_index(&self) -> u64 { self.page_index }
    #[must_use]
    pub const fn source_ordinal(&self) -> u64 { self.source_ordinal }
    #[must_use]
    pub const fn source_digest(&self) -> Sha256Digest { self.source_digest }
    #[must_use]
    pub const fn conversation_revision(&self) -> u64 { self.conversation_revision }
    #[must_use]
    pub const fn byte_start(&self) -> u64 { self.byte_start }
    #[must_use]
    pub const fn byte_end(&self) -> u64 { self.byte_end }
    #[must_use]
    pub const fn clause_digest(&self) -> Sha256Digest { self.clause_digest }
    #[must_use]
    pub const fn requirement_id(&self) -> RequirementId { self.requirement_id }
    #[must_use]
    pub const fn previous_page_digest(&self) -> Sha256Digest { self.previous_page_digest }
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }
}

/// Atomically published root of a complete source-backed page chain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceObligationLedgerRoot {
    source_binding: Sha256Digest,
    catalog_binding: Sha256Digest,
    conversation_revision: u64,
    page_count: u64,
    final_page_digest: Sha256Digest,
    digest: Sha256Digest,
}

impl SourceObligationLedgerRoot {
    /// Constructs a complete root after every source page was verified.
    pub fn new(
        source_binding: Sha256Digest,
        catalog_binding: Sha256Digest,
        conversation_revision: u64,
        page_count: u64,
        final_page_digest: Sha256Digest,
    ) -> Result<Self, SourceLedgerError> {
        let empty_digest = Sha256Digest::new([0; 32]);
        if (page_count == 0) != (final_page_digest == empty_digest) {
            return Err(SourceLedgerError::InvalidField);
        }
        let mut value = Self {
            source_binding,
            catalog_binding,
            conversation_revision,
            page_count,
            final_page_digest,
            digest: Sha256Digest::new([0; 32]),
        };
        value.digest = digest(&value.canonical_bytes());
        Ok(value)
    }

    /// Restores a published root and verifies its separately persisted digest.
    pub fn restore(
        canonical: &[u8],
        expected_digest: Sha256Digest,
    ) -> Result<Self, SourceLedgerError> {
        const FIELDS: usize = 32 + 32 + 8 + 8 + 32;
        if canonical.len() != ROOT_DOMAIN.len() + FIELDS || !canonical.starts_with(ROOT_DOMAIN) {
            return Err(SourceLedgerError::InvalidEncoding);
        }
        let mut offset = ROOT_DOMAIN.len();
        let value = Self::new(
            read_digest(canonical, &mut offset)?,
            read_digest(canonical, &mut offset)?,
            read_u64(canonical, &mut offset)?,
            read_u64(canonical, &mut offset)?,
            read_digest(canonical, &mut offset)?,
        )?;
        if value.digest != expected_digest {
            return Err(SourceLedgerError::DigestMismatch);
        }
        Ok(value)
    }

    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(ROOT_DOMAIN.len() + 112);
        bytes.extend_from_slice(ROOT_DOMAIN);
        bytes.extend_from_slice(self.source_binding.as_bytes());
        bytes.extend_from_slice(self.catalog_binding.as_bytes());
        bytes.extend_from_slice(&self.conversation_revision.to_be_bytes());
        bytes.extend_from_slice(&self.page_count.to_be_bytes());
        bytes.extend_from_slice(self.final_page_digest.as_bytes());
        bytes
    }

    #[must_use]
    pub const fn source_binding(&self) -> Sha256Digest { self.source_binding }
    #[must_use]
    pub const fn catalog_binding(&self) -> Sha256Digest { self.catalog_binding }
    #[must_use]
    pub const fn conversation_revision(&self) -> u64 { self.conversation_revision }
    #[must_use]
    pub const fn page_count(&self) -> u64 { self.page_count }
    #[must_use]
    pub const fn final_page_digest(&self) -> Sha256Digest { self.final_page_digest }
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }
}

/// Exact batch qualification of every hard requirement in one published source-backed root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceObligationQualification {
    ledger_digest: Sha256Digest,
    candidate: CandidateIdentity,
    evidence_digest: Sha256Digest,
    qualified: bool,
}

impl SourceObligationQualification {
    /// Binds one current gate-and-review conclusion to the complete immutable page root.
    #[must_use]
    pub const fn direct(
        ledger: SourceObligationLedgerRoot,
        candidate: CandidateIdentity,
        evidence_digest: Sha256Digest,
        satisfied: bool,
    ) -> Self {
        Self {
            ledger_digest: ledger.digest,
            candidate,
            evidence_digest,
            qualified: satisfied,
        }
    }

    #[must_use]
    pub const fn ledger_digest(&self) -> Sha256Digest { self.ledger_digest }
    #[must_use]
    pub const fn candidate(&self) -> CandidateIdentity { self.candidate }
    #[must_use]
    pub const fn evidence_digest(&self) -> Sha256Digest { self.evidence_digest }
    #[must_use]
    pub const fn qualified(&self) -> bool { self.qualified }
}

fn occurrence_id(
    source_ordinal: u64,
    source_digest: Sha256Digest,
    conversation_revision: u64,
    byte_start: u64,
    byte_end: u64,
    clause_digest: Sha256Digest,
) -> RequirementId {
    let mut hasher = Sha256::new();
    hasher.update(OCCURRENCE_DOMAIN);
    hasher.update(source_ordinal.to_be_bytes());
    hasher.update(source_digest.as_bytes());
    hasher.update(conversation_revision.to_be_bytes());
    hasher.update(byte_start.to_be_bytes());
    hasher.update(byte_end.to_be_bytes());
    hasher.update(clause_digest.as_bytes());
    RequirementId::new(Sha256Digest::new(hasher.finalize().into()))
}

fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::new(Sha256::digest(bytes).into())
}

fn read_u64(bytes: &[u8], offset: &mut usize) -> Result<u64, SourceLedgerError> {
    let end = offset.checked_add(8).ok_or(SourceLedgerError::InvalidEncoding)?;
    let exact: [u8; 8] = bytes
        .get(*offset..end)
        .ok_or(SourceLedgerError::InvalidEncoding)?
        .try_into()
        .map_err(|_| SourceLedgerError::InvalidEncoding)?;
    *offset = end;
    Ok(u64::from_be_bytes(exact))
}

fn read_digest(bytes: &[u8], offset: &mut usize) -> Result<Sha256Digest, SourceLedgerError> {
    let end = offset.checked_add(32).ok_or(SourceLedgerError::InvalidEncoding)?;
    let exact: [u8; 32] = bytes
        .get(*offset..end)
        .ok_or(SourceLedgerError::InvalidEncoding)?
        .try_into()
        .map_err(|_| SourceLedgerError::InvalidEncoding)?;
    *offset = end;
    Ok(Sha256Digest::new(exact))
}
