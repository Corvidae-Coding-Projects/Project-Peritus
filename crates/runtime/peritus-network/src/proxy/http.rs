//! Bounded HTTP/1 request and response head parsing.

use std::io::{Read, Write};

use peritus_sandbox::{DnsName, NetworkHost, Transport};

use crate::{DestinationRequest, NetworkError, NetworkErrorKind, NetworkOperation, RecoveryClass};

pub(super) struct RequestHead {
    pub(super) method: String,
    pub(super) version: String,
    pub(super) destination: DestinationRequest,
    pub(super) origin_target: String,
    pub(super) headers: Vec<(String, Vec<u8>)>,
    pub(super) content_length: u64,
    routing_header: String,
}

impl RequestHead {
    pub(super) fn routing_authorization(&self) -> &str {
        &self.routing_header
    }

    pub(super) fn encode(
        &self,
        credential: Option<(&str, &[u8])>,
    ) -> Result<Vec<u8>, NetworkError> {
        let mut out = Vec::new();
        append(&mut out, self.method.as_bytes())?;
        append(&mut out, b" ")?;
        append(&mut out, self.origin_target.as_bytes())?;
        append(&mut out, b" ")?;
        append(&mut out, self.version.as_bytes())?;
        append(&mut out, b"\r\n")?;
        for (name, value) in &self.headers {
            if name.eq_ignore_ascii_case("proxy-authorization")
                || name.eq_ignore_ascii_case("proxy-connection")
                || name.eq_ignore_ascii_case("connection")
                || name.eq_ignore_ascii_case("host")
            {
                continue;
            }
            append(&mut out, name.as_bytes())?;
            append(&mut out, b": ")?;
            append(&mut out, value)?;
            append(&mut out, b"\r\n")?;
        }
        append(&mut out, b"Host: ")?;
        append(&mut out, host_header(&self.destination).as_bytes())?;
        append(&mut out, b"\r\n")?;
        if let Some((name, value)) = credential {
            append(&mut out, name.as_bytes())?;
            append(&mut out, b": ")?;
            append(&mut out, value)?;
            append(&mut out, b"\r\n")?;
        }
        append(&mut out, b"Connection: close\r\n\r\n")?;
        Ok(out)
    }

    pub(super) fn follow(&mut self, target: &crate::RedirectTarget) {
        self.destination.clone_from(target.request());
        target.path_and_query().clone_into(&mut self.origin_target);
        self.content_length = 0;
    }
}

fn host_header(destination: &DestinationRequest) -> String {
    let host = match destination.host() {
        NetworkHost::Dns(name) => name.as_str().to_owned(),
        NetworkHost::Ip(std::net::IpAddr::V4(address)) => address.to_string(),
        NetworkHost::Ip(std::net::IpAddr::V6(address)) => format!("[{address}]"),
    };
    if destination.port() == 80 { host } else { format!("{host}:{}", destination.port()) }
}

pub(super) struct ResponseHead {
    pub(super) bytes: Vec<u8>,
    pub(super) status: u16,
    pub(super) content_length: Option<u64>,
    pub(super) location: Option<String>,
}

pub(super) fn read_request(
    stream: &mut impl Read,
    maximum: u32,
) -> Result<RequestHead, NetworkError> {
    let bytes = read_head(stream, maximum)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| protocol_error("HTTP request head is not UTF-8"))?;
    let mut lines = text[..text.len().saturating_sub(4)].split("\r\n");
    let first = lines.next().ok_or_else(|| protocol_error("HTTP request line is missing"))?;
    let mut parts = first.split(' ');
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || method.is_empty()
        || target.is_empty()
        || !matches!(version, "HTTP/1.0" | "HTTP/1.1")
        || !method.bytes().all(|byte| byte.is_ascii_uppercase())
    {
        return Err(protocol_error("HTTP request line is malformed"));
    }
    let mut headers = Vec::new();
    let mut routing_header = None;
    let mut host_header = None;
    let mut content_length = 0_u64;
    for line in lines {
        let (name, value) =
            line.split_once(':').ok_or_else(|| protocol_error("HTTP header is malformed"))?;
        if name.is_empty() || name.len() > 128 || !name.bytes().all(is_header_name_byte) {
            return Err(protocol_error("HTTP header name is invalid"));
        }
        let value = value.trim_matches([' ', '\t']);
        if value.bytes().any(|byte| byte.is_ascii_control() && byte != b'\t') {
            return Err(protocol_error("HTTP header value contains control bytes"));
        }
        if name.eq_ignore_ascii_case("proxy-authorization") {
            if routing_header.replace(value.to_owned()).is_some() {
                return Err(protocol_error("proxy routing header is duplicated"));
            }
        } else if name.eq_ignore_ascii_case("host") {
            host_header = Some(value.to_owned());
        } else if name.eq_ignore_ascii_case("content-length") {
            content_length =
                value.parse().map_err(|_| protocol_error("content length is invalid"))?;
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(protocol_error("chunked request bodies are unsupported"));
        }
        headers.push((name.to_owned(), value.as_bytes().to_vec()));
    }
    let routing_header =
        routing_header.ok_or_else(|| credential_error("proxy routing header is missing"))?;
    let (destination, origin_target) = destination(method, target, host_header.as_deref())?;
    Ok(RequestHead {
        method: method.to_owned(),
        version: version.to_owned(),
        destination,
        origin_target,
        headers,
        content_length,
        routing_header,
    })
}

pub(super) fn read_response(
    stream: &mut impl Read,
    maximum: u32,
) -> Result<ResponseHead, NetworkError> {
    let bytes = read_head(stream, maximum)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| protocol_error("HTTP response head is not UTF-8"))?;
    let mut lines = text[..text.len().saturating_sub(4)].split("\r\n");
    let first = lines.next().ok_or_else(|| protocol_error("HTTP response status is missing"))?;
    let mut parts = first.split(' ');
    let version = parts.next().unwrap_or_default();
    let status = parts
        .next()
        .ok_or_else(|| protocol_error("HTTP response status is missing"))?
        .parse::<u16>()
        .map_err(|_| protocol_error("HTTP response status is invalid"))?;
    if !matches!(version, "HTTP/1.0" | "HTTP/1.1") || !(100..=599).contains(&status) {
        return Err(protocol_error("HTTP response status line is invalid"));
    }
    let mut content_length = None;
    let mut location = None;
    for line in lines {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| protocol_error("HTTP response header is malformed"))?;
        let value = value.trim_matches([' ', '\t']);
        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(
                value.parse().map_err(|_| protocol_error("response content length is invalid"))?,
            );
        }
        if name.eq_ignore_ascii_case("location") {
            location = Some(value.to_owned());
        }
    }
    Ok(ResponseHead { bytes, status, content_length, location })
}

fn destination(
    method: &str,
    target: &str,
    host_header: Option<&str>,
) -> Result<(DestinationRequest, String), NetworkError> {
    if method == "CONNECT" {
        let (host, port) = parse_authority(target, None)?;
        return Ok((request(host, port)?, String::new()));
    }
    if let Some(remainder) = target.strip_prefix("http://") {
        let split = remainder.find('/').unwrap_or(remainder.len());
        let (host, port) = parse_authority(&remainder[..split], Some(80))?;
        let path = if split == remainder.len() { "/" } else { &remainder[split..] };
        return Ok((request(host, port)?, path.to_owned()));
    }
    if target.starts_with('/') {
        let (host, port) = parse_authority(
            host_header.ok_or_else(|| protocol_error("origin-form request has no Host header"))?,
            Some(80),
        )?;
        return Ok((request(host, port)?, target.to_owned()));
    }
    Err(protocol_error("proxy request target is unsupported"))
}

fn request(host: &str, port: u16) -> Result<DestinationRequest, NetworkError> {
    let host = match host.parse() {
        Ok(address) => NetworkHost::Ip(address),
        Err(_) => NetworkHost::Dns(
            DnsName::new(host).map_err(|_| protocol_error("destination host is invalid"))?,
        ),
    };
    DestinationRequest::new(host, Transport::Tcp, port)
}

fn parse_authority(authority: &str, default: Option<u16>) -> Result<(&str, u16), NetworkError> {
    if let Some(rest) = authority.strip_prefix('[') {
        let close = rest.find(']').ok_or_else(|| protocol_error("IPv6 authority is malformed"))?;
        let host = &rest[..close];
        let suffix = &rest[close + 1..];
        let port = suffix
            .strip_prefix(':')
            .and_then(|value| value.parse().ok())
            .or(default)
            .ok_or_else(|| protocol_error("authority port is missing"))?;
        return Ok((host, port));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => {
            let port = port.parse().map_err(|_| protocol_error("authority port is invalid"))?;
            if port == 0 {
                return Err(protocol_error("authority port is zero"));
            }
            Ok((host, port))
        }
        _ => default
            .map(|port| (authority, port))
            .ok_or_else(|| protocol_error("authority port is missing")),
    }
}

fn read_head(stream: &mut impl Read, maximum: u32) -> Result<Vec<u8>, NetworkError> {
    let maximum = usize::try_from(maximum).unwrap_or(usize::MAX);
    let mut bytes = Vec::with_capacity(maximum.min(8_192));
    let mut byte = [0_u8; 1];
    while bytes.len() < maximum {
        stream.read_exact(&mut byte).map_err(|_| io_error("HTTP head could not be read"))?;
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            return Ok(bytes);
        }
    }
    Err(protocol_error("HTTP head exceeds its byte ceiling"))
}

fn append(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), NetworkError> {
    if out.len().saturating_add(bytes.len()) > 1024 * 1024 {
        return Err(protocol_error("rewritten HTTP head exceeds its bound"));
    }
    out.write_all(bytes).map_err(|_| io_error("HTTP head cannot be assembled"))
}

const fn is_header_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

const fn protocol_error(detail: &'static str) -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::InvalidInput,
        NetworkOperation::Proxy,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

const fn credential_error(detail: &'static str) -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Credential,
        NetworkOperation::Credential,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

const fn io_error(detail: &'static str) -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Io,
        NetworkOperation::Relay,
        RecoveryClass::CancelAndJoin,
        detail,
    )
}
