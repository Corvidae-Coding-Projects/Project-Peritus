//! Checked structured executable and argument values.

use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;

use crate::{ProcessError, error::invalid};

const LEGACY_MAX_EXECUTABLE_BYTES: usize = 4_096;
const LEGACY_MAX_ARGUMENT_BYTES: usize = 64 * 1_024;
const LEGACY_MAX_ARGUMENT_COUNT: usize = 4_096;
const LEGACY_MAX_ARGV_BYTES: usize = 2 * 1_024 * 1_024;

/// One checked direct-execution command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    executable: OsString,
    arguments: Vec<OsString>,
}

/// Produces a stable C4-safe reference to one exact native executable value.
///
/// The reference carries only a platform-tagged digest. The exact executable remains in the C2
/// execution plan and must be supplied independently; this function does not create a launchable
/// path or grant script/shell interpretation.
#[must_use]
pub fn native_executable_reference(value: &OsStr) -> String {
    let mut canonical = Vec::from(b"peritus.native-executable.v1\0".as_slice());
    #[cfg(unix)]
    {
        canonical.push(1);
        canonical.extend_from_slice(native_bytes(value));
    }
    #[cfg(windows)]
    {
        canonical.push(2);
        for unit in native_units(value) {
            canonical.extend_from_slice(&unit.to_be_bytes());
        }
    }
    let digest = peritus_codec::sha256(&canonical);
    let mut reference = String::from("\0peritus-native-executable-v1:");
    for byte in digest.as_bytes() {
        write!(&mut reference, "{byte:02x}").expect("writing hexadecimal into String cannot fail");
    }
    reference
}

/// Checks a C4 native-executable reference against an exact C2 executable value.
#[must_use]
pub fn native_executable_reference_matches(reference: &str, value: &OsStr) -> bool {
    is_native_executable_reference(reference)
        && reference == native_executable_reference(value)
}

/// Reports whether text is the reserved, structurally valid C4 native-executable reference.
///
/// Its leading NUL makes the representation disjoint from every launchable literal executable.
#[must_use]
pub fn is_native_executable_reference(value: &str) -> bool {
    value
        .strip_prefix("\0peritus-native-executable-v1:")
        .is_some_and(|digest| {
            digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

impl CommandSpec {
    /// Creates a structured native command without parsing or invoking a shell.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty executable or a native string containing NUL. Operating
    /// system argv capacity is checked immediately before authorization consumption and repeated
    /// by native helpers immediately before the target exec effect.
    pub fn new<I, S>(executable: impl Into<OsString>, arguments: I) -> Result<Self, ProcessError>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let executable = executable.into();
        if executable.is_empty() || contains_nul(&executable) {
            return Err(invalid("executable is empty or contains NUL"));
        }
        let mut checked = Vec::new();
        for argument in arguments {
            let argument = argument.into();
            if contains_nul(&argument) {
                return Err(invalid("argument contains NUL"));
            }
            checked.push(argument);
        }
        Ok(Self { executable, arguments: checked })
    }

    /// Returns the literal native executable value.
    #[must_use]
    pub fn executable(&self) -> &OsStr {
        &self.executable
    }

    /// Returns literal native arguments in execution order.
    #[must_use]
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    /// Returns the native code-unit count used for platform admission accounting.
    #[must_use]
    pub fn native_len(&self) -> usize {
        native_len(&self.executable)
            + self.arguments.iter().map(|value| native_len(value) + 1).sum::<usize>()
    }

    pub(crate) fn legacy_text(&self) -> Option<(&str, Vec<&str>)> {
        let executable = self.executable.to_str()?;
        let arguments = self
            .arguments
            .iter()
            .map(|value| value.to_str())
            .collect::<Option<Vec<_>>>()?;
        Some((executable, arguments))
    }

    pub(crate) fn uses_legacy_encoding(&self) -> bool {
        let Some((executable, arguments)) = self.legacy_text() else {
            return false;
        };
        if executable.len() > LEGACY_MAX_EXECUTABLE_BYTES
            || arguments.len() > LEGACY_MAX_ARGUMENT_COUNT
            || arguments.iter().any(|argument| argument.len() > LEGACY_MAX_ARGUMENT_BYTES)
        {
            return false;
        }
        let total = arguments.iter().try_fold(executable.len(), |total, argument| {
            total.checked_add(argument.len())?.checked_add(1)
        });
        total.is_some_and(|total| total <= LEGACY_MAX_ARGV_BYTES)
    }

    /// Encodes the exact Windows command line for `CreateProcessW` without shell expansion.
    ///
    /// # Errors
    /// Returns an error when the quoted command line exceeds the documented 32,767 UTF-16 units,
    /// including its terminating NUL.
    #[cfg(windows)]
    pub fn windows_command_line(&self) -> Result<Vec<u16>, ProcessError> {
        let mut line = Vec::new();
        quote_windows_argument(&mut line, &self.executable);
        for argument in &self.arguments {
            line.push(u16::from(b' '));
            quote_windows_argument(&mut line, argument);
        }
        line.push(0);
        if line.len() > 32_767 {
            return Err(invalid("quoted Windows command line exceeds CreateProcessW capacity"));
        }
        Ok(line)
    }
}

#[cfg(unix)]
pub(crate) fn native_bytes(value: &OsStr) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;

    value.as_bytes()
}

#[cfg(unix)]
pub(crate) fn contains_nul(value: &OsStr) -> bool {
    native_bytes(value).contains(&0)
}

#[cfg(unix)]
pub(crate) fn native_len(value: &OsStr) -> usize {
    native_bytes(value).len()
}

#[cfg(windows)]
pub(crate) fn native_units(value: &OsStr) -> impl Iterator<Item = u16> + '_ {
    use std::os::windows::ffi::OsStrExt;

    value.encode_wide()
}

#[cfg(windows)]
pub(crate) fn contains_nul(value: &OsStr) -> bool {
    native_units(value).any(|unit| unit == 0)
}

#[cfg(windows)]
pub(crate) fn native_len(value: &OsStr) -> usize {
    native_units(value).count()
}

#[cfg(windows)]
fn quote_windows_argument(output: &mut Vec<u16>, value: &OsStr) {
    let value = native_units(value).collect::<Vec<_>>();
    let quote = value.is_empty()
        || value
            .iter()
            .any(|unit| matches!(*unit, 0x20 | 0x09 | 0x22));
    if !quote {
        output.extend_from_slice(&value);
        return;
    }
    output.push(u16::from(b'"'));
    let mut backslashes = 0_usize;
    for unit in value {
        if unit == u16::from(b'\\') {
            backslashes += 1;
            continue;
        }
        if unit == u16::from(b'"') {
            output.extend(core::iter::repeat_n(u16::from(b'\\'), backslashes * 2 + 1));
        } else {
            output.extend(core::iter::repeat_n(u16::from(b'\\'), backslashes));
        }
        backslashes = 0;
        output.push(unit);
    }
    output.extend(core::iter::repeat_n(u16::from(b'\\'), backslashes * 2));
    output.push(u16::from(b'"'));
}
