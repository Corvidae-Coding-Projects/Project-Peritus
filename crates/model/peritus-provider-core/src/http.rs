//! Bounded owned HTTP request, response, status, and header values.

use crate::{ProviderCoreError, ProviderCoreErrorKind};

mod header;
mod limits;
mod message;

pub use header::{Header, HeaderName, HeaderValue, HttpHeaders};
pub use limits::HttpLimits;
pub use message::{HttpMethod, HttpRequest, HttpResponse, StatusCode};

pub const fn http_error(kind: ProviderCoreErrorKind, detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::new(kind, "http", detail)
}
