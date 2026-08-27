//! Stable startup and runtime composition failures for configured tools.

use core::fmt;

use peritus_tool_router::{RouterError, RouterErrorKind};

const MAX_DETAIL_BYTES: usize = 4_096;

/// Stable class for a tool-component composition failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolComponentErrorKind {
    /// The configured allowlist exceeds its hard production bound.
    Capacity,
    /// A configured name is repeated.
    DuplicateTool,
    /// A configured name has no production dispatcher route.
    UnknownTool,
    /// A compiled descriptor catalog is internally inconsistent.
    Catalog,
    /// The exact B1 operation registry could not be constructed.
    OperationRegistry,
    /// The C4 descriptor registry rejected its exact bindings.
    ToolRegistry,
    /// A call requires a tool while the configured allowlist is empty.
    NoToolsConfigured,
    /// A prepared call is not the exact configured descriptor.
    UnregisteredCall,
    /// A scoped dispatcher is not the registered implementation for the call.
    DispatcherMismatch,
    /// The authoritative C4 router rejected the operation.
    Router,
}

impl ToolComponentErrorKind {
    /// Returns the compatibility-stable daemon component code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Capacity => "PERITUS-DAEMON-TOOL-001",
            Self::DuplicateTool => "PERITUS-DAEMON-TOOL-002",
            Self::UnknownTool => "PERITUS-DAEMON-TOOL-003",
            Self::Catalog => "PERITUS-DAEMON-TOOL-004",
            Self::OperationRegistry => "PERITUS-DAEMON-TOOL-005",
            Self::ToolRegistry => "PERITUS-DAEMON-TOOL-006",
            Self::NoToolsConfigured => "PERITUS-DAEMON-TOOL-007",
            Self::UnregisteredCall => "PERITUS-DAEMON-TOOL-008",
            Self::DispatcherMismatch => "PERITUS-DAEMON-TOOL-009",
            Self::Router => "PERITUS-DAEMON-TOOL-010",
        }
    }
}

/// Bounded failure with an optional underlying C4 router classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolComponentError {
    kind: ToolComponentErrorKind,
    operation: &'static str,
    detail: String,
    router_kind: Option<RouterErrorKind>,
}

impl ToolComponentError {
    pub(super) fn new(
        kind: ToolComponentErrorKind,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self { kind, operation, detail: bounded(detail.into()), router_kind: None }
    }

    pub(super) fn router(error: RouterError) -> Self {
        Self {
            kind: ToolComponentErrorKind::Router,
            operation: error.operation(),
            detail: error.detail().to_owned(),
            router_kind: Some(error.kind()),
        }
    }

    /// Returns the stable component failure class.
    #[must_use]
    pub const fn kind(&self) -> ToolComponentErrorKind {
        self.kind
    }

    /// Returns the failed composition or routing operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Returns bounded, credential-free diagnostic detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// Returns the authoritative C4 classification when the router rejected the request.
    #[must_use]
    pub const fn router_kind(&self) -> Option<RouterErrorKind> {
        self.router_kind
    }
}

impl fmt::Display for ToolComponentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} during {}: {}", self.kind.code(), self.operation, self.detail)
    }
}

impl std::error::Error for ToolComponentError {}

fn bounded(mut detail: String) -> String {
    if detail.len() <= MAX_DETAIL_BYTES {
        return detail;
    }
    let mut end = MAX_DETAIL_BYTES;
    while !detail.is_char_boundary(end) {
        end -= 1;
    }
    detail.truncate(end);
    detail
}
