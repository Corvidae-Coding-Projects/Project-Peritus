//! Allocation-bounded asynchronous PRTS frame transport.

use peritus_app_protocol::{AppMessage, AppProtocolLimits, decode_app_message, encode_app_message};
use peritus_codec::{HEADER_LEN, MAGIC};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

/// One bidirectional stream of complete canonical A3 PRTS frames.
pub struct AppFrameStream<S> {
    stream: S,
    limits: AppProtocolLimits,
}

impl<S> AppFrameStream<S> {
    /// Wraps an authenticated byte stream under fixed receive limits.
    #[must_use]
    pub const fn new(stream: S, limits: AppProtocolLimits) -> Self {
        Self { stream, limits }
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
    ///
    /// # Errors
    ///
    /// Returns a transport error for a partial frame or I/O failure and invalid input for
    /// malformed A3. A connection that closes between frames returns `Ok(None)`.
    pub async fn read_or_eof(&mut self) -> Result<Option<AppMessage>, DaemonError> {
        let mut header = [0_u8; HEADER_LEN];
        match self.stream.read(&mut header[..1]).await {
            Ok(0) => return Ok(None),
            Ok(_) => {}
            Err(error) => return Err(transport_read(error)),
        }
        self.stream.read_exact(&mut header[1..]).await.map_err(transport_read)?;
        if header[..4] != MAGIC {
            return Err(protocol("PRTS frame magic is invalid"));
        }
        let payload_len = usize::try_from(u32::from_be_bytes(
            header[12..16].try_into().expect("fixed PRTS header"),
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
        let mut frame = Vec::with_capacity(HEADER_LEN + payload_len);
        frame.extend_from_slice(&header);
        frame.resize(HEADER_LEN + payload_len, 0);
        self.stream.read_exact(&mut frame[HEADER_LEN..]).await.map_err(transport_read)?;
        decode_app_message(&frame, self.limits).map(Some).map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::InvalidInput,
                DaemonRecovery::CorrectRequest,
                "decode application frame",
                "application frame violates the negotiated protocol",
                error,
            )
        })
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
}
