//! Fail-closed validation and private wire projection for both compatible dialects.

mod chat;
mod hosted;
mod responses;
mod validation;
mod value;

use std::collections::BTreeMap;
use std::io::{self, Write as _};

use base64::Engine as _;
use peritus_model_protocol::{MediaInput, ModelRequest, WireDialect};
use peritus_provider_core::{
    Credential, Header, HeaderName, HttpHeaders, HttpMethod, HttpRequest, ProviderCoreError,
};
use serde_json::Value;

use crate::{CompatibleConfig, CompatibleProfile, error};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum WirePathStep {
    Key(String),
    Index(usize),
}

#[derive(Clone, Copy)]
pub(super) struct InlineMediaSource<'a> {
    media_type: &'a str,
    bytes: &'a [u8],
}

pub(super) struct InlineMedia<'a> {
    pub source: InlineMediaSource<'a>,
    pub path: Vec<WirePathStep>,
}

pub(super) struct WireRequest<'a> {
    value: Value,
    inline_media: Vec<InlineMedia<'a>>,
}

impl<'a> WireRequest<'a> {
    pub(super) const fn new(value: Value, inline_media: Vec<InlineMedia<'a>>) -> Self {
        Self { value, inline_media }
    }
}

pub(super) struct PreparedRequest {
    body: Vec<u8>,
}

impl PreparedRequest {
    pub(super) fn len(&self) -> usize {
        self.body.len()
    }
}

pub fn validate(
    profile: &CompatibleProfile,
    request: &ModelRequest,
) -> Result<(), ProviderCoreError> {
    validate_for_service(profile, request, None)
}

pub fn validate_for_service(
    profile: &CompatibleProfile,
    request: &ModelRequest,
    service: Option<peritus_provider_core::hosted::HostedService>,
) -> Result<(), ProviderCoreError> {
    validation::validate(profile, request, service)?;
    match profile.provider_profile().dialect() {
        WireDialect::CompatibleResponses => {
            if request.options().generation().seed().is_some()
                || !request.options().generation().stop_sequences().is_empty()
            {
                return Err(error::invalid(
                    "Responses compatibility does not map seed or stop sequences",
                ));
            }
        }
        WireDialect::CompatibleChatCompletions => {
            if request.options().generation().stop_sequences().len() > 4 {
                return Err(error::invalid(
                    "Chat Completions compatibility accepts at most four stop sequences",
                ));
            }
        }
        _ => return Err(error::configuration("compatible profile dialect changed")),
    }
    Ok(())
}

pub fn encode(
    profile: &CompatibleProfile,
    request: &ModelRequest,
) -> Result<Vec<u8>, ProviderCoreError> {
    validate(profile, request)?;
    let wire = project(profile, request, None)?;
    encode_wire(&wire, usize::MAX)
}

pub(super) fn prepare(
    config: &CompatibleConfig,
    profile: &CompatibleProfile,
    request: &ModelRequest,
) -> Result<PreparedRequest, ProviderCoreError> {
    validate_for_service(profile, request, config.hosted_service())?;
    let wire = project(profile, request, config.hosted_service())?;
    let body = encode_wire(&wire, config.http_limits().max_request_body_bytes())?;
    Ok(PreparedRequest { body })
}

pub(super) fn http_request(
    config: &CompatibleConfig,
    prepared: &PreparedRequest,
    credential: Credential,
) -> Result<HttpRequest, ProviderCoreError> {
    let mut values = vec![
        config.auth().project(credential)?,
        Header::new(HeaderName::new("content-type".to_owned())?, b"application/json".to_vec())?,
        Header::new(HeaderName::new("accept".to_owned())?, b"text/event-stream".to_vec())?,
    ];
    for header in config.fixed_headers() {
        values.push(header.project()?);
    }
    let headers = HttpHeaders::new(values, config.http_limits())?;
    HttpRequest::new(
        HttpMethod::Post,
        config.endpoint().clone(),
        headers,
        prepared.body.clone(),
        config.http_limits(),
    )
}

pub(super) fn media_url(
    media: &MediaInput,
) -> Result<(Value, Option<InlineMediaSource<'_>>), ProviderCoreError> {
    if let Some(bytes) = media.inline_bytes_for_wire() {
        return Ok((
            Value::Null,
            Some(InlineMediaSource { media_type: media.media_type().as_str(), bytes }),
        ));
    }
    if let Some((_, reference)) = media.reference_for_wire() {
        return Ok((Value::String(reference.to_owned()), None));
    }
    Err(error::invalid("compatible image input was unresolved"))
}

fn project<'a>(
    profile: &CompatibleProfile,
    request: &'a ModelRequest,
    service: Option<peritus_provider_core::hosted::HostedService>,
) -> Result<WireRequest<'a>, ProviderCoreError> {
    match profile.provider_profile().dialect() {
        WireDialect::CompatibleResponses => responses::project(request),
        WireDialect::CompatibleChatCompletions => chat::project(request, service),
        _ => Err(error::configuration("compatible profile dialect changed")),
    }
}

fn encode_wire(
    wire: &WireRequest<'_>,
    maximum: usize,
) -> Result<Vec<u8>, ProviderCoreError> {
    let locations = media_locations(wire)?;
    let mut counter = CountingSink::new(maximum);
    let mut path = Vec::new();
    if write_value(&mut counter, &wire.value, &locations, &wire.inline_media, &mut path).is_err() {
        return Err(wire_error(counter.exceeded));
    }
    let exact = counter.length;
    let mut body = BodySink::new(exact);
    if write_value(&mut body, &wire.value, &locations, &wire.inline_media, &mut path).is_err()
        || body.bytes.len() != exact
    {
        return Err(wire_error(body.exceeded));
    }
    Ok(body.bytes)
}

fn media_locations(
    wire: &WireRequest<'_>,
) -> Result<BTreeMap<Vec<WirePathStep>, usize>, ProviderCoreError> {
    let mut locations = BTreeMap::new();
    for (index, media) in wire.inline_media.iter().enumerate() {
        if locations.insert(media.path.clone(), index).is_some()
            || value_at_path(&wire.value, &media.path).is_none_or(|value| !value.is_null())
        {
            return Err(error::invalid("compatible inline-media projection was inconsistent"));
        }
    }
    Ok(locations)
}

fn value_at_path<'a>(mut value: &'a Value, path: &[WirePathStep]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            WirePathStep::Key(key) => value.get(key)?,
            WirePathStep::Index(index) => value.get(*index)?,
        };
    }
    Some(value)
}

fn write_value<S: WireSink>(
    sink: &mut S,
    value: &Value,
    locations: &BTreeMap<Vec<WirePathStep>, usize>,
    media: &[InlineMedia<'_>],
    path: &mut Vec<WirePathStep>,
) -> io::Result<()> {
    if let Some(index) = locations.get(path) {
        return write_inline_media(sink, media[*index].source);
    }
    match value {
        Value::Array(values) => {
            sink.write_all(b"[")?;
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    sink.write_all(b",")?;
                }
                path.push(WirePathStep::Index(index));
                let result = write_value(sink, value, locations, media, path);
                path.pop();
                result?;
            }
            sink.write_all(b"]")
        }
        Value::Object(values) => {
            sink.write_all(b"{")?;
            for (index, (name, value)) in values.iter().enumerate() {
                if index != 0 {
                    sink.write_all(b",")?;
                }
                serde_json::to_writer(&mut *sink, name).map_err(json_io)?;
                sink.write_all(b":")?;
                path.push(WirePathStep::Key(name.clone()));
                let result = write_value(sink, value, locations, media, path);
                path.pop();
                result?;
            }
            sink.write_all(b"}")
        }
        _ => serde_json::to_writer(sink, value).map_err(json_io),
    }
}

fn write_inline_media<S: WireSink>(
    sink: &mut S,
    media: InlineMediaSource<'_>,
) -> io::Result<()> {
    sink.write_all(b"\"data:")?;
    sink.write_all(media.media_type.as_bytes())?;
    sink.write_all(b";base64,")?;
    sink.write_base64(media.bytes)?;
    sink.write_all(b"\"")
}

fn json_io(_error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "compatible JSON serialization failed")
}

fn wire_error(exceeded: bool) -> ProviderCoreError {
    if exceeded {
        ProviderCoreError::limit_exceeded(
            "compatible_request",
            "compatible wire request exceeds the selected HTTP body bound",
        )
    } else {
        error::invalid("compatible request serialization failed")
    }
}

fn base64_length(bytes: usize) -> Option<usize> {
    bytes
        .checked_div(3)
        .and_then(|groups| groups.checked_mul(4))
        .and_then(|complete| complete.checked_add(if bytes % 3 == 0 { 0 } else { 4 }))
}

trait WireSink: io::Write {
    fn write_base64(&mut self, bytes: &[u8]) -> io::Result<()>;
}

struct CountingSink {
    length: usize,
    maximum: usize,
    exceeded: bool,
}

impl CountingSink {
    const fn new(maximum: usize) -> Self {
        Self { length: 0, maximum, exceeded: false }
    }

    fn admit(&mut self, bytes: usize) -> io::Result<()> {
        let Some(next) = self.length.checked_add(bytes) else {
            self.exceeded = true;
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "wire size overflowed"));
        };
        if next > self.maximum {
            self.exceeded = true;
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "wire size exceeded"));
        }
        self.length = next;
        Ok(())
    }
}

impl io::Write for CountingSink {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.admit(buffer.len())?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl WireSink for CountingSink {
    fn write_base64(&mut self, bytes: &[u8]) -> io::Result<()> {
        let Some(length) = base64_length(bytes.len()) else {
            self.exceeded = true;
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "base64 size overflowed"));
        };
        self.admit(length)
    }
}

struct BodySink {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded: bool,
}

impl BodySink {
    fn new(maximum: usize) -> Self {
        Self { bytes: Vec::with_capacity(maximum), maximum, exceeded: false }
    }

    fn admit(&mut self, bytes: usize) -> io::Result<usize> {
        let Some(next) = self.bytes.len().checked_add(bytes) else {
            self.exceeded = true;
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "wire size overflowed"));
        };
        if next > self.maximum {
            self.exceeded = true;
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "wire size exceeded"));
        }
        Ok(next)
    }
}

impl io::Write for BodySink {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.admit(buffer.len())?;
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl WireSink for BodySink {
    fn write_base64(&mut self, bytes: &[u8]) -> io::Result<()> {
        let Some(length) = base64_length(bytes.len()) else {
            self.exceeded = true;
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "base64 size overflowed"));
        };
        let next = self.admit(length)?;
        let start = self.bytes.len();
        self.bytes.resize(next, 0);
        let written = base64::engine::general_purpose::STANDARD
            .encode_slice(bytes, &mut self.bytes[start..])
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "base64 encoding failed"))?;
        if written != length {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "base64 length changed"));
        }
        Ok(())
    }
}
