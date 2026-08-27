//! Blocking public A3 wire client used by subprocess conformance cases.

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, ClientHello,
    ControlEnvelope, ControlPayload, CorrelationId, NegotiationOutcome, ProtocolContext, RequestId,
    ServerHello, decode_app_message, encode_app_message,
};
use peritus_codec::HEADER_LEN;

const IO_BOUND: Duration = Duration::from_secs(5);

pub(super) struct WireClient {
    stream: UnixStream,
    context: ProtocolContext,
    limits: AppProtocolLimits,
}

impl WireClient {
    #[allow(
        clippy::needless_pass_by_value,
        reason = "the test client consumes one complete hello as the opening handshake input"
    )]
    pub(super) fn establish(endpoint: &Path, hello: ClientHello) -> io::Result<Self> {
        let (stream, server) = exchange_hello(endpoint, &hello)?;
        let negotiated = match server.outcome() {
            NegotiationOutcome::Compatible(value) | NegotiationOutcome::Downgraded(value) => value,
            NegotiationOutcome::Incompatible(reason) => {
                return Err(io::Error::other(format!("unexpected incompatibility: {reason:?}")));
            }
        };
        let session = server
            .established_session()
            .ok_or_else(|| io::Error::other("compatible hello returned no durable session"))?;
        let context = ProtocolContext::new(hello.protocol_id(), negotiated.version(), session);
        Ok(Self { stream, context, limits: negotiated.limits() })
    }

    pub(super) const fn context(&self) -> ProtocolContext {
        self.context
    }

    pub(super) fn write(&mut self, message: &AppMessage) -> io::Result<()> {
        write_message(&mut self.stream, message, self.limits)
    }

    pub(super) fn read(&mut self) -> io::Result<AppMessage> {
        read_message(&mut self.stream, self.limits)
    }

    pub(super) fn request(
        &mut self,
        identity: u8,
        payload: AppRequestPayload,
    ) -> io::Result<AppMessage> {
        self.request_with_encoding_limits(identity, payload, self.limits)
    }

    pub(super) fn request_with_encoding_limits(
        &mut self,
        identity: u8,
        payload: AppRequestPayload,
        encoding_limits: AppProtocolLimits,
    ) -> io::Result<AppMessage> {
        let request = AppRequestEnvelope::new(
            self.context,
            RequestId::new([identity; 16]).map_err(super::debug_error)?,
            CorrelationId::new([identity.wrapping_add(64); 16]).map_err(super::debug_error)?,
            payload,
        )
        .map_err(super::debug_error)?;
        write_message(&mut self.stream, &AppMessage::Request(request), encoding_limits)?;
        self.read()
    }

    pub(super) fn request_bound(
        &mut self,
        payload: AppRequestPayload,
        request_id: RequestId,
        correlation_id: CorrelationId,
    ) -> io::Result<AppMessage> {
        let request = AppRequestEnvelope::new(self.context, request_id, correlation_id, payload)
            .map_err(super::debug_error)?;
        self.write(&AppMessage::Request(request))?;
        self.read()
    }

    pub(super) fn control(&mut self, identity: u8, payload: ControlPayload) -> io::Result<()> {
        let control = ControlEnvelope::new(
            self.context,
            CorrelationId::new([identity; 16]).map_err(super::debug_error)?,
            payload,
        );
        self.write(&AppMessage::Control(control))
    }
}

pub(super) fn exchange_hello(
    endpoint: &Path,
    hello: &ClientHello,
) -> io::Result<(UnixStream, ServerHello)> {
    let mut stream = UnixStream::connect(endpoint)?;
    stream.set_read_timeout(Some(IO_BOUND))?;
    stream.set_write_timeout(Some(IO_BOUND))?;
    write_message(
        &mut stream,
        &AppMessage::ClientHello(hello.clone()),
        AppProtocolLimits::PRODUCTION,
    )?;
    let AppMessage::ServerHello(server) = read_message(&mut stream, AppProtocolLimits::PRODUCTION)?
    else {
        return Err(io::Error::other("hello did not return ServerHello"));
    };
    Ok((stream, server))
}

pub(super) fn raw_connect(endpoint: &Path) -> io::Result<UnixStream> {
    let stream = UnixStream::connect(endpoint)?;
    stream.set_read_timeout(Some(IO_BOUND))?;
    stream.set_write_timeout(Some(IO_BOUND))?;
    Ok(stream)
}

fn write_message(
    stream: &mut UnixStream,
    message: &AppMessage,
    limits: AppProtocolLimits,
) -> io::Result<()> {
    let bytes = encode_app_message(message, limits).map_err(super::debug_error)?;
    stream.write_all(&bytes)
}

fn read_message(stream: &mut UnixStream, limits: AppProtocolLimits) -> io::Result<AppMessage> {
    let mut header = [0_u8; HEADER_LEN];
    stream.read_exact(&mut header)?;
    let payload_len = usize::try_from(u32::from_be_bytes(
        header[12..16].try_into().expect("fixed PRTS payload length field"),
    ))
    .map_err(super::debug_error)?;
    let frame_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or_else(|| io::Error::other("A3 frame length overflow"))?;
    if frame_len > limits.codec().max_frame_bytes {
        return Err(io::Error::other("A3 response exceeded negotiated frame limit"));
    }
    let mut bytes = Vec::with_capacity(frame_len);
    bytes.extend_from_slice(&header);
    bytes.resize(frame_len, 0);
    stream.read_exact(&mut bytes[HEADER_LEN..])?;
    decode_app_message(&bytes, limits).map_err(super::debug_error)
}
