//! Export identities, records, batches, acknowledgements, and adapter contract.

use core::fmt;

use peritus_types::Sha256Digest;

use super::encoding::batch_digest;
use crate::{
    MetricPoint, OtelEvent, OtelSpan, TelemetryError, TelemetryErrorKind, buffer::BufferedRecord,
};

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

/// Redaction-safe export value.
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

/// One stable-sequence record inside an export batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportItem {
    pub(super) sequence: u64,
    pub(super) record: ExportRecord,
}

impl ExportItem {
    /// Returns stable submitted sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Borrows the redaction-safe record.
    #[must_use]
    pub const fn record(&self) -> &ExportRecord {
        &self.record
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
            .map(|record| ExportItem { sequence: record.sequence, record: record.record })
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
    /// Borrows sequenced records.
    #[must_use]
    pub fn items(&self) -> &[ExportItem] {
        &self.items
    }

    /// Encodes one complete redaction-safe batch for durable local-file export.
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
            let record = item.record.canonical_bytes()?;
            let length = u64::try_from(record.len()).map_err(|_| {
                TelemetryError::new(
                    TelemetryErrorKind::SequenceOverflow,
                    "encode local telemetry batch",
                    "record length is unrepresentable",
                )
            })?;
            bytes.extend_from_slice(&item.sequence.to_be_bytes());
            bytes.extend_from_slice(&length.to_be_bytes());
            bytes.extend_from_slice(&record);
        }
        Ok(bytes)
    }
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

/// Content-free exporter error.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ExporterError {
    code: ExporterErrorCode,
    retryable: bool,
}

impl ExporterError {
    /// Creates a stable exporter failure without provider response text.
    #[must_use]
    pub const fn new(code: ExporterErrorCode, retryable: bool) -> Self {
        Self { code, retryable }
    }
    /// Returns the stable category.
    #[must_use]
    pub const fn code(self) -> ExporterErrorCode {
        self.code
    }
    /// Returns whether the same exact batch may be retried.
    #[must_use]
    pub const fn retryable(self) -> bool {
        self.retryable
    }
}

impl fmt::Debug for ExporterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExporterError")
            .field("code", &self.code)
            .field("retryable", &self.retryable)
            .finish()
    }
}

impl fmt::Display for ExporterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "telemetry exporter {:?} (retryable={})", self.code, self.retryable)
    }
}

impl std::error::Error for ExporterError {}

/// Synchronous whole-batch exporter contract.
pub trait Exporter {
    /// Exports one immutable idempotent batch.
    ///
    /// # Errors
    ///
    /// Returns a content-free explicit adapter failure. The caller retains the complete batch.
    fn export(&mut self, batch: &ExportBatch) -> Result<ExportAck, ExporterError>;
    /// Flushes and releases exporter-owned resources.
    ///
    /// # Errors
    ///
    /// Returns a content-free explicit adapter shutdown failure.
    fn shutdown(&mut self) -> Result<(), ExporterError>;
}
