//! Checked HTTP header names, values, and collections.

use core::fmt;

use zeroize::Zeroizing;

use crate::{ProviderCoreError, ProviderCoreErrorKind};

use super::{HttpLimits, http_error};

/// A syntactically valid lowercase HTTP header name.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HeaderName(String);

impl HeaderName {
    /// Validates and canonicalizes an HTTP header name.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, non-ASCII, or non-token input.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "taking ownership makes the validated-value boundary explicit"
    )]
    pub fn new(value: String) -> Result<Self, ProviderCoreError> {
        if value.is_empty() || value.len() > 256 || !value.bytes().all(is_header_name_byte) {
            return Err(http_error(
                ProviderCoreErrorKind::InvalidHttp,
                "header name is empty, oversized, or contains a non-token byte",
            ));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    /// Borrows the canonical name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn unsafe_for_request(&self) -> bool {
        self.0.starts_with("proxy-")
            || matches!(
                self.0.as_str(),
                "connection"
                    | "content-length"
                    | "host"
                    | "keep-alive"
                    | "proxy-authenticate"
                    | "proxy-authorization"
                    | "te"
                    | "trailer"
                    | "transfer-encoding"
                    | "upgrade"
            )
    }
}

impl fmt::Debug for HeaderName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("HeaderName").field(&self.0).finish()
    }
}

/// An owned checked HTTP header value whose formatting never reveals the bytes.
#[derive(Eq, PartialEq)]
pub struct HeaderValue {
    bytes: Zeroizing<Vec<u8>>,
    sensitive: bool,
}

impl HeaderValue {
    /// Creates a nonsensitive checked value.
    ///
    /// # Errors
    ///
    /// Rejects carriage returns, line feeds, NUL, DEL, or other forbidden controls.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ProviderCoreError> {
        Self::checked(bytes, false)
    }

    pub(crate) fn sensitive(bytes: Vec<u8>) -> Result<Self, ProviderCoreError> {
        Self::checked(bytes, true)
    }

    fn checked(bytes: Vec<u8>, sensitive: bool) -> Result<Self, ProviderCoreError> {
        if !bytes.iter().copied().all(is_header_value_byte) {
            return Err(http_error(
                ProviderCoreErrorKind::InvalidHttp,
                "header value contains a forbidden control byte",
            ));
        }
        Ok(Self { bytes: Zeroizing::new(bytes), sensitive })
    }

    /// Borrows a response value only when it is known to be nonsensitive.
    ///
    /// Credential-bearing request values and sensitive response headers return `None`.
    #[must_use]
    pub fn nonsensitive_bytes(&self) -> Option<&[u8]> {
        (!self.sensitive).then_some(self.bytes.as_slice())
    }

    /// Returns whether the value is credential-bearing and must remain secret.
    #[must_use]
    pub const fn is_sensitive(&self) -> bool {
        self.sensitive
    }

    pub(crate) fn wire_bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

impl fmt::Debug for HeaderValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HeaderValue([redacted])")
    }
}

/// One owned HTTP header.
#[derive(Eq, PartialEq)]
pub struct Header {
    name: HeaderName,
    value: HeaderValue,
}

impl Header {
    /// Creates a nonsensitive header.
    ///
    /// # Errors
    ///
    /// Returns validation failures from the header value.
    pub fn new(name: HeaderName, value: Vec<u8>) -> Result<Self, ProviderCoreError> {
        Ok(Self { name, value: HeaderValue::new(value)? })
    }

    pub(crate) const fn from_value(name: HeaderName, value: HeaderValue) -> Self {
        Self { name, value }
    }

    /// Returns the canonical name.
    #[must_use]
    pub const fn name(&self) -> &HeaderName {
        &self.name
    }

    /// Returns the redaction-aware value.
    #[must_use]
    pub const fn value(&self) -> &HeaderValue {
        &self.value
    }
}

impl fmt::Debug for Header {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Header")
            .field("name", &self.name)
            .field("value", &self.value)
            .finish()
    }
}

/// A bounded owned sequence of HTTP headers.
#[derive(Eq, PartialEq)]
pub struct HttpHeaders {
    entries: Vec<Header>,
    byte_count: usize,
}

impl HttpHeaders {
    /// Validates cumulative header count and bytes.
    ///
    /// # Errors
    ///
    /// Rejects headers outside the supplied bounds.
    pub fn new(entries: Vec<Header>, limits: HttpLimits) -> Result<Self, ProviderCoreError> {
        let byte_count = entries.iter().try_fold(0_usize, |total, header| {
            total
                .checked_add(header.name.as_str().len())
                .and_then(|value| value.checked_add(header.value.wire_bytes().len()))
                .ok_or_else(|| {
                    http_error(ProviderCoreErrorKind::LimitExceeded, "header byte count overflowed")
                })
        })?;
        if entries.len() > limits.max_headers() || byte_count > limits.max_header_bytes() {
            return Err(http_error(
                ProviderCoreErrorKind::LimitExceeded,
                "headers exceed their count or cumulative byte bound",
            ));
        }
        Ok(Self { entries, byte_count })
    }

    /// Creates an empty header set.
    #[must_use]
    pub const fn empty() -> Self {
        Self { entries: Vec::new(), byte_count: 0 }
    }

    /// Returns the number of entries.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether there are no entries.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns cumulative name and value bytes.
    #[must_use]
    pub const fn byte_count(&self) -> usize {
        self.byte_count
    }

    /// Iterates the headers in wire order.
    pub fn iter(&self) -> core::slice::Iter<'_, Header> {
        self.entries.iter()
    }

    /// Returns the first value with the supplied canonical or mixed-case name.
    #[must_use]
    pub fn first(&self, name: &str) -> Option<&HeaderValue> {
        self.entries
            .iter()
            .find(|header| header.name.as_str().eq_ignore_ascii_case(name))
            .map(Header::value)
    }

    pub(crate) fn validate_for_request(&self, limits: HttpLimits) -> Result<(), ProviderCoreError> {
        if self.entries.iter().any(|header| header.name.unsafe_for_request()) {
            return Err(http_error(
                ProviderCoreErrorKind::InvalidHttp,
                "request contains a connection-controlled or proxy header",
            ));
        }
        self.validate_limits(limits)
    }

    pub(crate) const fn validate_limits(
        &self,
        limits: HttpLimits,
    ) -> Result<(), ProviderCoreError> {
        if self.len() > limits.max_headers() || self.byte_count > limits.max_header_bytes() {
            return Err(http_error(
                ProviderCoreErrorKind::LimitExceeded,
                "headers exceed transport limits",
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for HttpHeaders {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpHeaders")
            .field("entry_count", &self.entries.len())
            .field("byte_count", &self.byte_count)
            .finish_non_exhaustive()
    }
}

impl<'a> IntoIterator for &'a HttpHeaders {
    type Item = &'a Header;
    type IntoIter = core::slice::Iter<'a, Header>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
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

const fn is_header_value_byte(byte: u8) -> bool {
    byte == b'\t' || (byte >= 0x20 && byte != 0x7f)
}
