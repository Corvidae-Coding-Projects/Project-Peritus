//! Bounded PRTS framing over the platform's local IPC stream.

use std::{ffi::OsStr, pin::Pin};

use peritus_app_protocol::{AppMessage, AppProtocolLimits, decode_app_message, encode_app_message};
use peritus_codec::{HEADER_LEN, MAGIC};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::ClientError;

trait LocalIo: AsyncRead + AsyncWrite {}
impl<T: AsyncRead + AsyncWrite> LocalIo for T {}
type BoxedLocalIo = Pin<Box<dyn LocalIo + Send>>;

pub struct FrameStream {
    stream: BoxedLocalIo,
    limits: AppProtocolLimits,
}

impl FrameStream {
    pub(crate) async fn connect(endpoint: &OsStr) -> Result<Self, ClientError> {
        Ok(Self { stream: connect_local(endpoint).await?, limits: AppProtocolLimits::PRODUCTION })
    }

    pub(crate) const fn set_limits(&mut self, limits: AppProtocolLimits) {
        self.limits = limits;
    }

    pub(crate) const fn limits(&self) -> AppProtocolLimits {
        self.limits
    }

    pub(crate) async fn read(&mut self) -> Result<AppMessage, ClientError> {
        let mut header = [0_u8; HEADER_LEN];
        self.stream
            .read_exact(&mut header)
            .await
            .map_err(|error| ClientError::connection("read daemon frame", error.to_string()))?;
        let payload_len = checked_payload_length(&header, self.limits)?;
        let mut frame = Vec::with_capacity(HEADER_LEN + payload_len);
        frame.extend_from_slice(&header);
        frame.resize(HEADER_LEN + payload_len, 0);
        self.stream
            .read_exact(&mut frame[HEADER_LEN..])
            .await
            .map_err(|error| ClientError::connection("read daemon payload", error.to_string()))?;
        decode_app_message(&frame, self.limits).map_err(Into::into)
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

fn checked_payload_length(
    header: &[u8; HEADER_LEN],
    limits: AppProtocolLimits,
) -> Result<usize, ClientError> {
    if header[..4] != MAGIC {
        return Err(ClientError::protocol("read daemon frame", "invalid PRTS frame magic"));
    }
    let payload_len =
        usize::try_from(u32::from_be_bytes([header[12], header[13], header[14], header[15]]))
            .map_err(|_| {
                ClientError::protocol("read daemon frame", "payload length cannot be represented")
            })?;
    let codec = limits.codec();
    if payload_len > codec.max_payload_bytes
        || HEADER_LEN.checked_add(payload_len).is_none_or(|size| size > codec.max_frame_bytes)
    {
        return Err(ClientError::protocol(
            "read daemon frame",
            "declared payload exceeds negotiated limits",
        ));
    }
    Ok(payload_len)
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
    use super::{HEADER_LEN, MAGIC, checked_payload_length};
    use peritus_app_protocol::AppProtocolLimits;

    #[test]
    fn invalid_magic_is_rejected_before_payload_allocation() {
        let header = [0; HEADER_LEN];
        let error = checked_payload_length(&header, AppProtocolLimits::PRODUCTION).unwrap_err();
        assert!(error.detail().contains("magic"));
    }

    #[test]
    fn oversized_payload_is_rejected_before_payload_allocation() {
        let mut header = [0; HEADER_LEN];
        header[..4].copy_from_slice(&MAGIC);
        header[12..16].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(checked_payload_length(&header, AppProtocolLimits::PRODUCTION).is_err());
    }

    #[test]
    fn bounded_payload_length_is_preserved() {
        let mut header = [0; HEADER_LEN];
        header[..4].copy_from_slice(&MAGIC);
        header[12..16].copy_from_slice(&128_u32.to_be_bytes());
        assert_eq!(checked_payload_length(&header, AppProtocolLimits::PRODUCTION).unwrap(), 128,);
    }
}
