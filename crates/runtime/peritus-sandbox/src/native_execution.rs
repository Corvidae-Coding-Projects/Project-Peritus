//! Exact current-platform process authority carried by version-three sandbox plans.

use std::{
    cmp::Ordering,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

use crate::SandboxError;

/// Exact native executable, working-directory, and environment-name authority.
///
/// Argument and environment values remain bound by the enclosing C2 execution-plan digest. This
/// value covers the C3 policy fields whose native identity cannot be represented by the legacy
/// portable UTF-8 sandbox projections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeExecutionAuthority {
    executable: OsString,
    working_directory: PathBuf,
    inherited_environment: Vec<OsString>,
    literal_environment: Vec<OsString>,
}

impl NativeExecutionAuthority {
    /// Creates and canonically orders one exact native authority.
    ///
    /// # Errors
    /// Rejects empty/NUL process paths, environment names containing equals or NUL, or native
    /// environment-name collisions.
    pub fn new(
        executable: impl Into<OsString>,
        working_directory: impl Into<PathBuf>,
        inherited_environment: Vec<OsString>,
        literal_environment: Vec<OsString>,
    ) -> Result<Self, SandboxError> {
        let executable = executable.into();
        let working_directory = working_directory.into();
        if executable.is_empty()
            || contains_nul(&executable)
            || working_directory.as_os_str().is_empty()
            || contains_nul(working_directory.as_os_str())
        {
            return Err(crate::error::invalid(
                "native process path is empty or contains NUL",
            ));
        }
        let mut authority = Self {
            executable,
            working_directory,
            inherited_environment,
            literal_environment,
        };
        validate_names(&mut authority.inherited_environment)?;
        validate_names(&mut authority.literal_environment)?;
        Ok(authority)
    }

    /// Returns the exact native executable.
    #[must_use]
    pub fn executable(&self) -> &OsStr {
        &self.executable
    }

    /// Returns the exact canonical native working directory.
    #[must_use]
    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    /// Returns exact native ambient names authorized for inheritance.
    #[must_use]
    pub fn inherited_environment(&self) -> &[OsString] {
        &self.inherited_environment
    }

    /// Returns exact native names authorized for literal assignment.
    #[must_use]
    pub fn literal_environment(&self) -> &[OsString] {
        &self.literal_environment
    }
}

fn validate_names(names: &mut [OsString]) -> Result<(), SandboxError> {
    for name in &*names {
        if name.is_empty()
            || contains_nul(name)
            || contains_equals(name)
        {
            return Err(crate::error::invalid(
                "native environment name is invalid",
            ));
        }
        #[cfg(windows)]
        if native_units(name).count() > i32::MAX as usize {
            return Err(crate::error::invalid(
                "native environment name exceeds the platform comparison API",
            ));
        }
    }
    names.sort_by(|left, right| name_cmp(left, right));
    if names.windows(2).any(|pair| name_cmp(&pair[0], &pair[1]) == Ordering::Equal) {
        return Err(crate::error::invalid("native environment names collide"));
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn native_bytes(value: &OsStr) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;

    value.as_bytes()
}

#[cfg(unix)]
fn contains_nul(value: &OsStr) -> bool {
    native_bytes(value).contains(&0)
}

#[cfg(unix)]
fn contains_equals(value: &OsStr) -> bool {
    native_bytes(value).contains(&b'=')
}

#[cfg(unix)]
fn name_cmp(left: &OsStr, right: &OsStr) -> Ordering {
    native_bytes(left).cmp(native_bytes(right))
}

#[cfg(windows)]
pub(crate) fn native_units(value: &OsStr) -> impl Iterator<Item = u16> + '_ {
    use std::os::windows::ffi::OsStrExt;

    value.encode_wide()
}

#[cfg(windows)]
fn contains_nul(value: &OsStr) -> bool {
    native_units(value).any(|unit| unit == 0)
}

#[cfg(windows)]
fn contains_equals(value: &OsStr) -> bool {
    native_units(value).any(|unit| unit == u16::from(b'='))
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "CompareStringOrdinal is the documented native Windows environment identity boundary"
)]
fn name_cmp(left: &OsStr, right: &OsStr) -> Ordering {
    use windows_sys::Win32::Globalization::{
        CSTR_EQUAL, CSTR_GREATER_THAN, CSTR_LESS_THAN, CompareStringOrdinal,
    };

    let left = native_units(left).collect::<Vec<_>>();
    let right = native_units(right).collect::<Vec<_>>();
    let left_len = i32::try_from(left.len()).expect("validated Windows environment name length");
    let right_len = i32::try_from(right.len()).expect("validated Windows environment name length");
    // SAFETY: both buffers and exact validated signed lengths remain live for this call.
    match unsafe {
        CompareStringOrdinal(left.as_ptr(), left_len, right.as_ptr(), right_len, 1)
    } {
        CSTR_LESS_THAN => Ordering::Less,
        CSTR_EQUAL => Ordering::Equal,
        CSTR_GREATER_THAN => Ordering::Greater,
        _ => left.cmp(&right),
    }
}
