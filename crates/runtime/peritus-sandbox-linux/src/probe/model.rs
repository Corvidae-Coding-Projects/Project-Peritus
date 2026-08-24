//! Platform-neutral probe request and bounded observation values.

#[cfg(target_os = "linux")]
use super::probe_error;
use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery, ProxyRoute};
use peritus_types::Sha256Digest;
use std::path::{Path, PathBuf};

/// Parsed Linux kernel release tuple.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KernelVersion {
    pub(super) major: u16,
    pub(super) minor: u16,
    pub(super) patch: u16,
}

impl KernelVersion {
    /// Creates a version tuple.
    #[must_use]
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self { major, minor, patch }
    }
    /// Returns the major version.
    #[must_use]
    pub const fn major(self) -> u16 {
        self.major
    }
    /// Returns the minor version.
    #[must_use]
    pub const fn minor(self) -> u16 {
        self.minor
    }
    /// Returns the patch version.
    #[must_use]
    pub const fn patch(self) -> u16 {
        self.patch
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn parse(release: &str) -> Result<Self, LinuxError> {
        let numeric = release.split_once('-').map_or(release, |(head, _)| head);
        let mut fields = numeric.split('.');
        let parse = |field: Option<&str>| {
            field
                .unwrap_or("0")
                .parse::<u16>()
                .map_err(|_| probe_error("kernel release is not a supported numeric tuple"))
        };
        Ok(Self::new(parse(fields.next())?, parse(fields.next())?, parse(fields.next())?))
    }
}

/// Native architecture support classification.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Architecture {
    /// Linux x86-64.
    X86_64,
    /// Linux `AArch64`.
    Aarch64,
    /// Any other architecture, retained for diagnostics but never advertised.
    Unsupported(String),
}

impl Architecture {
    pub(super) fn current() -> Self {
        match std::env::consts::ARCH {
            "x86_64" => Self::X86_64,
            "aarch64" => Self::Aarch64,
            other => Self::Unsupported(other.to_owned()),
        }
    }

    /// Reports whether R-C3-005 supports this architecture.
    #[must_use]
    pub const fn supported(&self) -> bool {
        matches!(self, Self::X86_64 | Self::Aarch64)
    }
}

/// Independently observed namespace handles and a functional isolation exercise.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "these are independent runtime probe facts, not mutually exclusive state"
)]
pub struct NamespaceSupport {
    /// User namespace handle exists and unprivileged namespaces are enabled.
    pub user: bool,
    /// Mount namespace handle exists.
    pub mount: bool,
    /// PID namespace handle exists.
    pub pid: bool,
    /// IPC namespace handle exists.
    pub ipc: bool,
    /// UTS namespace handle exists.
    pub uts: bool,
    /// Network namespace handle exists.
    pub network: bool,
    /// The configured bubblewrap completed a real all-namespace launch.
    pub functional: bool,
}

impl NamespaceSupport {
    /// Reports complete namespace support required for restricted execution.
    #[must_use]
    pub const fn complete(self) -> bool {
        self.user
            && self.mount
            && self.pid
            && self.ipc
            && self.uts
            && self.network
            && self.functional
    }
}

/// Bubblewrap installation identity and execution result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BubblewrapProbe {
    pub(super) path: PathBuf,
    pub(super) version: Option<String>,
    pub(super) executable_digest: Option<Sha256Digest>,
    pub(super) functional: bool,
}

impl BubblewrapProbe {
    /// Configured absolute executable path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Bounded `--version` output when execution succeeded.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
    /// Hash of the exact executable bytes.
    #[must_use]
    pub const fn executable_digest(&self) -> Option<Sha256Digest> {
        self.executable_digest
    }
    /// Reports whether a real namespace exercise succeeded.
    #[must_use]
    pub const fn functional(&self) -> bool {
        self.functional
    }
}

/// Inputs whose identities and reachability are probed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbeRequest {
    pub(super) bubblewrap_path: PathBuf,
    pub(super) helper_path: PathBuf,
    pub(super) cgroup_root: PathBuf,
    pub(super) proxy_route: Option<ProxyRoute>,
}

impl ProbeRequest {
    /// Creates a probe request from explicit installed paths.
    ///
    /// # Errors
    /// Rejects non-absolute executable or cgroup paths.
    pub fn new(
        bubblewrap_path: PathBuf,
        helper_path: PathBuf,
        cgroup_root: PathBuf,
        proxy_route: Option<ProxyRoute>,
    ) -> Result<Self, LinuxError> {
        if !bubblewrap_path.is_absolute()
            || !helper_path.is_absolute()
            || !cgroup_root.is_absolute()
        {
            return Err(LinuxError::new(
                LinuxErrorKind::InvalidPlan,
                LinuxOperation::Probe,
                LinuxRecovery::CorrectRequest,
                "probe paths must be absolute",
            ));
        }
        Ok(Self { bubblewrap_path, helper_path, cgroup_root, proxy_route })
    }
    /// Returns the bubblewrap path.
    #[must_use]
    pub fn bubblewrap_path(&self) -> &Path {
        &self.bubblewrap_path
    }
    /// Returns the helper path.
    #[must_use]
    pub fn helper_path(&self) -> &Path {
        &self.helper_path
    }
    /// Returns the configured cgroup v2 parent.
    #[must_use]
    pub fn cgroup_root(&self) -> &Path {
        &self.cgroup_root
    }
    /// Returns the optional managed proxy route.
    #[must_use]
    pub const fn proxy_route(&self) -> Option<ProxyRoute> {
        self.proxy_route
    }
}
