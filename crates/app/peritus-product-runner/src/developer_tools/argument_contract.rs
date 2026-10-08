//! Shared version-one contracts for advertised arguments and their execution boundaries.

/// Stable version returned by capacity-aware developer-tool results.
pub(super) const VERSION: &str = "peritus.developer-tool-arguments.v1";

/// Maximum recursive depth accepted by the provider-facing JSON representation.
pub(super) const MAX_SCHEMA_DEPTH: usize = 64;

/// Maximum bytes in one router-to-process stdin control.
pub(super) const STDIN_CONTROL_BYTES: usize = 65_536;

/// Largest terminal dimension representable by the process protocol.
pub(super) const MAX_TERMINAL_DIMENSION: u64 = u16::MAX as u64;

/// A command-stdin call is transport bounded, then streamed through acknowledged backend chunks.
pub(super) const COMMAND_STDIN_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"contract_version":{"enum":["peritus.developer-tool-arguments.v1"],"type":"string"},"handle":{"type":"string"},"text":{"minLength":1,"type":"string"}},"required":["handle","text"],"type":"object"}"#;

/// Zero/zero observes without resizing; positive dimensions issue one represented backend resize.
pub(super) const COMMAND_RESIZE_SCHEMA: &str = r#"{"additionalProperties":false,"oneOf":[{"properties":{"columns":{"const":0},"rows":{"const":0}}},{"properties":{"columns":{"minimum":1},"rows":{"minimum":1}}}],"properties":{"columns":{"maximum":65535,"minimum":0,"type":"integer"},"contract_version":{"enum":["peritus.developer-tool-arguments.v1"],"type":"string"},"handle":{"type":"string"},"rows":{"maximum":65535,"minimum":0,"type":"integer"}},"required":["columns","handle","rows"],"type":"object"}"#;

/// Each scope call is one transport-bounded enrollment page; later calls extend the same scope.
pub(super) const WORKSPACE_SCOPE_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"contract_version":{"enum":["peritus.developer-tool-arguments.v1"],"type":"string"},"paths":{"items":{"type":"string"},"minItems":1,"type":"array"}},"required":["paths"],"type":"object"}"#;
