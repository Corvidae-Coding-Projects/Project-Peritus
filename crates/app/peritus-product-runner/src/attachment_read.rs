//! Exact immutable attachment range request and bounded response.

use peritus_types::Sha256Digest;

/// Exact immutable file-version slice requested by a read-only developer tool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttachmentReadRequest {
    attachment: crate::control::OperationId,
    version: crate::control::OperationId,
    source_digest: Sha256Digest,
    selected_digest: Sha256Digest,
    source_bytes: u64,
    range: (u64, u64),
    offset: u64,
    max_bytes: u32,
}

impl AttachmentReadRequest {
    /// Constructs an exact range read bound to both immutable source and selected-byte digests.
    ///
    /// # Errors
    /// Rejects empty/reversed source ranges, offsets outside the selected bytes, or chunk sizes
    /// outside the bounded read-tool page size.
    #[allow(
        clippy::too_many_arguments,
        reason = "each field is part of the exact immutable source and range binding"
    )]
    pub fn new(
        attachment: crate::control::OperationId,
        version: crate::control::OperationId,
        source_digest: Sha256Digest,
        selected_digest: Sha256Digest,
        source_bytes: u64,
        range: (u64, u64),
        offset: u64,
        max_bytes: u32,
    ) -> Result<Self, String> {
        let empty_source = source_bytes == 0 && range == (0, 0) && offset == 0;
        if (!empty_source && range.0 >= range.1)
            || range.1 > source_bytes
            || offset < range.0
            || (!empty_source && offset >= range.1)
            // Four bytes can always contain one complete UTF-8 scalar value.
            || !(4..=32 * 1024).contains(&max_bytes)
        {
            return Err("invalid immutable attachment range".to_owned());
        }
        Ok(Self {
            attachment,
            version,
            source_digest,
            selected_digest,
            source_bytes,
            range,
            offset,
            max_bytes,
        })
    }

    /// Attachment operation identity.
    #[must_use]
    pub const fn attachment(self) -> crate::control::OperationId {
        self.attachment
    }
    /// Immutable version operation identity.
    #[must_use]
    pub const fn version(self) -> crate::control::OperationId {
        self.version
    }
    /// Digest of the complete source at user-confirmation time.
    #[must_use]
    pub const fn source_digest(self) -> Sha256Digest {
        self.source_digest
    }
    /// Digest of the selected immutable byte range.
    #[must_use]
    pub const fn selected_digest(self) -> Sha256Digest {
        self.selected_digest
    }
    /// Complete source length observed at confirmation.
    #[must_use]
    pub const fn source_bytes(self) -> u64 {
        self.source_bytes
    }
    /// Exact absolute source byte range selected by the user.
    #[must_use]
    pub const fn range(self) -> (u64, u64) {
        self.range
    }
    /// Absolute source byte offset requested for this page.
    #[must_use]
    pub const fn offset(self) -> u64 {
        self.offset
    }
    /// Maximum bytes requested for this page.
    #[must_use]
    pub const fn max_bytes(self) -> u32 {
        self.max_bytes
    }
}

/// Bounded UTF-8 page read from a user-confirmed immutable file version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentReadResponse {
    source_digest: Sha256Digest,
    selected_digest: Sha256Digest,
    source_bytes: u64,
    range: (u64, u64),
    offset: u64,
    next_offset: Option<u64>,
    text: String,
}

impl AttachmentReadResponse {
    /// Builds a response only after the host has revalidated its exact immutable binding.
    ///
    /// # Errors
    /// Rejects empty or oversized pages, ranges past the selected end, and inconsistent
    /// continuation offsets.
    pub fn new(
        request: AttachmentReadRequest,
        text: String,
        next_offset: Option<u64>,
    ) -> Result<Self, String> {
        let end = request
            .offset
            .checked_add(u64::try_from(text.len()).map_err(|_| "attachment page overflow")?)
            .ok_or("attachment page overflow")?;
        let empty_selection = request.range == (0, 0)
            && request.source_bytes == 0
            && request.offset == 0
            && text.is_empty()
            && next_offset.is_none();
        if (text.is_empty() && !empty_selection)
            || text.len() > request.max_bytes as usize
            || end > request.range.1
            || next_offset != (end < request.range.1).then_some(end)
        {
            return Err("invalid immutable attachment page".to_owned());
        }
        Ok(Self {
            source_digest: request.source_digest,
            selected_digest: request.selected_digest,
            source_bytes: request.source_bytes,
            range: request.range,
            offset: request.offset,
            next_offset,
            text,
        })
    }

    /// Digest of the complete source at confirmation time.
    #[must_use]
    pub const fn source_digest(&self) -> Sha256Digest {
        self.source_digest
    }
    /// Digest of the selected immutable range.
    #[must_use]
    pub const fn selected_digest(&self) -> Sha256Digest {
        self.selected_digest
    }
    /// Complete source size.
    #[must_use]
    pub const fn source_bytes(&self) -> u64 {
        self.source_bytes
    }
    /// Exact selected absolute source byte range.
    #[must_use]
    pub const fn range(&self) -> (u64, u64) {
        self.range
    }
    /// Absolute offset of the returned text.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }
    /// Next absolute offset, or `None` when the selected range is complete.
    #[must_use]
    pub const fn next_offset(&self) -> Option<u64> {
        self.next_offset
    }
    /// Borrow the exact returned UTF-8 text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}
