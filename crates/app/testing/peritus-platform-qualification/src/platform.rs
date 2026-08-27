//! Validated packaged-host platform contracts and declared deltas.

use crate::{QualificationError, QualificationErrorCode, QualificationRecovery};

mod model;

pub use model::{NativePrerequisite, PlatformDelta, PlatformVersion};

/// Supported H2 operating-system family.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Platform {
    /// Linux with the C3 native namespace, Landlock, seccomp, and cgroup backend.
    Linux,
    /// macOS with the C3 Seatbelt backend.
    Macos,
    /// Windows with the C3 token, `AppContainer`, Job Object, ACL, and WFP backend.
    Windows,
}

impl Platform {
    /// Returns the canonical manifest spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Windows => "windows",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, QualificationError> {
        match value {
            "linux" => Ok(Self::Linux),
            "macos" => Ok(Self::Macos),
            "windows" => Ok(Self::Windows),
            _ => Err(invalid("package platform is not supported by H2")),
        }
    }
}

/// Supported release architecture.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Architecture {
    /// AMD64/x86-64.
    X86_64,
    /// 64-bit Arm.
    Aarch64,
}

impl Architecture {
    /// Returns the canonical manifest spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, QualificationError> {
        match value {
            "x86_64" => Ok(Self::X86_64),
            "aarch64" => Ok(Self::Aarch64),
            _ => Err(invalid("package architecture is not supported by H2")),
        }
    }
}

/// Exact H2 host target.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct QualificationTarget {
    platform: Platform,
    architecture: Architecture,
    version: PlatformVersion,
}

impl QualificationTarget {
    /// Creates a target observation.
    #[must_use]
    pub const fn new(
        platform: Platform,
        architecture: Architecture,
        version: PlatformVersion,
    ) -> Self {
        Self { platform, architecture, version }
    }

    /// Returns the operating-system family.
    #[must_use]
    pub const fn platform(self) -> Platform {
        self.platform
    }

    /// Returns the machine architecture.
    #[must_use]
    pub const fn architecture(self) -> Architecture {
        self.architecture
    }

    /// Returns the observed operating-system version.
    #[must_use]
    pub const fn version(self) -> PlatformVersion {
        self.version
    }
}

/// Frozen production host contract for one operating-system family.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformContract {
    platform: Platform,
    minimum_version: PlatformVersion,
    architectures: &'static [Architecture],
    prerequisites: &'static [NativePrerequisite],
    deltas: &'static [PlatformDelta],
}

impl PlatformContract {
    /// Returns the reviewed C3/H2 production contract for a platform.
    #[must_use]
    pub const fn production(platform: Platform) -> Self {
        match platform {
            Platform::Linux => Self {
                platform,
                minimum_version: PlatformVersion::new(6, 6, 0, 0),
                architectures: &LINUX_ARCHITECTURES,
                prerequisites: &LINUX_PREREQUISITES,
                deltas: &PLATFORM_DELTAS,
            },
            Platform::Macos => Self {
                platform,
                minimum_version: PlatformVersion::new(15, 0, 0, 0),
                architectures: &MACOS_ARCHITECTURES,
                prerequisites: &MACOS_PREREQUISITES,
                deltas: &PLATFORM_DELTAS,
            },
            Platform::Windows => Self {
                platform,
                minimum_version: PlatformVersion::new(11, 0, 0, 26_100),
                architectures: &WINDOWS_ARCHITECTURES,
                prerequisites: &WINDOWS_PREREQUISITES,
                deltas: &PLATFORM_DELTAS,
            },
        }
    }

    /// Returns the operating-system family.
    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    /// Returns the minimum production version.
    #[must_use]
    pub const fn minimum_version(&self) -> PlatformVersion {
        self.minimum_version
    }

    /// Borrows supported architectures.
    #[must_use]
    pub const fn architectures(&self) -> &'static [Architecture] {
        self.architectures
    }

    /// Borrows closed native prerequisites.
    #[must_use]
    pub const fn prerequisites(&self) -> &'static [NativePrerequisite] {
        self.prerequisites
    }

    /// Borrows the complete cross-platform delta declaration.
    #[must_use]
    pub const fn deltas(&self) -> &'static [PlatformDelta] {
        self.deltas
    }

    /// Validates that a subject can be considered for production qualification.
    ///
    /// # Errors
    ///
    /// Rejects platform, architecture, or minimum-version mismatches before scenarios run.
    pub fn validate_target(&self, target: QualificationTarget) -> Result<(), QualificationError> {
        if target.platform != self.platform {
            return Err(unsupported("qualification target platform differs from the contract"));
        }
        if !self.architectures.contains(&target.architecture) {
            return Err(unsupported("qualification target architecture is unsupported"));
        }
        let version_supported = match self.platform {
            Platform::Windows => target.version.build() >= self.minimum_version.build(),
            Platform::Linux | Platform::Macos => target.version >= self.minimum_version,
        };
        if !version_supported {
            return Err(unsupported("qualification target is older than the production minimum"));
        }
        Ok(())
    }
}

const LINUX_ARCHITECTURES: [Architecture; 2] = [Architecture::X86_64, Architecture::Aarch64];
const MACOS_ARCHITECTURES: [Architecture; 2] = [Architecture::X86_64, Architecture::Aarch64];
const WINDOWS_ARCHITECTURES: [Architecture; 1] = [Architecture::X86_64];

const LINUX_PREREQUISITES: [NativePrerequisite; 10] = [
    NativePrerequisite::new(
        "linux.namespaces",
        "functional user, mount, PID, IPC, UTS, and network namespaces",
        true,
    ),
    NativePrerequisite::new(
        "linux.bubblewrap",
        "reviewed bubblewrap executable supporting the required namespaces",
        true,
    ),
    NativePrerequisite::new("linux.landlock", "Landlock ABI 3 or newer", true),
    NativePrerequisite::new("linux.seccomp", "seccomp-BPF enforcement", true),
    NativePrerequisite::new(
        "linux.cgroup-v2",
        "delegated cgroup v2 controllers required by the checked plan",
        true,
    ),
    NativePrerequisite::new("linux.pty", "host PTY support when requested", true),
    NativePrerequisite::new(
        "linux.secret-service",
        "Secret Service credential store for configured direct providers or secrets",
        true,
    ),
    NativePrerequisite::new(
        "linux.systemd-user",
        "systemd user manager for packaged autostart",
        true,
    ),
    NativePrerequisite::new(
        "peritus.linux-helper",
        "digest-matched peritus-linux-sandbox-helper",
        false,
    ),
    NativePrerequisite::new(
        "peritus.local-proxy",
        "managed loopback proxy bridge when egress is requested",
        false,
    ),
];

const MACOS_PREREQUISITES: [NativePrerequisite; 8] = [
    NativePrerequisite::new("macos.seatbelt", "functional system Seatbelt profile mechanism", true),
    NativePrerequisite::new("macos.process-group", "process-group ownership and signalling", true),
    NativePrerequisite::new("macos.pty", "host PTY support when requested", true),
    NativePrerequisite::new("macos.rlimits", "required native rlimits plus C2 supervision", true),
    NativePrerequisite::new(
        "macos.keychain",
        "Keychain access for configured direct providers or secrets",
        true,
    ),
    NativePrerequisite::new("macos.launchd", "per-user launchd LaunchAgent support", true),
    NativePrerequisite::new(
        "peritus.macos-helper",
        "digest-matched peritus-macos-sandbox-helper",
        false,
    ),
    NativePrerequisite::new(
        "peritus.local-proxy",
        "managed loopback proxy when egress is requested",
        false,
    ),
];

const WINDOWS_PREREQUISITES: [NativePrerequisite; 10] = [
    NativePrerequisite::new(
        "windows.restricted-token",
        "restricted primary token and low-integrity or AppContainer support",
        true,
    ),
    NativePrerequisite::new("windows.job-object", "kill-on-close Job Object support", true),
    NativePrerequisite::new(
        "windows.handle-list",
        "exact inherited-handle-list process creation",
        true,
    ),
    NativePrerequisite::new(
        "windows.acl",
        "workspace ACL inspection, application, and reversal",
        true,
    ),
    NativePrerequisite::new("windows.conpty", "ConPTY support when requested", true),
    NativePrerequisite::new(
        "windows.credential-manager",
        "Credential Manager for configured direct providers or secrets",
        true,
    ),
    NativePrerequisite::new(
        "windows.bfe-wfp",
        "BFE and dynamic WFP management when managed egress is requested",
        true,
    ),
    NativePrerequisite::new(
        "windows.task-scheduler",
        "per-user Task Scheduler logon trigger",
        true,
    ),
    NativePrerequisite::new(
        "peritus.windows-helper",
        "digest-matched peritus-windows-sandbox-helper.exe",
        false,
    ),
    NativePrerequisite::new(
        "peritus.local-proxy",
        "managed loopback proxy when egress is requested",
        false,
    ),
];

const PLATFORM_DELTAS: [PlatformDelta; 6] = [
    PlatformDelta {
        id: "ipc.address",
        linux: "mode-0600 Unix socket below the protected state root",
        macos: "mode-0600 Unix socket below the protected state root",
        windows: "owner-restricted local named pipe with no filesystem object",
    },
    PlatformDelta {
        id: "service.supervisor",
        linux: "systemd user service",
        macos: "per-user launchd LaunchAgent",
        windows: "per-user Task Scheduler logon task",
    },
    PlatformDelta {
        id: "service.logs",
        linux: "systemd user journal",
        macos: "owner-private files below Library/Logs/Peritus",
        windows: "Task Scheduler operational event log plus explicitly configured G0 local telemetry",
    },
    PlatformDelta {
        id: "process.terminal",
        linux: "Unix PTY with process-group ownership",
        macos: "Unix PTY with process-group ownership",
        windows: "ConPTY with a protected helper control channel",
    },
    PlatformDelta {
        id: "process.exit",
        linux: "exit code or POSIX signal",
        macos: "exit code or POSIX signal",
        windows: "exit code or Windows exception classification",
    },
    PlatformDelta {
        id: "sandbox.native",
        linux: "namespaces, Landlock, seccomp, cgroup v2, and rlimits",
        macos: "Seatbelt, process groups, rlimits, and C2-supervised dimensions",
        windows: "restricted token or AppContainer, Job Object, ACLs, WFP, and native limits",
    },
];

fn invalid(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::InvalidInput,
        QualificationRecovery::CorrectInput,
        "validate platform contract",
        detail,
    )
}

fn unsupported(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::Unsupported,
        QualificationRecovery::ConfigureHost,
        "validate qualification target",
        detail,
    )
}
