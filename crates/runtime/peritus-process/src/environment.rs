//! Deterministic clear-and-set child environments.

use std::{
    cmp::Ordering,
    ffi::{OsStr, OsString},
    fmt,
};

use crate::{
    ProcessError,
    command::{contains_nul, native_len},
    error::invalid,
};

const LEGACY_MAX_ENVIRONMENT_NAMES: usize = 1_024;
const LEGACY_MAX_ENVIRONMENT_NAME_BYTES: usize = 255;
const LEGACY_MAX_ENVIRONMENT_VALUE_BYTES: usize = 64 * 1_024;
const LEGACY_MAX_ENVIRONMENT_BYTES: usize = 2 * 1_024 * 1_024;

/// One validated native environment variable.
#[derive(Clone, Eq, PartialEq)]
pub struct EnvironmentVariable {
    name: OsString,
    value: OsString,
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
    pub(crate) fn restore(
        name: OsString,
        value: OsString,
        source: EnvironmentValueSource,
    ) -> Result<Self, ProcessError> {
        validate_name(&name)?;
        if contains_nul(&value) {
            return Err(environment_error("environment value contains NUL"));
        }
        Ok(Self { name, value, source })
    }

    /// Creates a checked literal child-environment binding.
    ///
    /// # Errors
    ///
    /// Returns an error when the native name is empty or contains equals or NUL, or when the
    /// native value contains NUL.
    pub fn new(
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> Result<Self, ProcessError> {
        let name = name.into();
        let value = value.into();
        validate_name(&name)?;
        if contains_nul(&value) {
            return Err(environment_error("environment value contains NUL"));
        }
        Ok(Self { name, value, source: EnvironmentValueSource::Literal })
    }

    fn inherited(
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> Result<Self, ProcessError> {
        let mut variable = Self::new(name, value)?;
        variable.source = EnvironmentValueSource::Inherited;
        Ok(variable)
    }

    /// Returns the checked native variable name.
    #[must_use]
    pub fn name(&self) -> &OsStr {
        &self.name
    }

    /// Returns the exact native value delivered to the child.
    #[must_use]
    pub fn value(&self) -> &OsStr {
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
            .field("value_native_units", &native_len(&self.value))
            .field("source", &self.source)
            .finish()
    }
}

/// How the final child environment was resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EnvironmentSource {
    /// No ambient variables were inherited.
    Cleared,
    /// Only the listed ambient native names were considered.
    Allowlisted(Vec<OsString>),
}

/// One resolved deterministic child environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvironmentPlan {
    source: EnvironmentSource,
    variables: Vec<EnvironmentVariable>,
}

impl EnvironmentPlan {
    pub(crate) fn restore(
        source: EnvironmentSource,
        variables: Vec<EnvironmentVariable>,
    ) -> Result<Self, ProcessError> {
        let source = match source {
            EnvironmentSource::Cleared => EnvironmentSource::Cleared,
            EnvironmentSource::Allowlisted(mut names) => {
                for name in &names {
                    validate_name(name)?;
                }
                sort_names(&mut names);
                reject_duplicate_names(names.iter().map(OsString::as_os_str))?;
                EnvironmentSource::Allowlisted(names)
            }
        };
        if variables.iter().any(|variable| {
            variable.source == EnvironmentValueSource::Inherited
                && match &source {
                    EnvironmentSource::Cleared => true,
                    EnvironmentSource::Allowlisted(names) => names
                        .binary_search_by(|name| native_name_cmp(name, variable.name()))
                        .is_err(),
                }
        }) {
            return Err(invalid(
                "restored inherited environment value is outside its allowlist",
            ));
        }
        Self::finish(source, variables)
    }

    /// Creates an environment from only explicit literal bindings.
    ///
    /// # Errors
    ///
    /// Returns an error for duplicate names under the current platform's native semantics.
    pub fn cleared(bindings: Vec<EnvironmentVariable>) -> Result<Self, ProcessError> {
        Self::finish(EnvironmentSource::Cleared, bindings)
    }

    /// Resolves only named ambient variables, then applies explicit bindings.
    ///
    /// Missing allowlisted variables are omitted. Explicit bindings replace the same native name.
    /// The resulting values, including non-Unicode values on Unix or unpaired UTF-16 values on
    /// Windows, are frozen into the execution plan instead of being read again at launch.
    ///
    /// # Errors
    ///
    /// Returns an error for a native-invalid or duplicate allowlist/binding name.
    pub fn allowlisted<I, S>(
        allowlist: I,
        bindings: Vec<EnvironmentVariable>,
    ) -> Result<Self, ProcessError>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let mut allowlist = allowlist.into_iter().map(Into::into).collect::<Vec<_>>();
        for name in &allowlist {
            validate_name(name)?;
        }
        sort_names(&mut allowlist);
        reject_duplicate_names(allowlist.iter().map(OsString::as_os_str))?;

        let mut resolved = Vec::new();
        for name in &allowlist {
            if let Some(value) = std::env::var_os(name) {
                resolved.push(EnvironmentVariable::inherited(name.clone(), value)?);
            }
        }

        let mut bindings = bindings;
        sort_variables(&mut bindings);
        reject_duplicate_names(bindings.iter().map(EnvironmentVariable::name))?;
        sort_variables(&mut resolved);
        for variable in bindings {
            match resolved.binary_search_by(|probe| {
                native_environment_name_cmp(probe.name(), variable.name())
            }) {
                Ok(index) => resolved[index] = variable,
                Err(index) => resolved.insert(index, variable),
            }
        }
        Self::finish(EnvironmentSource::Allowlisted(allowlist), resolved)
    }

    fn finish(
        source: EnvironmentSource,
        mut variables: Vec<EnvironmentVariable>,
    ) -> Result<Self, ProcessError> {
        sort_variables(&mut variables);
        reject_duplicate_names(variables.iter().map(EnvironmentVariable::name))?;
        Ok(Self { source, variables })
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

    pub(crate) fn uses_legacy_encoding(&self) -> bool {
        if let EnvironmentSource::Allowlisted(names) = &self.source {
            if names.len() > LEGACY_MAX_ENVIRONMENT_NAMES {
                return false;
            }
            let Some(names) =
                names.iter().map(|name| name.to_str()).collect::<Option<Vec<_>>>()
            else {
                return false;
            };
            if names.iter().any(|name| !legacy_valid_name(name))
                || has_legacy_collision(&names)
            {
                return false;
            }
        }
        if self.variables.len() > LEGACY_MAX_ENVIRONMENT_NAMES {
            return false;
        }
        let mut legacy_names = Vec::with_capacity(self.variables.len());
        let mut total = Some(0_usize);
        for variable in &self.variables {
            let Some(name) = variable.name.to_str() else {
                return false;
            };
            let Some(value) = variable.value.to_str() else {
                return false;
            };
            if !legacy_valid_name(name) || value.len() > LEGACY_MAX_ENVIRONMENT_VALUE_BYTES {
                return false;
            }
            legacy_names.push(name);
            total = total
                .and_then(|current| current.checked_add(name.len()))
                .and_then(|current| current.checked_add(value.len()))
                .and_then(|current| current.checked_add(2));
        }
        !has_legacy_collision(&legacy_names)
            && total.is_some_and(|total| total <= LEGACY_MAX_ENVIRONMENT_BYTES)
    }
}

/// Compares native environment names using the operating system's actual identity semantics.
#[must_use]
pub fn native_environment_name_cmp(left: &OsStr, right: &OsStr) -> Ordering {
    native_name_cmp(left, right)
}

/// Reports native environment-name equality on the current operating system.
#[must_use]
pub fn native_environment_names_equal(left: &OsStr, right: &OsStr) -> bool {
    native_name_cmp(left, right) == Ordering::Equal
}

fn sort_names(names: &mut [OsString]) {
    names.sort_by(|left, right| native_name_cmp(left, right));
}

fn sort_variables(variables: &mut [EnvironmentVariable]) {
    variables.sort_by(|left, right| native_name_cmp(left.name(), right.name()));
}

fn reject_duplicate_names<'a>(
    names: impl IntoIterator<Item = &'a OsStr>,
) -> Result<(), ProcessError> {
    let mut previous: Option<&OsStr> = None;
    for name in names {
        if previous.is_some_and(|previous| native_environment_names_equal(previous, name)) {
            return Err(invalid("environment contains a native name collision"));
        }
        previous = Some(name);
    }
    Ok(())
}

fn validate_name(name: &OsStr) -> Result<(), ProcessError> {
    if name.is_empty() || contains_nul(name) || contains_equals(name) {
        return Err(environment_error("environment name is empty or contains '=' or NUL"));
    }
    #[cfg(windows)]
    if native_len(name) > i32::MAX as usize {
        return Err(environment_error("environment name exceeds the native comparison API"));
    }
    Ok(())
}

#[cfg(unix)]
fn contains_equals(value: &OsStr) -> bool {
    crate::command::native_bytes(value).contains(&b'=')
}

#[cfg(windows)]
fn contains_equals(value: &OsStr) -> bool {
    crate::command::native_units(value).any(|unit| unit == u16::from(b'='))
}

#[cfg(unix)]
fn native_name_cmp(left: &OsStr, right: &OsStr) -> Ordering {
    crate::command::native_bytes(left).cmp(crate::command::native_bytes(right))
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "CompareStringOrdinal is the documented Windows environment name identity and ordering boundary"
)]
fn native_name_cmp(left: &OsStr, right: &OsStr) -> Ordering {
    use windows_sys::Win32::Globalization::{
        CSTR_EQUAL, CSTR_GREATER_THAN, CSTR_LESS_THAN, CompareStringOrdinal,
    };

    let left = crate::command::native_units(left).collect::<Vec<_>>();
    let right = crate::command::native_units(right).collect::<Vec<_>>();
    let left_len = i32::try_from(left.len()).expect("validated Windows environment name length");
    let right_len = i32::try_from(right.len()).expect("validated Windows environment name length");
    // SAFETY: both buffers remain live for the call, lengths describe them exactly, and the
    // constructor rejects lengths outside the API's signed count representation.
    match unsafe {
        CompareStringOrdinal(left.as_ptr(), left_len, right.as_ptr(), right_len, 1)
    } {
        CSTR_LESS_THAN => Ordering::Less,
        CSTR_EQUAL => Ordering::Equal,
        CSTR_GREATER_THAN => Ordering::Greater,
        _ => left.cmp(&right),
    }
}

fn legacy_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= LEGACY_MAX_ENVIRONMENT_NAME_BYTES
        && name.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
}

fn has_legacy_collision(names: &[&str]) -> bool {
    let mut folded = names.iter().map(|name| name.to_ascii_uppercase()).collect::<Vec<_>>();
    folded.sort();
    folded.windows(2).any(|pair| pair[0] == pair[1])
}

const fn environment_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        crate::ErrorCode::InvalidEnvironment,
        crate::ProcessOperation::Validate,
        crate::RecoveryClass::CorrectRequest,
        detail,
    )
}
