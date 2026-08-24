//! Redacted observations retained after an HTTP exchange.

use super::model::FakeHttpHeader;
use sha2::{Digest, Sha256};

/// A redacted captured header. Raw values are never retained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedHttpHeader {
    name: String,
    value_bytes: usize,
    value_sha256: [u8; 32],
    sensitive: bool,
}

impl CapturedHttpHeader {
    /// Returns the normalized lowercase name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the raw value byte count.
    #[must_use]
    pub const fn value_bytes(&self) -> usize {
        self.value_bytes
    }

    /// Returns the SHA-256 digest of the raw value.
    #[must_use]
    pub const fn value_sha256(&self) -> [u8; 32] {
        self.value_sha256
    }

    /// Returns whether the header name conventionally identifies credential material.
    #[must_use]
    pub const fn is_sensitive(&self) -> bool {
        self.sensitive
    }
}

/// A bounded, redacted request observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedHttpRequest {
    method: String,
    target_bytes: usize,
    target_sha256: [u8; 32],
    headers: Vec<CapturedHttpHeader>,
    body_bytes: usize,
    body_sha256: [u8; 32],
    matched: bool,
}

impl CapturedHttpRequest {
    /// Returns the bounded validated method.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Returns the request-target byte count.
    #[must_use]
    pub const fn target_bytes(&self) -> usize {
        self.target_bytes
    }

    /// Returns the request-target SHA-256 digest.
    #[must_use]
    pub const fn target_sha256(&self) -> [u8; 32] {
        self.target_sha256
    }

    /// Returns redacted captured headers.
    #[must_use]
    pub fn headers(&self) -> &[CapturedHttpHeader] {
        &self.headers
    }

    /// Returns the request-body byte count.
    #[must_use]
    pub const fn body_bytes(&self) -> usize {
        self.body_bytes
    }

    /// Returns the request-body SHA-256 digest.
    #[must_use]
    pub const fn body_sha256(&self) -> [u8; 32] {
        self.body_sha256
    }

    /// Returns whether the complete request matched the caller-owned expectation.
    #[must_use]
    pub const fn matched(&self) -> bool {
        self.matched
    }
}

/// How the response-writing side of an exchange ended.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FakeHttpTermination {
    /// Every scripted response byte was written.
    Completed,
    /// The script deliberately closed at its configured fault point.
    ScriptedClose,
    /// The peer closed while the response was being written.
    PeerClosed,
}

/// The complete direct observation from one fake HTTP exchange.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FakeHttpExchange {
    request: CapturedHttpRequest,
    chunks_sent: usize,
    termination: FakeHttpTermination,
}

impl FakeHttpExchange {
    /// Returns the redacted request observation.
    #[must_use]
    pub const fn request(&self) -> &CapturedHttpRequest {
        &self.request
    }

    /// Returns the count of completely written body chunks.
    #[must_use]
    pub const fn chunks_sent(&self) -> usize {
        self.chunks_sent
    }

    /// Returns the response termination classification.
    #[must_use]
    pub const fn termination(&self) -> FakeHttpTermination {
        self.termination
    }
}

pub struct ParsedRequest {
    pub method: String,
    pub target: String,
    pub headers: Vec<FakeHttpHeader>,
    pub body: Vec<u8>,
}

impl ParsedRequest {
    pub(crate) fn capture(&self, matched: bool) -> CapturedHttpRequest {
        CapturedHttpRequest {
            method: self.method.clone(),
            target_bytes: self.target.len(),
            target_sha256: digest(self.target.as_bytes()),
            headers: self.headers.iter().map(capture_header).collect(),
            body_bytes: self.body.len(),
            body_sha256: digest(&self.body),
            matched,
        }
    }
}

pub const fn exchange(
    request: CapturedHttpRequest,
    chunks_sent: usize,
    termination: FakeHttpTermination,
) -> FakeHttpExchange {
    FakeHttpExchange { request, chunks_sent, termination }
}

fn capture_header(header: &FakeHttpHeader) -> CapturedHttpHeader {
    let name = header.name().to_ascii_lowercase();
    CapturedHttpHeader {
        sensitive: is_sensitive_name(&name),
        name,
        value_bytes: header.value().len(),
        value_sha256: digest(header.value()),
    }
}

fn is_sensitive_name(name: &str) -> bool {
    ["authorization", "cookie", "token", "secret", "api-key", "api_key", "proxy-auth"]
        .iter()
        .any(|marker| name.contains(marker))
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
