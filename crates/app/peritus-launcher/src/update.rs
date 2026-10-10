//! Cached public release discovery and native self-update composition.

mod download;
mod install;
mod process;
mod release;

use std::{fs, time::Duration};

use crate::{AppLayout, LauncherError, terminal::Terminal};
use release::Release;

const CHECK_INTERVAL: Duration = Duration::from_hours(6);

/// Cancels optional discovery when the interactive launch owner exits.
pub struct Discovery(Option<tokio::task::JoinHandle<()>>);

impl Drop for Discovery {
    fn drop(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
        }
    }
}

pub fn start_discovery(layout: &AppLayout) -> Discovery {
    if cfg!(feature = "system-package")
        || !automatic_checks_enabled(layout)
        || check_is_fresh(layout)
    {
        return Discovery(None);
    }
    discover(layout.clone(), release::latest())
}

fn discover(
    layout: AppLayout,
    query: impl Future<Output = Result<Option<Release>, LauncherError>> + Send + 'static,
) -> Discovery {
    Discovery(Some(tokio::spawn(async move {
        let result = async {
            let release = query.await?;
            let path = layout.cache_root().join("available-update");
            let value = release.as_ref().map_or("", Release::tag);
            let current = crate::persistence::read_exact_or_publish(&path, value.as_bytes())?;
            if current != value.as_bytes() {
                crate::persistence::replace_recovery_file(&path, value.as_bytes())?;
            }
            record_check(&layout)
        }
        .await;
        if let Err(error) = result {
            // Discovery is optional; retain a diagnostic without writing into the active TUI.
            let _ = fs::write(layout.logs_root().join("update-discovery.log"), error.to_string());
        }
    })))
}

fn cached_release(layout: &AppLayout) -> Option<Release> {
    fs::read_to_string(layout.cache_root().join("available-update"))
        .ok()
        .and_then(|tag| Release::from_tag(tag.trim()).ok())
}

pub async fn offer_on_startup(layout: &AppLayout) -> Result<bool, LauncherError> {
    if cfg!(feature = "system-package") || !automatic_checks_enabled(layout) {
        return Ok(false);
    }
    let Some(release) = cached_release(layout).filter(Release::is_newer) else {
        return Ok(false);
    };
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

pub fn configure_checks(layout: &AppLayout, enabled: bool) -> Result<(), LauncherError> {
    require_self_managed_install()?;
    let value = if enabled { b"enabled\n".as_slice() } else { b"disabled\n".as_slice() };
    persist_check_setting(layout, value)?;
    announce(if enabled {
        "Automatic startup update checks are enabled."
    } else {
        "Automatic startup update checks are disabled. Run `peritus update` to check manually."
    })
}

pub async fn run_explicit(layout: &AppLayout) -> Result<(), LauncherError> {
    tokio::select! {
        result = run_explicit_inner(layout) => result,
        signal = tokio::signal::ctrl_c() => {
            signal.map_err(|error| LauncherError::Update(format!("listen for update cancellation: {error}")))?;
            Err(LauncherError::Update("update cancelled; staging is retained for retry".into()))
        }
    }
}

async fn run_explicit_inner(layout: &AppLayout) -> Result<(), LauncherError> {
    require_self_managed_install()?;
    announce("Checking for Peritus updates...")?;
    let Some(release) = release::latest().await? else {
        return Err(LauncherError::Update(
            "the public release service has no current release".to_owned(),
        ));
    };
    record_check(layout)?;
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
    let package = tokio::select! {
        result = download::package(layout, release) => result?,
        signal = tokio::signal::ctrl_c() => {
            signal.map_err(|error| LauncherError::Update(format!("listen for update cancellation: {error}")))?;
            return Err(LauncherError::Update("update cancelled; staging is retained for retry".into()));
        }
    };
    install::apply(&package, release).await
}

fn announce_completion(release: &Release) -> Result<(), LauncherError> {
    if cfg!(windows) {
        announce(
            "The update will finish in the background after this command exits. Start Peritus again in a moment.",
        )?;
    } else {
        announce(&format!(
            "Peritus {} is installed. Run `peritus` to continue.",
            release.version()
        ))?;
    }
    Ok(())
}

fn announce(message: &str) -> Result<(), LauncherError> {
    Terminal::stdio().line(message)
}

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
