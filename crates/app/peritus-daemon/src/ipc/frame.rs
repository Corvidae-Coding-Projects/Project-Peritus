//! Allocation-bounded asynchronous PRTS frame transport.

use peritus_app_protocol::{
    AppErrorCode, AppMessage, AppProtocolError, AppProtocolLimits, decode_app_message,
    encode_app_message,
};
use peritus_codec::{CodecError, CodecErrorKind, FrameReceiver};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

// This is a retry cadence, never an expiry or a bound on attempts or received bytes.
const ALLOCATION_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// One bidirectional stream of complete canonical A3 PRTS frames.
pub struct AppFrameStream<S> {
    stream: S,
    limits: AppProtocolLimits,
    receiver: FrameReceiver,
    allocation_retry_at: Option<tokio::time::Instant>,
}

impl<S> AppFrameStream<S> {
    /// Wraps an authenticated byte stream under fixed receive limits.
    #[must_use]
    pub const fn new(stream: S, limits: AppProtocolLimits) -> Self {
        Self {
            stream,
            limits,
            receiver: FrameReceiver::new(limits.codec()),
            allocation_retry_at: None,
        }
    }
    /// Returns the underlying authenticated transport.
    #[must_use]
    pub fn into_inner(self) -> S {
        self.stream
    }
}

impl<S> AppFrameStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// Reads, bounds, and completely decodes one A3 frame.
    ///
    /// # Errors
    ///
    /// Returns a transport error for truncation/I/O failure or invalid input for malformed A3.
    pub async fn read(&mut self) -> Result<AppMessage, DaemonError> {
        self.read_or_eof().await?.ok_or_else(|| {
            transport_read(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "authenticated local stream closed before an application frame",
            ))
        })
    }

    /// Reads one frame, treating an EOF exactly on a frame boundary as a clean disconnect.
    /// Partial bytes remain owned by this stream if the read future is cancelled. Temporary
    /// allocation pressure waits and retries with the same receiver instead of dropping the
    /// connection. The caller can still cancel the wait for shutdown or other connection work.
    ///
    /// # Errors
    ///
    /// Returns a transport error for a partial frame or I/O failure and invalid input for
    /// malformed A3. A connection that closes between frames returns `Ok(None)`.
    pub async fn read_or_eof(&mut self) -> Result<Option<AppMessage>, DaemonError> {
        loop {
            if let Some(retry_at) = self.allocation_retry_at {
                tokio::time::sleep_until(retry_at).await;
                self.allocation_retry_at = None;
            }
            match self.read_once().await {
                Err(error) => self.defer_allocation_retry(error)?,
                result => return result,
            }
        }
    }

    fn defer_allocation_retry(&mut self, error: DaemonError) -> Result<(), DaemonError> {
        if error.code_kind() != DaemonErrorCode::ResourceLimit
            || error.recovery() != DaemonRecovery::Retry
        {
            return Err(error);
        }
        self.allocation_retry_at = Some(tokio::time::Instant::now() + ALLOCATION_RETRY_INTERVAL);
        Ok(())
    }

    async fn read_once(&mut self) -> Result<Option<AppMessage>, DaemonError> {
        while let Some(buffer) = self.receiver.prepare_read().map_err(receive_codec_error)? {
            let count = self.stream.read(buffer).await.map_err(transport_read)?;
            if count == 0 {
                if self.receiver.is_empty() {
                    return Ok(None);
                }
                return Err(truncated());
            }
            self.receiver.accept_read(count).map_err(receive_codec_error)?;
        }
        let frame = self.receiver.complete_frame().ok_or_else(truncated)?;
        let message = decode_app_message(frame, self.limits)
            .map_err(|error| protocol_error(error, "decode application frame"))?;
        // Do not discard received bytes if owned domain decoding encounters allocator pressure.
        self.receiver.take_frame(self.limits.codec()).map_err(receive_codec_error)?;
        Ok(Some(message))
    }

    /// Encodes and writes one complete canonical A3 frame.
    ///
    /// # Errors
    ///
    /// Returns a protocol or transport error without writing a partial second frame.
    pub async fn write(&mut self, message: &AppMessage) -> Result<(), DaemonError> {
        let frame = encode_app_message(message, self.limits)
            .map_err(|error| protocol_error(error, "encode application frame"))?;
        self.stream.write_all(&frame).await.map_err(transport_write)?;
        self.stream.flush().await.map_err(transport_write)
    }
}

fn protocol_error(error: AppProtocolError, operation: &'static str) -> DaemonError {
    let (code, recovery) = if error.code() == AppErrorCode::Backpressure {
        (DaemonErrorCode::ResourceLimit, DaemonRecovery::Retry)
    } else {
        (DaemonErrorCode::InvalidInput, DaemonRecovery::CorrectRequest)
    };
    DaemonError::with_source(
        code,
        recovery,
        operation,
        "application message could not complete under the selected codec contract",
        error,
    )
}

fn truncated() -> DaemonError {
    transport_read(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        "authenticated local stream closed inside an application frame",
    ))
}

fn transport_read(error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Transport,
        DaemonRecovery::Retry,
        "read application frame",
        "authenticated local stream closed or failed",
        error,
    )
}

fn transport_write(error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Transport,
        DaemonRecovery::Retry,
        "write application frame",
        "authenticated local stream write failed",
        error,
    )
}

fn receive_codec_error(error: CodecError) -> DaemonError {
    let (code, recovery) = match error.kind() {
        CodecErrorKind::AllocationUnavailable => {
            (DaemonErrorCode::ResourceLimit, DaemonRecovery::Retry)
        }
        CodecErrorKind::LimitExceeded => {
            (DaemonErrorCode::ResourceLimit, DaemonRecovery::CorrectRequest)
        }
        _ => (DaemonErrorCode::InvalidInput, DaemonRecovery::CorrectRequest),
    };
    DaemonError::with_source(
        code,
        recovery,
        "read application frame",
        "canonical frame receipt could not advance",
        error,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_codec::{HEADER_LEN, MAGIC};

    #[tokio::test]
    async fn frame_boundary_eof_is_a_clean_disconnect() {
        let (client, server) = tokio::io::duplex(64);
        drop(client);
        let mut frames = AppFrameStream::new(server, AppProtocolLimits::PRODUCTION);
        assert!(frames.read_or_eof().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn partial_header_is_a_transport_failure() {
        let (mut client, server) = tokio::io::duplex(64);
        client.write_all(&MAGIC[..1]).await.unwrap();
        drop(client);
        let mut frames = AppFrameStream::new(server, AppProtocolLimits::PRODUCTION);
        let error = frames.read_or_eof().await.unwrap_err();
        assert_eq!(error.code_kind(), DaemonErrorCode::Transport);
        assert_eq!(error.operation(), "read application frame");
    }

    #[tokio::test]
    async fn allocation_retry_preserves_partial_frame_and_the_next_message() {
        use std::{future::poll_fn, task::Poll};

        let message = AppMessage::ClientHello(
            peritus_app_protocol::ClientHello::new(
                peritus_app_protocol::ProtocolId::new([7; 16]).unwrap(),
                vec![peritus_app_protocol::CURRENT_PROTOCOL_RANGE],
                Vec::new(),
                Vec::new(),
                AppProtocolLimits::PRODUCTION,
                "capacity-retry-test".to_owned(),
            )
            .unwrap(),
        );
        let encoded = encode_app_message(&message, AppProtocolLimits::PRODUCTION).unwrap();
        let split = HEADER_LEN + 1;
        let (mut peer, stream) = tokio::io::duplex(4096);
        let mut frames = AppFrameStream::new(stream, AppProtocolLimits::PRODUCTION);
        peer.write_all(&encoded[..split]).await.unwrap();
        {
            let mut pending = Box::pin(frames.read_or_eof());
            poll_fn(|context| {
                assert!(pending.as_mut().poll(context).is_pending());
                Poll::Ready(())
            })
            .await;
        }
        peer.write_all(&encoded[split..]).await.unwrap();
        peer.write_all(&encoded).await.unwrap();
        drop(peer);
        frames
            .defer_allocation_retry(receive_codec_error(CodecError::at(
                CodecErrorKind::AllocationUnavailable,
                split,
            )))
            .unwrap();
        let scheduled_retry = frames.allocation_retry_at;
        {
            let mut waiting = Box::pin(frames.read_or_eof());
            poll_fn(|context| {
                assert!(waiting.as_mut().poll(context).is_pending());
                Poll::Ready(())
            })
            .await;
        }
        assert_eq!(
            frames.allocation_retry_at, scheduled_retry,
            "cancellation keeps retry progress"
        );
        assert_eq!(frames.read_or_eof().await.unwrap(), Some(message.clone()));
        assert_eq!(frames.read_or_eof().await.unwrap(), Some(message));
        assert!(frames.read_or_eof().await.unwrap().is_none());
    }

    #[test]
    fn capacity_retry_does_not_retry_malformed_input_or_transport_failure() {
        let (_, stream) = tokio::io::duplex(64);
        let mut frames = AppFrameStream::new(stream, AppProtocolLimits::PRODUCTION);
        for kind in [CodecErrorKind::LimitExceeded, CodecErrorKind::InvalidMagic] {
            let error = frames
                .defer_allocation_retry(receive_codec_error(CodecError::at(kind, 0)))
                .unwrap_err();
            assert_eq!(error.recovery(), DaemonRecovery::CorrectRequest);
            assert_eq!(frames.allocation_retry_at, None);
        }
        let error = frames.defer_allocation_retry(truncated()).unwrap_err();
        assert_eq!(error.code_kind(), DaemonErrorCode::Transport);
        assert_eq!(frames.allocation_retry_at, None);
    }

    #[tokio::test]
    async fn cancelled_reads_retain_partial_headers_and_payloads() {
        use peritus_app_protocol::{
            AppRequestEnvelope, AppRequestPayload, CorrelationId, ProtocolContext, ProtocolId,
            ProtocolVersion, RequestId,
        };
        let context = ProtocolContext::new(
            ProtocolId::new([1; 16]).unwrap(),
            ProtocolVersion::new(1, 0).unwrap(),
            peritus_types::SessionId::new([2; 16]).unwrap(),
        );
        let message = AppMessage::Request(
            AppRequestEnvelope::new(
                context,
                RequestId::new([3; 16]).unwrap(),
                CorrelationId::new([4; 16]).unwrap(),
                AppRequestPayload::DaemonStatus,
            )
            .unwrap(),
        );
        let encoded = encode_app_message(&message, AppProtocolLimits::PRODUCTION).unwrap();
        for split in [1, HEADER_LEN - 1, HEADER_LEN + 1, encoded.len() - 1] {
            let (mut client, server) = tokio::io::duplex(4096);
            let mut frames = AppFrameStream::new(server, AppProtocolLimits::PRODUCTION);
            client.write_all(&encoded[..split]).await.unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(5), frames.read_or_eof())
                    .await
                    .is_err()
            );
            client.write_all(&encoded[split..]).await.unwrap();
            client.write_all(&encoded).await.unwrap();
            drop(client);
            assert_eq!(frames.read_or_eof().await.unwrap(), Some(message.clone()), "split {split}");
            assert_eq!(
                frames.read_or_eof().await.unwrap(),
                Some(message.clone()),
                "next frame {split}"
            );
            assert!(frames.read_or_eof().await.unwrap().is_none());
        }
    }
}
