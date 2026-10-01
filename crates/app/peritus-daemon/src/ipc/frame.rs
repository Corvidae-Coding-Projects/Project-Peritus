//! Allocation-bounded asynchronous PRTS frame transport.

use peritus_app_protocol::{AppMessage, AppProtocolLimits, decode_app_message, encode_app_message};
use peritus_codec::{HEADER_LEN, MAGIC};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

/// One bidirectional stream of complete canonical A3 PRTS frames.
pub struct AppFrameStream<S> {
    stream: S,
    limits: AppProtocolLimits,
    header: [u8; HEADER_LEN],
    header_read: usize,
    frame: Vec<u8>,
    frame_read: usize,
}

impl<S> AppFrameStream<S> {
    /// Wraps an authenticated byte stream under fixed receive limits.
    #[must_use]
    pub const fn new(stream: S, limits: AppProtocolLimits) -> Self {
        Self {
            stream,
            limits,
            header: [0; HEADER_LEN],
            header_read: 0,
            frame: Vec::new(),
            frame_read: 0,
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
    /// Partial bytes remain owned by this stream if the read future is cancelled.
    ///
    /// # Errors
    ///
    /// Returns a transport error for a partial frame or I/O failure and invalid input for
    /// malformed A3. A connection that closes between frames returns `Ok(None)`.
    pub async fn read_or_eof(&mut self) -> Result<Option<AppMessage>, DaemonError> {
        while self.header_read < HEADER_LEN {
            let count = self
                .stream
                .read(&mut self.header[self.header_read..])
                .await
                .map_err(transport_read)?;
            if count == 0 {
                if self.header_read == 0 {
                    return Ok(None);
                }
                return Err(truncated());
            }
            self.header_read += count;
        }
        self.prepare_frame()?;
        while self.frame_read < self.frame.len() {
            let count = self
                .stream
                .read(&mut self.frame[self.frame_read..])
                .await
                .map_err(transport_read)?;
            if count == 0 {
                return Err(truncated());
            }
            self.frame_read += count;
        }
        let result = decode_app_message(&self.frame, self.limits).map(Some).map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::InvalidInput,
                DaemonRecovery::CorrectRequest,
                "decode application frame",
                "application frame violates the negotiated protocol",
                error,
            )
        });
        self.header_read = 0;
        self.frame_read = 0;
        self.frame.clear();
        result
    }

    fn prepare_frame(&mut self) -> Result<(), DaemonError> {
        if !self.frame.is_empty() {
            return Ok(());
        }
        if self.header[..4] != MAGIC {
            return Err(protocol("PRTS frame magic is invalid"));
        }
        let payload_len = usize::try_from(u32::from_be_bytes(
            self.header[12..16].try_into().expect("fixed PRTS header"),
        ))
        .map_err(|_| protocol("PRTS payload length cannot be represented"))?;
        let codec = self.limits.codec();
        if payload_len > codec.max_payload_bytes
            || HEADER_LEN
                .checked_add(payload_len)
                .is_none_or(|length| length > codec.max_frame_bytes)
        {
            return Err(DaemonError::new(
                DaemonErrorCode::ResourceLimit,
                DaemonRecovery::CorrectRequest,
                "read application frame",
                "declared PRTS payload exceeds the pre-allocation bound",
            ));
        }
        self.frame.extend_from_slice(&self.header);
        self.frame.resize(HEADER_LEN + payload_len, 0);
        self.frame_read = HEADER_LEN;
        Ok(())
    }

    /// Encodes and writes one complete canonical A3 frame.
    ///
    /// # Errors
    ///
    /// Returns a protocol or transport error without writing a partial second frame.
    pub async fn write(&mut self, message: &AppMessage) -> Result<(), DaemonError> {
        let frame = encode_app_message(message, self.limits).map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::InvalidInput,
                DaemonRecovery::CorrectRequest,
                "encode application frame",
                "application message violates the negotiated protocol",
                error,
            )
        })?;
        self.stream.write_all(&frame).await.map_err(transport_write)?;
        self.stream.flush().await.map_err(transport_write)
    }
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

fn protocol(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "read application frame",
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
