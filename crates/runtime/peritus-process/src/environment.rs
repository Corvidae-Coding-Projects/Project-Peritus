//! Deterministic clear-and-set child environments.

use std::{collections::BTreeMap, fmt};

use crate::{ProcessError, error::invalid};

const MAX_ENVIRONMENT_NAMES: usize = 1_024;
const MAX_ENVIRONMENT_NAME_BYTES: usize = 255;
const MAX_ENVIRONMENT_VALUE_BYTES: usize = 64 * 1_024;
const MAX_ENVIRONMENT_BYTES: usize = 2 * 1_024 * 1_024;

/// One validated portable environment variable.
#[derive(Clone, Eq, PartialEq)]
pub struct EnvironmentVariable {
    name: String,
    value: String,
    source: EnvironmentValueSource,
}

/// Provenance of one resolved child-environment value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EnvironmentValueSource {
    /// The value was captured from an explicitly allowlisted host variable.
    Inherited,
    /// The value was supplied as a literal execution-plan binding.
    Literal,
}

impl EnvironmentVariable {
    /// Creates a checked literal child-environment binding.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-portable name, NUL, or an over-limit value.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Result<Self, ProcessError> {
        let name = name.into();
        let value = value.into();
        if !valid_name(&name) {
            return Err(ProcessError::new(
                crate::ErrorCode::InvalidEnvironment,
                crate::ProcessOperation::Validate,
                crate::RecoveryClass::CorrectRequest,
                "environment name is not portable ASCII or exceeds its bound",
            ));
        }
        if value.len() > MAX_ENVIRONMENT_VALUE_BYTES || value.as_bytes().contains(&0) {
            return Err(ProcessError::new(
                crate::ErrorCode::InvalidEnvironment,
                crate::ProcessOperation::Validate,
                crate::RecoveryClass::CorrectRequest,
                "environment value contains NUL or exceeds its bound",
            ));
        }
        Ok(Self { name, value, source: EnvironmentValueSource::Literal })
    }

    fn inherited(name: impl Into<String>, value: impl Into<String>) -> Result<Self, ProcessError> {
        let mut variable = Self::new(name, value)?;
        variable.source = EnvironmentValueSource::Inherited;
        Ok(variable)
    }

    /// Returns the checked variable name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the exact literal value delivered to the child.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Returns whether this resolved value was inherited or explicitly supplied.
    #[must_use]
    pub const fn source(&self) -> EnvironmentValueSource {
        self.source
    }
}

impl fmt::Debug for EnvironmentVariable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvironmentVariable")
            .field("name", &self.name)
            .field("value_bytes", &self.value.len())
            .field("source", &self.source)
            .finish()
    }
}

/// How the final child environment was resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EnvironmentSource {
    /// No ambient variables were inherited.
    Cleared,
    /// Only the listed ambient names were considered.
    Allowlisted(Vec<String>),
}

/// One resolved deterministic child environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvironmentPlan {
    source: EnvironmentSource,
    variables: Vec<EnvironmentVariable>,
}

impl EnvironmentPlan {
    /// Creates an environment from only explicit literal bindings.
    ///
    /// # Errors
    ///
    /// Returns an error for duplicate/case-folding names or complete-size overflow.
    pub fn cleared(bindings: Vec<EnvironmentVariable>) -> Result<Self, ProcessError> {
        Self::finish(EnvironmentSource::Cleared, bindings)
    }

    /// Resolves only named ambient variables, then applies explicit bindings.
    ///
    /// Missing allowlisted variables are omitted. Explicit bindings replace the same canonical
    /// name. The resulting values, not later ambient state, are bound into the execution plan.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid/duplicate allowlist names, non-Unicode host values, duplicate
    /// explicit names, or complete-size overflow.
    pub fn allowlisted(
        allowlist: Vec<String>,
        bindings: Vec<EnvironmentVariable>,
    ) -> Result<Self, ProcessError> {
        if allowlist.len() > MAX_ENVIRONMENT_NAMES {
            return Err(invalid("environment allowlist exceeds its bound"));
        }
        let mut canonical = BTreeMap::new();
        for name in allowlist {
            if !valid_name(&name) {
                return Err(invalid("environment allowlist contains an invalid name"));
            }
            let folded = fold_name(&name);
            if canonical.insert(folded, name).is_some() {
                return Err(invalid("environment allowlist contains a case-fold collision"));
            }
        }
        let mut resolved = Vec::new();
        for name in canonical.values() {
            if let Some(value) = std::env::var_os(name) {
                let value = value
                    .into_string()
                    .map_err(|_| invalid("allowlisted ambient environment value is not Unicode"))?;
                resolved.push(EnvironmentVariable::inherited(name.clone(), value)?);
            }
        }
        let mut by_name: BTreeMap<String, EnvironmentVariable> =
            resolved.into_iter().map(|variable| (fold_name(variable.name()), variable)).collect();
        for variable in bindings {
            by_name.insert(fold_name(variable.name()), variable);
        }
        let mut normalized_allowlist: Vec<String> = canonical.into_values().collect();
        normalized_allowlist.sort_by_key(|name| fold_name(name));
        Self::finish(
            EnvironmentSource::Allowlisted(normalized_allowlist),
            by_name.into_values().collect(),
        )
    }

    fn finish(
        source: EnvironmentSource,
        variables: Vec<EnvironmentVariable>,
    ) -> Result<Self, ProcessError> {
        if variables.len() > MAX_ENVIRONMENT_NAMES {
            return Err(invalid("environment binding count exceeds its bound"));
        }
        let mut canonical = BTreeMap::new();
        let mut total = 0_usize;
        for variable in variables {
            total = total
                .checked_add(variable.name.len())
                .and_then(|value| value.checked_add(variable.value.len()))
                .and_then(|value| value.checked_add(2))
                .ok_or_else(|| invalid("environment byte accounting overflowed"))?;
            if total > MAX_ENVIRONMENT_BYTES {
                return Err(invalid("complete environment exceeds its bound"));
            }
            if canonical.insert(fold_name(&variable.name), variable).is_some() {
                return Err(invalid("environment contains a case-fold collision"));
            }
        }
        Ok(Self { source, variables: canonical.into_values().collect() })
    }

    /// Returns how the ambient environment was constrained.
    #[must_use]
    pub const fn source(&self) -> &EnvironmentSource {
        &self.source
    }

    /// Returns the canonical final child bindings.
    #[must_use]
    pub fn variables(&self) -> &[EnvironmentVariable] {
        &self.variables
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_ENVIRONMENT_NAME_BYTES
        && name.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
}

fn fold_name(name: &str) -> String {
    name.to_ascii_uppercase()
}
