//! Explicitly approved provider installation through the vendors' native installers.

use std::{path::Path, process::Command, time::Duration};
#[cfg(windows)]
use std::path::PathBuf;

use peritus_process::{
    NativeProcessProbe, NativeWindowsContainmentIdentity, ProbeObservation, ProcessProbe,
    ProcessTreeIdentity, ProcessTreeQuiescence,
};
#[cfg(windows)]
use peritus_process::NativeWindowsProcessOwner;
use peritus_product_state::ProviderKind;
use peritus_provider_core::{CancellationToken, cancel_first};
use sha2::{Digest as _, Sha256};
use tokio::{io::AsyncReadExt as _, process::Child};

use crate::{
    AccountProvider, OnboardingError, ProviderEffectStore,
    effects::{InstallEffect, InstallRestart},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Installer {
    url: &'static str,
    interpreter: &'static str,
    windows: bool,
}

impl Installer {
    const fn for_provider(kind: ProviderKind, windows: bool) -> Result<Self, OnboardingError> {
        let (url, interpreter) = match (kind, windows) {
            (ProviderKind::CodexAccount, false) => ("https://chatgpt.com/codex/install.sh", "sh"),
            (ProviderKind::CodexAccount, true) => {
                ("https://chatgpt.com/codex/install.ps1", "powershell.exe")
            }
            (ProviderKind::ClaudeAccount, false) => ("https://claude.ai/install.sh", "bash"),
            (ProviderKind::ClaudeAccount, true) => {
                ("https://claude.ai/install.ps1", "powershell.exe")
            }
            _ => return Err(OnboardingError::UnsupportedProvider),
        };
        Ok(Self { url, interpreter, windows })
    }

    fn download(self, target: &Path) -> Command {
        let mut command = Command::new(if self.windows { "curl.exe" } else { "curl" });
        command
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--output",
            ])
            .arg(target)
            .arg(self.url);
        command
    }

    fn execute(self, script: &Path) -> Command {
        let mut command = Command::new(self.interpreter);
        if self.windows {
            command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
        }
        command.arg(script);
        command
    }
}

/// Installs one official account tool after the user has explicitly approved the download.
///
/// This function does not obtain credentials or sign in. The vendor's installer owns its
/// native binary, dependencies, and installation policy. Terminal ownership stays with the
/// foreground process while that installer runs. Callers must request approval first.
///
/// # Errors
///
/// Returns an unsupported-route, download, installer, or installed-executable failure.
pub async fn install_account_provider(
    kind: ProviderKind,
    cancellation: &CancellationToken,
    effects: &ProviderEffectStore,
) -> Result<AccountProvider, OnboardingError> {
    if cancellation.is_cancelled() {
        return Err(OnboardingError::Cancelled);
    }
    let installer = Installer::for_provider(kind, cfg!(windows))?;
    match effects.reconcile_install(kind)? {
        InstallRestart::None => {}
        InstallRestart::Pending {
            effect,
            identity: Some(identity),
            windows_containment,
            cancellation_requested,
        } => {
            return reconnect_install(
                kind,
                cancellation,
                effects,
                effect,
                identity,
                windows_containment,
                cancellation_requested,
            )
            .await;
        }
        InstallRestart::Pending { effect, identity: None, .. } => {
            return Err(reconciliation_error(
                kind,
                "restart adoption",
                &effect,
                "the installer crossed preparation without publishing an exact process identity",
            ));
        }
        InstallRestart::Terminal { effect, success, code } => {
            return settle_terminal_install(kind, effect, success, code);
        }
    }
    if let Ok(provider) = AccountProvider::discover(kind) {
        return Ok(provider);
    }
    let temporary = tempfile::Builder::new()
        .prefix("peritus-provider-install-")
        .tempdir()
        .map_err(|error| installation_error(kind, "temporary directory creation", &error))?;
    let script =
        temporary.path().join(if installer.windows { "install.ps1" } else { "install.sh" });
    run_command(kind, "download", installer.download(&script), cancellation, None).await?;
    let downloaded = script_identity(kind, &script, cancellation).await?;
    let executable = script_identity(kind, &script, cancellation).await?;
    if downloaded != executable {
        return Err(installation_error(
            kind,
            "download inspection",
            &"installer content changed before execution",
        ));
    }
    let operation = effects.begin_install(
        kind,
        installer.url,
        hex(&downloaded.sha256),
        temporary.path().to_owned(),
    )?;
    #[cfg(windows)]
    {
        let _temporary_path = temporary.keep();
        return launch_windows_installer_owner(kind, cancellation, effects, operation).await;
    }
    #[cfg(not(windows))]
    {
        let mut operation = operation;
        let result = async {
        run_command(
            kind,
            "native installer",
            installer.execute(&script),
            cancellation,
            Some(&mut operation),
        )
        .await?;
        AccountProvider::discover(kind)
        }
        .await;
        let cleanup = temporary.close();
        operation.cleanup(cleanup.is_ok())?;
        if result.is_ok() && cleanup.is_ok() {
            operation.settled()?;
        }
        reconcile_temporary_directory(kind, result, cleanup)
    }
}

async fn run_command(
    kind: ProviderKind,
    stage: &'static str,
    mut command: Command,
    cancellation: &CancellationToken,
    mut effect: Option<&mut InstallEffect>,
) -> Result<(), OnboardingError> {
    if cancellation.is_cancelled() {
        return Err(OnboardingError::Cancelled);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let mut child = command.spawn().map_err(|error| installation_error(kind, stage, &error))?;
    let root_pid = child.id().ok_or_else(|| {
        installation_error(kind, stage, &"spawned child has no operating-system identity")
    })?;
    let mut probe = NativeProcessProbe::new();
    let identity = match probe.capture_isolated_child(root_pid) {
        Ok(identity) => identity,
        Err(error) => {
            let cleanup = stop_unidentified_child(&mut child).await;
            if let Some(effect) = effect.as_mut() {
                effect.child_cleanup(cleanup.is_ok(), cleanup.as_ref().err().cloned())?;
                effect.terminal(false, None)?;
            }
            return Err(match cleanup {
                Ok(()) => installation_error(kind, stage, &error),
                Err(cleanup) => OnboardingError::InstallationReconciliation {
                    provider: kind.label(),
                    stage,
                    primary: error.to_string(),
                    cleanup,
                },
            });
        }
    };
    if let Some(effect) = effect.as_mut()
        && let Err(error) = effect.running(identity)
    {
        let cleanup = stop_child(&mut child, identity).await;
        return Err(match cleanup {
            Ok(()) => error,
            Err(cleanup) => OnboardingError::InstallationReconciliation {
                provider: kind.label(),
                stage,
                primary: error.to_string(),
                cleanup,
            },
        });
    }
    let status = match cancel_first(cancellation, child.wait()).await {
        Some(Ok(status)) => status,
        Some(Err(error)) => {
            if let Some(effect) = effect.as_mut() {
                effect.terminal(false, None)?;
            }
            let cleanup = stop_child(&mut child, identity).await;
            if let Some(effect) = effect.as_mut() {
                effect.child_cleanup(cleanup.is_ok(), cleanup.as_ref().err().cloned())?;
            }
            return Err(match cleanup {
                Ok(()) => installation_error(kind, stage, &error),
                Err(cleanup) => OnboardingError::InstallationReconciliation {
                    provider: kind.label(),
                    stage,
                    primary: error.to_string(),
                    cleanup,
                },
            });
        }
        None => {
            if let Some(effect) = effect.as_mut() {
                effect.cancellation_requested()?;
            }
            return match stop_child(&mut child, identity).await {
                Ok(()) => {
                    if let Some(effect) = effect.as_mut() {
                        effect.child_cleanup(true, None)?;
                        effect.terminal(false, None)?;
                    }
                    Err(OnboardingError::Cancelled)
                }
                Err(cleanup) => {
                    if let Some(effect) = effect.as_mut() {
                        effect.child_cleanup(false, Some(cleanup.clone()))?;
                    }
                    Err(OnboardingError::InstallationReconciliation {
                        provider: kind.label(),
                        stage,
                        primary: "user cancellation requested".to_owned(),
                        cleanup,
                    })
                }
            };
        }
    };
    if let Some(effect) = effect.as_mut() {
        effect.child_cleanup(true, None)?;
        effect.terminal(status.success(), status.code())?;
    }
    if status.success() { Ok(()) } else { Err(installation_error(kind, stage, &status)) }
}

async fn stop_child(
    child: &mut Child,
    identity: ProcessTreeIdentity,
) -> Result<(), String> {
    let mut probe = NativeProcessProbe::new();
    let kill = if identity.complete_containment() {
        probe.terminate(identity).err().map(|error| error.to_string())
    } else {
        child.start_kill().err().map(|error| error.to_string())
    };
    match (kill, child.wait().await) {
        (None, Ok(_)) => Ok(()),
        (Some(kill), Ok(_)) => Err(format!("child could not be stopped: {kill}")),
        (None, Err(wait)) => Err(format!("child could not be reaped: {wait}")),
        (Some(kill), Err(wait)) => {
            Err(format!("child could not be stopped ({kill}) or reaped ({wait})"))
        }
    }
}

async fn stop_unidentified_child(child: &mut Child) -> Result<(), String> {
    let kill = child.start_kill().err();
    match (kill, child.wait().await) {
        (None, Ok(_)) => Ok(()),
        (Some(kill), Ok(_)) => Err(format!("child could not be stopped: {kill}")),
        (None, Err(wait)) => Err(format!("child could not be reaped: {wait}")),
        (Some(kill), Err(wait)) => {
            Err(format!("child could not be stopped ({kill}) or reaped ({wait})"))
        }
    }
}

#[cfg(windows)]
const INSTALL_OWNER_FLAG: &str = "--peritus-provider-installer-owner";

/// Returns the exact hidden installer-owner record requested by this process invocation.
#[cfg(windows)]
#[must_use]
pub fn account_installer_owner_argument() -> Option<PathBuf> {
    let mut arguments = std::env::args_os();
    let _program = arguments.next()?;
    if arguments.next()?.to_str()? != INSTALL_OWNER_FLAG {
        return None;
    }
    let path = PathBuf::from(arguments.next()?);
    arguments.next().is_none().then_some(path)
}

/// Runs the independently retained Windows installer owner described by one durable record.
///
/// The owner enters its exact named Job Object and publishes that Job plus its OS birth identity
/// before it dispatches PowerShell. The record therefore remains sufficient for another launcher
/// generation to observe, cancel, or settle the same operation.
///
/// # Errors
/// Returns a validated journal, native owner, script identity, spawn, or installer failure.
#[cfg(windows)]
pub fn run_account_installer_owner(path: &Path) -> Result<(), OnboardingError> {
    let (kind, mut effect) = ProviderEffectStore::reopen_install_owner(path)?;
    let installer = Installer::for_provider(kind, true)?;
    if effect.url() != installer.url {
        return Err(reconciliation_error(
            kind,
            "Windows owner admission",
            &effect,
            "the durable installer URL differs from the official provider URL",
        ));
    }
    let script = effect.temporary_directory().join("install.ps1");
    let observed = script_identity_sync(kind, &script)?;
    if hex(&observed.sha256) != effect.script_sha256() {
        return Err(reconciliation_error(
            kind,
            "Windows owner admission",
            &effect,
            "the installer content differs from its durable SHA-256 identity",
        ));
    }
    let mut digest = Sha256::new();
    digest.update(b"peritus-provider-installer-windows-owner-v1\0");
    digest.update(effect.operation_id().as_bytes());
    digest.update(b"\0");
    digest.update(effect.script_sha256().as_bytes());
    let job_identity = peritus_types::Sha256Digest::new(digest.finalize().into());
    let job_name = format!("Local\\PeritusJob-Provider-{}", effect.operation_id());
    let owner = NativeWindowsProcessOwner::activate_current(job_identity, job_name)
        .map_err(|error| installation_error(kind, "Windows owner activation", &error))?;
    effect.running_windows(owner.identity())?;
    let status = installer.execute(&script).status();
    match status {
        Ok(status) => {
            effect.child_cleanup(true, None)?;
            effect.terminal(status.success(), status.code())?;
            if status.success() {
                Ok(())
            } else {
                Err(installation_error(kind, "native installer", &status))
            }
        }
        Err(error) => {
            effect.child_cleanup(true, None)?;
            effect.terminal(false, None)?;
            Err(installation_error(kind, "native installer", &error))
        }
    }
}

#[cfg(windows)]
async fn launch_windows_installer_owner(
    kind: ProviderKind,
    cancellation: &CancellationToken,
    effects: &ProviderEffectStore,
    mut effect: InstallEffect,
) -> Result<AccountProvider, OnboardingError> {
    let record = effect.journal_path().to_owned();
    let executable = std::env::current_exe()
        .map_err(|error| installation_error(kind, "Windows owner executable", &error))?;
    let mut command = Command::new(executable);
    command.arg(INSTALL_OWNER_FLAG).arg(&record);
    NativeWindowsProcessOwner::configure_detached_command(&mut command);
    let mut child = match tokio::process::Command::from(command).spawn() {
        Ok(child) => child,
        Err(error) => {
            effect.child_cleanup(true, None)?;
            effect.terminal(false, None)?;
            effect.reconcile_cleanup()?;
            effect.settled()?;
            return Err(installation_error(kind, "Windows owner spawn", &error));
        }
    };
    loop {
        match effects.reconcile_install(kind)? {
            InstallRestart::Pending {
                mut effect,
                identity: None,
                ..
            } => {
                if cancellation.is_cancelled() {
                    let kill = child.start_kill();
                    let wait = child.wait().await;
                    match effects.reconcile_install(kind)? {
                        InstallRestart::Pending { mut effect, identity: None, .. } => {
                            let cleanup = kill.is_ok() && wait.is_ok();
                            effect.child_cleanup(
                                cleanup,
                                (!cleanup).then(|| {
                                    format!("owner stop: {kill:?}; owner wait: {wait:?}")
                                }),
                            )?;
                            effect.terminal(false, None)?;
                            effect.reconcile_cleanup()?;
                            effect.settled()?;
                            return Err(OnboardingError::Cancelled);
                        }
                        _ => continue,
                    }
                }
                match child.try_wait() {
                    Ok(None) => {}
                    Ok(Some(status)) => {
                        match effects.reconcile_install(kind)? {
                            InstallRestart::Terminal { effect, success, code } => {
                                return settle_terminal_install(kind, effect, success, code);
                            }
                            InstallRestart::Pending { identity: Some(_), .. } => continue,
                            InstallRestart::Pending {
                                mut effect,
                                identity: None,
                                ..
                            } => {
                                effect.child_cleanup(true, None)?;
                                effect.terminal(false, status.code())?;
                                effect.reconcile_cleanup()?;
                                effect.settled()?;
                                return Err(installation_error(
                                    kind,
                                    "Windows owner activation",
                                    &status,
                                ));
                            }
                            InstallRestart::None => return AccountProvider::discover(kind),
                        }
                    }
                    Err(error) => {
                        return Err(reconciliation_error(
                            kind,
                            "Windows owner observation",
                            &effect,
                            &error.to_string(),
                        ));
                    }
                }
            }
            InstallRestart::Pending {
                effect,
                identity: Some(identity),
                windows_containment: Some(containment),
                cancellation_requested,
            } => {
                drop(child);
                return reconnect_install(
                    kind,
                    cancellation,
                    effects,
                    effect,
                    identity,
                    Some(containment),
                    cancellation_requested,
                )
                .await;
            }
            InstallRestart::Pending { effect, identity: Some(_), .. } => {
                return Err(reconciliation_error(
                    kind,
                    "Windows owner activation",
                    &effect,
                    "the published owner has no reopenable Job Object identity",
                ));
            }
            InstallRestart::Terminal { effect, success, code } => {
                let _ = child.wait().await;
                return settle_terminal_install(kind, effect, success, code);
            }
            InstallRestart::None => {
                return AccountProvider::discover(kind).map_err(|error| {
                    installation_error(kind, "Windows owner reconciliation", &error)
                });
            }
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

#[cfg(windows)]
fn script_identity_sync(
    kind: ProviderKind,
    script: &Path,
) -> Result<ScriptIdentity, OnboardingError> {
    use std::io::Read as _;

    let metadata = std::fs::symlink_metadata(script)
        .map_err(|error| installation_error(kind, "download inspection", &error))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(installation_error(
            kind,
            "download inspection",
            &"installer is empty or is not a regular file",
        ));
    }
    let mut file = std::fs::File::open(script)
        .map_err(|error| installation_error(kind, "download inspection", &error))?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut chunk = [0_u8; 64 * 1_024];
    loop {
        let count = file
            .read(&mut chunk)
            .map_err(|error| installation_error(kind, "download inspection", &error))?;
        if count == 0 {
            break;
        }
        bytes = bytes.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        hasher.update(&chunk[..count]);
    }
    if bytes != metadata.len() {
        return Err(installation_error(
            kind,
            "download inspection",
            &"installer content changed during inspection",
        ));
    }
    Ok(ScriptIdentity {
        bytes,
        sha256: hasher.finalize().into(),
    })
}

#[cfg(windows)]
async fn reconnect_windows_install(
    kind: ProviderKind,
    cancellation: &CancellationToken,
    effects: &ProviderEffectStore,
) -> Result<AccountProvider, OnboardingError> {
    loop {
        match effects.reconcile_install(kind)? {
            InstallRestart::Pending {
                mut effect,
                identity: Some(_),
                windows_containment: Some(containment),
                mut cancellation_requested,
            } => {
                if cancellation.is_cancelled() && !cancellation_requested {
                    effect.cancellation_requested()?;
                    cancellation_requested = true;
                }
                let observation = NativeWindowsProcessOwner::observe_durable(&containment)
                    .map_err(|error| {
                        reconciliation_error(
                            kind,
                            "Windows restart observation",
                            &effect,
                            &error.to_string(),
                        )
                    })?;
                match observation {
                    ProbeObservation::ExactLive => {
                        if cancellation_requested {
                            NativeWindowsProcessOwner::terminate_durable(&containment).map_err(
                                |error| {
                                    reconciliation_error(
                                        kind,
                                        "Windows restart cancellation",
                                        &effect,
                                        &error.to_string(),
                                    )
                                },
                            )?;
                        }
                    }
                    ProbeObservation::ExactAbsent => {
                        let quiescence =
                            NativeWindowsProcessOwner::observe_durable_quiescence(&containment)
                                .map_err(|error| {
                                    reconciliation_error(
                                        kind,
                                        "Windows restart containment observation",
                                        &effect,
                                        &error.to_string(),
                                    )
                                })?;
                        if quiescence != ProcessTreeQuiescence::Quiescent {
                            return Err(reconciliation_error(
                                kind,
                                "Windows restart containment observation",
                                &effect,
                                "the owner exited but complete Job Object quiescence is unverifiable",
                            ));
                        }
                        return match effects.reconcile_install(kind)? {
                            InstallRestart::Terminal { effect, success, code } => {
                                settle_terminal_install(kind, effect, success, code)
                            }
                            InstallRestart::Pending {
                                effect,
                                cancellation_requested: persisted_cancellation,
                                ..
                            } => settle_reconnected_install(
                                kind,
                                effect,
                                cancellation_requested || persisted_cancellation,
                            ),
                            InstallRestart::None => AccountProvider::discover(kind),
                        };
                    }
                    ProbeObservation::Mismatched => {
                        return Err(reconciliation_error(
                            kind,
                            "Windows restart observation",
                            &effect,
                            "the recorded owner PID now names a different process; ownership was retained",
                        ));
                    }
                    ProbeObservation::Unverifiable => {
                        return Err(reconciliation_error(
                            kind,
                            "Windows restart observation",
                            &effect,
                            "the persisted Job Object and owner birth identity cannot be verified",
                        ));
                    }
                }
            }
            InstallRestart::Pending { effect, identity: None, .. } => {
                return Err(reconciliation_error(
                    kind,
                    "Windows restart adoption",
                    &effect,
                    "the prepared operation has not published an owner; automatic redispatch is forbidden",
                ));
            }
            InstallRestart::Pending { effect, identity: Some(_), .. } => {
                return Err(reconciliation_error(
                    kind,
                    "Windows restart adoption",
                    &effect,
                    "the installer owner has no reopenable Job Object identity",
                ));
            }
            InstallRestart::Terminal { effect, success, code } => {
                return settle_terminal_install(kind, effect, success, code);
            }
            InstallRestart::None => return AccountProvider::discover(kind),
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn reconnect_install(
    kind: ProviderKind,
    cancellation: &CancellationToken,
    effects: &ProviderEffectStore,
    mut effect: InstallEffect,
    identity: ProcessTreeIdentity,
    windows_containment: Option<NativeWindowsContainmentIdentity>,
    mut cancellation_requested: bool,
) -> Result<AccountProvider, OnboardingError> {
    #[cfg(windows)]
    {
        if windows_containment.is_none() {
            return Err(reconciliation_error(
                kind,
                "restart adoption",
                &effect,
                "the Windows installer has no durable Job Object identity",
            ));
        }
        let _ = (&mut effect, identity, &mut cancellation_requested);
        return reconnect_windows_install(kind, cancellation, effects).await;
    }
    #[cfg(not(windows))]
    {
    let _ = (effects, windows_containment);
    let mut probe = NativeProcessProbe::new();
    loop {
        if cancellation.is_cancelled() && !cancellation_requested {
            effect.cancellation_requested()?;
            cancellation_requested = true;
        }
        let observation = probe.observe(identity).map_err(|error| {
            reconciliation_error(kind, "restart observation", &effect, &error.to_string())
        })?;
        match observation {
            ProbeObservation::ExactLive => {
                if cancellation_requested {
                    probe.terminate(identity).map_err(|error| {
                        reconciliation_error(
                            kind,
                            "restart cancellation",
                            &effect,
                            &error.to_string(),
                        )
                    })?;
                }
            }
            ProbeObservation::ExactAbsent => {
                let quiescence = probe.observe_quiescence(identity).map_err(|error| {
                    reconciliation_error(
                        kind,
                        "restart containment observation",
                        &effect,
                        &error.to_string(),
                    )
                })?;
                if quiescence != ProcessTreeQuiescence::Quiescent {
                    return Err(reconciliation_error(
                        kind,
                        "restart containment observation",
                        &effect,
                        "the root exited but complete process-tree absence is unverifiable",
                    ));
                }
                return settle_reconnected_install(kind, effect, cancellation_requested);
            }
            ProbeObservation::Mismatched => {
                return Err(reconciliation_error(
                    kind,
                    "restart observation",
                    &effect,
                    "the recorded PID now names a different process; ownership was retained",
                ));
            }
            ProbeObservation::Unverifiable => {
                return Err(reconciliation_error(
                    kind,
                    "restart observation",
                    &effect,
                    "the exact process birth or complete tree containment is unverifiable",
                ));
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    }
}

fn settle_terminal_install(
    kind: ProviderKind,
    mut effect: InstallEffect,
    success: bool,
    code: Option<i32>,
) -> Result<AccountProvider, OnboardingError> {
    let operation_id = effect.operation_id().to_owned();
    if success {
        let provider = AccountProvider::discover(kind);
        effect.reconcile_cleanup()?;
        effect.settled()?;
        return provider;
    }
    effect.reconcile_cleanup()?;
    effect.settled()?;
    Err(OnboardingError::InstallationReconciliation {
        provider: kind.label(),
        stage: "restart reconciliation",
        primary: format!(
            "owned installer operation {operation_id} reached terminal code {code:?}"
        ),
        cleanup: "terminal ownership was reconciled; explicit approval is required to retry"
            .to_owned(),
    })
}

fn settle_reconnected_install(
    kind: ProviderKind,
    mut effect: InstallEffect,
    cancelled: bool,
) -> Result<AccountProvider, OnboardingError> {
    let discovered = (!cancelled).then(|| AccountProvider::discover(kind));
    let success = discovered.as_ref().is_some_and(Result::is_ok);
    effect.child_cleanup(true, None)?;
    effect.terminal(success, None)?;
    effect.reconcile_cleanup()?;
    effect.settled()?;
    match discovered {
        Some(provider) => provider,
        None => Err(OnboardingError::Cancelled),
    }
}

fn reconciliation_error(
    kind: ProviderKind,
    stage: &'static str,
    effect: &InstallEffect,
    primary: &str,
) -> OnboardingError {
    OnboardingError::InstallationReconciliation {
        provider: kind.label(),
        stage,
        primary: format!("owned installer operation {}: {primary}", effect.operation_id()),
        cleanup: format!(
            "the durable record remains at {}; inspect this exact operation and its recorded owner before retrying",
            effect.journal_path().display(),
        ),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScriptIdentity {
    bytes: u64,
    sha256: [u8; 32],
}

async fn script_identity(
    kind: ProviderKind,
    script: &Path,
    cancellation: &CancellationToken,
) -> Result<ScriptIdentity, OnboardingError> {
    if cancellation.is_cancelled() {
        return Err(OnboardingError::Cancelled);
    }
    let metadata = tokio::fs::symlink_metadata(script)
        .await
        .map_err(|error| installation_error(kind, "download inspection", &error))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(installation_error(
            kind,
            "download inspection",
            &"installer is empty or is not a regular file",
        ));
    }
    let mut file = tokio::fs::File::open(script)
        .await
        .map_err(|error| installation_error(kind, "download inspection", &error))?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut chunk = vec![0_u8; 64 * 1024];
    loop {
        let count = match cancel_first(cancellation, file.read(&mut chunk)).await {
            Some(result) => result
                .map_err(|error| installation_error(kind, "download inspection", &error))?,
            None => return Err(OnboardingError::Cancelled),
        };
        if count == 0 {
            break;
        }
        bytes = bytes.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        hasher.update(&chunk[..count]);
    }
    if bytes != metadata.len() {
        return Err(installation_error(
            kind,
            "download inspection",
            &"installer content changed during inspection",
        ));
    }
    Ok(ScriptIdentity { bytes, sha256: hasher.finalize().into() })
}

fn reconcile_temporary_directory(
    kind: ProviderKind,
    result: Result<AccountProvider, OnboardingError>,
    cleanup: std::io::Result<()>,
) -> Result<AccountProvider, OnboardingError> {
    match (result, cleanup) {
        (result, Ok(())) => result,
        (Ok(_), Err(cleanup)) => Err(OnboardingError::InstallationReconciliation {
            provider: kind.label(),
            stage: "temporary workspace cleanup",
            primary: "installer completed and its executable was discovered".to_owned(),
            cleanup: cleanup.to_string(),
        }),
        (Err(primary), Err(cleanup)) => Err(OnboardingError::InstallationReconciliation {
            provider: kind.label(),
            stage: "temporary workspace cleanup",
            primary: primary.to_string(),
            cleanup: cleanup.to_string(),
        }),
    }
}

fn installation_error(
    kind: ProviderKind,
    stage: &'static str,
    detail: &dyn std::fmt::Display,
) -> OnboardingError {
    OnboardingError::Installation { provider: kind.label(), stage, detail: detail.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_account_platform_uses_the_official_native_installer() {
        for (kind, windows, url, interpreter) in [
            (ProviderKind::CodexAccount, false, "https://chatgpt.com/codex/install.sh", "sh"),
            (
                ProviderKind::CodexAccount,
                true,
                "https://chatgpt.com/codex/install.ps1",
                "powershell.exe",
            ),
            (ProviderKind::ClaudeAccount, false, "https://claude.ai/install.sh", "bash"),
            (ProviderKind::ClaudeAccount, true, "https://claude.ai/install.ps1", "powershell.exe"),
        ] {
            let installer = Installer::for_provider(kind, windows).expect("account installer");
            assert_eq!(installer.url, url);
            assert_eq!(installer.interpreter, interpreter);
            let path = Path::new("path with spaces/install-script");
            let download = installer.download(path);
            let args = download
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            assert!(args.windows(2).any(|pair| pair == ["--proto", "=https"]));
            assert!(!args.iter().any(|arg| arg == "--max-time" || arg == "--connect-timeout"));
            assert!(!args.iter().any(|arg| arg == "--max-filesize"));
            assert_eq!(args.last().expect("URL"), url);
            let execution = installer.execute(path);
            assert_eq!(execution.get_program(), interpreter);
            assert_eq!(execution.get_args().last().expect("script argument"), path.as_os_str());
        }
    }

    #[test]
    fn direct_api_routes_never_have_an_executable_installer() {
        for kind in [
            ProviderKind::OpenAiApi,
            ProviderKind::AnthropicApi,
            ProviderKind::GoogleGeminiApi,
            ProviderKind::CompatibleEndpoint,
        ] {
            for windows in [false, true] {
                assert!(matches!(
                    Installer::for_provider(kind, windows),
                    Err(OnboardingError::UnsupportedProvider)
                ));
            }
        }
    }

    #[tokio::test]
    async fn command_failure_is_not_reported_as_installation_success() {
        let command = Command::new("peritus-missing-installer-fixture-command");
        assert!(matches!(
            run_command(
                ProviderKind::CodexAccount,
                "download",
                command,
                &CancellationToken::new(),
                None,
            )
            .await,
            Err(OnboardingError::Installation { stage: "download", .. })
        ));
    }
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    value
}
