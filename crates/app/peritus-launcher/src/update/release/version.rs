//! Pure decimal release version parser shared by discovery and the launcher.

use crate::LauncherError;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    pub(super) fn from_tag(tag: &str) -> Result<Self, LauncherError> {
        Self::parse(tag.strip_prefix('v').ok_or_else(|| malformed(tag))?)
    }

    pub(super) fn parse(value: &str) -> Result<Self, LauncherError> {
        let mut parts = value.split('.');
        let major = component(parts.next(), value)?;
        let minor = component(parts.next(), value)?;
        let patch = component(parts.next(), value)?;
        if parts.next().is_some() {
            return Err(malformed(value));
        }
        Ok(Self { major, minor, patch })
    }

    pub(super) fn render(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

fn component(value: Option<&str>, version: &str) -> Result<u64, LauncherError> {
    let value = value.ok_or_else(|| malformed(version))?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(malformed(version));
    }
    value.parse().map_err(|_| malformed(version))
}

fn malformed(value: &str) -> LauncherError {
    LauncherError::Update(format!("release version is not vMAJOR.MINOR.PATCH: {value}"))
}
