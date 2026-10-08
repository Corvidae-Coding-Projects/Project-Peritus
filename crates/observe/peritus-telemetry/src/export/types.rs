//! Export identities, canonical batches, progress, cancellation, and adapter ownership.

use core::fmt;
use std::num::NonZeroUsize;

use peritus_types::Sha256Digest;

use super::encoding::batch_digest;
use crate::{
    MetricPoint, OtelEvent, OtelSpan, TelemetryError, TelemetryErrorKind,
    buffer::{BufferedRecord, DispositionPrefix},
};

const OWNERSHIP_PREFIX: &[u8] = b"PERITUS-C7-PENDING-EXPORT-V1\0";

/// Nonzero 16-byte logical export-stream identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExportStreamId([u8; 16]);

impl ExportStreamId {
    /// Creates an export-stream identity.
    ///
    /// # Errors
    ///
    /// Rejects the all-zero representation.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, TelemetryError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(TelemetryError::new(
            TelemetryErrorKind::InvalidConfiguration,
            "validate export stream",
            "all-zero export stream identity is reserved",
        ))
    }

    /// Borrows exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Redaction-safe export value accepted from a telemetry projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExportRecord {
    /// Completed OpenTelemetry-compatible span.
    Span(OtelSpan),
    /// Content-free OpenTelemetry-compatible event.
    Event(OtelEvent),
    /// Stable monotonic metric point.
    Metric(MetricPoint),
}

impl ExportRecord {
    pub(crate) fn canonical_bytes(&self) -> Result<Vec<u8>, TelemetryError> {
        super::encoding::encode_record(self)
    }
}

/// One stable-sequence canonical record inside an export batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportItem {
    pub(super) sequence: u64,
    pub(super) canonical: Box<[u8]>,
    prefix_digest: Sha256Digest,
    accepted_total: u64,
    gap_before: Option<DispositionPrefix>,
}

impl ExportItem {
    /// Returns stable submitted sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Borrows the exact redaction-safe canonical record bytes accepted by the buffer.
    #[must_use]
    pub fn canonical_record(&self) -> &[u8] {
        &self.canonical
    }

    fn into_buffered(self) -> BufferedRecord {
        BufferedRecord {
            sequence: self.sequence,
            canonical: self.canonical,
            prefix_digest: self.prefix_digest,
            accepted_total: self.accepted_total,
            gap_before: self.gap_before,
            resident_bytes: 0,
        }
    }
}

/// Immutable idempotent export batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportBatch {
    stream_id: ExportStreamId,
    batch_id: Sha256Digest,
    first_sequence: u64,
    last_sequence: u64,
    items: Vec<ExportItem>,
}

impl ExportBatch {
    pub(crate) fn from_buffered(
        stream_id: ExportStreamId,
        records: Vec<BufferedRecord>,
    ) -> Result<Self, TelemetryError> {
        let items = records
            .into_iter()
            .map(|record| ExportItem {
                sequence: record.sequence,
                canonical: record.canonical,
                prefix_digest: record.prefix_digest,
                accepted_total: record.accepted_total,
                gap_before: record.gap_before,
            })
            .collect::<Vec<_>>();
        let first_sequence = items.first().map_or(0, ExportItem::sequence);
        let last_sequence = items.last().map_or(0, ExportItem::sequence);
        let batch_id = batch_digest(stream_id, &items)?;
        Ok(Self { stream_id, batch_id, first_sequence, last_sequence, items })
    }

    /// Returns the export-stream identity.
    #[must_use]
    pub const fn stream_id(&self) -> ExportStreamId {
        self.stream_id
    }
    /// Returns deterministic batch identity.
    #[must_use]
    pub const fn batch_id(&self) -> Sha256Digest {
        self.batch_id
    }
    /// Returns first included stable sequence.
    #[must_use]
    pub const fn first_sequence(&self) -> u64 {
        self.first_sequence
    }
    /// Returns last included stable sequence.
    #[must_use]
    pub const fn last_sequence(&self) -> u64 {
        self.last_sequence
    }
    /// Returns included record count.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.items.len()
    }
    /// Returns whether the batch is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    /// Borrows sequenced canonical records.
    #[must_use]
    pub fn items(&self) -> &[ExportItem] {
        &self.items
    }

    pub(crate) fn buffered_records(&self) -> Vec<BufferedRecord> {
        self.items.clone().into_iter().map(ExportItem::into_buffered).collect()
    }

    pub(crate) fn ownership_bytes(&self) -> Result<Vec<u8>, TelemetryError> {
        let mut bytes = OWNERSHIP_PREFIX.to_vec();
        bytes.extend_from_slice(self.stream_id.as_bytes());
        bytes.extend_from_slice(self.batch_id.as_bytes());
        bytes.extend_from_slice(&self.first_sequence.to_be_bytes());
        bytes.extend_from_slice(&self.last_sequence.to_be_bytes());
        let count = u64::try_from(self.items.len()).map_err(|_| ownership_error(
            "pending export item count is not representable",
        ))?;
        bytes.extend_from_slice(&count.to_be_bytes());
        for item in &self.items {
            bytes.extend_from_slice(&item.sequence.to_be_bytes());
            bytes.extend_from_slice(item.prefix_digest.as_bytes());
            bytes.extend_from_slice(&item.accepted_total.to_be_bytes());
            match item.gap_before {
                Some(gap) => {
                    bytes.push(1);
                    bytes.extend_from_slice(&gap.sequence.to_be_bytes());
                    bytes.extend_from_slice(gap.prefix_digest.as_bytes());
                    bytes.extend_from_slice(&gap.accepted_total.to_be_bytes());
                }
                None => bytes.push(0),
            }
            let length = u64::try_from(item.canonical.len()).map_err(|_| ownership_error(
                "pending export canonical length is not representable",
            ))?;
            bytes.extend_from_slice(&length.to_be_bytes());
            bytes.extend_from_slice(&item.canonical);
        }
        let checksum = peritus_codec::sha256(&bytes);
        bytes.extend_from_slice(checksum.as_bytes());
        Ok(bytes)
    }

    pub(crate) fn from_ownership_bytes(bytes: &[u8]) -> Result<Self, TelemetryError> {
        let minimum = OWNERSHIP_PREFIX
            .len()
            .checked_add(16 + 32 + 8 + 8 + 8 + Sha256Digest::LENGTH)
            .ok_or_else(|| ownership_error("pending export fixed length overflows"))?;
        if bytes.len() < minimum || !bytes.starts_with(OWNERSHIP_PREFIX) {
            return Err(ownership_error("pending export marker or length is invalid"));
        }
        let checksum_start = bytes.len() - Sha256Digest::LENGTH;
        let checksum = Sha256Digest::new(
            bytes[checksum_start..]
                .try_into()
                .map_err(|_| ownership_error("pending export checksum length is invalid"))?,
        );
        if peritus_codec::sha256(&bytes[..checksum_start]) != checksum {
            return Err(ownership_error("pending export checksum does not match"));
        }
        let mut offset = OWNERSHIP_PREFIX.len();
        let stream_id = ExportStreamId::new(take_ownership::<16>(bytes, &mut offset)?)?;
        let stored_batch_id = Sha256Digest::new(take_ownership::<32>(bytes, &mut offset)?);
        let stored_first = u64::from_be_bytes(take_ownership::<8>(bytes, &mut offset)?);
        let stored_last = u64::from_be_bytes(take_ownership::<8>(bytes, &mut offset)?);
        let count = usize::try_from(u64::from_be_bytes(take_ownership::<8>(bytes, &mut offset)?))
            .map_err(|_| ownership_error("pending export item count is not representable"))?;
        let mut records = Vec::new();
        for _ in 0..count {
            let sequence = u64::from_be_bytes(take_ownership::<8>(bytes, &mut offset)?);
            let prefix_digest = Sha256Digest::new(take_ownership::<32>(bytes, &mut offset)?);
            let accepted_total = u64::from_be_bytes(take_ownership::<8>(bytes, &mut offset)?);
            let gap_before = match take_ownership::<1>(bytes, &mut offset)?[0] {
                0 => None,
                1 => Some(DispositionPrefix {
                    sequence: u64::from_be_bytes(take_ownership::<8>(bytes, &mut offset)?),
                    prefix_digest: Sha256Digest::new(take_ownership::<32>(bytes, &mut offset)?),
                    accepted_total: u64::from_be_bytes(take_ownership::<8>(
                        bytes,
                        &mut offset,
                    )?),
                }),
                _ => return Err(ownership_error("pending export gap tag is invalid")),
            };
            let length = usize::try_from(u64::from_be_bytes(take_ownership::<8>(
                bytes,
                &mut offset,
            )?))
            .map_err(|_| ownership_error("pending export canonical length is not representable"))?;
            let end = offset
                .checked_add(length)
                .ok_or_else(|| ownership_error("pending export canonical offset overflows"))?;
            if end > checksum_start {
                return Err(ownership_error("pending export canonical bytes are truncated"));
            }
            let canonical = bytes[offset..end].to_vec().into_boxed_slice();
            offset = end;
            records.push(BufferedRecord {
                sequence,
                canonical,
                prefix_digest,
                accepted_total,
                gap_before,
                resident_bytes: 0,
            });
        }
        if offset != checksum_start || records.is_empty() {
            return Err(ownership_error(
                "pending export item count does not consume its exact bytes",
            ));
        }
        let batch = Self::from_buffered(stream_id, records)?;
        if batch.batch_id != stored_batch_id
            || batch.first_sequence != stored_first
            || batch.last_sequence != stored_last
            || batch.items.len() != count
        {
            return Err(ownership_error(
                "pending export identity does not match its canonical records",
            ));
        }
        Ok(batch)
    }

    /// Encodes one complete redaction-safe batch for durable local-file export.
    ///
    /// Existing accepted canonical record bytes are copied verbatim; they are never decoded or
    /// projected again while under export custody.
    ///
    /// # Errors
    ///
    /// Returns a telemetry serialization failure if a collection length is unrepresentable.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, TelemetryError> {
        let mut bytes = b"PERITUS-C7-LOCAL-BATCH-V1\0".to_vec();
        bytes.extend_from_slice(self.stream_id.as_bytes());
        bytes.extend_from_slice(self.batch_id.as_bytes());
        bytes.extend_from_slice(&self.first_sequence.to_be_bytes());
        bytes.extend_from_slice(&self.last_sequence.to_be_bytes());
        let count = u64::try_from(self.items.len()).map_err(|_| {
            TelemetryError::new(
                TelemetryErrorKind::SequenceOverflow,
                "encode local telemetry batch",
                "batch item count is unrepresentable",
            )
        })?;
        bytes.extend_from_slice(&count.to_be_bytes());
        for item in &self.items {
            let length = u64::try_from(item.canonical.len()).map_err(|_| {
                TelemetryError::new(
                    TelemetryErrorKind::SequenceOverflow,
                    "encode local telemetry batch",
                    "record length is unrepresentable",
                )
            })?;
            bytes.extend_from_slice(&item.sequence.to_be_bytes());
            bytes.extend_from_slice(&length.to_be_bytes());
            bytes.extend_from_slice(&item.canonical);
        }
        Ok(bytes)
    }
}

fn take_ownership<const N: usize>(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<[u8; N], TelemetryError> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| ownership_error("pending export field offset overflows"))?;
    let value = bytes
        .get(*offset..end)
        .ok_or_else(|| ownership_error("pending export is truncated"))?
        .try_into()
        .map_err(|_| ownership_error("pending export field length is invalid"))?;
    *offset = end;
    Ok(value)
}

const fn ownership_error(detail: &'static str) -> TelemetryError {
    TelemetryError::new(
        TelemetryErrorKind::InvalidCheckpoint,
        "validate pending telemetry export",
        detail,
    )
}

/// Exact whole-batch exporter acknowledgement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExportAck {
    stream_id: ExportStreamId,
    batch_id: Sha256Digest,
    first_sequence: u64,
    last_sequence: u64,
    count: usize,
}

impl ExportAck {
    /// Creates an acknowledgement from an adapter response.
    #[must_use]
    pub const fn new(
        stream_id: ExportStreamId,
        batch_id: Sha256Digest,
        first_sequence: u64,
        last_sequence: u64,
        count: usize,
    ) -> Self {
        Self { stream_id, batch_id, first_sequence, last_sequence, count }
    }

    /// Creates an exact acknowledgement of the supplied immutable batch.
    #[must_use]
    pub const fn accept(batch: &ExportBatch) -> Self {
        Self::new(
            batch.stream_id,
            batch.batch_id,
            batch.first_sequence,
            batch.last_sequence,
            batch.items.len(),
        )
    }

    pub(super) fn matches(self, batch: &ExportBatch) -> bool {
        self.stream_id == batch.stream_id
            && self.batch_id == batch.batch_id
            && self.first_sequence == batch.first_sequence
            && self.last_sequence == batch.last_sequence
            && self.count == batch.items.len()
    }
}

/// Stable phase reported by an exporter-owned operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExportPhase {
    /// Immutable input was accepted and the operation owns no externally visible result yet.
    Prepared,
    /// Canonical bytes are being transferred to exporter-owned storage or transport.
    Writing,
    /// Acceptance is being durably committed or verified.
    Committing,
    /// Exporter-owned temporary resources are being removed.
    Cleaning,
    /// Acceptance or cleanup has completed truthfully.
    Complete,
}

/// Content-free progress for one exporter-owned operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExportProgress {
    phase: ExportPhase,
    completed_bytes: u64,
    total_bytes: Option<u64>,
}

impl ExportProgress {
    /// Creates a progress snapshot with an optional exact byte total.
    #[must_use]
    pub const fn new(
        phase: ExportPhase,
        completed_bytes: u64,
        total_bytes: Option<u64>,
    ) -> Self {
        Self { phase, completed_bytes, total_bytes }
    }
    /// Returns the current stable operation phase.
    #[must_use]
    pub const fn phase(self) -> ExportPhase {
        self.phase
    }
    /// Returns bytes durably or transport-locally completed so far.
    #[must_use]
    pub const fn completed_bytes(self) -> u64 {
        self.completed_bytes
    }
    /// Returns the exact operation byte total when the adapter knows it.
    #[must_use]
    pub const fn total_bytes(self) -> Option<u64> {
        self.total_bytes
    }
}

/// Caller-selected action for one finite exporter poll.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportPollControl {
    /// Advance by at most the supplied positive canonical-byte work budget.
    Continue {
        /// Positive byte budget for this poll only; it is never a session or export ceiling.
        max_bytes: NonZeroUsize,
    },
    /// Cancel the active operation and truthfully clean up exporter-owned temporary state.
    Cancel,
}

impl ExportPollControl {
    /// Creates one positive finite-work continuation poll.
    #[must_use]
    pub const fn continue_with(max_bytes: NonZeroUsize) -> Self {
        Self::Continue { max_bytes }
    }
    /// Returns the finite byte budget for a continuation poll.
    #[must_use]
    pub const fn max_bytes(self) -> Option<NonZeroUsize> {
        match self {
            Self::Continue { max_bytes } => Some(max_bytes),
            Self::Cancel => None,
        }
    }
}

/// Result of polling an exporter-owned batch operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportPoll {
    /// More caller-owned polling is required.
    Pending(ExportProgress),
    /// The complete exact batch was accepted.
    Accepted {
        /// Exact whole-batch acknowledgement.
        ack: ExportAck,
        /// Final exporter progress.
        progress: ExportProgress,
    },
    /// Cancellation completed and the exporter accepted none of the batch.
    Cancelled(ExportProgress),
}

/// Result of polling exporter resource shutdown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExporterShutdownPoll {
    /// More caller-owned polling is required.
    Pending(ExportProgress),
    /// All exporter-owned resources were truthfully released.
    Complete(ExportProgress),
    /// Shutdown cancellation completed without claiming resource release.
    Cancelled(ExportProgress),
}

/// Stable exporter failure class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExporterErrorCode {
    /// Destination is temporarily unavailable.
    Unavailable,
    /// Destination rejected the complete batch.
    Rejected,
    /// Destination returned a malformed or contradictory response.
    Protocol,
    /// Exporter shutdown failed.
    Shutdown,
}

/// Content-free exporter error with truthful owned-resource cleanup status.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ExporterError {
    code: ExporterErrorCode,
    retryable: bool,
    cleanup_complete: bool,
}

impl ExporterError {
    /// Creates a stable exporter failure after complete operation cleanup.
    #[must_use]
    pub const fn new(code: ExporterErrorCode, retryable: bool) -> Self {
        Self { code, retryable, cleanup_complete: true }
    }
    /// Creates a stable exporter failure with explicit cleanup status.
    #[must_use]
    pub const fn with_cleanup_status(
        code: ExporterErrorCode,
        retryable: bool,
        cleanup_complete: bool,
    ) -> Self {
        Self { code, retryable, cleanup_complete }
    }
    /// Returns the stable category.
    #[must_use]
    pub const fn code(self) -> ExporterErrorCode {
        self.code
    }
    /// Returns whether the same exact batch may be retried after complete cleanup.
    #[must_use]
    pub const fn retryable(self) -> bool {
        self.retryable
    }
    /// Returns whether the failed operation released every exporter-owned temporary resource.
    #[must_use]
    pub const fn cleanup_complete(self) -> bool {
        self.cleanup_complete
    }
}

impl fmt::Debug for ExporterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExporterError")
            .field("code", &self.code)
            .field("retryable", &self.retryable)
            .field("cleanup_complete", &self.cleanup_complete)
            .finish()
    }
}

impl fmt::Display for ExporterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "telemetry exporter {:?} (retryable={}, cleanup_complete={})",
            self.code, self.retryable, self.cleanup_complete,
        )
    }
}

impl std::error::Error for ExporterError {}

/// Owned, explicitly polled whole-batch exporter contract.
///
/// An adapter owns at most one operation from `begin_*` until its corresponding poll reports a
/// terminal result. It must perform no detached work. A poll error with incomplete cleanup leaves
/// that same operation active so the caller can poll with [`ExportPollControl::Cancel`].
pub trait Exporter {
    /// Begins ownership of one immutable idempotent batch without claiming acceptance.
    ///
    /// # Errors
    ///
    /// Returns a content-free failure only when no operation was started.
    fn begin_export(&mut self, batch: &ExportBatch) -> Result<ExportProgress, ExporterError>;
    /// Advances or cancels the active export operation.
    ///
    /// # Errors
    ///
    /// Returns a content-free explicit adapter failure with truthful cleanup status.
    fn poll_export(&mut self, control: ExportPollControl) -> Result<ExportPoll, ExporterError>;
    /// Begins exporter resource shutdown without claiming cleanup.
    ///
    /// # Errors
    ///
    /// Returns a content-free failure only when no shutdown operation was started.
    fn begin_shutdown(&mut self) -> Result<ExportProgress, ExporterError>;
    /// Advances or cancels the active exporter shutdown operation.
    ///
    /// # Errors
    ///
    /// Returns a content-free explicit adapter failure with truthful cleanup status.
    fn poll_shutdown(
        &mut self,
        control: ExportPollControl,
    ) -> Result<ExporterShutdownPoll, ExporterError>;
}
