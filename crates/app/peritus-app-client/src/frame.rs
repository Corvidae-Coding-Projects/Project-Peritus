//! Bounded PRTS framing over the platform's local IPC stream.

use std::{ffi::OsStr, pin::Pin};

use peritus_app_protocol::{
    AppMessage, AppProtocolError, AppProtocolLimits, decode_app_message, encode_app_message,
};
use peritus_codec::{CodecErrorKind, FrameReceiver};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::ClientError;

// Retry spacing does not expire the request or bound the number of capacity retries.
const ALLOCATION_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

trait LocalIo: AsyncRead + AsyncWrite {}
impl<T: AsyncRead + AsyncWrite> LocalIo for T {}
type BoxedLocalIo = Pin<Box<dyn LocalIo + Send>>;

pub struct FrameStream {
    stream: BoxedLocalIo,
    limits: AppProtocolLimits,
    receiver: FrameReceiver,
    allocation_retry_at: Option<tokio::time::Instant>,
}

impl FrameStream {
    pub(crate) async fn connect(endpoint: &OsStr) -> Result<Self, ClientError> {
        let limits = AppProtocolLimits::PRODUCTION;
        Ok(Self {
            stream: connect_local(endpoint).await?,
            limits,
            receiver: FrameReceiver::new(limits.codec()),
            allocation_retry_at: None,
        })
    }

    pub(crate) fn set_limits(&mut self, limits: AppProtocolLimits) -> Result<(), ClientError> {
        self.receiver.set_limits(limits.codec()).map_err(AppProtocolError::from_codec)?;
        self.limits = limits;
        Ok(())
    }

    pub(crate) const fn limits(&self) -> AppProtocolLimits {
        self.limits
    }

    pub(crate) async fn read(&mut self) -> Result<AppMessage, ClientError> {
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

    fn defer_allocation_retry(&mut self, error: ClientError) -> Result<(), ClientError> {
        let allocation_unavailable = std::error::Error::source(&error)
            .and_then(|source| source.downcast_ref::<AppProtocolError>())
            .and_then(AppProtocolError::codec_source)
            .is_some_and(|source| source.kind() == CodecErrorKind::AllocationUnavailable);
        if !allocation_unavailable {
            return Err(error);
        }
        self.allocation_retry_at = Some(tokio::time::Instant::now() + ALLOCATION_RETRY_INTERVAL);
        Ok(())
    }

    async fn read_once(&mut self) -> Result<AppMessage, ClientError> {
        while let Some(buffer) =
            self.receiver.prepare_read().map_err(AppProtocolError::from_codec)?
        {
            let count =
                self.stream.read(buffer).await.map_err(|error| {
                    ClientError::connection("read daemon frame", error.to_string())
                })?;
            if count == 0 {
                return Err(ClientError::connection(
                    "read daemon frame",
                    "daemon stream closed before the frame was complete",
                ));
            }
            self.receiver.accept_read(count).map_err(AppProtocolError::from_codec)?;
        }
        let frame = self.receiver.complete_frame().ok_or_else(|| {
            ClientError::protocol("decode daemon frame", "canonical frame is incomplete")
        })?;
        let message = decode_app_message(frame, self.limits)?;
        // Decode capacity failures leave the original complete frame owned for the same retry.
        self.receiver.take_frame(self.limits.codec()).map_err(AppProtocolError::from_codec)?;
        Ok(message)
    }

    pub(crate) async fn write(&mut self, message: &AppMessage) -> Result<(), ClientError> {
        let frame = encode_app_message(message, self.limits)?;
        self.stream
            .write_all(&frame)
            .await
            .map_err(|error| ClientError::connection("write daemon frame", error.to_string()))?;
        self.stream
            .flush()
            .await
            .map_err(|error| ClientError::connection("flush daemon frame", error.to_string()))
    }
}

#[cfg(unix)]
async fn connect_local(endpoint: &OsStr) -> Result<BoxedLocalIo, ClientError> {
    let stream = tokio::net::UnixStream::connect(std::path::Path::new(endpoint))
        .await
        .map_err(|error| ClientError::connection("connect Unix endpoint", error.to_string()))?;
    Ok(Box::pin(stream))
}

#[cfg(windows)]
fn connect_local(endpoint: &OsStr) -> std::future::Ready<Result<BoxedLocalIo, ClientError>> {
    let result = endpoint
        .to_str()
        .ok_or_else(|| {
            ClientError::new(
                crate::ClientErrorKind::Unsupported,
                "connect Windows endpoint",
                "Windows named-pipe endpoint must be Unicode",
            )
        })
        .and_then(|endpoint| {
            tokio::net::windows::named_pipe::ClientOptions::new()
                .open(endpoint)
                .map(|stream| Box::pin(stream) as BoxedLocalIo)
                .map_err(|error| {
                    ClientError::connection("connect Windows named pipe", error.to_string())
                })
        });
    std::future::ready(result)
}

#[cfg(not(any(unix, windows)))]
fn connect_local(_endpoint: &OsStr) -> std::future::Ready<Result<BoxedLocalIo, ClientError>> {
    std::future::ready(Err(ClientError::new(
        crate::ClientErrorKind::Unsupported,
        "connect local endpoint",
        "this target has no supported Peritus local transport",
    )))
}

#[cfg(test)]
mod tests {
    use super::FrameStream;
    use peritus_app_protocol::{
        AppMessage, AppProtocolLimits, decode_app_message, schema::generated_fixture_cases,
    };
    use peritus_codec::{CodecErrorKind, CodecLimits, decode_frame_header, encode_frame};
    use std::{
        future::{Future, poll_fn},
        task::Poll,
    };
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn cancelling_a_client_read_preserves_the_same_frame_and_next_frame() {
        let encoded = generated_fixture_cases()
            .unwrap()
            .into_iter()
            .find(|case| case.accepted && case.case == "minimal-client-hello")
            .unwrap()
            .payload;
        let message: AppMessage =
            decode_app_message(&encoded, AppProtocolLimits::PRODUCTION).unwrap();
        for split in
            [1, peritus_codec::HEADER_LEN - 1, peritus_codec::HEADER_LEN + 1, encoded.len() - 1]
        {
            let (mut peer, stream) = tokio::io::duplex(4096);
            let mut frames = FrameStream {
                stream: Box::pin(stream),
                limits: AppProtocolLimits::PRODUCTION,
                receiver: peritus_codec::FrameReceiver::new(CodecLimits::PRODUCTION),
                allocation_retry_at: None,
            };
            peer.write_all(&encoded[..split]).await.unwrap();
            {
                let mut read = std::pin::pin!(frames.read());
                assert!(poll_fn(|cx| Poll::Ready(read.as_mut().poll(cx).is_pending())).await);
            }
            assert_eq!(frames.receiver.received_bytes(), split);
            peer.write_all(&encoded[split..]).await.unwrap();
            peer.write_all(&encoded).await.unwrap();
            frames
                .defer_allocation_retry(
                    peritus_app_protocol::AppProtocolError::from_codec(
                        peritus_codec::CodecError::at(CodecErrorKind::AllocationUnavailable, split),
                    )
                    .into(),
                )
                .unwrap();
            let scheduled_retry = frames.allocation_retry_at;
            {
                let mut waiting = std::pin::pin!(frames.read());
                assert!(poll_fn(|cx| Poll::Ready(waiting.as_mut().poll(cx).is_pending())).await);
            }
            assert_eq!(frames.allocation_retry_at, scheduled_retry);
            assert_eq!(frames.read().await.unwrap(), message);
            assert_eq!(frames.read().await.unwrap(), message);
        }
    }

    #[test]
    fn client_capacity_wait_does_not_retry_explicit_limits_or_failed_transport() {
        let (_, stream) = tokio::io::duplex(64);
        let mut frames = FrameStream {
            stream: Box::pin(stream),
            limits: AppProtocolLimits::PRODUCTION,
            receiver: peritus_codec::FrameReceiver::new(CodecLimits::PRODUCTION),
            allocation_retry_at: None,
        };
        for kind in [CodecErrorKind::LimitExceeded, CodecErrorKind::InvalidMagic] {
            let error = peritus_app_protocol::AppProtocolError::from_codec(
                peritus_codec::CodecError::at(kind, 0),
            );
            assert!(frames.defer_allocation_retry(error.into()).is_err());
            assert_eq!(frames.allocation_retry_at, None);
        }
        assert!(
            frames
                .defer_allocation_retry(crate::ClientError::connection("test", "closed"))
                .is_err()
        );
        assert_eq!(frames.allocation_retry_at, None);
    }

    #[test]
    fn invalid_magic_is_rejected_before_payload_allocation() {
        let header = [0; peritus_codec::HEADER_LEN];
        assert_eq!(
            decode_frame_header(&header, AppProtocolLimits::PRODUCTION.codec()).unwrap_err().kind(),
            CodecErrorKind::InvalidMagic
        );
    }

    #[test]
    fn oversized_payload_is_rejected_before_payload_allocation() {
        let limits = CodecLimits::new(64, 48, 8, 8, 8, 4);
        let mut header = encode_frame(1, 1, &[], limits).unwrap();
        header[12..16].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(
            decode_frame_header(&header, limits).unwrap_err().kind(),
            CodecErrorKind::LimitExceeded
        );
    }

    #[test]
    fn bounded_payload_length_is_preserved() {
        let mut header = encode_frame(1, 1, &[], CodecLimits::PRODUCTION).unwrap();
        header[12..16].copy_from_slice(&128_u32.to_be_bytes());
        assert_eq!(
            decode_frame_header(&header, AppProtocolLimits::PRODUCTION.codec())
                .unwrap()
                .payload_len(),
            128
        );
    }
}
