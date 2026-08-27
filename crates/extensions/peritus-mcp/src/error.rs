//! MCP transport and authority-bridge failures.

use std::{error::Error, fmt};

/// Stable bridge failure class supplied by G0/C4 adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BridgeErrorClass {
    /// Request syntax or semantic input is invalid.
    InvalidRequest,
    /// Current authority does not expose or permit the request.
    Authorization,
    /// Requested tool/resource/prompt does not exist in the exposed view.
    NotFound,
    /// A downstream operation was cancelled.
    Cancelled,
    /// A downstream deadline elapsed.
    Timeout,
    /// A required subsystem is unavailable.
    Infrastructure,
    /// A downstream result could not be classified safely.
    Indeterminate,
}

/// Typed error returned by the daemon-owned MCP bridge implementation.
#[derive(Debug)]
pub struct BridgeError {
    class: BridgeErrorClass,
    code: String,
    detail: String,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl BridgeError {
    /// Creates a bridge error without a source.
    #[must_use]
    pub fn new(
        class: BridgeErrorClass,
        code: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::build(class, code.into(), detail.into(), None)
    }

    /// Creates a bridge error preserving a source.
    pub fn with_source(
        class: BridgeErrorClass,
        code: impl Into<String>,
        detail: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::build(class, code.into(), detail.into(), Some(Box::new(source)))
    }

    fn build(
        class: BridgeErrorClass,
        mut code: String,
        mut detail: String,
        source: Option<Box<dyn Error + Send + Sync>>,
    ) -> Self {
        code.truncate(128);
        detail.truncate(1024);
        Self { class, code, detail, source }
    }

    /// Returns the stable class.
    #[must_use]
    pub const fn class(&self) -> BridgeErrorClass {
        self.class
    }

    /// Borrows the stable downstream code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Borrows bounded causal detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl Error for BridgeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|source| source as &(dyn Error + 'static))
    }
}

/// MCP server/transport failure class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum McpErrorClass {
    /// Input JSON-RPC framing or syntax is invalid.
    Protocol,
    /// A configured server limit was exceeded.
    Limit,
    /// I/O failed.
    Transport,
    /// A bridge operation failed.
    Bridge,
    /// Server lifecycle is invalid for the request.
    Lifecycle,
}

/// Typed MCP server failure.
#[derive(Debug)]
pub struct McpError {
    class: McpErrorClass,
    operation: &'static str,
    detail: String,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl McpError {
    /// Creates an MCP error without a source.
    #[must_use]
    pub fn new(class: McpErrorClass, operation: &'static str, detail: impl Into<String>) -> Self {
        Self::build(class, operation, detail.into(), None)
    }

    /// Creates an MCP error preserving a source.
    pub fn with_source(
        class: McpErrorClass,
        operation: &'static str,
        detail: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::build(class, operation, detail.into(), Some(Box::new(source)))
    }

    fn build(
        class: McpErrorClass,
        operation: &'static str,
        mut detail: String,
        source: Option<Box<dyn Error + Send + Sync>>,
    ) -> Self {
        detail.truncate(1024);
        Self { class, operation, detail, source }
    }

    /// Returns the stable class.
    #[must_use]
    pub const fn class(&self) -> McpErrorClass {
        self.class
    }

    /// Returns the operation that failed.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Borrows bounded detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for McpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.detail)
    }
}

impl Error for McpError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref().map(|source| source as &(dyn Error + 'static))
    }
}
