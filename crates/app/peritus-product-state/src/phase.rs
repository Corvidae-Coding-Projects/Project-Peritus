//! Durable bootstrap phases and derived live-launch readiness.

use serde::Deserialize;
use serde::Serialize;

/// Last durably completed local-bootstrap boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BootstrapPhase {
    /// Stable installation and actor identities exist.
    IdentityReady,
    /// The canonical public approval registry is published.
    RegistryReady,
    /// A strict daemon configuration is published and can be started.
    ConfigurationReady,
}

impl BootstrapPhase {
    /// Returns whether `next` is the same phase or its exact successor.
    #[must_use]
    pub const fn permits(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::IdentityReady, Self::IdentityReady | Self::RegistryReady)
                | (Self::RegistryReady, Self::RegistryReady | Self::ConfigurationReady)
                | (Self::ConfigurationReady, Self::ConfigurationReady)
        )
    }

    /// Derives the next local effect required before daemon startup.
    #[must_use]
    pub const fn launch_readiness(self) -> LaunchReadiness {
        match self {
            Self::IdentityReady => LaunchReadiness::PublishRegistry,
            Self::RegistryReady => LaunchReadiness::PublishConfiguration,
            Self::ConfigurationReady => LaunchReadiness::CanStartDaemon,
        }
    }
}

/// Next effect required to make the local application launchable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchReadiness {
    /// Publish the empty canonical B1 public credential registry.
    PublishRegistry,
    /// Publish the generated strict G0 daemon configuration.
    PublishConfiguration,
    /// Durable bootstrap is complete; live daemon readiness may now be established.
    CanStartDaemon,
}
