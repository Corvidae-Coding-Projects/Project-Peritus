//! Disk-backed HTTP/1 request and response head processing.

use std::{
    io::Read,
    num::NonZeroU64,
    sync::Mutex,
    time::Instant,
};

use peritus_sandbox::{DnsName, NetworkHost, Transport};

use crate::{
    CancellationToken, DestinationRequest, NetworkError, NetworkErrorKind, NetworkOperation,
    RecoveryClass, RoutingToken,
};

use super::storage::TemporaryFile;

const PHYSICAL_BUFFER_BYTES: usize = 8_192;
// DnsName admits 253 canonical bytes after removing at most one trailing dot. Only the host is
// retained here; an arbitrarily long leading-zero port representation is parsed from disk.
const MAX_DNS_INPUT_BYTES: usize = 254;
// These are the two exact protocol encodings of the fixed 32-byte routing token, rather than a
// policy ceiling on HTTP field values.
const DIRECT_ROUTING_AUTHORIZATION_BYTES: u64 = 72;
const BASIC_ROUTING_AUTHORIZATION_BYTES: usize = 102;

#[derive(Clone, Copy)]
pub(super) struct Span {
    offset: u64,
    length: u64,
}

impl Span {
    const fn new(offset: u64, length: u64) -> Self {
        Self { offset, length }
    }

    fn end(self) -> Result<u64, NetworkError> {
        self.offset
            .checked_add(self.length)
            .ok_or_else(|| protocol_error("HTTP head span overflowed"))
    }

    fn subspan(self, relative_offset: u64, length: u64) -> Result<Self, NetworkError> {
        let offset = self
            .offset
            .checked_add(relative_offset)
            .ok_or_else(|| protocol_error("HTTP head span overflowed"))?;
        let end = relative_offset
            .checked_add(length)
            .ok_or_else(|| protocol_error("HTTP head span overflowed"))?;
        if end > self.length {
            return Err(protocol_error("HTTP head span is outside stored bytes"));
        }
        Ok(Self::new(offset, length))
    }
}

struct HeadCache {
    bytes: [u8; PHYSICAL_BUFFER_BYTES],
    offset: u64,
    length: usize,
}

impl HeadCache {
    const fn new() -> Self {
        Self { bytes: [0; PHYSICAL_BUFFER_BYTES], offset: 0, length: 0 }
    }
}

pub(super) struct StoredHead {
    file: TemporaryFile,
    length: u64,
    cache: Mutex<HeadCache>,
}

impl StoredHead {
    fn create() -> Result<Self, NetworkError> {
        Ok(Self {
            file: TemporaryFile::create("http-head")
                .map_err(|_| storage_error("HTTP head storage cannot be created"))?,
            length: 0,
            cache: Mutex::new(HeadCache::new()),
        })
    }

    fn append(&mut self, bytes: &[u8]) -> Result<(), NetworkError> {
        let count = u64::try_from(bytes.len())
            .map_err(|_| protocol_error("HTTP head length is not representable"))?;
        let next = self
            .length
            .checked_add(count)
            .ok_or_else(|| protocol_error("HTTP head length overflowed"))?;
        self.file
            .append(bytes)
            .map_err(|_| storage_error("HTTP head storage write failed"))?;
        self.length = next;
        Ok(())
    }

    fn finish(&self) -> Result<(), NetworkError> {
        self.file.flush().map_err(|_| storage_error("HTTP head storage flush failed"))
    }

    const fn whole(&self) -> Span {
        Span::new(0, self.length)
    }

    fn byte_at(&self, offset: u64) -> Result<u8, NetworkError> {
        if offset >= self.length {
            return Err(protocol_error("HTTP head byte is outside stored bytes"));
        }
        let mut cache = self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let cache_end = cache
            .offset
            .checked_add(u64::try_from(cache.length).map_err(|_| {
                protocol_error("HTTP head cache length is not representable")
            })?)
            .ok_or_else(|| protocol_error("HTTP head cache span overflowed"))?;
        if cache.length == 0 || offset < cache.offset || offset >= cache_end {
            let block = u64::try_from(PHYSICAL_BUFFER_BYTES)
                .map_err(|_| protocol_error("HTTP head buffer size is not representable"))?;
            let block_offset = (offset / block) * block;
            let remaining = self.length - block_offset;
            let wanted = usize::try_from(remaining.min(block))
                .map_err(|_| protocol_error("HTTP head cache length is not representable"))?;
            self.file
                .read_exact_at(block_offset, &mut cache.bytes[..wanted])
                .map_err(|_| storage_error("HTTP head storage read failed"))?;
            cache.offset = block_offset;
            cache.length = wanted;
        }
        let index = usize::try_from(offset - cache.offset)
            .map_err(|_| protocol_error("HTTP head cache offset is not representable"))?;
        cache
            .bytes
            .get(index)
            .copied()
            .ok_or_else(|| protocol_error("HTTP head cache is incomplete"))
    }

    fn visit(
        &self,
        span: Span,
        mut visit: impl FnMut(&[u8]) -> Result<(), NetworkError>,
    ) -> Result<(), NetworkError> {
        if span.end()? > self.length {
            return Err(protocol_error("HTTP head span is outside stored bytes"));
        }
        let mut offset = span.offset;
        let mut remaining = span.length;
        let mut buffer = [0_u8; PHYSICAL_BUFFER_BYTES];
        while remaining > 0 {
            let wanted = usize::try_from(remaining.min(PHYSICAL_BUFFER_BYTES as u64))
                .map_err(|_| protocol_error("HTTP head chunk is not representable"))?;
            self.file
                .read_exact_at(offset, &mut buffer[..wanted])
                .map_err(|_| storage_error("HTTP head storage read failed"))?;
            visit(&buffer[..wanted])?;
            let count = u64::try_from(wanted)
                .map_err(|_| protocol_error("HTTP head chunk is not representable"))?;
            offset = offset
                .checked_add(count)
                .ok_or_else(|| protocol_error("HTTP head span overflowed"))?;
            remaining -= count;
        }
        Ok(())
    }

    fn next_line(&self, offset: u64) -> Result<(Span, u64), NetworkError> {
        if offset >= self.length {
            return Err(protocol_error("HTTP head terminator is missing"));
        }
        let mut cursor = offset;
        while cursor < self.length {
            match self.byte_at(cursor)? {
                b'\r' => {
                    let line_feed = cursor
                        .checked_add(1)
                        .ok_or_else(|| protocol_error("HTTP line span overflowed"))?;
                    if line_feed >= self.length || self.byte_at(line_feed)? != b'\n' {
                        return Err(protocol_error("HTTP line ending is malformed"));
                    }
                    let next = line_feed
                        .checked_add(1)
                        .ok_or_else(|| protocol_error("HTTP line span overflowed"))?;
                    return Ok((Span::new(offset, cursor - offset), next));
                }
                b'\n' => return Err(protocol_error("HTTP line ending is malformed")),
                _ => {
                    cursor = cursor
                        .checked_add(1)
                        .ok_or_else(|| protocol_error("HTTP line span overflowed"))?;
                }
            }
        }
        Err(protocol_error("HTTP head terminator is missing"))
    }

    fn span_eq(&self, span: Span, expected: &[u8]) -> Result<bool, NetworkError> {
        if span.length != expected.len() as u64 {
            return Ok(false);
        }
        for (relative, expected_byte) in expected.iter().enumerate() {
            let relative = u64::try_from(relative)
                .map_err(|_| protocol_error("HTTP head comparison offset is not representable"))?;
            if self.byte_at(span.offset + relative)? != *expected_byte {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn span_eq_ignore_ascii_case(
        &self,
        span: Span,
        expected: &[u8],
    ) -> Result<bool, NetworkError> {
        if span.length != expected.len() as u64 {
            return Ok(false);
        }
        for (relative, expected_byte) in expected.iter().enumerate() {
            let relative = u64::try_from(relative)
                .map_err(|_| protocol_error("HTTP head comparison offset is not representable"))?;
            if !self
                .byte_at(span.offset + relative)?
                .eq_ignore_ascii_case(expected_byte)
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn span_starts_with(&self, span: Span, prefix: &[u8]) -> Result<bool, NetworkError> {
        if span.length < prefix.len() as u64 {
            return Ok(false);
        }
        self.span_eq(span.subspan(0, prefix.len() as u64)?, prefix)
    }

    fn find_byte(&self, span: Span, wanted: u8) -> Result<Option<u64>, NetworkError> {
        let mut relative = 0_u64;
        while relative < span.length {
            if self.byte_at(span.offset + relative)? == wanted {
                return Ok(Some(relative));
            }
            relative = relative
                .checked_add(1)
                .ok_or_else(|| protocol_error("HTTP head scan offset overflowed"))?;
        }
        Ok(None)
    }

    fn contains_byte(&self, span: Span, wanted: u8) -> Result<bool, NetworkError> {
        self.find_byte(span, wanted).map(|offset| offset.is_some())
    }

    fn read_into(&self, span: Span, output: &mut [u8]) -> Result<usize, NetworkError> {
        let length = usize::try_from(span.length)
            .map_err(|_| protocol_error("HTTP field length is not locally representable"))?;
        if length > output.len() {
            return Err(protocol_error("HTTP authority exceeds its semantic maximum"));
        }
        self.file
            .read_exact_at(span.offset, &mut output[..length])
            .map_err(|_| storage_error("HTTP head storage read failed"))?;
        Ok(length)
    }

    fn validate_utf8(&self) -> Result<(), NetworkError> {
        let mut remaining = 0_u8;
        let mut lower = 0x80_u8;
        let mut upper = 0xbf_u8;
        self.visit(self.whole(), |bytes| {
            for byte in bytes {
                if remaining == 0 {
                    match *byte {
                        0x00..=0x7f => {}
                        0xc2..=0xdf => {
                            remaining = 1;
                            lower = 0x80;
                            upper = 0xbf;
                        }
                        0xe0 => {
                            remaining = 2;
                            lower = 0xa0;
                            upper = 0xbf;
                        }
                        0xe1..=0xec | 0xee..=0xef => {
                            remaining = 2;
                            lower = 0x80;
                            upper = 0xbf;
                        }
                        0xed => {
                            remaining = 2;
                            lower = 0x80;
                            upper = 0x9f;
                        }
                        0xf0 => {
                            remaining = 3;
                            lower = 0x90;
                            upper = 0xbf;
                        }
                        0xf1..=0xf3 => {
                            remaining = 3;
                            lower = 0x80;
                            upper = 0xbf;
                        }
                        0xf4 => {
                            remaining = 3;
                            lower = 0x80;
                            upper = 0x8f;
                        }
                        _ => return Err(protocol_error("HTTP head is not UTF-8")),
                    }
                } else {
                    if *byte < lower || *byte > upper {
                        return Err(protocol_error("HTTP head is not UTF-8"));
                    }
                    remaining -= 1;
                    lower = 0x80;
                    upper = 0xbf;
                }
            }
            Ok(())
        })?;
        if remaining != 0 {
            return Err(protocol_error("HTTP head is not UTF-8"));
        }
        Ok(())
    }
}

struct StoredBytes {
    file: TemporaryFile,
    length: u64,
}

impl StoredBytes {
    fn create() -> Result<Self, NetworkError> {
        Ok(Self {
            file: TemporaryFile::create("http-target")
                .map_err(|_| storage_error("HTTP target storage cannot be created"))?,
            length: 0,
        })
    }

    fn replace_literal(&mut self, bytes: &[u8]) -> Result<(), NetworkError> {
        self.file
            .replace(bytes)
            .map_err(|_| storage_error("HTTP target storage write failed"))?;
        self.length = u64::try_from(bytes.len())
            .map_err(|_| protocol_error("HTTP target length is not representable"))?;
        Ok(())
    }

    fn replace_from(&mut self, source: &StoredHead, span: Span) -> Result<(), NetworkError> {
        self.file
            .begin_replace()
            .map_err(|_| storage_error("HTTP target storage write failed"))?;
        source.visit(span, |bytes| {
            self.file
                .append(bytes)
                .map_err(|_| storage_error("HTTP target storage write failed"))
        })?;
        self.length = span.length;
        Ok(())
    }

    fn visit(
        &self,
        mut visit: impl FnMut(&[u8]) -> Result<(), NetworkError>,
    ) -> Result<(), NetworkError> {
        let mut offset = 0_u64;
        let mut remaining = self.length;
        let mut buffer = [0_u8; PHYSICAL_BUFFER_BYTES];
        while remaining > 0 {
            let wanted = usize::try_from(remaining.min(PHYSICAL_BUFFER_BYTES as u64))
                .map_err(|_| protocol_error("HTTP target chunk is not representable"))?;
            self.file
                .read_exact_at(offset, &mut buffer[..wanted])
                .map_err(|_| storage_error("HTTP target storage read failed"))?;
            visit(&buffer[..wanted])?;
            let count = u64::try_from(wanted)
                .map_err(|_| protocol_error("HTTP target chunk is not representable"))?;
            offset = offset
                .checked_add(count)
                .ok_or_else(|| protocol_error("HTTP target span overflowed"))?;
            remaining -= count;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum MethodClass {
    Connect,
    Get,
    Head,
    Other,
}

pub(super) struct RequestHead {
    raw: StoredHead,
    method: Span,
    version: Span,
    headers_start: u64,
    method_class: MethodClass,
    pub(super) destination: DestinationRequest,
    origin_target: StoredBytes,
    pub(super) content_length: u64,
    routing_header: Span,
}

impl RequestHead {
    pub(super) fn verifies_routing(&self, token: &RoutingToken) -> Result<bool, NetworkError> {
        if self.routing_header.length != DIRECT_ROUTING_AUTHORIZATION_BYTES
            && self.routing_header.length
                != u64::try_from(BASIC_ROUTING_AUTHORIZATION_BYTES).map_err(|_| {
                    credential_error("proxy routing header length is not representable")
                })?
        {
            return Ok(false);
        }
        let mut bytes = [0_u8; BASIC_ROUTING_AUTHORIZATION_BYTES];
        let length = self.raw.read_into(self.routing_header, &mut bytes)?;
        let authorization = std::str::from_utf8(&bytes[..length])
            .map_err(|_| credential_error("proxy routing header is not UTF-8"))?;
        Ok(token.verifies_authorization(authorization))
    }

    pub(super) const fn is_connect(&self) -> bool {
        matches!(self.method_class, MethodClass::Connect)
    }

    pub(super) const fn supports_redirect_replay(&self) -> bool {
        matches!(self.method_class, MethodClass::Get | MethodClass::Head)
    }

    pub(super) fn write_to(
        &self,
        credential: Option<(&str, &[u8])>,
        mut output: impl FnMut(&[u8]) -> Result<(), NetworkError>,
    ) -> Result<u64, NetworkError> {
        let mut written = 0_u64;
        self.visit_encoded(credential, |bytes| {
            let count = u64::try_from(bytes.len()).map_err(|_| {
                protocol_error("rewritten HTTP head length is not representable")
            })?;
            let next = written
                .checked_add(count)
                .ok_or_else(|| protocol_error("rewritten HTTP head length overflowed"))?;
            output(bytes)?;
            written = next;
            Ok(())
        })?;
        Ok(written)
    }

    pub(super) fn encoded_len(
        &self,
        credential: Option<(&str, &[u8])>,
    ) -> Result<u64, NetworkError> {
        let mut length = 0_u64;
        self.visit_encoded(credential, |bytes| {
            let count = u64::try_from(bytes.len()).map_err(|_| {
                protocol_error("rewritten HTTP head length is not representable")
            })?;
            length = length
                .checked_add(count)
                .ok_or_else(|| protocol_error("rewritten HTTP head length overflowed"))?;
            Ok(())
        })?;
        Ok(length)
    }

    fn visit_encoded(
        &self,
        credential: Option<(&str, &[u8])>,
        mut visit: impl FnMut(&[u8]) -> Result<(), NetworkError>,
    ) -> Result<(), NetworkError> {
        self.raw.visit(self.method, &mut visit)?;
        visit(b" ")?;
        self.origin_target.visit(&mut visit)?;
        visit(b" ")?;
        self.raw.visit(self.version, &mut visit)?;
        visit(b"\r\n")?;
        let mut cursor = self.headers_start;
        loop {
            let (line, next) = self.raw.next_line(cursor)?;
            cursor = next;
            if line.length == 0 {
                break;
            }
            let header = parse_header(&self.raw, line)?;
            if header_name_is_filtered(&self.raw, header.name)? {
                continue;
            }
            self.raw.visit(line, &mut visit)?;
            visit(b"\r\n")?;
        }
        visit(b"Host: ")?;
        let host = host_header(&self.destination);
        visit(host.as_bytes())?;
        visit(b"\r\n")?;
        if let Some((name, value)) = credential {
            visit(name.as_bytes())?;
            visit(b": ")?;
            visit(value)?;
            visit(b"\r\n")?;
        }
        visit(b"Connection: close\r\n\r\n")
    }

    pub(super) fn follow(
        &mut self,
        candidate: &RedirectCandidate,
        response: &ResponseHead,
    ) -> Result<(), NetworkError> {
        match candidate.path {
            Some(path) => self.origin_target.replace_from(&response.raw, path)?,
            None => self.origin_target.replace_literal(b"/")?,
        }
        self.destination.clone_from(&candidate.destination);
        self.content_length = 0;
        Ok(())
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
    raw: StoredHead,
    pub(super) status: u16,
    pub(super) content_length: Option<u64>,
    location: Option<Span>,
}

impl ResponseHead {
    pub(super) const fn encoded_len(&self) -> u64 {
        self.raw.length
    }

    pub(super) fn write_to(
        &self,
        output: impl FnMut(&[u8]) -> Result<(), NetworkError>,
    ) -> Result<(), NetworkError> {
        self.raw.visit(self.raw.whole(), output)
    }

    pub(super) fn redirect_candidate(
        &self,
        original: &DestinationRequest,
    ) -> Result<Option<RedirectCandidate>, NetworkError> {
        let Some(location) = self.location else {
            return Ok(None);
        };
        if self.raw.contains_byte(location, b'#')? || self.raw.contains_byte(location, b'@')? {
            return Err(redirect_error("redirect URI is malformed"));
        }
        if self.raw.span_starts_with(location, b"/")? {
            return Ok(Some(RedirectCandidate {
                destination: original.clone(),
                path: Some(location),
            }));
        }
        if !self.raw.span_starts_with(location, b"http://")? {
            return Err(redirect_error(
                "redirect URI is not plain HTTP and cannot be followed by this proxy",
            ));
        }
        let remainder = location.subspan(7, location.length - 7)?;
        let slash = self.raw.find_byte(remainder, b'/')?;
        let authority_length = slash.unwrap_or(remainder.length);
        let authority = remainder.subspan(0, authority_length)?;
        let authority = parse_authority(&self.raw, authority, Some(80), redirect_error)?;
        let destination = redirect_request(
            authority.host.as_str(redirect_error)?,
            authority.port,
        )?;
        let path = match slash {
            Some(relative) => Some(remainder.subspan(relative, remainder.length - relative)?),
            None => None,
        };
        Ok(Some(RedirectCandidate { destination, path }))
    }
}

pub(super) struct RedirectCandidate {
    destination: DestinationRequest,
    path: Option<Span>,
}

impl RedirectCandidate {
    pub(super) const fn request(&self) -> &DestinationRequest {
        &self.destination
    }
}

pub(super) fn read_request(
    stream: &mut impl Read,
    maximum: Option<NonZeroU64>,
    cancellation: &CancellationToken,
    duration_limit: Option<NonZeroU64>,
    began: Instant,
) -> Result<RequestHead, NetworkError> {
    let raw = read_head(stream, maximum, cancellation, duration_limit, began)?;
    let (request_line, headers_start) = raw.next_line(0)?;
    let (method, target, version) = request_line_parts(&raw, request_line)?;
    let method_class = if raw.span_eq(method, b"CONNECT")? {
        MethodClass::Connect
    } else if raw.span_eq(method, b"GET")? {
        MethodClass::Get
    } else if raw.span_eq(method, b"HEAD")? {
        MethodClass::Head
    } else {
        MethodClass::Other
    };
    let mut cursor = headers_start;
    let mut routing_header = None;
    let mut host = None;
    let mut content_length = 0_u64;
    loop {
        let (line, next) = raw.next_line(cursor)?;
        cursor = next;
        if line.length == 0 {
            break;
        }
        let header = parse_header(&raw, line)?;
        if raw.span_eq_ignore_ascii_case(header.name, b"proxy-authorization")? {
            if routing_header.replace(header.value).is_some() {
                return Err(protocol_error("proxy routing header is duplicated"));
            }
        } else if raw.span_eq_ignore_ascii_case(header.name, b"host")? {
            host = Some(header.value);
        } else if raw.span_eq_ignore_ascii_case(header.name, b"content-length")? {
            content_length = parse_decimal(&raw, header.value, || {
                protocol_error("content length is invalid")
            })?;
        } else if raw.span_eq_ignore_ascii_case(header.name, b"transfer-encoding")? {
            return Err(protocol_error("chunked request bodies are unsupported"));
        }
    }
    if cursor != raw.length {
        return Err(protocol_error("HTTP request head has trailing bytes"));
    }
    let routing_header =
        routing_header.ok_or_else(|| credential_error("proxy routing header is missing"))?;
    let (destination, initial_target) = destination(&raw, method_class, target, host)?;
    let mut origin_target = StoredBytes::create()?;
    match initial_target {
        InitialTarget::Stored(span) => origin_target.replace_from(&raw, span)?,
        InitialTarget::Root => origin_target.replace_literal(b"/")?,
        InitialTarget::Empty => origin_target.replace_literal(b"")?,
    }
    Ok(RequestHead {
        raw,
        method,
        version,
        headers_start,
        method_class,
        destination,
        origin_target,
        content_length,
        routing_header,
    })
}

pub(super) fn read_response(
    stream: &mut impl Read,
    maximum: Option<NonZeroU64>,
    cancellation: &CancellationToken,
    duration_limit: Option<NonZeroU64>,
    began: Instant,
) -> Result<ResponseHead, NetworkError> {
    let raw = read_head(stream, maximum, cancellation, duration_limit, began)?;
    let (status_line, mut cursor) = raw.next_line(0)?;
    let (version, status_span) = response_status_parts(&raw, status_line)?;
    if !raw.span_eq(version, b"HTTP/1.0")? && !raw.span_eq(version, b"HTTP/1.1")? {
        return Err(protocol_error("HTTP response status line is invalid"));
    }
    let status_value = parse_decimal(&raw, status_span, || {
        protocol_error("HTTP response status is invalid")
    })?;
    let status = u16::try_from(status_value)
        .map_err(|_| protocol_error("HTTP response status is invalid"))?;
    if !(100..=599).contains(&status) {
        return Err(protocol_error("HTTP response status line is invalid"));
    }
    let mut content_length = None;
    let mut location = None;
    loop {
        let (line, next) = raw.next_line(cursor)?;
        cursor = next;
        if line.length == 0 {
            break;
        }
        let header = parse_header(&raw, line)?;
        if raw.span_eq_ignore_ascii_case(header.name, b"content-length")? {
            content_length = Some(parse_decimal(&raw, header.value, || {
                protocol_error("response content length is invalid")
            })?);
        }
        if raw.span_eq_ignore_ascii_case(header.name, b"location")? {
            location = Some(header.value);
        }
    }
    if cursor != raw.length {
        return Err(protocol_error("HTTP response head has trailing bytes"));
    }
    Ok(ResponseHead { raw, status, content_length, location })
}

enum InitialTarget {
    Stored(Span),
    Root,
    Empty,
}

fn destination(
    raw: &StoredHead,
    method: MethodClass,
    target: Span,
    host_header: Option<Span>,
) -> Result<(DestinationRequest, InitialTarget), NetworkError> {
    if method == MethodClass::Connect {
        let authority = parse_authority(raw, target, None, protocol_error)?;
        return Ok((
            request(authority.host.as_str(protocol_error)?, authority.port)?,
            InitialTarget::Empty,
        ));
    }
    if raw.span_starts_with(target, b"http://")? {
        let remainder = target.subspan(7, target.length - 7)?;
        let slash = raw.find_byte(remainder, b'/')?;
        let authority_length = slash.unwrap_or(remainder.length);
        let authority = parse_authority(
            raw,
            remainder.subspan(0, authority_length)?,
            Some(80),
            protocol_error,
        )?;
        let initial_target = match slash {
            Some(relative) => {
                InitialTarget::Stored(remainder.subspan(relative, remainder.length - relative)?)
            }
            None => InitialTarget::Root,
        };
        return Ok((
            request(authority.host.as_str(protocol_error)?, authority.port)?,
            initial_target,
        ));
    }
    if raw.span_starts_with(target, b"/")? {
        let host_header = host_header
            .ok_or_else(|| protocol_error("origin-form request has no Host header"))?;
        let authority = parse_authority(raw, host_header, Some(80), protocol_error)?;
        return Ok((
            request(authority.host.as_str(protocol_error)?, authority.port)?,
            InitialTarget::Stored(target),
        ));
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

fn redirect_request(host: &str, port: u16) -> Result<DestinationRequest, NetworkError> {
    let host = match host.parse() {
        Ok(address) => NetworkHost::Ip(address),
        Err(_) => NetworkHost::Dns(
            DnsName::new(host).map_err(|_| redirect_error("redirect host is invalid"))?,
        ),
    };
    DestinationRequest::new(host, Transport::Tcp, port)
        .map_err(|_| redirect_error("redirect destination is invalid"))
}

struct ParsedAuthority {
    host: BoundedHost,
    port: u16,
}

fn parse_authority(
    raw: &StoredHead,
    authority: Span,
    default_port: Option<u16>,
    semantic_error: fn(&'static str) -> NetworkError,
) -> Result<ParsedAuthority, NetworkError> {
    if authority.length == 0 {
        return Err(semantic_error("HTTP authority is empty"));
    }
    if raw.byte_at(authority.offset)? == b'[' {
        let bracket_body = authority.subspan(1, authority.length - 1)?;
        let close = raw
            .find_byte(bracket_body, b']')?
            .ok_or_else(|| semantic_error("IPv6 authority is malformed"))?;
        let host = bounded_host(raw, bracket_body.subspan(0, close)?, semantic_error)?;
        if host
            .as_str(semantic_error)?
            .parse::<std::net::Ipv6Addr>()
            .is_err()
        {
            return Err(semantic_error("bracketed authority is not an IPv6 address"));
        }
        let suffix_offset = close
            .checked_add(2)
            .ok_or_else(|| semantic_error("IPv6 authority span overflowed"))?;
        let suffix_length = authority
            .length
            .checked_sub(suffix_offset)
            .ok_or_else(|| semantic_error("IPv6 authority is malformed"))?;
        let port = if suffix_length == 0 {
            selected_default_port(default_port, semantic_error)?
        } else {
            let suffix = authority.subspan(suffix_offset, suffix_length)?;
            if raw.byte_at(suffix.offset)? != b':' {
                return Err(semantic_error("IPv6 authority is malformed"));
            }
            parse_port(raw, suffix.subspan(1, suffix.length - 1)?, semantic_error)?
        };
        return Ok(ParsedAuthority { host, port });
    }

    let mut colon = None;
    let mut relative = 0_u64;
    while relative < authority.length {
        if raw.byte_at(authority.offset + relative)? == b':' {
            if colon.is_some() {
                return Err(semantic_error("IPv6 authority must be bracketed"));
            }
            colon = Some(relative);
        }
        relative = relative
            .checked_add(1)
            .ok_or_else(|| semantic_error("HTTP authority span overflowed"))?;
    }
    let (host_span, port) = match colon {
        Some(colon) => (
            authority.subspan(0, colon)?,
            parse_port(
                raw,
                authority.subspan(colon + 1, authority.length - colon - 1)?,
                semantic_error,
            )?,
        ),
        None => (authority, selected_default_port(default_port, semantic_error)?),
    };
    Ok(ParsedAuthority {
        host: bounded_host(raw, host_span, semantic_error)?,
        port,
    })
}

fn selected_default_port(
    default_port: Option<u16>,
    semantic_error: fn(&'static str) -> NetworkError,
) -> Result<u16, NetworkError> {
    match default_port {
        Some(port) if port != 0 => Ok(port),
        Some(_) => Err(semantic_error("authority port is zero")),
        None => Err(semantic_error("authority port is missing")),
    }
}

fn parse_port(
    raw: &StoredHead,
    span: Span,
    semantic_error: fn(&'static str) -> NetworkError,
) -> Result<u16, NetworkError> {
    if span.length == 0 {
        return Err(semantic_error("authority port is invalid"));
    }
    let mut port = 0_u16;
    let mut relative = 0_u64;
    while relative < span.length {
        let byte = raw.byte_at(span.offset + relative)?;
        if !byte.is_ascii_digit() {
            return Err(semantic_error("authority port is invalid"));
        }
        port = port
            .checked_mul(10)
            .and_then(|value| value.checked_add(u16::from(byte - b'0')))
            .ok_or_else(|| semantic_error("authority port is invalid"))?;
        relative = relative
            .checked_add(1)
            .ok_or_else(|| semantic_error("authority port span overflowed"))?;
    }
    if port == 0 {
        return Err(semantic_error("authority port is zero"));
    }
    Ok(port)
}

struct BoundedHost {
    bytes: [u8; MAX_DNS_INPUT_BYTES],
    length: usize,
}

impl BoundedHost {
    const fn new() -> Self {
        Self { bytes: [0; MAX_DNS_INPUT_BYTES], length: 0 }
    }

    fn as_str(
        &self,
        semantic_error: fn(&'static str) -> NetworkError,
    ) -> Result<&str, NetworkError> {
        std::str::from_utf8(&self.bytes[..self.length])
            .map_err(|_| semantic_error("HTTP authority host is not UTF-8"))
    }
}

fn bounded_host(
    raw: &StoredHead,
    span: Span,
    semantic_error: fn(&'static str) -> NetworkError,
) -> Result<BoundedHost, NetworkError> {
    if span.length == 0 || span.length > MAX_DNS_INPUT_BYTES as u64 {
        return Err(semantic_error("HTTP authority host exceeds its semantic maximum"));
    }
    let mut value = BoundedHost::new();
    value.length = raw.read_into(span, &mut value.bytes)?;
    Ok(value)
}

struct HeaderSpan {
    name: Span,
    value: Span,
}

fn parse_header(raw: &StoredHead, line: Span) -> Result<HeaderSpan, NetworkError> {
    let colon = raw
        .find_byte(line, b':')?
        .ok_or_else(|| protocol_error("HTTP header is malformed"))?;
    if colon == 0 {
        return Err(protocol_error("HTTP header name is invalid"));
    }
    let name = line.subspan(0, colon)?;
    let mut relative = 0_u64;
    while relative < name.length {
        if !is_header_name_byte(raw.byte_at(name.offset + relative)?) {
            return Err(protocol_error("HTTP header name is invalid"));
        }
        relative += 1;
    }
    let mut value_start = colon + 1;
    while value_start < line.length
        && matches!(raw.byte_at(line.offset + value_start)?, b' ' | b'\t')
    {
        value_start += 1;
    }
    let mut cursor = value_start;
    let mut value_end = value_start;
    while cursor < line.length {
        let byte = raw.byte_at(line.offset + cursor)?;
        if byte.is_ascii_control() && byte != b'\t' {
            return Err(protocol_error("HTTP header value contains control bytes"));
        }
        if !matches!(byte, b' ' | b'\t') {
            value_end = cursor + 1;
        }
        cursor += 1;
    }
    Ok(HeaderSpan {
        name,
        value: line.subspan(value_start, value_end - value_start)?,
    })
}

fn header_name_is_filtered(raw: &StoredHead, name: Span) -> Result<bool, NetworkError> {
    Ok(raw.span_eq_ignore_ascii_case(name, b"proxy-authorization")?
        || raw.span_eq_ignore_ascii_case(name, b"proxy-connection")?
        || raw.span_eq_ignore_ascii_case(name, b"connection")?
        || raw.span_eq_ignore_ascii_case(name, b"host")?)
}

fn request_line_parts(
    raw: &StoredHead,
    line: Span,
) -> Result<(Span, Span, Span), NetworkError> {
    let first = raw
        .find_byte(line, b' ')?
        .ok_or_else(|| protocol_error("HTTP request line is malformed"))?;
    let remainder = line.subspan(first + 1, line.length - first - 1)?;
    let second_relative = raw
        .find_byte(remainder, b' ')?
        .ok_or_else(|| protocol_error("HTTP request line is malformed"))?;
    let method = line.subspan(0, first)?;
    let target = remainder.subspan(0, second_relative)?;
    let version = remainder.subspan(
        second_relative + 1,
        remainder.length - second_relative - 1,
    )?;
    if method.length == 0
        || target.length == 0
        || version.length == 0
        || raw.contains_byte(version, b' ')?
        || (!raw.span_eq(version, b"HTTP/1.0")? && !raw.span_eq(version, b"HTTP/1.1")?)
    {
        return Err(protocol_error("HTTP request line is malformed"));
    }
    let mut relative = 0_u64;
    while relative < method.length {
        if !raw.byte_at(method.offset + relative)?.is_ascii_uppercase() {
            return Err(protocol_error("HTTP request line is malformed"));
        }
        relative += 1;
    }
    Ok((method, target, version))
}

fn response_status_parts(raw: &StoredHead, line: Span) -> Result<(Span, Span), NetworkError> {
    let first = raw
        .find_byte(line, b' ')?
        .ok_or_else(|| protocol_error("HTTP response status is missing"))?;
    let version = line.subspan(0, first)?;
    let remainder = line.subspan(first + 1, line.length - first - 1)?;
    if version.length == 0 || remainder.length == 0 {
        return Err(protocol_error("HTTP response status line is invalid"));
    }
    let status_length = raw.find_byte(remainder, b' ')?.unwrap_or(remainder.length);
    if status_length == 0 {
        return Err(protocol_error("HTTP response status is missing"));
    }
    Ok((version, remainder.subspan(0, status_length)?))
}

fn parse_decimal(
    raw: &StoredHead,
    span: Span,
    error: impl Fn() -> NetworkError,
) -> Result<u64, NetworkError> {
    if span.length == 0 {
        return Err(error());
    }
    let mut value = 0_u64;
    let mut relative = 0_u64;
    while relative < span.length {
        let byte = raw.byte_at(span.offset + relative)?;
        if !byte.is_ascii_digit() {
            return Err(error());
        }
        let Some(next) = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(u64::from(byte - b'0')))
        else {
            return Err(error());
        };
        value = next;
        relative += 1;
    }
    Ok(value)
}

fn read_head(
    stream: &mut impl Read,
    maximum: Option<NonZeroU64>,
    cancellation: &CancellationToken,
    duration_limit: Option<NonZeroU64>,
    began: Instant,
) -> Result<StoredHead, NetworkError> {
    let mut head = StoredHead::create()?;
    let mut page = [0_u8; PHYSICAL_BUFFER_BYTES];
    let mut used = 0_usize;
    let mut observed = 0_u64;
    let mut terminator = 0_u32;
    loop {
        if cancellation.is_cancelled() {
            return Err(cancelled_error());
        }
        if let Some(limit) = duration_limit {
            let elapsed = u64::try_from(began.elapsed().as_millis()).map_err(|_| duration_error())?;
            if elapsed > limit.get() {
                return Err(duration_error());
            }
        }
        if maximum.is_some_and(|maximum| observed >= maximum.get()) {
            return Err(protocol_error("HTTP head exceeds its selected byte ceiling"));
        }
        let mut byte = [0_u8; 1];
        match stream.read_exact(&mut byte) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => return Err(io_error("HTTP head could not be read")),
        }
        page[used] = byte[0];
        used += 1;
        observed = observed
            .checked_add(1)
            .ok_or_else(|| protocol_error("HTTP head length overflowed"))?;
        terminator = (terminator << 8) | u32::from(byte[0]);
        if used == page.len() {
            head.append(&page)?;
            used = 0;
        }
        if observed >= 4 && terminator == u32::from_be_bytes(*b"\r\n\r\n") {
            if used > 0 {
                head.append(&page[..used])?;
            }
            head.finish()?;
            if head.length != observed {
                return Err(protocol_error("HTTP head storage length mismatched"));
            }
            head.validate_utf8()?;
            return Ok(head);
        }
    }
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

const fn redirect_error(detail: &'static str) -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Redirect,
        NetworkOperation::Redirect,
        RecoveryClass::Replan,
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

const fn storage_error(detail: &'static str) -> NetworkError {
    NetworkError::storage(NetworkOperation::Proxy, detail)
}

const fn cancelled_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::IncompleteTeardown,
        NetworkOperation::Relay,
        RecoveryClass::CancelAndJoin,
        "managed proxy head read was cancelled",
    )
}

const fn duration_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Limit,
        NetworkOperation::Relay,
        RecoveryClass::CancelAndJoin,
        "managed connection duration accounting overflowed or crossed its selected ceiling",
    )
}
