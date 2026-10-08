//! Explicit execution-environment contracts.

use crate::SandboxError;

/// A portable environment variable name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EnvironmentName(String);

impl EnvironmentName {
    /// Validates and canonicalizes an environment name to uppercase ASCII.
    ///
    /// # Errors
    /// Rejects empty or non-portable names.
    pub fn new(value: impl Into<String>) -> Result<Self, SandboxError> {
        let mut value = value.into();
        if value.is_empty() || !value.is_ascii() {
            return Err(crate::error::invalid("invalid environment name"));
        }
        if !value.as_bytes()[0].is_ascii_alphabetic() && value.as_bytes()[0] != b'_' {
            return Err(crate::error::invalid("invalid environment name prefix"));
        }
        if !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
            return Err(crate::error::invalid("invalid environment name character"));
        }
        value.make_ascii_uppercase();
        Ok(Self(value))
    }

    /// Returns canonical text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Inheritance behavior for the process environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EnvironmentMode {
    /// Start with an empty environment.
    Cleared,
    /// Inherit only the canonical names in the list.
    AllowListed(Vec<EnvironmentName>),
}

/// Contract for inherited and explicit non-secret environment entries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvironmentContract {
    mode: EnvironmentMode,
    literal_names: Vec<EnvironmentName>,
}

impl EnvironmentContract {
    /// Validates and canonicalizes the environment contract.
    ///
    /// # Errors
    /// Returns an error when supplied names violate their native constructors.
    pub fn new(
        mode: EnvironmentMode,
        mut literal_names: Vec<EnvironmentName>,
    ) -> Result<Self, SandboxError> {
        let mode = match mode {
            EnvironmentMode::Cleared => EnvironmentMode::Cleared,
            EnvironmentMode::AllowListed(mut names) => {
                names.sort();
                names.dedup();
                EnvironmentMode::AllowListed(names)
            }
        };
        literal_names.sort();
        literal_names.dedup();
        Ok(Self { mode, literal_names })
    }

    /// Returns inheritance mode.
    #[must_use]
    pub const fn mode(&self) -> &EnvironmentMode {
        &self.mode
    }
    /// Returns names allowed to receive literal values.
    #[must_use]
    pub fn literal_names(&self) -> &[EnvironmentName] {
        &self.literal_names
    }
    /// Reports whether a host value may be inherited.
    #[must_use]
    pub fn permits_inherited(&self, name: &EnvironmentName) -> bool {
        matches!(&self.mode, EnvironmentMode::AllowListed(names) if names.contains(name))
    }
    /// Reports whether a non-secret literal value may be assigned.
    #[must_use]
    pub fn permits_literal(&self, name: &EnvironmentName) -> bool {
        self.literal_names.contains(name)
    }
}
