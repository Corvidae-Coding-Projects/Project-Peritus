//! Minimal bounded HTTP/1 request parsing and scripted response writing.

use super::model::{
    ExpectedHttpRequest, FakeHttpFault, FakeHttpHeader, FakeHttpLimits, FakeHttpReleasePoint,
    ScriptedHttpResponse,
};
use super::observation::{FakeHttpExchange, FakeHttpTermination, ParsedRequest, exchange};
use super::server::Shared;
use super::{FakeHttpError, FakeHttpErrorKind};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::Arc;

pub fn serve(
    listener: &TcpListener,
    expected: &ExpectedHttpRequest,
    response: &ScriptedHttpResponse,
    limits: FakeHttpLimits,
    shared: &Arc<Shared>,
) -> Result<FakeHttpExchange, FakeHttpError> {
    let (mut stream, _peer) =
        listener.accept().map_err(|_source| io_error("HTTP accept failed"))?;
    {
        let mut control = shared.control.lock().map_err(|_poisoned| sync_error())?;
        if control.shutdown {
            return Err(io_error("HTTP worker was shut down"));
        }
        control.active =
            Some(stream.try_clone().map_err(|_source| io_error("HTTP stream ownership failed"))?);
    }
    let request = read_request(&mut stream, limits)?;
    let matched = expected.matches(&request);
    let captured = request.capture(matched);
    if !matched {
        let termination = write_mismatch(&mut stream);
        return Ok(exchange(captured, 0, termination));
    }
    let (chunks_sent, termination) = write_response(&mut stream, response, shared)?;
    Ok(exchange(captured, chunks_sent, termination))
}

fn read_request(
    stream: &mut TcpStream,
    limits: FakeHttpLimits,
) -> Result<ParsedRequest, FakeHttpError> {
    let mut received = Vec::new();
    let head_end = loop {
        if let Some(index) = find_head_end(&received) {
            if index + 4 > limits.max_header_bytes() {
                return Err(limit_error("HTTP request head exceeded its byte limit"));
            }
            break index + 4;
        }
        if received.len() >= limits.max_header_bytes() {
            return Err(limit_error("HTTP request head exceeded its byte limit"));
        }
        let remaining = limits.max_header_bytes() + limits.max_body_bytes() - received.len();
        let mut buffer = [0_u8; 1024];
        let read_limit = buffer.len().min(remaining);
        let count = stream
            .read(&mut buffer[..read_limit])
            .map_err(|_source| io_error("HTTP request read failed"))?;
        if count == 0 {
            return Err(malformed("HTTP request ended before its head"));
        }
        received.extend_from_slice(&buffer[..count]);
    };
    let (method, target, headers, content_length) = parse_head(&received[..head_end], limits)?;
    if content_length > limits.max_body_bytes() {
        return Err(limit_error("HTTP request body exceeded its byte limit"));
    }
    let mut body = received[head_end..].to_vec();
    if body.len() > content_length {
        return Err(malformed("HTTP request contained bytes beyond its declared body"));
    }
    while body.len() < content_length {
        let mut buffer = [0_u8; 1024];
        let read_limit = buffer.len().min(content_length - body.len());
        let count = stream
            .read(&mut buffer[..read_limit])
            .map_err(|_source| io_error("HTTP request body read failed"))?;
        if count == 0 {
            return Err(malformed("HTTP request body ended before its declared length"));
        }
        body.extend_from_slice(&buffer[..count]);
    }
    Ok(ParsedRequest { method, target, headers, body })
}

fn parse_head(
    head: &[u8],
    limits: FakeHttpLimits,
) -> Result<(String, String, Vec<FakeHttpHeader>, usize), FakeHttpError> {
    let text =
        std::str::from_utf8(head).map_err(|_utf8| malformed("HTTP request head is not UTF-8"))?;
    let mut lines = text[..text.len() - 4].split("\r\n");
    let request_line = lines.next().ok_or_else(|| malformed("HTTP request line is missing"))?;
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || method.is_empty()
        || method.len() > 16
        || !method.bytes().all(|byte| byte.is_ascii_uppercase())
        || target.is_empty()
        || !target.starts_with('/')
        || !matches!(version, "HTTP/1.0" | "HTTP/1.1")
    {
        return Err(malformed("HTTP request line is invalid"));
    }
    let mut headers = Vec::new();
    let mut content_length = None;
    for line in lines {
        if headers.len() == limits.max_headers() {
            return Err(limit_error("HTTP request header count exceeded its limit"));
        }
        let (name, raw_value) =
            line.split_once(':').ok_or_else(|| malformed("HTTP request header is invalid"))?;
        let value = raw_value.trim_matches([' ', '\t']).as_bytes().to_vec();
        let header = FakeHttpHeader::new(name, value)
            .map_err(|_invalid| malformed("HTTP request header is invalid"))?;
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(malformed("transfer-encoded fake requests are unsupported"));
        }
        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(malformed("HTTP request has duplicate content lengths"));
            }
            content_length = Some(
                std::str::from_utf8(header.value())
                    .ok()
                    .and_then(|value| value.parse::<usize>().ok())
                    .ok_or_else(|| malformed("HTTP content length is invalid"))?,
            );
        }
        headers.push(header);
    }
    Ok((method.to_owned(), target.to_owned(), headers, content_length.unwrap_or(0)))
}

fn write_mismatch(stream: &mut TcpStream) -> FakeHttpTermination {
    let bytes = b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
    if stream.write_all(bytes).is_ok() {
        FakeHttpTermination::Completed
    } else {
        FakeHttpTermination::PeerClosed
    }
}

fn write_response(
    stream: &mut TcpStream,
    response: &ScriptedHttpResponse,
    shared: &Arc<Shared>,
) -> Result<(usize, FakeHttpTermination), FakeHttpError> {
    pause_if_selected(shared, response.release, FakeHttpReleasePoint::BeforeHeaders)?;
    if peer_closed(stream) {
        return Ok((0, FakeHttpTermination::PeerClosed));
    }
    let status = format!("HTTP/1.1 {} {}\r\n", response.status, reason(response.status));
    if write_part(stream, status.as_bytes()).is_err() {
        return Ok((0, FakeHttpTermination::PeerClosed));
    }
    for header in &response.headers {
        if write_part(stream, header.name().as_bytes()).is_err()
            || write_part(stream, b": ").is_err()
            || write_part(stream, header.value()).is_err()
            || write_part(stream, b"\r\n").is_err()
        {
            return Ok((0, FakeHttpTermination::PeerClosed));
        }
    }
    if write_part(stream, b"\r\n").is_err() {
        return Ok((0, FakeHttpTermination::PeerClosed));
    }
    if response.fault == FakeHttpFault::CloseAfterHeaders {
        return scripted_close(stream, shared, response.release, 0);
    }
    let mut sent = 0;
    for (index, chunk) in response.chunks.iter().enumerate() {
        if response.fault == FakeHttpFault::CloseAfterChunks(sent) {
            return scripted_close(stream, shared, response.release, sent);
        }
        pause_if_selected(shared, response.release, FakeHttpReleasePoint::BeforeChunk(index))?;
        if peer_closed(stream) {
            return Ok((sent, FakeHttpTermination::PeerClosed));
        }
        if write_part(stream, chunk).is_err() {
            return Ok((sent, FakeHttpTermination::PeerClosed));
        }
        sent += 1;
    }
    if response.fault == FakeHttpFault::CloseAfterChunks(sent) {
        return scripted_close(stream, shared, response.release, sent);
    }
    let _shutdown = stream.shutdown(Shutdown::Write);
    Ok((sent, FakeHttpTermination::Completed))
}

fn scripted_close(
    stream: &TcpStream,
    shared: &Arc<Shared>,
    release: Option<FakeHttpReleasePoint>,
    sent: usize,
) -> Result<(usize, FakeHttpTermination), FakeHttpError> {
    pause_if_selected(shared, release, FakeHttpReleasePoint::BeforeClose)?;
    let _shutdown = stream.shutdown(Shutdown::Both);
    Ok((sent, FakeHttpTermination::ScriptedClose))
}

fn pause_if_selected(
    shared: &Arc<Shared>,
    selected: Option<FakeHttpReleasePoint>,
    current: FakeHttpReleasePoint,
) -> Result<(), FakeHttpError> {
    if selected != Some(current) {
        return Ok(());
    }
    let mut control = shared.control.lock().map_err(|_poisoned| sync_error())?;
    control.blocked = Some(current);
    shared.changed.notify_all();
    control = shared
        .changed
        .wait_while(control, |state| !state.released && !state.shutdown)
        .map_err(|_poisoned| sync_error())?;
    control.blocked = None;
    let shutdown = control.shutdown;
    drop(control);
    if shutdown {
        return Err(io_error("HTTP worker was shut down at its release point"));
    }
    Ok(())
}

fn write_part(stream: &mut TcpStream, bytes: &[u8]) -> std::io::Result<()> {
    stream.write_all(bytes)
}

fn peer_closed(stream: &TcpStream) -> bool {
    if stream.set_nonblocking(true).is_err() {
        return false;
    }
    let mut byte = [0_u8; 1];
    let closed = matches!(stream.peek(&mut byte), Ok(0));
    let _blocking = stream.set_nonblocking(false);
    closed
}

fn find_head_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

const fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Scripted",
    }
}

const fn malformed(detail: &'static str) -> FakeHttpError {
    FakeHttpError::new(FakeHttpErrorKind::MalformedRequest, detail)
}

const fn limit_error(detail: &'static str) -> FakeHttpError {
    FakeHttpError::new(FakeHttpErrorKind::RequestLimit, detail)
}

const fn io_error(detail: &'static str) -> FakeHttpError {
    FakeHttpError::new(FakeHttpErrorKind::Io, detail)
}

const fn sync_error() -> FakeHttpError {
    FakeHttpError::new(FakeHttpErrorKind::Io, "fake HTTP worker synchronization failed")
}
