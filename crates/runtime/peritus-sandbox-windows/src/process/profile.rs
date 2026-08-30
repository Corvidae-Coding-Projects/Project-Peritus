//! Checked `AppContainer` profile construction and native identity derivation.

use crate::{WindowsError, WindowsOperation, error};

use super::{AppContainerProfile, validate_sid};

const MAX_PROFILE_NAME_BYTES: usize = 128;

impl AppContainerProfile {
    /// Creates a checked `AppContainer` name/SID binding.
    ///
    /// # Errors
    /// Rejects an empty/control-bearing name or malformed SID.
    pub fn new(name: impl Into<String>, sid: impl Into<String>) -> Result<Self, WindowsError> {
        let name = name.into();
        let sid = sid.into();
        validate_profile_name(&name)?;
        validate_sid(&sid)?;
        Ok(Self { name, sid })
    }

    /// Derives the exact `AppContainer` SID assigned by the current Windows host.
    ///
    /// # Errors
    /// Rejects malformed names, non-Windows hosts, or failed native SID derivation.
    pub fn derive_for_current_host(name: impl Into<String>) -> Result<Self, WindowsError> {
        let name = name.into();
        validate_profile_name(&name)?;
        #[cfg(target_os = "windows")]
        {
            crate::native::derive_profile(name)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = name;
            Err(error::invalid(
                WindowsOperation::Validate,
                "AppContainer SID derivation requires a native Windows host",
            ))
        }
    }

    /// Returns the installed profile name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the exact `AppContainer` SID.
    #[must_use]
    pub fn sid(&self) -> &str {
        &self.sid
    }
}

fn validate_profile_name(name: &str) -> Result<(), WindowsError> {
    if name.is_empty()
        || name.len() > MAX_PROFILE_NAME_BYTES
        || !name.is_ascii()
        || name.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(error::invalid(
            WindowsOperation::Validate,
            "AppContainer name is empty, excessive, or contains controls",
        ));
    }
    Ok(())
}
