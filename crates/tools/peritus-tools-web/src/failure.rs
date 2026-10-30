//! Classified page failures and bounded recovery hints.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt;

/// Why a page could not be read successfully.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// A browser or human must complete a site challenge.
    ChallengeRequired,
    /// The upstream asked the caller to wait.
    RateLimited,
    /// The page requires authentication.
    AuthenticationRequired,
    /// The server denied access without an identified challenge.
    Forbidden,
    /// A successful HTML response has no readable body.
    NeedsRendering,
    /// A temporary upstream or transport failure occurred.
    Transient,
    /// Another unsuccessful HTTP response was received.
    Http,
}

/// An unsuccessful page observation, never a retained research source.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PageFailure {
    pub(crate) kind: FailureKind,
    pub(crate) url: String,
    pub(crate) status: u16,
    pub(crate) retry_after: Option<String>,
    pub(crate) diagnostic: String,
}

impl PageFailure {
    pub(crate) fn new(kind: FailureKind, url: &str, status: u16, diagnostic: &str) -> Self {
        Self {
            kind,
            url: url.into(),
            status,
            retry_after: None,
            diagnostic: diagnostic.chars().take(500).collect(),
        }
    }

    /// Whether an isolated browser can be attempted under existing process authority.
    #[must_use]
    pub const fn rendering_recommended(&self) -> bool {
        self.retry_after.is_none()
            && matches!(self.kind, FailureKind::ChallengeRequired | FailureKind::NeedsRendering)
    }

    /// A structured, unsuccessful observation with an actionable recovery hint.
    #[must_use]
    pub fn observation(&self) -> Value {
        let next = match self.kind {
            FailureKind::ChallengeRequired => {
                "Use an authorized selected browser tab if isolated rendering cannot complete the challenge. Do not repeatedly retry this blocked route."
            }
            FailureKind::NeedsRendering => "Render this page with process permission.",
            FailureKind::RateLimited => {
                "Honor retry_after/backoff before requesting this origin again."
            }
            FailureKind::AuthenticationRequired => {
                "Select an authorized signed-in browser tab; credentials are not imported into the HTTP reader."
            }
            FailureKind::Forbidden => {
                "Access was denied. Use an authorized source or selected browser tab; do not blindly retry."
            }
            FailureKind::Transient => {
                "Retry later with bounded backoff, or use another primary source."
            }
            FailureKind::Http => "Check the URL and upstream response before retrying.",
        };
        json!({"success":false,"failure":self.kind,"url":self.url,"http_status":(self.status != 0).then_some(self.status),
            "retry_after":self.retry_after,"diagnostic":self.diagnostic,"next_action":next})
    }
}

impl fmt::Display for PageFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: HTTP {} from {}: {}", self.kind, self.status, self.url, self.diagnostic)?;
        if let Some(retry) = &self.retry_after {
            write!(f, "; Retry-After: {retry}")?;
        }
        Ok(())
    }
}

impl std::error::Error for PageFailure {}

pub const fn classify(status: u16, challenged: bool) -> FailureKind {
    if status == 429 {
        FailureKind::RateLimited
    } else if challenged {
        FailureKind::ChallengeRequired
    } else {
        match status {
            401 => FailureKind::AuthenticationRequired,
            403 => FailureKind::Forbidden,
            500..=599 => FailureKind::Transient,
            _ => FailureKind::Http,
        }
    }
}

pub fn challenge_html(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    (lower.contains("<title>just a moment") && lower.contains("/cdn-cgi/challenge-platform/"))
        || (lower.contains("<title>attention required! | cloudflare")
            && lower.contains("cf-error-details"))
}
