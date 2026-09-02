//! Cached public release discovery and native self-update composition.

mod download;
mod install;
mod release;

use std::{fs, time::Duration};

use crate::{AppLayout, LauncherError, terminal::Terminal};
use release::Release;

const CHECK_INTERVAL: Duration = Duration::from_hours(6);

pub async fn offer_on_startup(layout: &AppLayout) -> Result<bool, LauncherError> {
    if !automatic_checks_enabled(layout) || check_is_fresh(layout) {
        return Ok(false);
    }
    let Ok(release) = release::latest().await else {
        return Ok(false);
    };
    let _ = record_check(layout);
    let Some(release) = release.filter(Release::is_newer) else {
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
    let value = if enabled { b"enabled\n".as_slice() } else { b"disabled\n".as_slice() };
    persist_check_setting(layout, value)?;
    announce(if enabled {
        "Automatic startup update checks are enabled."
    } else {
        "Automatic startup update checks are disabled. Run `peritus update` to check manually."
    })
}

pub async fn run_explicit(layout: &AppLayout) -> Result<(), LauncherError> {
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

async fn apply(layout: &AppLayout, release: &Release) -> Result<(), LauncherError> {
    let package = download::package(layout, release).await?;
    install::apply(&package, release)
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

#[cfg(test)]
mod tests {
    use super::*;

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
