//! Per-user supervisor contracts for the foreground G0 daemon.

use crate::{
    InstallPath, Platform, QualificationError, QualificationErrorCode, QualificationRecovery,
};

/// Native per-user supervisor selected for a target platform.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SupervisorKind {
    /// systemd user manager.
    SystemdUser,
    /// `launchd` `LaunchAgent` in the logged-in user's domain.
    LaunchAgent,
    /// Windows Task Scheduler task with a user-logon trigger.
    WindowsTaskScheduler,
}

impl SupervisorKind {
    /// Returns the reviewed supervisor for a platform.
    #[must_use]
    pub const fn for_platform(platform: Platform) -> Self {
        match platform {
            Platform::Linux => Self::SystemdUser,
            Platform::Macos => Self::LaunchAgent,
            Platform::Windows => Self::WindowsTaskScheduler,
        }
    }
}

/// Failure restart behavior owned by the external supervisor.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RestartPolicy {
    on_failure: bool,
    maximum_attempts: Option<u16>,
    window_seconds: u32,
    delay_seconds: u16,
}

impl RestartPolicy {
    /// Returns platform-native production restart behavior.
    #[must_use]
    pub const fn production(platform: Platform) -> Self {
        Self {
            on_failure: true,
            maximum_attempts: match platform {
                Platform::Linux | Platform::Windows => Some(5),
                Platform::Macos => None,
            },
            window_seconds: 300,
            delay_seconds: 5,
        }
    }

    /// Reports whether a nonzero daemon exit is restarted.
    #[must_use]
    pub const fn on_failure(self) -> bool {
        self.on_failure
    }

    /// Returns the maximum attempts in one restart window.
    #[must_use]
    pub const fn maximum_attempts(self) -> Option<u16> {
        self.maximum_attempts
    }

    /// Returns the restart accounting window.
    #[must_use]
    pub const fn window_seconds(self) -> u32 {
        self.window_seconds
    }

    /// Returns the delay before restart.
    #[must_use]
    pub const fn delay_seconds(self) -> u16 {
        self.delay_seconds
    }
}

/// Native stdout/stderr ownership for the daemon service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceLogContract {
    /// Both streams are retained by the systemd user journal.
    SystemdJournal,
    /// Each stream is appended to an exact owner-private file.
    PrivateFiles {
        /// Exact standard-output log path.
        stdout: InstallPath,
        /// Exact standard-error log path.
        stderr: InstallPath,
    },
    /// Task lifecycle is retained by the Windows Task Scheduler operational event log; Peritus
    /// application telemetry remains governed by the strict G0 telemetry configuration.
    WindowsTaskSchedulerLog,
}

/// Exact foreground daemon invocation and lifecycle expected from native packaging.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceContract {
    platform: Platform,
    supervisor: SupervisorKind,
    executable: InstallPath,
    arguments: [String; 3],
    definition: InstallPath,
    logs: ServiceLogContract,
    restart: RestartPolicy,
    user_scoped: bool,
    shell_wrapped: bool,
    autostart: bool,
}

impl ServiceContract {
    /// Constructs the production per-user supervisor contract from a release layout.
    ///
    /// # Errors
    ///
    /// Returns a layout error if required descendants cannot be constructed.
    pub fn production(layout: &crate::ReleaseLayout) -> Result<Self, QualificationError> {
        let platform = layout.platform();
        let suffix = if platform == Platform::Windows { ".exe" } else { "" };
        let executable = layout.binary_directory().join(platform, &format!("peritusd{suffix}"))?;
        let logs = match platform {
            Platform::Linux => ServiceLogContract::SystemdJournal,
            Platform::Macos => ServiceLogContract::PrivateFiles {
                stdout: layout.log_root().join(platform, "peritusd.stdout.log")?,
                stderr: layout.log_root().join(platform, "peritusd.stderr.log")?,
            },
            Platform::Windows => ServiceLogContract::WindowsTaskSchedulerLog,
        };
        let service = Self {
            platform,
            supervisor: SupervisorKind::for_platform(platform),
            executable,
            arguments: [
                "serve".to_owned(),
                "--config".to_owned(),
                layout.config_file().as_str().to_owned(),
            ],
            definition: layout.service_definition().clone(),
            logs,
            restart: RestartPolicy::production(platform),
            user_scoped: true,
            shell_wrapped: false,
            autostart: true,
        };
        service.validate()?;
        Ok(service)
    }

    /// Returns the target platform.
    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    /// Returns the native supervisor.
    #[must_use]
    pub const fn supervisor(&self) -> SupervisorKind {
        self.supervisor
    }

    /// Borrows the exact `peritusd` executable path.
    #[must_use]
    pub const fn executable(&self) -> &InstallPath {
        &self.executable
    }

    /// Borrows the exact structured argv after the executable.
    #[must_use]
    pub const fn arguments(&self) -> &[String; 3] {
        &self.arguments
    }

    /// Borrows the native supervisor definition path.
    #[must_use]
    pub const fn definition(&self) -> &InstallPath {
        &self.definition
    }

    /// Borrows stdout/stderr ownership.
    #[must_use]
    pub const fn logs(&self) -> &ServiceLogContract {
        &self.logs
    }

    /// Returns the bounded restart policy.
    #[must_use]
    pub const fn restart(&self) -> RestartPolicy {
        self.restart
    }

    /// Reports whether the service runs as the installing user.
    #[must_use]
    pub const fn user_scoped(&self) -> bool {
        self.user_scoped
    }

    /// Reports whether a shell is interposed between the supervisor and `peritusd`.
    #[must_use]
    pub const fn shell_wrapped(&self) -> bool {
        self.shell_wrapped
    }

    /// Reports whether login autostart is installed.
    #[must_use]
    pub const fn autostart(&self) -> bool {
        self.autostart
    }

    fn validate(&self) -> Result<(), QualificationError> {
        if self.supervisor != SupervisorKind::for_platform(self.platform)
            || self.arguments[0] != "serve"
            || self.arguments[1] != "--config"
            || self.arguments[2].is_empty()
            || !self.user_scoped
            || self.shell_wrapped
            || !self.autostart
        {
            return Err(service_error(
                "service must directly supervise `peritusd serve --config <absolute-file>` as the installing user",
            ));
        }
        Ok(())
    }
}

fn service_error(detail: &'static str) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::Lifecycle,
        QualificationRecovery::RebuildRelease,
        "validate daemon supervisor contract",
        detail,
    )
}
