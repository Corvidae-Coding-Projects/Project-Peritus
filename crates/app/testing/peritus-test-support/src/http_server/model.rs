//! Bounded request, response, and redacted observation types.

use super::observation::ParsedRequest;
use super::validation::{
    encoded_header_bytes, headers_match, is_header_name_byte, valid_chunk_index, valid_release,
};
use super::{FakeHttpError, FakeHttpErrorKind};
use std::fmt;

/// Resource limits enforced by one fake HTTP server.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FakeHttpLimits {
    header_bytes: usize,
    headers: usize,
    body_bytes: usize,
    chunk_bytes: usize,
    chunks: usize,
}

impl FakeHttpLimits {
    /// Creates nonzero limits.
    ///
    /// # Errors
    ///
    /// Returns an invalid-configuration error when any limit is zero.
    pub const fn new(
        max_header_bytes: usize,
        max_headers: usize,
        max_body_bytes: usize,
        max_chunk_bytes: usize,
        max_chunks: usize,
    ) -> Result<Self, FakeHttpError> {
        if max_header_bytes == 0
            || max_headers == 0
            || max_body_bytes == 0
            || max_chunk_bytes == 0
            || max_chunks == 0
            || max_header_bytes.checked_add(max_body_bytes).is_none()
        {
            return Err(FakeHttpError::new(
                FakeHttpErrorKind::InvalidConfiguration,
                "fake HTTP limits must be nonzero",
            ));
        }
        Ok(Self {
            header_bytes: max_header_bytes,
            headers: max_headers,
            body_bytes: max_body_bytes,
            chunk_bytes: max_chunk_bytes,
            chunks: max_chunks,
        })
    }

    /// Returns the maximum request-head byte count.
    #[must_use]
    pub const fn max_header_bytes(self) -> usize {
        self.header_bytes
    }

    /// Returns the maximum request/response header count.
    #[must_use]
    pub const fn max_headers(self) -> usize {
        self.headers
    }

    /// Returns the maximum request or complete scripted-body byte count.
    #[must_use]
    pub const fn max_body_bytes(self) -> usize {
        self.body_bytes
    }

    /// Returns the maximum byte count of one scripted response chunk.
    #[must_use]
    pub const fn max_chunk_bytes(self) -> usize {
        self.chunk_bytes
    }

    /// Returns the maximum scripted response chunk count.
    #[must_use]
    pub const fn max_chunks(self) -> usize {
        self.chunks
    }
}

impl Default for FakeHttpLimits {
    fn default() -> Self {
        Self {
            header_bytes: 16 * 1024,
            headers: 64,
            body_bytes: 1024 * 1024,
            chunk_bytes: 64 * 1024,
            chunks: 128,
        }
    }
}

/// One caller-owned exact HTTP header used in an expectation or response script.
#[derive(Clone, Eq, PartialEq)]
pub struct FakeHttpHeader {
    name: String,
    value: Vec<u8>,
}

impl FakeHttpHeader {
    /// Creates a syntactically safe header from exact bytes.
    ///
    /// # Errors
    ///
    /// Returns an invalid-configuration error for an invalid name or line-breaking value.
    pub fn new(name: impl Into<String>, value: impl Into<Vec<u8>>) -> Result<Self, FakeHttpError> {
        let name = name.into();
        let value = value.into();
        if name.is_empty() || !name.bytes().all(is_header_name_byte) {
            return Err(FakeHttpError::new(
                FakeHttpErrorKind::InvalidConfiguration,
                "fake HTTP header name is invalid",
            ));
        }
        if value.iter().any(|byte| matches!(byte, b'\r' | b'\n')) {
            return Err(FakeHttpError::new(
                FakeHttpErrorKind::InvalidConfiguration,
                "fake HTTP header value contains a line break",
            ));
        }
        Ok(Self { name, value })
    }

    /// Returns the header name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the exact caller-owned value.
    #[must_use]
    pub fn value(&self) -> &[u8] {
        &self.value
    }
}

impl fmt::Debug for FakeHttpHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FakeHttpHeader")
            .field("name", &self.name)
            .field("value_bytes", &self.value.len())
            .field("value", &"[REDACTED]")
            .finish()
    }
}

/// Header comparison policy for an expected request.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum HeaderMatchMode {
    /// The request must contain exactly the expected headers in the expected order.
    #[default]
    Exact,
    /// The expected ordered headers must be present; unlisted headers are permitted.
    AllowAdditional,
}

/// An exact, bounded request expectation.
#[derive(Clone, Eq, PartialEq)]
pub struct ExpectedHttpRequest {
    method: String,
    target: String,
    headers: Vec<FakeHttpHeader>,
    body: Vec<u8>,
    header_mode: HeaderMatchMode,
}

impl ExpectedHttpRequest {
    /// Creates an exact request expectation and validates it against `limits`.
    ///
    /// # Errors
    ///
    /// Returns an invalid-configuration error for malformed or oversized fields.
    pub fn new(
        method: impl Into<String>,
        target: impl Into<String>,
        headers: Vec<FakeHttpHeader>,
        body: impl Into<Vec<u8>>,
        limits: FakeHttpLimits,
    ) -> Result<Self, FakeHttpError> {
        let value = Self {
            method: method.into(),
            target: target.into(),
            headers,
            body: body.into(),
            header_mode: HeaderMatchMode::Exact,
        };
        value.validate(limits)?;
        Ok(value)
    }

    pub(crate) fn validate(&self, limits: FakeHttpLimits) -> Result<(), FakeHttpError> {
        if self.method.is_empty()
            || self.method.len() > 16
            || !self.method.bytes().all(|byte| byte.is_ascii_uppercase())
        {
            return Err(invalid("expected HTTP method is invalid"));
        }
        let request_head_bytes = encoded_header_bytes(&self.headers).and_then(|header_bytes| {
            self.method
                .len()
                .checked_add(self.target.len())?
                .checked_add(header_bytes)?
                .checked_add(16)
        });
        if self.target.is_empty()
            || !self.target.starts_with('/')
            || request_head_bytes.is_none_or(|bytes| bytes > limits.max_header_bytes())
        {
            return Err(invalid("expected HTTP target is invalid or oversized"));
        }
        if self.headers.len() > limits.max_headers() || self.body.len() > limits.max_body_bytes() {
            return Err(invalid("expected HTTP request exceeds configured limits"));
        }
        Ok(())
    }

    /// Selects exact or allow-additional header matching.
    #[must_use]
    pub const fn header_match_mode(mut self, header_mode: HeaderMatchMode) -> Self {
        self.header_mode = header_mode;
        self
    }

    pub(crate) fn matches(&self, request: &ParsedRequest) -> bool {
        self.method == request.method
            && self.target == request.target
            && self.body == request.body
            && headers_match(&self.headers, &request.headers, self.header_mode)
    }
}

impl fmt::Debug for ExpectedHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExpectedHttpRequest")
            .field("method", &self.method)
            .field("target_bytes", &self.target.len())
            .field("target", &"[REDACTED]")
            .field("headers", &self.headers)
            .field("body_bytes", &self.body.len())
            .field("body", &"[REDACTED]")
            .field("header_mode", &self.header_mode)
            .finish()
    }
}

/// A deliberate response termination point.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FakeHttpFault {
    /// Send the complete scripted response.
    #[default]
    Complete,
    /// Close immediately after the response head.
    CloseAfterHeaders,
    /// Close after sending the given number of body chunks.
    CloseAfterChunks(usize),
}

/// An optional point at which the owned worker waits for explicit release.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FakeHttpReleasePoint {
    /// Wait before writing the response head.
    BeforeHeaders,
    /// Wait before writing the zero-based body chunk.
    BeforeChunk(usize),
    /// Wait immediately before a scripted close.
    BeforeClose,
}

/// A bounded HTTP response script.
#[derive(Clone, Eq, PartialEq)]
pub struct ScriptedHttpResponse {
    pub(crate) status: u16,
    pub(crate) headers: Vec<FakeHttpHeader>,
    pub(crate) chunks: Vec<Vec<u8>>,
    pub(crate) fault: FakeHttpFault,
    pub(crate) release: Option<FakeHttpReleasePoint>,
}

/// One expected request and scripted response in a bounded multi-exchange server sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FakeHttpExchangeScript {
    pub(crate) expected: ExpectedHttpRequest,
    pub(crate) response: ScriptedHttpResponse,
}

impl FakeHttpExchangeScript {
    /// Binds one validated request expectation to its response script.
    #[must_use]
    pub const fn new(expected: ExpectedHttpRequest, response: ScriptedHttpResponse) -> Self {
        Self { expected, response }
    }

    pub(crate) fn validate(&self, limits: FakeHttpLimits) -> Result<(), FakeHttpError> {
        self.expected.validate(limits)?;
        self.response.validate(limits)
    }
}

impl fmt::Debug for ScriptedHttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScriptedHttpResponse")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("chunk_count", &self.chunks.len())
            .field("body_bytes", &self.chunks.iter().map(Vec::len).sum::<usize>())
            .field("chunks", &"[REDACTED]")
            .field("fault", &self.fault)
            .field("release", &self.release)
            .finish()
    }
}

impl ScriptedHttpResponse {
    /// Creates and validates a response script.
    ///
    /// # Errors
    ///
    /// Returns an invalid-configuration error when status, headers, chunks, or control points are
    /// invalid or exceed `limits`.
    pub fn new(
        status: u16,
        headers: Vec<FakeHttpHeader>,
        chunks: Vec<Vec<u8>>,
        fault: FakeHttpFault,
        release: Option<FakeHttpReleasePoint>,
        limits: FakeHttpLimits,
    ) -> Result<Self, FakeHttpError> {
        let value = Self { status, headers, chunks, fault, release };
        value.validate(limits)?;
        Ok(value)
    }

    pub(crate) fn validate(&self, limits: FakeHttpLimits) -> Result<(), FakeHttpError> {
        let total = self.chunks.iter().try_fold(0_usize, |sum, chunk| sum.checked_add(chunk.len()));
        let response_head_bytes =
            encoded_header_bytes(&self.headers).and_then(|bytes| bytes.checked_add(32));
        if !(100..=599).contains(&self.status)
            || self.headers.len() > limits.max_headers()
            || response_head_bytes.is_none_or(|bytes| bytes > limits.max_header_bytes())
            || self.chunks.len() > limits.max_chunks()
            || self.chunks.iter().any(|chunk| chunk.len() > limits.max_chunk_bytes())
            || total.is_none_or(|total| total > limits.max_body_bytes())
            || !valid_chunk_index(self.fault, self.chunks.len())
            || !valid_release(self.release, self.fault, self.chunks.len())
        {
            return Err(invalid("scripted HTTP response is invalid or oversized"));
        }
        Ok(())
    }
}

const fn invalid(detail: &'static str) -> FakeHttpError {
    FakeHttpError::new(FakeHttpErrorKind::InvalidConfiguration, detail)
}
