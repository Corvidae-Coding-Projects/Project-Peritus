use std::{ffi::OsStr, pin::Pin, time::Duration};

use peritus_app_protocol::{
    AppEventEnvelope, AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload,
    AppResponseEnvelope, ClientHello, ControlEnvelope, ControlPayload, CorrelationId,
    HeartbeatReply, NegotiationOutcome, ProtocolContext, ProtocolFeatureName, ProtocolId,
    RequestId, VersionRange, WellKnownProtocolFeature, decode_app_message, encode_app_message,
};
use peritus_codec::{HEADER_LEN, MAGIC};
use peritus_types::SessionId;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{error::CliError, id::generated_id};

trait LocalIo: AsyncRead + AsyncWrite {}
impl<T: AsyncRead + AsyncWrite> LocalIo for T {}
type BoxedLocalIo = Pin<Box<dyn LocalIo + Send>>;

#[derive(Clone, Copy)]
pub struct RequestIdentity {
    pub(crate) request_id: RequestId,
    pub(crate) correlation_id: CorrelationId,
}

pub struct Client {
    stream: BoxedLocalIo,
    context: ProtocolContext,
    limits: AppProtocolLimits,
    timeout: Duration,
}

impl Client {
    pub(crate) async fn connect(
        endpoint: &OsStr,
        requested_session: Option<SessionId>,
        timeout: Duration,
        required: &[WellKnownProtocolFeature],
    ) -> Result<Self, CliError> {
        let stream = tokio::time::timeout(timeout, connect_local(endpoint))
            .await
            .map_err(|_| CliError::connection("connect to daemon", "connection timed out"))??;
        let protocol_id = ProtocolId::new(generated_id(b"protocol")).map_err(|_| {
            CliError::runtime("create protocol identity", "generated zero identifier")
        })?;
        let required_features = required
            .iter()
            .copied()
            .map(ProtocolFeatureName::well_known)
            .collect::<Result<Vec<_>, _>>()?;
        let hello = ClientHello::new_with_session(
            protocol_id,
            requested_session,
            vec![VersionRange::new(1, 0, 0)?],
            required_features,
            Vec::new(),
            AppProtocolLimits::PRODUCTION,
            format!("peritus/{}", env!("CARGO_PKG_VERSION")),
        )?;
        let provisional_session = match requested_session {
            Some(session) => session,
            None => SessionId::new(generated_id(b"unestablished-session")).map_err(|_| {
                CliError::runtime(
                    "create provisional session identity",
                    "generated zero identifier",
                )
            })?,
        };
        let mut client = Self {
            stream,
            context: ProtocolContext::new(
                protocol_id,
                VersionRange::new(1, 0, 0)?.preferred(),
                provisional_session,
            ),
            limits: AppProtocolLimits::PRODUCTION,
            timeout,
        };
        client.write_message(&AppMessage::ClientHello(hello)).await?;
        let message = client.read_with_timeout("read server hello").await?;
        let AppMessage::ServerHello(server) = message else {
            return Err(CliError::negotiation("daemon did not answer with ServerHello"));
        };
        if server.protocol_id() != protocol_id {
            return Err(CliError::negotiation("daemon echoed a different protocol identity"));
        }
        let (version, limits) = match server.outcome() {
            NegotiationOutcome::Compatible(protocol) | NegotiationOutcome::Downgraded(protocol) => {
                (protocol.version(), protocol.limits())
            }
            NegotiationOutcome::Incompatible(reason) => {
                return Err(CliError::negotiation(format!("incompatible protocol: {reason:?}")));
            }
        };
        let session = server.established_session().ok_or_else(|| {
            CliError::negotiation("compatible negotiation did not establish a durable session")
        })?;
        client.context = ProtocolContext::new(protocol_id, version, session);
        client.limits = limits;
        Ok(client)
    }

    pub(crate) const fn context(&self) -> ProtocolContext {
        self.context
    }

    pub(crate) const fn limits(&self) -> AppProtocolLimits {
        self.limits
    }

    pub(crate) fn new_request_identity() -> Result<RequestIdentity, CliError> {
        let request_id = RequestId::new(generated_id(b"request")).map_err(|_| {
            CliError::runtime("create request identity", "generated zero identifier")
        })?;
        let correlation_id = CorrelationId::new(generated_id(b"correlation")).map_err(|_| {
            CliError::runtime("create correlation identity", "generated zero identifier")
        })?;
        Ok(RequestIdentity { request_id, correlation_id })
    }

    pub(crate) async fn request(
        &mut self,
        identity: RequestIdentity,
        payload: AppRequestPayload,
    ) -> Result<AppResponseEnvelope, CliError> {
        let request = AppRequestEnvelope::new(
            self.context,
            identity.request_id,
            identity.correlation_id,
            payload,
        )?;
        self.write_message(&AppMessage::Request(request)).await?;
        let response = tokio::time::timeout(self.timeout, async {
            loop {
                match self.read_message().await? {
                    AppMessage::Response(response)
                        if response.request_id() == identity.request_id
                            && response.correlation_id() == identity.correlation_id =>
                    {
                        return Ok(response);
                    }
                    AppMessage::Event(event) if heartbeat(&event).is_some() => {
                        self.reply_heartbeat(&event).await?;
                    }
                    AppMessage::Response(_) => {
                        return Err(CliError::protocol(
                            "await daemon response",
                            "received response for another request",
                        ));
                    }
                    _ => {
                        return Err(CliError::protocol(
                            "await daemon response",
                            "received an unexpected application frame",
                        ));
                    }
                }
            }
        })
        .await
        .map_err(|_| CliError::connection("await daemon response", "request timed out"))??;
        if response.context() != self.context {
            return Err(CliError::protocol(
                "validate daemon response",
                "response context differs from negotiated context",
            ));
        }
        Ok(response)
    }

    pub(crate) async fn read_event(&mut self) -> Result<AppEventEnvelope, CliError> {
        match self.read_message().await? {
            AppMessage::Event(event) => {
                if event.context() == self.context {
                    Ok(event)
                } else {
                    Err(CliError::protocol(
                        "read daemon event",
                        "event context differs from negotiated context",
                    ))
                }
            }
            AppMessage::Response(_) => Err(CliError::protocol(
                "read daemon event",
                "unexpected response while streaming events",
            )),
            _ => Err(CliError::protocol(
                "read daemon event",
                "unexpected frame family while streaming events",
            )),
        }
    }

    pub(crate) async fn write_control(
        &mut self,
        correlation_id: CorrelationId,
        payload: ControlPayload,
    ) -> Result<(), CliError> {
        let control = ControlEnvelope::new(self.context, correlation_id, payload);
        self.write_message(&AppMessage::Control(control)).await
    }

    pub(crate) async fn reply_heartbeat(
        &mut self,
        event: &AppEventEnvelope,
    ) -> Result<bool, CliError> {
        let Some(heartbeat) = heartbeat(event) else {
            return Ok(false);
        };
        let correlation =
            CorrelationId::new(generated_id(b"heartbeat-correlation")).map_err(|_| {
                CliError::runtime("create heartbeat correlation", "generated zero identifier")
            })?;
        self.write_control(
            correlation,
            ControlPayload::HeartbeatReply(HeartbeatReply::new(
                heartbeat.heartbeat_id(),
                heartbeat.sequence(),
            )),
        )
        .await?;
        Ok(true)
    }

    async fn read_with_timeout(&mut self, operation: &'static str) -> Result<AppMessage, CliError> {
        tokio::time::timeout(self.timeout, self.read_message())
            .await
            .map_err(|_| CliError::connection(operation, "operation timed out"))?
    }

    async fn read_message(&mut self) -> Result<AppMessage, CliError> {
        let mut header = [0_u8; HEADER_LEN];
        self.stream
            .read_exact(&mut header)
            .await
            .map_err(|error| CliError::connection("read daemon frame", error.to_string()))?;
        if header[..4] != MAGIC {
            return Err(CliError::protocol("read daemon frame", "invalid PRTS frame magic"));
        }
        let payload_len =
            usize::try_from(u32::from_be_bytes([header[12], header[13], header[14], header[15]]))
                .map_err(|_| {
                CliError::protocol("read daemon frame", "payload length cannot be represented")
            })?;
        let codec = self.limits.codec();
        if payload_len > codec.max_payload_bytes
            || HEADER_LEN
                .checked_add(payload_len)
                .is_none_or(|length| length > codec.max_frame_bytes)
        {
            return Err(CliError::protocol(
                "read daemon frame",
                "declared payload exceeds negotiated limits",
            ));
        }
        let mut frame = Vec::with_capacity(HEADER_LEN + payload_len);
        frame.extend_from_slice(&header);
        frame.resize(HEADER_LEN + payload_len, 0);
        self.stream
            .read_exact(&mut frame[HEADER_LEN..])
            .await
            .map_err(|error| CliError::connection("read daemon frame", error.to_string()))?;
        decode_app_message(&frame, self.limits).map_err(Into::into)
    }

    async fn write_message(&mut self, message: &AppMessage) -> Result<(), CliError> {
        let frame = encode_app_message(message, self.limits)?;
        self.stream
            .write_all(&frame)
            .await
            .map_err(|error| CliError::connection("write daemon frame", error.to_string()))?;
        self.stream
            .flush()
            .await
            .map_err(|error| CliError::connection("flush daemon frame", error.to_string()))
    }
}

const fn heartbeat(event: &AppEventEnvelope) -> Option<&peritus_app_protocol::DaemonHeartbeat> {
    match event.payload() {
        peritus_app_protocol::AppEventPayload::Heartbeat(heartbeat) => Some(heartbeat),
        _ => None,
    }
}

#[cfg(unix)]
async fn connect_local(endpoint: &OsStr) -> Result<BoxedLocalIo, CliError> {
    use std::path::Path;
    let stream = tokio::net::UnixStream::connect(Path::new(endpoint))
        .await
        .map_err(|error| CliError::connection("connect Unix endpoint", error.to_string()))?;
    Ok(Box::pin(stream))
}

#[cfg(windows)]
async fn connect_local(endpoint: &OsStr) -> Result<BoxedLocalIo, CliError> {
    let endpoint = endpoint
        .to_str()
        .ok_or_else(|| CliError::usage("Windows named-pipe endpoint must be Unicode"))?;
    let stream = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(endpoint)
        .map_err(|error| CliError::connection("connect Windows named pipe", error.to_string()))?;
    Ok(Box::pin(stream))
}

#[cfg(not(any(unix, windows)))]
async fn connect_local(_endpoint: &OsStr) -> Result<BoxedLocalIo, CliError> {
    Err(CliError::connection(
        "connect local endpoint",
        "this target has no supported Peritus local transport",
    ))
}
