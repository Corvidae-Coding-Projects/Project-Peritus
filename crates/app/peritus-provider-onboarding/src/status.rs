//! Minimal user-facing provider status facts.

use std::path::{Path, PathBuf};

use peritus_product_state::ProviderKind;

/// Current non-secret provider login state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderStatus {
    /// The official executable is installed and reports a usable account.
    Ready,
    /// The official executable is installed but has no usable account session.
    SignedOut,
    /// The official executable is not installed or cannot be pinned.
    Unavailable,
    /// The installed executable returned a response whose login state is not understood.
    Unknown,
    /// Peritus could not obtain status because local process infrastructure failed.
    Infrastructure,
    /// A legacy uncertain state retained for source compatibility.
    NeedsAttention,
}

impl ProviderStatus {
    /// Returns a concise state label that never contains account data.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::SignedOut => "Sign in",
            Self::Unavailable => "Not installed",
            Self::Unknown => "Status unknown",
            Self::Infrastructure => "Status unavailable",
            Self::NeedsAttention => "Needs attention",
        }
    }
}

/// One observed account-backed provider card.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderObservation {
    kind: ProviderKind,
    status: ProviderStatus,
    executable: Option<PathBuf>,
    diagnostic: Option<String>,
}

impl ProviderObservation {
    pub(crate) const fn new(
        kind: ProviderKind,
        status: ProviderStatus,
        executable: Option<PathBuf>,
    ) -> Self {
        Self { kind, status, executable, diagnostic: None }
    }

    pub(crate) fn with_diagnostic(
        kind: ProviderKind,
        status: ProviderStatus,
        executable: Option<PathBuf>,
        diagnostic: String,
    ) -> Self {
        Self { kind, status, executable, diagnostic: Some(diagnostic) }
    }

    /// Returns the provider kind.
    #[must_use]
    pub const fn kind(&self) -> ProviderKind {
        self.kind
    }

    /// Returns the current status.
    #[must_use]
    pub const fn status(&self) -> ProviderStatus {
        self.status
    }

    /// Borrows the checked executable path when installed.
    #[must_use]
    pub fn executable(&self) -> Option<&Path> {
        self.executable.as_deref()
    }

    /// Borrows a credential-safe causal diagnostic when status could not be established.
    #[must_use]
    pub fn diagnostic(&self) -> Option<&str> {
        self.diagnostic.as_deref()
    }
}
