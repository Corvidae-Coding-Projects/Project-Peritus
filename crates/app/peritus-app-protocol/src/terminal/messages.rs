//! Exact attachment-bound terminal input, output, resize, and terminal messages.

use crate::{CorrelationId, RequestId, TerminalAttachmentId};
use peritus_types::ProcessId;

use super::{TerminalError, TerminalErrorKind, error::reject};

/// Immutable attachment/process/action correlation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerminalBinding {
    attachment: TerminalAttachmentId,
    process: ProcessId,
    originating_request: RequestId,
}

impl TerminalBinding {
    /// Creates an exact terminal binding.
    #[must_use]
    pub const fn new(
        attachment_id: TerminalAttachmentId,
        process_id: ProcessId,
        originating_request_id: RequestId,
    ) -> Self {
        Self {
            attachment: attachment_id,
            process: process_id,
            originating_request: originating_request_id,
        }
    }
    /// Returns the attachment identity.
    #[must_use]
    pub const fn attachment_id(self) -> TerminalAttachmentId {
        self.attachment
    }
    /// Returns the C2-owned process identity being observed.
    #[must_use]
    pub const fn process_id(self) -> ProcessId {
        self.process
    }
    /// Returns the action request that created the attachment.
    #[must_use]
    pub const fn originating_request_id(self) -> RequestId {
        self.originating_request
    }
}

/// Closed output stream classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TerminalStream {
    /// Standard output bytes.
    Stdout,
    /// Standard error bytes.
    Stderr,
    /// Combined pseudo-terminal output bytes whose stdout/stderr origin is not separable.
    Terminal,
}

/// One ordered nonempty opaque terminal output chunk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalOutput {
    binding: TerminalBinding,
    sequence: u64,
    offset: u64,
    stream: TerminalStream,
    bytes: Vec<u8>,
}

impl TerminalOutput {
    /// Creates bounded opaque terminal output.
    ///
    /// # Errors
    ///
    /// Rejects a zero byte bound or empty/oversized output bytes.
    pub fn new(
        binding: TerminalBinding,
        sequence: u64,
        offset: u64,
        stream: TerminalStream,
        bytes: Vec<u8>,
        maximum_chunk_bytes: usize,
    ) -> Result<Self, TerminalError> {
        bounded_bytes(&bytes, maximum_chunk_bytes)?;
        Ok(Self { binding, sequence, offset, stream, bytes })
    }
    /// Returns the exact attachment binding.
    #[must_use]
    pub const fn binding(&self) -> TerminalBinding {
        self.binding
    }
    /// Returns the zero-based global output sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Returns the globally conserved byte offset.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }
    /// Returns the source stream.
    #[must_use]
    pub const fn stream(&self) -> TerminalStream {
        self.stream
    }
    /// Borrows exact opaque output bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// One bounded opaque terminal input request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalInput {
    binding: TerminalBinding,
    bytes: Vec<u8>,
}

impl TerminalInput {
    /// Creates nonempty bounded terminal input.
    ///
    /// # Errors
    ///
    /// Rejects a zero byte bound or empty/oversized input bytes.
    pub fn new(
        binding: TerminalBinding,
        bytes: Vec<u8>,
        maximum_chunk_bytes: usize,
    ) -> Result<Self, TerminalError> {
        bounded_bytes(&bytes, maximum_chunk_bytes)?;
        Ok(Self { binding, bytes })
    }
    /// Returns the exact attachment binding.
    #[must_use]
    pub const fn binding(&self) -> TerminalBinding {
        self.binding
    }
    /// Borrows exact opaque input bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

const fn bounded_bytes(bytes: &[u8], maximum: usize) -> Result<(), TerminalError> {
    if maximum == 0 {
        return Err(reject(TerminalErrorKind::InvalidLimit, "terminal byte limit is zero"));
    }
    if bytes.is_empty() || bytes.len() > maximum {
        return Err(reject(
            TerminalErrorKind::InvalidInput,
            "terminal bytes are empty or exceed their negotiated bound",
        ));
    }
    Ok(())
}

/// Positive bounded terminal dimensions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerminalResize {
    binding: TerminalBinding,
    columns: u16,
    rows: u16,
}

impl TerminalResize {
    /// Creates positive dimensions within explicit negotiated maxima.
    ///
    /// # Errors
    ///
    /// Rejects zero dimensions/maxima or dimensions above their maxima.
    pub const fn new(
        binding: TerminalBinding,
        columns: u16,
        rows: u16,
        maximum_columns: u16,
        maximum_rows: u16,
    ) -> Result<Self, TerminalError> {
        if maximum_columns == 0 || maximum_rows == 0 {
            return Err(reject(
                TerminalErrorKind::InvalidLimit,
                "terminal dimension maximum is zero",
            ));
        }
        if columns == 0 || rows == 0 || columns > maximum_columns || rows > maximum_rows {
            return Err(reject(
                TerminalErrorKind::InvalidInput,
                "terminal dimensions are zero or exceed negotiated maxima",
            ));
        }
        Ok(Self { binding, columns, rows })
    }
    /// Returns the exact attachment binding.
    #[must_use]
    pub const fn binding(self) -> TerminalBinding {
        self.binding
    }
    /// Returns positive columns.
    #[must_use]
    pub const fn columns(self) -> u16 {
        self.columns
    }
    /// Returns positive rows.
    #[must_use]
    pub const fn rows(self) -> u16 {
        self.rows
    }
}

/// Correlated idempotent detach fact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerminalDetach {
    binding: TerminalBinding,
    correlation_id: CorrelationId,
}

impl TerminalDetach {
    /// Creates an exact detach fact.
    #[must_use]
    pub const fn new(binding: TerminalBinding, correlation_id: CorrelationId) -> Self {
        Self { binding, correlation_id }
    }
    /// Returns the exact attachment binding.
    #[must_use]
    pub const fn binding(self) -> TerminalBinding {
        self.binding
    }
    /// Returns request/response correlation.
    #[must_use]
    pub const fn correlation_id(self) -> CorrelationId {
        self.correlation_id
    }
}

/// Correlated terminal cancellation fact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerminalCancellation {
    binding: TerminalBinding,
    correlation_id: CorrelationId,
}

impl TerminalCancellation {
    /// Creates an exact cancellation fact.
    #[must_use]
    pub const fn new(binding: TerminalBinding, correlation_id: CorrelationId) -> Self {
        Self { binding, correlation_id }
    }
    /// Returns the exact attachment binding.
    #[must_use]
    pub const fn binding(self) -> TerminalBinding {
        self.binding
    }
    /// Returns request/response correlation.
    #[must_use]
    pub const fn correlation_id(self) -> CorrelationId {
        self.correlation_id
    }
}

/// Closed observed process-exit classification without OS ownership claims.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TerminalExitDisposition {
    /// The process reported a portable exit code.
    Code(i32),
    /// The process was observed to terminate by a numeric signal.
    Signal(i32),
    /// The adapter could not provide a stronger portable classification.
    Unknown,
}

/// Terminal exit observation fenced to the exact final output position.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerminalExit {
    binding: TerminalBinding,
    next_sequence: u64,
    final_offset: u64,
    disposition: TerminalExitDisposition,
}

impl TerminalExit {
    /// Creates an exit observation at an exact final output fence.
    #[must_use]
    pub const fn new(
        binding: TerminalBinding,
        next_sequence: u64,
        final_offset: u64,
        disposition: TerminalExitDisposition,
    ) -> Self {
        Self { binding, next_sequence, final_offset, disposition }
    }
    /// Returns the exact attachment binding.
    #[must_use]
    pub const fn binding(self) -> TerminalBinding {
        self.binding
    }
    /// Returns the sequence that would follow the final output chunk.
    #[must_use]
    pub const fn next_sequence(self) -> u64 {
        self.next_sequence
    }
    /// Returns the exact conserved final output offset.
    #[must_use]
    pub const fn final_offset(self) -> u64 {
        self.final_offset
    }
    /// Returns the observed exit classification.
    #[must_use]
    pub const fn disposition(self) -> TerminalExitDisposition {
        self.disposition
    }
}
