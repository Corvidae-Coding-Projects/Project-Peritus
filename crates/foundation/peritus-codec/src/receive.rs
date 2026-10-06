//! Incremental receipt of exact canonical frames without allocating declared payloads.

use crate::{CodecError, CodecErrorKind, CodecLimits, HEADER_LEN, decode_frame_header};

/// Physical input window; repeated windows have no cumulative work or elapsed allowance.
pub const RECEIVE_WINDOW_BYTES: usize = 64 * 1024;

/// Owns one exact versioned frame across partial reads, cancellation, and allocation failure.
///
/// Header declarations establish syntax and representability, never an allocation request.
/// Payload storage grows only after bytes arrive through a physical input window. A temporary
/// allocation failure retains that window so preparing another read retries its adoption before
/// allowing the transport to overwrite it. This type neither authenticates nor dispatches data.
#[derive(Debug)]
pub struct FrameReceiver {
    limits: CodecLimits,
    header: [u8; HEADER_LEN],
    header_read: usize,
    expected: Option<usize>,
    frame: Vec<u8>,
    window: Vec<u8>,
    prepared: usize,
    pending: usize,
    #[cfg(test)]
    fail_next_adoption: bool,
}

impl FrameReceiver {
    /// Creates an empty receiver under one peer's selected codec contract.
    #[must_use]
    pub const fn new(limits: CodecLimits) -> Self {
        Self {
            limits,
            header: [0; HEADER_LEN],
            header_read: 0,
            expected: None,
            frame: Vec::new(),
            window: Vec::new(),
            prepared: 0,
            pending: 0,
            #[cfg(test)]
            fail_next_adoption: false,
        }
    }

    /// Returns the next owned transport buffer, or `None` once the frame is complete.
    ///
    /// Repeated calls before accepting bytes preserve the buffer. A cancelled asynchronous read
    /// may therefore resume using the same receiver. Already received pending bytes are adopted
    /// before another buffer is exposed.
    ///
    /// # Errors
    /// Rejects malformed headers or caller-selected limits before accepting a payload. A physical
    /// allocation failure returns [`CodecErrorKind::AllocationUnavailable`] and retains progress.
    pub fn prepare_read(&mut self) -> Result<Option<&mut [u8]>, CodecError> {
        self.adopt_pending()?;
        if self.header_read < HEADER_LEN {
            self.prepared = HEADER_LEN - self.header_read;
            return Ok(Some(&mut self.header[self.header_read..]));
        }
        let expected = if let Some(expected) = self.expected {
            expected
        } else {
            let expected = decode_frame_header(&self.header, self.limits)?.frame_len()?;
            self.frame
                .try_reserve_exact(HEADER_LEN)
                .map_err(|_| allocation_error(self.header_read))?;
            self.frame.extend_from_slice(&self.header);
            self.expected = Some(expected);
            expected
        };
        let remaining = expected - self.frame.len();
        if remaining == 0 {
            self.prepared = 0;
            return Ok(None);
        }
        let next = remaining.min(RECEIVE_WINDOW_BYTES);
        if next > self.window.len() {
            self.window
                .try_reserve_exact(next - self.window.len())
                .map_err(|_| allocation_error(self.frame.len()))?;
            self.window.resize(next, 0);
        }
        self.prepared = next;
        Ok(Some(&mut self.window[..next]))
    }

    /// Accepts exactly the nonzero byte count reported by the last transport read.
    ///
    /// # Errors
    /// Rejects a count outside the exposed buffer without moving the frontier. A temporary
    /// allocation failure retains all received bytes; retry [`Self::prepare_read`] to continue.
    pub fn accept_read(&mut self, count: usize) -> Result<(), CodecError> {
        if count == 0 || count > self.prepared || self.pending != 0 {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, self.received_bytes()));
        }
        self.prepared = 0;
        if self.header_read < HEADER_LEN {
            self.header_read += count;
            return Ok(());
        }
        self.pending = count;
        self.adopt_pending()
    }

    /// Returns the number of exact transport bytes retained, including a pending input window.
    #[must_use]
    pub const fn received_bytes(&self) -> usize {
        if self.expected.is_some() { self.frame.len() + self.pending } else { self.header_read }
    }

    /// Reports whether no bytes of the next frame have arrived.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.header_read == 0
    }

    /// Selects a peer contract at a frame boundary without discarding any received bytes.
    ///
    /// # Errors
    /// Rejects renegotiation during receipt of an existing frame.
    pub const fn set_limits(&mut self, limits: CodecLimits) -> Result<(), CodecError> {
        if !self.is_empty() {
            return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, self.received_bytes()));
        }
        self.limits = limits;
        Ok(())
    }

    /// Borrows a complete exact frame, including its original version-one header.
    #[must_use]
    pub fn complete_frame(&self) -> Option<&[u8]> {
        if self.pending == 0 && self.expected == Some(self.frame.len()) {
            Some(&self.frame)
        } else {
            None
        }
    }

    /// Moves out a complete frame and prepares the next frame under a selected contract.
    ///
    /// Incomplete receipt is never discarded by this operation.
    ///
    /// # Errors
    /// Returns truncation if the owned frame is not complete.
    pub fn take_frame(&mut self, next_limits: CodecLimits) -> Result<Vec<u8>, CodecError> {
        if self.complete_frame().is_none() {
            return Err(CodecError::at(CodecErrorKind::Truncated, self.received_bytes()));
        }
        let frame = core::mem::take(&mut self.frame);
        *self = Self::new(next_limits);
        Ok(frame)
    }

    fn adopt_pending(&mut self) -> Result<(), CodecError> {
        if self.pending == 0 {
            return Ok(());
        }
        #[cfg(test)]
        if core::mem::take(&mut self.fail_next_adoption) {
            return Err(allocation_error(self.frame.len()));
        }
        self.frame.try_reserve(self.pending).map_err(|_| allocation_error(self.frame.len()))?;
        self.frame.extend_from_slice(&self.window[..self.pending]);
        self.pending = 0;
        Ok(())
    }
}

const fn allocation_error(offset: usize) -> CodecError {
    CodecError::at(CodecErrorKind::AllocationUnavailable, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FORMAT_VERSION, MAGIC, encode_frame};

    fn receive(receiver: &mut FrameReceiver, bytes: &[u8]) -> Result<usize, CodecError> {
        let Some(buffer) = receiver.prepare_read()? else { return Ok(0) };
        let count = buffer.len().min(bytes.len());
        buffer[..count].copy_from_slice(&bytes[..count]);
        receiver.accept_read(count)?;
        Ok(count)
    }

    #[test]
    fn largest_declared_frame_does_not_allocate_its_payload() {
        let limits = CodecLimits::new(usize::MAX, u32::MAX as usize, 1, 1, 1, 1);
        let mut header = [0; HEADER_LEN];
        header[..4].copy_from_slice(&MAGIC);
        header[4..6].copy_from_slice(&FORMAT_VERSION.to_be_bytes());
        header[6..8].copy_from_slice(&1u16.to_be_bytes());
        header[8..10].copy_from_slice(&1u16.to_be_bytes());
        header[12..].copy_from_slice(&u32::MAX.to_be_bytes());
        let mut receiver = FrameReceiver::new(limits);
        assert_eq!(receive(&mut receiver, &header).unwrap(), HEADER_LEN);
        assert_eq!(receiver.prepare_read().unwrap().unwrap().len(), RECEIVE_WINDOW_BYTES);
        assert_eq!(receiver.frame.len(), HEADER_LEN);
        assert_eq!(receiver.frame.capacity(), HEADER_LEN);
        assert_eq!(receiver.received_bytes(), HEADER_LEN);
        assert!(receiver.complete_frame().is_none());
    }

    #[test]
    fn partial_reads_and_allocation_retry_preserve_exact_bytes_and_next_frame() {
        let limits = CodecLimits::PRODUCTION;
        let encoded = encode_frame(77, 1, &vec![29; 2 * RECEIVE_WINDOW_BYTES + 3], limits).unwrap();
        let mut receiver = FrameReceiver::new(limits);
        let mut offset = 0;
        for count in [1, HEADER_LEN - 1, 7] {
            offset += receive(&mut receiver, &encoded[offset..offset + count]).unwrap();
        }
        let buffer = receiver.prepare_read().unwrap().unwrap();
        let count = buffer.len().min(encoded.len() - offset);
        buffer[..count].copy_from_slice(&encoded[offset..offset + count]);
        receiver.fail_next_adoption = true;
        assert_eq!(
            receiver.accept_read(count).unwrap_err().kind(),
            CodecErrorKind::AllocationUnavailable
        );
        offset += count;
        assert_eq!(receiver.received_bytes(), offset);
        assert!(receiver.complete_frame().is_none());
        assert_eq!(receiver.take_frame(limits).unwrap_err().kind(), CodecErrorKind::Truncated);
        while offset < encoded.len() {
            offset += receive(&mut receiver, &encoded[offset..]).unwrap();
        }
        assert!(receiver.prepare_read().unwrap().is_none());
        assert_eq!(receiver.take_frame(limits).unwrap(), encoded);
        assert!(receiver.is_empty());
        assert_eq!(receive(&mut receiver, &encoded).unwrap(), HEADER_LEN);
    }

    #[test]
    fn invalid_header_and_explicit_peer_capacity_reject_before_payload_receipt() {
        let limits = CodecLimits::new(64, 48, 8, 8, 8, 4);
        let mut header = encode_frame(77, 1, &[], limits).unwrap();
        header[12..].copy_from_slice(&49u32.to_be_bytes());
        let mut receiver = FrameReceiver::new(limits);
        receive(&mut receiver, &header).unwrap();
        assert_eq!(receiver.prepare_read().unwrap_err().kind(), CodecErrorKind::LimitExceeded);
        assert!(receiver.frame.is_empty());
        header[5] = 2;
        let mut receiver = FrameReceiver::new(limits);
        receive(&mut receiver, &header).unwrap();
        assert_eq!(
            receiver.prepare_read().unwrap_err().kind(),
            CodecErrorKind::UnsupportedFormatVersion
        );
        assert!(receiver.window.is_empty());
    }
}
