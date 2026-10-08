//! Cached release discovery and durable native self-update composition.

mod download;
mod install;
mod process;
mod release;

use std::{fs, time::Duration};

use peritus_provider_core::{CancellationToken, cancel_first};

use crate::{AppLayout, LauncherError, terminal::Terminal};
use release::Release;

const CHECK_INTERVAL: Duration = Duration::from_hours(6);
const MAX_DISCOVERY_RECEIPT_BYTES: u64 = 256 * 1024;

pub(super) struct StartupDiscovery {
    cancellation: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for StartupDiscovery {
    fn drop(&mut self) {
        let _ = self.cancellation.cancel();
        self.task.abort();
    }
}

/// Offers only an already completed durable discovery result.
///
/// Network discovery is owned separately by [`start_discovery`] and never delays launch.
pub(super) async fn offer_on_startup(layout: &AppLayout) -> Result<bool, LauncherError> {
    if cfg!(feature = "system-package") || !automatic_checks_enabled(layout) {
        return Ok(false);
    }
    let Ok(Some(release)) = cached_discovery(layout) else {
        return Ok(false);
    };
    if !release.is_newer() {
        return Ok(false);
    }
    let accepted = {
        let mut terminal = Terminal::stdio();
        terminal.line(&format!(
            "Peritus {} is available (installed: {}).",
            release.version(),
            env!("CARGO_PKG_VERSION")
        ))?;
        terminal.confirm("Install the update now? [Y/n]: ", true)?
    };
    if !accepted {
        return Ok(false);
    }
    announce(&format!("Downloading and verifying Peritus {}...", release.version()))?;
    apply(layout, &release).await?;
    announce_completion(&release)?;
    Ok(true)
}

/// Starts a caller-owned, cancellation-aware release poll when the six-hour policy permits it.
pub(super) fn start_discovery(layout: &AppLayout) -> Option<StartupDiscovery> {
    if cfg!(feature = "system-package")
        || !automatic_checks_enabled(layout)
        || check_is_fresh(layout)
    {
        return None;
    }
    let cancellation = CancellationToken::new();
    let worker_cancellation = cancellation.clone();
    let layout = layout.clone();
    let task = tokio::spawn(async move {
        let Some(result) = cancel_first(&worker_cancellation, release::latest()).await else {
            return;
        };
        let Ok(discovery) = result else {
            return;
        };
        if persist_discovery(&layout, discovery.as_ref()).is_ok() {
            let _ = record_check(&layout);
        }
    });
    Some(StartupDiscovery { cancellation, task })
}

pub(super) fn configure_checks(
    layout: &AppLayout,
    enabled: bool,
) -> Result<(), LauncherError> {
    require_self_managed_install()?;
    let value = if enabled { b"enabled\n".as_slice() } else { b"disabled\n".as_slice() };
    persist_check_setting(layout, value)?;
    announce(if enabled {
        "Automatic startup update checks are enabled."
    } else {
        "Automatic startup update checks are disabled. Run `peritus update` to check manually."
    })
}

pub(super) async fn run_explicit(layout: &AppLayout) -> Result<(), LauncherError> {
    require_self_managed_install()?;
    if let Some(release) = download::pending_release(layout)? {
        announce(&format!("Resuming the acknowledged Peritus {} update...", release.version()))?;
        apply(layout, &release).await?;
        return announce_completion(&release);
    }
    announce("Checking for Peritus updates...")?;
    let release = release::latest().await?;
    persist_discovery(layout, release.as_ref())?;
    record_check(layout)?;
    let Some(release) = release else {
        return Err(LauncherError::Update(
            "the public release service has no current release".to_owned(),
        ));
    };
    if !release.is_newer() {
        announce(&format!("Peritus {} is already current.", env!("CARGO_PKG_VERSION")))?;
        return Ok(());
    }
    announce(&format!("Downloading and verifying Peritus {}...", release.version()))?;
    apply(layout, &release).await?;
    announce_completion(&release)
}

fn require_self_managed_install() -> Result<(), LauncherError> {
    if cfg!(feature = "system-package") {
        return Err(LauncherError::Update(
            "this installation is managed by your system package manager; update Peritus with apt or dnf instead of `peritus update`".to_owned(),
        ));
    }
    Ok(())
}

async fn apply(layout: &AppLayout, release: &Release) -> Result<(), LauncherError> {
    require_self_managed_install()?;
    let mut package = download::package(layout, release).await?;
    install::apply(&mut package, release)
}

fn announce_completion(release: &Release) -> Result<(), LauncherError> {
    if cfg!(windows) {
        announce(
            "The exact update owner will finish in the background. Start Peritus again after it completes.",
        )?;
    } else {
        announce(&format!(
            "Peritus {} is installed. Run `peritus` to continue.",
            release.version()
        ))?;
    }
    Ok(())
}

fn announce(message: &str) -> Result<(), LauncherError> { Terminal::stdio().line(message) }

fn check_is_fresh(layout: &AppLayout) -> bool {
    fs::metadata(layout.cache_root().join("update-check"))
        .and_then(|metadata| metadata.modified())
        .and_then(|modified| modified.elapsed().map_err(std::io::Error::other))
        .is_ok_and(|elapsed| elapsed < CHECK_INTERVAL)
}

fn automatic_checks_enabled(layout: &AppLayout) -> bool {
    fs::read(layout.config_root().join("update-checks"))
        .map_or(true, |value| value != b"disabled\n")
}

fn persist_check_setting(layout: &AppLayout, value: &[u8]) -> Result<(), LauncherError> {
    let path = layout.config_root().join("update-checks");
    let current = crate::persistence::read_exact_or_publish(&path, value)?;
    if current != value {
        crate::persistence::replace_recovery_file(&path, value)?;
    }
    Ok(())
}

fn record_check(layout: &AppLayout) -> Result<(), LauncherError> {
    let path = layout.cache_root().join("update-check");
    fs::write(&path, format!("{}\n", env!("CARGO_PKG_VERSION")))
        .map_err(|error| LauncherError::filesystem("record update check", path, error))
}

fn discovery_path(layout: &AppLayout) -> std::path::PathBuf {
    layout.update_effects_root().join("available-release.json")
}

fn persist_discovery(
    layout: &AppLayout,
    release: Option<&Release>,
) -> Result<(), LauncherError> {
    let path = discovery_path(layout);
    let bytes = serde_json::to_vec(&release)
        .map_err(|error| LauncherError::Update(format!("encode release discovery: {error}")))?;
    match fs::metadata(&path) {
        Ok(_) => crate::persistence::replace_recovery_file(&path, &bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let actual = crate::persistence::read_exact_or_publish(&path, &bytes)?;
            if actual == bytes {
                Ok(())
            } else {
                crate::persistence::replace_recovery_file(&path, &bytes)
            }
        }
        Err(error) => Err(LauncherError::filesystem(
            "inspect release discovery receipt",
            path,
            error,
        )),
    }
}

fn cached_discovery(layout: &AppLayout) -> Result<Option<Release>, LauncherError> {
    let path = discovery_path(layout);
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(LauncherError::filesystem(
                "inspect release discovery receipt",
                path,
                error,
            ));
        }
    };
    if !metadata.is_file() || metadata.len() > MAX_DISCOVERY_RECEIPT_BYTES {
        return Err(LauncherError::Update(
            "release discovery receipt is not a bounded regular file".to_owned(),
        ));
    }
    let bytes = fs::read(&path)
        .map_err(|error| LauncherError::filesystem("read release discovery receipt", path, error))?;
    let release: Option<Release> = serde_json::from_slice(&bytes)
        .map_err(|error| LauncherError::Update(format!("decode release discovery: {error}")))?;
    if let Some(release) = &release {
        release.validate()?;
    }
    Ok(release)
}

#[cfg(windows)]
/// Returns the hidden exact update-owner receipt requested by this invocation.
#[must_use]
pub fn update_owner_argument() -> Option<std::path::PathBuf> {
    install::update_owner_argument()
}

#[cfg(windows)]
/// Runs one independently retained exact-birth Windows update owner.
///
/// # Errors
/// Returns a receipt, package, native ownership, installation, or pair-verification failure.
pub fn run_update_owner(path: &std::path::Path) -> Result<(), LauncherError> {
    install::run_update_owner(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_ownership_matches_build_configuration() {
        assert_eq!(require_self_managed_install().is_err(), cfg!(feature = "system-package"));
    }

    #[cfg(feature = "system-package")]
    #[tokio::test]
    async fn system_package_never_queries_release_service_or_changes_settings() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let layout = AppLayout::for_test(temporary.path()).prepare().expect("layout");
        assert!(!offer_on_startup(&layout).await.expect("no startup update"));
        assert!(
            run_explicit(&layout)
                .await
                .expect_err("package ownership")
                .to_string()
                .contains("apt or dnf")
        );
        assert!(configure_checks(&layout, true).is_err());
        assert!(!layout.config_root().join("update-checks").exists());
        assert!(!layout.cache_root().join("update-check").exists());
    }

    #[test]
    fn fresh_check_suppresses_network_poll() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let layout = AppLayout::for_test(temporary.path()).prepare().expect("layout");
        assert!(!check_is_fresh(&layout));
        record_check(&layout).expect("record");
        assert!(check_is_fresh(&layout));
    }

    #[test]
    fn automatic_checks_default_on_and_persist_off() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let layout = AppLayout::for_test(temporary.path()).prepare().expect("layout");
        assert!(automatic_checks_enabled(&layout));
        persist_check_setting(&layout, b"disabled\n").expect("disable");
        assert!(!automatic_checks_enabled(&layout));
    }
}
