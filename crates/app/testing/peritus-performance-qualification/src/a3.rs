//! Small blocking A3 client for the disposable Linux qualification subject.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use peritus_app_protocol::{
    AppEventEnvelope, AppEventPayload, AppMessage, AppProtocolLimits, AppRequestEnvelope,
    AppRequestPayload, AppResponseEnvelope, ClientHello, ControlEnvelope, ControlPayload,
    CorrelationId, HeartbeatReply, NegotiationOutcome, ProtocolContext, ProtocolId, RequestId,
    VersionRange, decode_app_message, encode_app_message,
};
use peritus_codec::{HEADER_LEN, MAGIC};
use peritus_types::SessionId;

use crate::{SubjectError, identity::IdentitySource};

const IO_BOUND: Duration = Duration::from_secs(30);

pub struct A3Client {
    stream: UnixStream,
    context: ProtocolContext,
    limits: AppProtocolLimits,
}

impl A3Client {
    pub fn connect(
        endpoint: &Path,
        requested_session: Option<SessionId>,
        identities: &mut IdentitySource,
    ) -> Result<Self, SubjectError> {
        let mut stream = UnixStream::connect(endpoint)?;
        stream.set_read_timeout(Some(IO_BOUND))?;
        stream.set_write_timeout(Some(IO_BOUND))?;
        let protocol_id = identities.next(ProtocolId::new)?;
        let hello = ClientHello::new_with_session(
            protocol_id,
            requested_session,
            vec![VersionRange::new(1, 0, 0)?],
            Vec::new(),
            Vec::new(),
            AppProtocolLimits::PRODUCTION,
            format!("peritus-performance-qualification/{}", env!("CARGO_PKG_VERSION")),
        )?;
        write_message(&mut stream, &AppMessage::ClientHello(hello), AppProtocolLimits::PRODUCTION)?;
        let AppMessage::ServerHello(server) =
            read_message(&mut stream, AppProtocolLimits::PRODUCTION)?
        else {
            return Err(SubjectError::UnexpectedResponse(
                "daemon did not answer ClientHello with ServerHello".to_owned(),
            ));
        };
        if server.protocol_id() != protocol_id {
            return Err(SubjectError::UnexpectedResponse(
                "daemon echoed a different protocol identity".to_owned(),
            ));
        }
        let negotiated = match server.outcome() {
            NegotiationOutcome::Compatible(value) | NegotiationOutcome::Downgraded(value) => value,
            NegotiationOutcome::Incompatible(reason) => {
                return Err(SubjectError::UnexpectedResponse(format!(
                    "daemon rejected A3 negotiation: {reason:?}"
                )));
            }
        };
        let session = server.established_session().ok_or_else(|| {
            SubjectError::UnexpectedResponse(
                "compatible A3 negotiation established no durable session".to_owned(),
            )
        })?;
        Ok(Self {
            stream,
            context: ProtocolContext::new(protocol_id, negotiated.version(), session),
            limits: negotiated.limits(),
        })
    }

    pub const fn session_id(&self) -> SessionId {
        self.context.session_id()
    }

    pub fn request(
        &mut self,
        request_id: RequestId,
        correlation_id: CorrelationId,
        payload: AppRequestPayload,
        identities: &mut IdentitySource,
    ) -> Result<AppResponseEnvelope, SubjectError> {
        let request = AppRequestEnvelope::new(self.context, request_id, correlation_id, payload)?;
        write_message(&mut self.stream, &AppMessage::Request(request), self.limits)?;
        loop {
            match read_message(&mut self.stream, self.limits)? {
                AppMessage::Response(response)
                    if response.request_id() == request_id
                        && response.correlation_id() == correlation_id =>
                {
                    if response.context() != self.context {
                        return Err(SubjectError::UnexpectedResponse(
                            "daemon response context differed from the negotiated context"
                                .to_owned(),
                        ));
                    }
                    return Ok(response);
                }
                AppMessage::Event(event) if heartbeat(&event).is_some() => {
                    self.reply_heartbeat(&event, identities)?;
                }
                AppMessage::Response(_) => {
                    return Err(SubjectError::UnexpectedResponse(
                        "daemon returned a response for another request".to_owned(),
                    ));
                }
                _ => {
                    return Err(SubjectError::UnexpectedResponse(
                        "daemon returned an unexpected frame while awaiting a response".to_owned(),
                    ));
                }
            }
        }
    }

    fn reply_heartbeat(
        &mut self,
        event: &AppEventEnvelope,
        identities: &mut IdentitySource,
    ) -> Result<(), SubjectError> {
        let heartbeat = heartbeat(event).expect("caller checked heartbeat event");
        let correlation = identities.next(CorrelationId::new)?;
        let reply = ControlEnvelope::new(
            self.context,
            correlation,
            ControlPayload::HeartbeatReply(HeartbeatReply::new(
                heartbeat.heartbeat_id(),
                heartbeat.sequence(),
            )),
        );
        write_message(&mut self.stream, &AppMessage::Control(reply), self.limits)
    }
}

const fn heartbeat(event: &AppEventEnvelope) -> Option<&peritus_app_protocol::DaemonHeartbeat> {
    match event.payload() {
        AppEventPayload::Heartbeat(heartbeat) => Some(heartbeat),
        _ => None,
    }
}

fn write_message(
    stream: &mut UnixStream,
    message: &AppMessage,
    limits: AppProtocolLimits,
) -> Result<(), SubjectError> {
    let bytes = encode_app_message(message, limits)?;
    stream.write_all(&bytes)?;
    stream.flush()?;
    Ok(())
}

fn read_message(
    stream: &mut UnixStream,
    limits: AppProtocolLimits,
) -> Result<AppMessage, SubjectError> {
    let mut header = [0_u8; HEADER_LEN];
    stream.read_exact(&mut header)?;
    if header[..4] != MAGIC {
        return Err(SubjectError::UnexpectedResponse(
            "daemon frame had invalid PRTS magic".to_owned(),
        ));
    }
    let payload_len = usize::try_from(u32::from_be_bytes(
        header[12..16].try_into().expect("fixed four-byte A3 payload length field"),
    ))
    .map_err(|_| SubjectError::UnexpectedResponse("A3 payload length overflowed".to_owned()))?;
    let frame_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or_else(|| SubjectError::UnexpectedResponse("A3 frame length overflowed".to_owned()))?;
    if payload_len > limits.codec().max_payload_bytes || frame_len > limits.codec().max_frame_bytes
    {
        return Err(SubjectError::UnexpectedResponse(
            "daemon frame exceeded negotiated A3 limits".to_owned(),
        ));
    }
    let mut frame = Vec::with_capacity(frame_len);
    frame.extend_from_slice(&header);
    frame.resize(frame_len, 0);
    stream.read_exact(&mut frame[HEADER_LEN..])?;
    decode_app_message(&frame, limits).map_err(SubjectError::Protocol)
}
