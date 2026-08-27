//! Canonical binary codecs and closed dispatcher for all A3 application families.

mod artifact;
mod command;
mod control;
mod daemon;
mod error;
mod event;
mod hello;
mod primitive;
mod prompt;
mod request;
mod response;
mod subscription;
mod terminal;

use crate::{
    APP_SCHEMA_V1, AppEventEnvelope, AppProtocolError, AppProtocolLimits, AppRequestEnvelope,
    AppResponseEnvelope, CLIENT_HELLO_FAMILY, CONTROL_FAMILY, ClientHello, ControlEnvelope,
    EVENT_FAMILY, REQUEST_FAMILY, RESPONSE_FAMILY, SERVER_HELLO_FAMILY, ServerHello,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, decode_frame, encode_message};

/// Closed typed value produced by dispatching one complete A3 PRTS frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppMessage {
    /// Client negotiation input, family 94.
    ClientHello(ClientHello),
    /// Server negotiation output, family 95.
    ServerHello(ServerHello),
    /// Application request, family 96.
    Request(AppRequestEnvelope),
    /// Terminal application response, family 97.
    Response(AppResponseEnvelope),
    /// Application event, family 98.
    Event(AppEventEnvelope),
    /// Application control, family 99.
    Control(ControlEnvelope),
}

impl AppMessage {
    /// Returns the permanently assigned PRTS family tag.
    #[must_use]
    pub const fn family(&self) -> u16 {
        match self {
            Self::ClientHello(_) => CLIENT_HELLO_FAMILY,
            Self::ServerHello(_) => SERVER_HELLO_FAMILY,
            Self::Request(_) => REQUEST_FAMILY,
            Self::Response(_) => RESPONSE_FAMILY,
            Self::Event(_) => EVENT_FAMILY,
            Self::Control(_) => CONTROL_FAMILY,
        }
    }
}

/// Encodes one closed application message as a complete canonical PRTS frame.
///
/// # Errors
///
/// Returns a stable codec-derived application error when the supplied codec limits cannot contain
/// the complete canonical frame or when duplicated envelope bindings are inconsistent.
pub fn encode_app_message(
    message: &AppMessage,
    limits: AppProtocolLimits,
) -> Result<Vec<u8>, AppProtocolError> {
    let codec = limits.codec();
    let encoded = match message {
        AppMessage::ClientHello(value) => encode_message(value, codec),
        AppMessage::ServerHello(value) => encode_message(value, codec),
        AppMessage::Request(value) => encode_message(value, codec),
        AppMessage::Response(value) => encode_message(value, codec),
        AppMessage::Event(value) => encode_message(value, codec),
        AppMessage::Control(value) => encode_message(value, codec),
    }
    .map_err(AppProtocolError::from_codec)?;
    // Trait-level encoding receives only CodecLimits. Reusing the strict negotiated decoder here
    // applies every A3 collection, flow-control, chunk, and diagnostic ceiling before bytes leave
    // the public dispatcher.
    decode_app_message(&encoded, limits)?;
    Ok(encoded)
}

/// Encodes one prompt binding into its exact canonical semantic bytes.
///
/// The returned bytes omit an application envelope and PRTS frame so a durable target registry
/// can bind the prompt before any connection-specific publication. They are accepted only after
/// the strict decoder reproduces the same checked binding under the supplied limits.
///
/// # Errors
///
/// Returns a stable codec-derived application error when the binding exceeds the supplied limits.
pub fn encode_prompt_binding_value(
    binding: &crate::PromptBinding,
    limits: AppProtocolLimits,
) -> Result<Vec<u8>, AppProtocolError> {
    let mut writer = CanonicalWriter::new(limits.codec());
    prompt::write_prompt_binding(&mut writer, binding).map_err(AppProtocolError::from_codec)?;
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, limits.codec());
    let decoded =
        prompt::read_prompt_binding(&mut reader, limits).map_err(AppProtocolError::from_codec)?;
    reader.finish().map_err(AppProtocolError::from_codec)?;
    if decoded != *binding {
        return Err(AppProtocolError::new(crate::AppErrorCode::MalformedFrame, None));
    }
    Ok(bytes)
}

/// Decodes and completely consumes one canonical prompt binding value.
///
/// These envelope-free bytes let a durable prompt owner reconstruct an awaiting challenge after
/// process restart.
///
/// # Errors
///
/// Returns a stable protocol error when the value is malformed, noncanonical, or out of bounds.
pub fn decode_prompt_binding_value(
    bytes: &[u8],
    limits: AppProtocolLimits,
) -> Result<crate::PromptBinding, AppProtocolError> {
    let mut reader = CanonicalReader::new(bytes, limits.codec());
    let binding =
        prompt::read_prompt_binding(&mut reader, limits).map_err(AppProtocolError::from_codec)?;
    reader.finish().map_err(AppProtocolError::from_codec)?;
    if encode_prompt_binding_value(&binding, limits)? != bytes {
        return Err(AppProtocolError::new(crate::AppErrorCode::MalformedFrame, None));
    }
    Ok(binding)
}

/// Decodes and completely consumes one canonical A3 PRTS frame under negotiated limits.
///
/// The family and schema are checked before payload dispatch. Every closed tag and semantic
/// constructor is validated; exact embedded B3 frames remain byte-for-byte unchanged.
///
/// # Errors
///
/// Returns stable distinct errors for unsupported family/schema, unknown tags, malformed or
/// noncanonical values, truncation, trailing bytes, and resource-limit violations.
pub fn decode_app_message(
    bytes: &[u8],
    limits: AppProtocolLimits,
) -> Result<AppMessage, AppProtocolError> {
    let frame = decode_frame(bytes, limits.codec()).map_err(AppProtocolError::from_codec)?;
    let header = frame.header();
    if !matches!(
        header.family(),
        CLIENT_HELLO_FAMILY
            | SERVER_HELLO_FAMILY
            | REQUEST_FAMILY
            | RESPONSE_FAMILY
            | EVENT_FAMILY
            | CONTROL_FAMILY
    ) {
        return Err(AppProtocolError::new(crate::AppErrorCode::UnsupportedFamily, None));
    }
    if header.schema_version() != APP_SCHEMA_V1 {
        return Err(AppProtocolError::new(crate::AppErrorCode::UnsupportedSchema, None));
    }
    let mut reader = CanonicalReader::new(frame.payload(), limits.codec());
    let message = match header.family() {
        CLIENT_HELLO_FAMILY => {
            AppMessage::ClientHello(hello::read_client_hello(&mut reader, limits)?)
        }
        SERVER_HELLO_FAMILY => {
            AppMessage::ServerHello(hello::read_server_hello(&mut reader, limits)?)
        }
        REQUEST_FAMILY => AppMessage::Request(request::read_request(&mut reader, limits)?),
        RESPONSE_FAMILY => AppMessage::Response(response::read_response(&mut reader, limits)?),
        EVENT_FAMILY => AppMessage::Event(event::read_event(&mut reader, limits)?),
        CONTROL_FAMILY => AppMessage::Control(control::read_control(&mut reader, limits)?),
        _ => return Err(AppProtocolError::new(crate::AppErrorCode::UnsupportedFamily, None)),
    };
    reader.finish().map_err(AppProtocolError::from_codec)?;
    Ok(message)
}

#[cfg(test)]
mod tests;
