//! Durable user restrictions intersected with immutable daemon host policy.

use super::ControlError;
use serde::Deserialize;
use serde::Serialize;

/// One independently restrictable execution capability.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionCapability {
    /// Inspect workspace files and metadata through authorized tools.
    Read,
    /// Mutate workspace files through the existing authority gateway.
    Write,
    /// Start or control owned workspace processes.
    Process,
    /// Admit provider or process activity that may use the network.
    Network,
}

impl PermissionCapability {
    /// Canonical capability order used by public projections.
    pub const ALL: [Self; 4] = [Self::Read, Self::Write, Self::Process, Self::Network];

    const fn bit(self) -> u8 {
        match self {
            Self::Read => 1,
            Self::Write => 2,
            Self::Process => 4,
            Self::Network => 8,
        }
    }
}

/// Immutable upper bounds resolved from daemon configuration and workspace trust.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostPermissions {
    allowed: u8,
}

impl HostPermissions {
    /// Constructs a fail-closed ceiling that admits no capability.
    #[must_use]
    pub const fn none() -> Self {
        Self { allowed: 0 }
    }

    /// Constructs the fully available host ceiling. This type cannot grant authority by itself.
    #[must_use]
    pub const fn all() -> Self {
        Self { allowed: 0b1111 }
    }

    /// Removes one capability from a host ceiling.
    #[must_use]
    pub const fn without(mut self, capability: PermissionCapability) -> Self {
        self.allowed &= !capability.bit();
        self
    }

    /// Reports whether the immutable host policy admits a capability in principle.
    #[must_use]
    pub const fn allows(self, capability: PermissionCapability) -> bool {
        self.allowed & capability.bit() != 0
    }
}

/// Current workspace-scoped user restriction overlay.
///
/// `true` means only that this overlay does not prohibit the capability. Every lower authority,
/// approval, lease, sandbox, goal and path constraint remains independently mandatory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionPolicy {
    schema: u16,
    revision: u64,
    allowed: u8,
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self { schema: 1, revision: 0, allowed: 0b1111 }
    }
}

impl PermissionPolicy {
    /// Returns the monotonic restriction revision; zero is the implicit unrestricted overlay.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Reports the overlay value before intersecting it with host policy.
    #[must_use]
    pub const fn requested(&self, capability: PermissionCapability) -> bool {
        self.allowed & capability.bit() != 0
    }

    /// Reports the enforced intersection. A policy record can never make a host denial true.
    #[must_use]
    pub const fn effective(&self, host: HostPermissions, capability: PermissionCapability) -> bool {
        self.requested(capability) && host.allows(capability)
    }

    /// Returns one host-intersected snapshot suitable for a concrete effect boundary.
    #[must_use]
    pub fn effective_permissions(&self, host: HostPermissions) -> HostPermissions {
        PermissionCapability::ALL.into_iter().fold(host, |permissions, capability| {
            if self.requested(capability) { permissions } else { permissions.without(capability) }
        })
    }

    /// Plans one exact expected-revision restriction change.
    ///
    /// # Errors
    /// Rejects stale policy revisions, no-op changes, and grants above the host ceiling.
    pub fn apply(
        &self,
        expected_revision: u64,
        capability: PermissionCapability,
        allowed: bool,
        host: HostPermissions,
    ) -> Result<Self, ControlError> {
        self.validate()?;
        if self.revision != expected_revision {
            return Err(ControlError::StaleRevision);
        }
        if allowed && !host.allows(capability) {
            return Err(ControlError::InvalidInput);
        }
        if self.requested(capability) == allowed {
            return Err(ControlError::InvalidInput);
        }
        let mut next = self.clone();
        next.revision = next.revision.checked_add(1).ok_or(ControlError::Capacity)?;
        if allowed {
            next.allowed |= capability.bit();
        } else {
            next.allowed &= !capability.bit();
        }
        next.validate()?;
        Ok(next)
    }

    /// Encodes one bounded canonical stored projection.
    ///
    /// # Errors
    /// Rejects an invalid schema or an encoding failure.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ControlError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| ControlError::InvalidInput)
    }

    /// Decodes and verifies canonical durable bytes.
    ///
    /// # Errors
    /// Rejects malformed, noncanonical, or unsupported stored state.
    pub fn parse(bytes: &[u8]) -> Result<Self, ControlError> {
        if bytes.len() > 4096 {
            return Err(ControlError::Capacity);
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| ControlError::InvalidInput)?;
        value.validate()?;
        if value.canonical_bytes()? != bytes {
            return Err(ControlError::InvalidInput);
        }
        Ok(value)
    }

    const fn validate(&self) -> Result<(), ControlError> {
        if self.schema != 1 || self.allowed & !0b1111 != 0 {
            return Err(ControlError::UnsupportedSchema);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrowing_is_durable_and_broadening_above_host_rejects() {
        let host = HostPermissions::all().without(PermissionCapability::Network);
        let initial = PermissionPolicy::default();
        assert!(!initial.effective(host, PermissionCapability::Network));
        assert_eq!(
            initial.apply(0, PermissionCapability::Network, true, host),
            Err(ControlError::InvalidInput)
        );
        let narrowed = initial.apply(0, PermissionCapability::Write, false, host).expect("narrow");
        assert_eq!(narrowed.revision(), 1);
        assert!(!narrowed.effective(host, PermissionCapability::Write));
        assert_eq!(
            PermissionPolicy::parse(&narrowed.canonical_bytes().unwrap()).unwrap(),
            narrowed
        );
    }
}
