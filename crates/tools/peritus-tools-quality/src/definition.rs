//! Explicit, bounded quality check definitions.

use peritus_patch::WorkspacePath;
use peritus_process::CommandSpec;
use peritus_types::GateId;

use crate::{QualityError, QualityErrorKind};

const MAX_GATE_NAME_BYTES: usize = 128;
const MAX_SOURCE_LABEL_BYTES: usize = 256;

/// Provenance of one check definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckSource {
    /// An exact B2/project definition supplied by the caller.
    Explicit(String),
    /// A known check projected from a valid Cargo manifest.
    CargoManifest,
    /// A zero-argument public recipe projected from a Justfile.
    JustfileRecipe(String),
}

/// Whether later policy may require the definition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckRequirement {
    /// The explicit B2/project definition marks this gate required.
    Required,
    /// The explicit B2/project definition marks this gate optional.
    Optional,
    /// The check was discovered only and carries no acceptance requirement.
    Discovered,
}

/// Stable name of a caller-resolved deterministic environment profile.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EnvironmentProfile(String);

impl EnvironmentProfile {
    /// Creates a portable bounded profile name.
    ///
    /// # Errors
    /// Returns a typed failure for an empty, oversized, or nonportable value.
    pub fn new(value: impl Into<String>) -> Result<Self, QualityError> {
        let value = value.into();
        if !valid_name(&value, 64) {
            return Err(invalid("environment profile name is invalid"));
        }
        Ok(Self(value))
    }

    /// Borrows the stable profile name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Frozen process success predicate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExpectedSuccess {
    /// The process must exit with this exact code.
    ExitCode(i32),
}

/// Optional complete-output parser applied before a check may pass.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OutputParser {
    /// No content parser; process/artifact completeness still applies.
    None,
    /// Complete output must be valid UTF-8 within this bound.
    Utf8 {
        /// Maximum complete output bytes accepted by the parser.
        maximum_bytes: u32,
    },
    /// Complete output must be one JSON value within this bound.
    Json {
        /// Maximum complete output bytes accepted by the parser.
        maximum_bytes: u32,
    },
    /// Complete output must be a JSON object whose `success` member is exactly `true`.
    JsonSuccess {
        /// Maximum complete output bytes accepted by the parser.
        maximum_bytes: u32,
    },
}

impl OutputParser {
    pub(crate) const fn maximum_bytes(self) -> Option<u32> {
        match self {
            Self::None => None,
            Self::Utf8 { maximum_bytes }
            | Self::Json { maximum_bytes }
            | Self::JsonSuccess { maximum_bytes } => Some(maximum_bytes),
        }
    }
}

/// Complete deterministic definition of one invocable quality check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckDefinition {
    gate_name: String,
    gate_id: GateId,
    source: CheckSource,
    requirement: CheckRequirement,
    executable: String,
    arguments: Vec<String>,
    working_directory: Option<WorkspacePath>,
    environment_profile: EnvironmentProfile,
    timeout_millis: u64,
    output_bytes: u64,
    parser: OutputParser,
    expected_success: ExpectedSuccess,
}

impl CheckDefinition {
    /// Creates a complete bounded check definition.
    ///
    /// # Errors
    /// Returns a typed failure for invalid names, provenance/requirement mismatch, argv, or zero
    /// timeout/output/parser bounds.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gate_name: impl Into<String>,
        gate_id: GateId,
        source: CheckSource,
        requirement: CheckRequirement,
        executable: impl Into<String>,
        arguments: Vec<String>,
        working_directory: Option<WorkspacePath>,
        environment_profile: EnvironmentProfile,
        timeout_millis: u64,
        output_bytes: u64,
        parser: OutputParser,
        expected_success: ExpectedSuccess,
    ) -> Result<Self, QualityError> {
        let gate_name = gate_name.into();
        let executable = executable.into();
        validate_definition(
            &gate_name,
            &source,
            requirement,
            timeout_millis,
            output_bytes,
            parser,
        )?;
        CommandSpec::new(executable.clone(), arguments.clone())?;
        Ok(Self {
            gate_name,
            gate_id,
            source,
            requirement,
            executable,
            arguments,
            working_directory,
            environment_profile,
            timeout_millis,
            output_bytes,
            parser,
            expected_success,
        })
    }

    /// Returns the stable gate name.
    #[must_use]
    pub fn gate_name(&self) -> &str {
        &self.gate_name
    }
    /// Returns the stable B2 gate identity.
    #[must_use]
    pub const fn gate_id(&self) -> GateId {
        self.gate_id
    }
    /// Returns definition provenance.
    #[must_use]
    pub const fn source(&self) -> &CheckSource {
        &self.source
    }
    /// Returns required, optional, or discovery-only status.
    #[must_use]
    pub const fn requirement(&self) -> CheckRequirement {
        self.requirement
    }
    /// Returns the literal executable.
    #[must_use]
    pub fn executable(&self) -> &str {
        &self.executable
    }
    /// Returns literal arguments.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
    /// Returns an optional workspace-relative working directory.
    #[must_use]
    pub const fn working_directory(&self) -> Option<&WorkspacePath> {
        self.working_directory.as_ref()
    }
    /// Returns the named environment profile.
    #[must_use]
    pub const fn environment_profile(&self) -> &EnvironmentProfile {
        &self.environment_profile
    }
    /// Returns the wall-time ceiling.
    #[must_use]
    pub const fn timeout_millis(&self) -> u64 {
        self.timeout_millis
    }
    /// Returns the output/parser byte ceiling.
    #[must_use]
    pub const fn output_bytes(&self) -> u64 {
        self.output_bytes
    }
    /// Returns the complete-output parser.
    #[must_use]
    pub const fn parser(&self) -> OutputParser {
        self.parser
    }
    /// Returns the frozen process success predicate.
    #[must_use]
    pub const fn expected_success(&self) -> ExpectedSuccess {
        self.expected_success
    }

    pub(crate) fn command(&self) -> Result<CommandSpec, QualityError> {
        CommandSpec::new(self.executable.clone(), self.arguments.clone()).map_err(Into::into)
    }
}

fn validate_definition(
    name: &str,
    source: &CheckSource,
    requirement: CheckRequirement,
    timeout: u64,
    output: u64,
    parser: OutputParser,
) -> Result<(), QualityError> {
    if !valid_name(name, MAX_GATE_NAME_BYTES) {
        return Err(invalid("gate name is invalid"));
    }
    if let CheckSource::Explicit(label) | CheckSource::JustfileRecipe(label) = source
        && (!valid_name(label, MAX_SOURCE_LABEL_BYTES) && !valid_recipe(label))
    {
        return Err(invalid("definition source label is invalid"));
    }
    if !matches!(source, CheckSource::Explicit(_)) && requirement != CheckRequirement::Discovered {
        return Err(invalid("discovered definitions cannot claim B2 required/optional policy"));
    }
    if timeout == 0 || output == 0 || parser.maximum_bytes().is_some_and(|bound| bound == 0) {
        return Err(invalid("timeout, output, and selected parser bounds must be nonzero"));
    }
    if parser.maximum_bytes().is_some_and(|bound| u64::from(bound) > output) {
        return Err(invalid("parser bound exceeds the retained output bound"));
    }
    Ok(())
}

fn valid_name(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':' | b'/')
        })
}

fn valid_recipe(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SOURCE_LABEL_BYTES
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn invalid(detail: &'static str) -> QualityError {
    QualityError::new(QualityErrorKind::InvalidInput, detail)
}
