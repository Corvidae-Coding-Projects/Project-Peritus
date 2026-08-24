//! Bounded literal values carried by the Linux helper manifest.

use super::manifest_error;
use crate::LinuxError;
use peritus_sandbox::SecretRequirement;
use std::fmt;

/// Literal shell-free target executable and argv.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetCommand {
    program: String,
    arguments: Vec<String>,
}

impl TargetCommand {
    /// Creates a bounded literal command.
    ///
    /// # Errors
    /// Rejects empty/NUL-bearing programs, NUL-bearing arguments, or more than 256 arguments.
    pub fn new(program: String, arguments: Vec<String>) -> Result<Self, LinuxError> {
        if program.is_empty() || program.as_bytes().contains(&0) || arguments.len() > 256 {
            return Err(manifest_error("target command is empty, contains NUL, or is oversized"));
        }
        if arguments
            .iter()
            .any(|argument| argument.as_bytes().contains(&0) || argument.len() > 64 * 1024)
        {
            return Err(manifest_error("target argument contains NUL or is oversized"));
        }
        Ok(Self { program, arguments })
    }
    /// Returns the exact program.
    #[must_use]
    pub fn program(&self) -> &str {
        &self.program
    }
    /// Returns literal arguments.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
}

/// One exact non-secret environment assignment.
#[derive(Clone, Eq, PartialEq)]
pub struct EnvironmentEntry {
    pub(super) name: String,
    value: String,
}

impl EnvironmentEntry {
    /// Creates a portable environment assignment.
    ///
    /// # Errors
    /// Rejects invalid names, NUL, or values exceeding 64 KiB.
    pub fn new(name: String, value: String) -> Result<Self, LinuxError> {
        let valid_name = !name.is_empty()
            && name.len() <= 255
            && name.bytes().enumerate().all(|(index, byte)| {
                byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
            });
        if !valid_name || value.len() > 64 * 1024 || value.as_bytes().contains(&0) {
            return Err(manifest_error("environment assignment is invalid or oversized"));
        }
        Ok(Self { name, value })
    }
    /// Returns the name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the exact non-secret value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for EnvironmentEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvironmentEntry")
            .field("name", &self.name)
            .field("value_bytes", &self.value.len())
            .finish()
    }
}

/// Exact protected inherited handle retained across helper descriptor closure.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct InheritedHandle {
    pub(super) descriptor: u64,
    pub(super) label: String,
}

impl InheritedHandle {
    /// Creates a handle declaration. Standard streams and manifest input cannot be reused.
    ///
    /// # Errors
    /// Rejects descriptor numbers below 3 and invalid labels.
    pub fn new(descriptor: u64, label: String) -> Result<Self, LinuxError> {
        if descriptor < 3
            || label.is_empty()
            || label.len() > 128
            || !label.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err(manifest_error("protected inherited handle is invalid"));
        }
        Ok(Self { descriptor, label })
    }
    /// Returns the descriptor number.
    #[must_use]
    pub const fn descriptor(&self) -> u64 {
        self.descriptor
    }
    /// Returns the non-sensitive label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Nonsensitive manifest binding from one checked secret destination to one protected handle.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProtectedPayloadBinding {
    pub(super) requirement: SecretRequirement,
    pub(super) handle: InheritedHandle,
    pub(super) payload_len: u32,
}

impl ProtectedPayloadBinding {
    /// Creates one exact checked delivery binding without exposing payload bytes.
    ///
    /// # Errors
    /// Rejects an empty or over-limit protected payload.
    pub fn new(
        requirement: SecretRequirement,
        handle: InheritedHandle,
        payload_len: usize,
    ) -> Result<Self, LinuxError> {
        let payload_len = u32::try_from(payload_len)
            .ok()
            .filter(|length| (1..=1024 * 1024).contains(length))
            .ok_or_else(|| manifest_error("protected payload length is invalid"))?;
        Ok(Self { requirement, handle, payload_len })
    }

    /// Returns the checked secret reference and exact destination.
    #[must_use]
    pub const fn requirement(&self) -> &SecretRequirement {
        &self.requirement
    }

    /// Returns the inherited handle declaration.
    #[must_use]
    pub const fn handle(&self) -> &InheritedHandle {
        &self.handle
    }

    /// Returns the exact protected byte length.
    #[must_use]
    pub const fn payload_len(&self) -> u32 {
        self.payload_len
    }
}
