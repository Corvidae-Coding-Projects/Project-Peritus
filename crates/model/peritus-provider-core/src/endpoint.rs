//! Validated HTTP(S) endpoints.

use core::fmt;

use crate::{ProviderCoreError, ProviderCoreErrorKind};

const MAX_ENDPOINT_BYTES: usize = 4_096;

/// An absolute, credential-free HTTP(S) endpoint.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct Endpoint {
    serialized: String,
    parsed: url::Url,
}

impl Endpoint {
    /// Parses and validates an endpoint.
    ///
    /// # Errors
    ///
    /// Rejects oversized or non-absolute URLs, schemes other than HTTP(S), user information,
    /// fragments, secret-bearing query fields, backslashes, and path traversal segments.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "taking ownership makes the validated-value boundary explicit"
    )]
    pub fn new(value: String) -> Result<Self, ProviderCoreError> {
        if value.is_empty()
            || value.len() > MAX_ENDPOINT_BYTES
            || value.trim() != value
            || value.contains('#')
            || value.contains('\\')
        {
            return Err(invalid_endpoint("endpoint syntax is unsafe or outside its byte bound"));
        }
        validate_raw_path(&value)?;
        let parsed = url::Url::parse(&value)
            .map_err(|_| invalid_endpoint("endpoint is not a valid absolute URL"))?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err(invalid_endpoint("endpoint must use HTTP(S) and include a host"));
        }
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
        {
            return Err(invalid_endpoint(
                "endpoint must not contain user information or a fragment",
            ));
        }
        for (name, _) in parsed.query_pairs() {
            if secret_query_name(&name) {
                return Err(invalid_endpoint("endpoint query contains a secret-bearing field"));
            }
        }
        let serialized = parsed.as_str().to_owned();
        Ok(Self { serialized, parsed })
    }

    /// Borrows the canonical serialized endpoint.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    /// Returns a validated endpoint with the supplied absolute path.
    ///
    /// # Errors
    ///
    /// Rejects a non-absolute path, query, fragment, backslash, traversal, or oversized result.
    pub fn with_path(&self, path: &str) -> Result<Self, ProviderCoreError> {
        if !path.starts_with('/') || path.contains(['?', '#', '\\']) {
            return Err(invalid_endpoint("endpoint path must be absolute and contain no query"));
        }
        validate_path_segments(path)?;
        let mut next = self.parsed.clone();
        next.set_path(path);
        next.set_query(None);
        Self::new(next.to_string())
    }

    pub(crate) const fn url(&self) -> &url::Url {
        &self.parsed
    }
}

impl fmt::Debug for Endpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Endpoint([redacted])")
    }
}

fn validate_raw_path(value: &str) -> Result<(), ProviderCoreError> {
    let scheme_end =
        value.find("://").ok_or_else(|| invalid_endpoint("endpoint is not an absolute URL"))?;
    let after_authority = &value[scheme_end + 3..];
    let raw_path = after_authority.find('/').map_or("/", |start| &after_authority[start..]);
    let raw_path = raw_path.split('?').next().unwrap_or(raw_path);
    validate_path_segments(raw_path)
}

fn validate_path_segments(raw_path: &str) -> Result<(), ProviderCoreError> {
    for segment in raw_path.split('/') {
        let decoded = decode_path_segment(segment)?;
        if matches!(decoded.as_str(), "." | "..") {
            return Err(invalid_endpoint("endpoint path must not contain traversal segments"));
        }
    }
    Ok(())
}

fn decode_path_segment(segment: &str) -> Result<String, ProviderCoreError> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(invalid_endpoint("endpoint contains malformed percent encoding"));
            }
            let high = hex(bytes[index + 1])
                .ok_or_else(|| invalid_endpoint("endpoint contains malformed percent encoding"))?;
            let low = hex(bytes[index + 2])
                .ok_or_else(|| invalid_endpoint("endpoint contains malformed percent encoding"))?;
            let value = high * 16 + low;
            if value == b'\\' {
                return Err(invalid_endpoint("endpoint path must not contain backslashes"));
            }
            decoded.push(value);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded)
        .map_err(|_| invalid_endpoint("endpoint path encoding is not valid UTF-8"))
}

const fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn secret_query_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase().replace('-', "_");
    matches!(
        name.as_str(),
        "key"
            | "api_key"
            | "apikey"
            | "token"
            | "access_token"
            | "auth"
            | "authorization"
            | "authentication"
            | "bearer"
            | "password"
            | "secret"
            | "credential"
            | "signature"
            | "sig"
            | "x_goog_api_key"
    ) || name.ends_with("_token")
        || name.ends_with("_secret")
        || name.ends_with("_password")
        || name.ends_with("_key")
        || name.ends_with("_credential")
        || name.ends_with("_signature")
        || name.ends_with("_sig")
}

const fn invalid_endpoint(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::new(ProviderCoreErrorKind::InvalidEndpoint, "endpoint", detail)
}
