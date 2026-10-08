//! Public provider, model, extension, tool, and output names.

use core::fmt;

use super::CheckedIdentity;
use crate::ProtocolError;

/// Checked provider name.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderName(CheckedIdentity);

impl ProviderName {
    /// Creates a checked provider name.
    ///
    /// # Errors
    ///
    /// Rejects empty or control-containing input. Enclosing protocols own physical byte limits.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        CheckedIdentity::new(value, "provider_name").map(Self)
    }

    /// Borrows the checked value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ProviderName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ProviderName").field(&self.as_str()).finish()
    }
}

/// Checked model name.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModelName(CheckedIdentity);

impl ModelName {
    /// Creates a checked model name.
    ///
    /// # Errors
    ///
    /// Rejects empty or control-containing input. Enclosing protocols own physical frame limits.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        CheckedIdentity::new(value, "model_name").map(Self)
    }

    /// Borrows the checked value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ModelName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ModelName").field(&self.as_str()).finish()
    }
}

/// Checked provider extension name.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExtensionName(CheckedIdentity);

impl ExtensionName {
    /// Creates a checked provider extension name.
    ///
    /// # Errors
    ///
    /// Rejects empty or control-containing input. Enclosing protocols own physical byte limits.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        CheckedIdentity::new(value, "extension_name").map(Self)
    }

    /// Borrows the checked value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ExtensionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ExtensionName").field(&self.as_str()).finish()
    }
}

/// Checked tool name.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ToolName(CheckedIdentity);

impl ToolName {
    /// Creates a checked tool name.
    ///
    /// # Errors
    ///
    /// Rejects empty or control-containing input. Provider adapters validate their wire grammar.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        CheckedIdentity::new(value, "tool_name").map(Self)
    }

    /// Borrows the checked value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ToolName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ToolName").field(&self.as_str()).finish()
    }
}

/// Checked structured-output name.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OutputName(CheckedIdentity);

impl OutputName {
    /// Creates a checked structured-output name.
    ///
    /// # Errors
    ///
    /// Rejects empty or control-containing input. Provider adapters validate their wire grammar.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        CheckedIdentity::new(value, "output_name").map(Self)
    }

    /// Borrows the checked value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for OutputName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("OutputName").field(&self.as_str()).finish()
    }
}
