//! Owned HTTP methods, status, requests, and responses.

use core::fmt;

use crate::{ByteStream, Endpoint, ProviderCoreError, ProviderCoreErrorKind};

use super::{HttpHeaders, HttpLimits, http_error};

/// Supported HTTP methods.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HttpMethod {
    /// `GET`.
    Get,
    /// `POST`.
    Post,
    /// `PUT`.
    Put,
    /// `PATCH`.
    Patch,
    /// `DELETE`.
    Delete,
}

/// An HTTP status code in the inclusive range 100 through 599.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StatusCode(u16);

impl StatusCode {
    /// Creates a checked status code.
    ///
    /// # Errors
    ///
    /// Rejects values outside 100 through 599.
    pub fn new(value: u16) -> Result<Self, ProviderCoreError> {
        if !(100..=599).contains(&value) {
            return Err(http_error(
                ProviderCoreErrorKind::InvalidHttp,
                "HTTP status is out of range",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the numeric status.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    /// Returns whether this is a successful status.
    #[must_use]
    pub const fn is_success(self) -> bool {
        self.0 >= 200 && self.0 < 300
    }
}

/// A validated, bounded, owned HTTP request.
pub struct HttpRequest {
    method: HttpMethod,
    endpoint: Endpoint,
    headers: HttpHeaders,
    body: Vec<u8>,
}

impl HttpRequest {
    /// Creates a request within the supplied limits.
    ///
    /// # Errors
    ///
    /// Rejects an oversized body, unsafe outbound headers, or headers outside transport limits.
    pub fn new(
        method: HttpMethod,
        endpoint: Endpoint,
        headers: HttpHeaders,
        body: Vec<u8>,
        limits: HttpLimits,
    ) -> Result<Self, ProviderCoreError> {
        headers.validate_for_request(limits)?;
        if body.len() > limits.max_request_body_bytes() {
            return Err(http_error(
                ProviderCoreErrorKind::LimitExceeded,
                "request body exceeds its byte bound",
            ));
        }
        Ok(Self { method, endpoint, headers, body })
    }

    /// Returns the method.
    #[must_use]
    pub const fn method(&self) -> HttpMethod {
        self.method
    }

    /// Returns the endpoint.
    #[must_use]
    pub const fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Returns the request headers.
    #[must_use]
    pub const fn headers(&self) -> &HttpHeaders {
        &self.headers
    }

    /// Returns the provider wire body.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub(crate) fn into_parts(self) -> (HttpMethod, Endpoint, HttpHeaders, Vec<u8>) {
        (self.method, self.endpoint, self.headers, self.body)
    }
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("endpoint", &self.endpoint)
            .field("headers", &self.headers)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

/// Owned HTTP response metadata and a bounded pull-based body.
pub struct HttpResponse {
    status: StatusCode,
    headers: HttpHeaders,
    body: Box<dyn ByteStream>,
}

impl HttpResponse {
    /// Creates a response, wrapping the body with the supplied bounds.
    ///
    /// # Errors
    ///
    /// Rejects headers outside the supplied transport limits.
    pub fn new(
        status: StatusCode,
        headers: HttpHeaders,
        body: Box<dyn ByteStream>,
        limits: HttpLimits,
    ) -> Result<Self, ProviderCoreError> {
        headers.validate_limits(limits)?;
        Ok(Self {
            status,
            headers,
            body: Box::new(crate::transport::BoundedByteStream::new(body, limits)),
        })
    }

    /// Returns the response status.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns response headers.
    #[must_use]
    pub const fn headers(&self) -> &HttpHeaders {
        &self.headers
    }

    /// Splits metadata from the owned body stream.
    #[must_use]
    pub fn into_parts(self) -> (StatusCode, HttpHeaders, Box<dyn ByteStream>) {
        (self.status, self.headers, self.body)
    }
}

impl fmt::Debug for HttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpResponse")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("body", &"[stream]")
            .finish()
    }
}
