//! Native Linux display and authority fixtures shared by graphical preview tests.

use std::{
    path::Path,
    process::{Child, Command},
    time::Duration,
};

use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_journal::{
    ApplicationPrincipalKind, NewApplicationPrincipal, NewApplicationSession, SqliteJournal,
    SqliteJournalOptions, StoreId,
};
use peritus_types::SessionId;

use crate::{AuthorityHandle, AuthorityOwner, DaemonError, DaemonLifecycle, StartupPhase};

/// Owns the isolated display even when a qualification assertion unwinds.
pub(in super::super) struct NativeDisplay(Child);

impl NativeDisplay {
    pub(in super::super) fn kill(&mut self) -> std::io::Result<()> {
        self.0.kill()
    }

    pub(in super::super) fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.0.wait()
    }
}

impl Drop for NativeDisplay {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

pub(in super::super) fn start_xvfb() -> (NativeDisplay, String) {
    for number in 120..220 {
        let socket = std::path::PathBuf::from(format!("/tmp/.X11-unix/X{number}"));
        if socket.exists() {
            continue;
        }
        let mut child = Command::new("/usr/bin/Xvfb")
            .args([
                format!(":{number}"),
                "-screen".to_owned(),
                "0".to_owned(),
                "640x480x24".to_owned(),
                "-nolisten".to_owned(),
                "tcp".to_owned(),
                "-ac".to_owned(),
            ])
            .spawn()
            .expect("start isolated Xvfb");
        for _ in 0..100 {
            if socket.exists() {
                return (NativeDisplay(child), format!(":{number}"));
            }
            if child.try_wait().expect("poll Xvfb").is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    panic!("no isolated Xvfb display available")
}

pub(in super::super) fn find_window(display: &str, title: &str) -> u64 {
    for _ in 0..200 {
        let output = Command::new("/usr/bin/xdotool")
            .env("DISPLAY", display)
            .args(["search", "--name", title])
            .output()
            .expect("search selected window");
        if output.status.success()
            && let Some(line) = String::from_utf8_lossy(&output.stdout).lines().next()
            && let Ok(window) = line.parse()
        {
            return window;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("controlled preview window did not appear")
}

pub(in super::super) fn test_authority(
    state: &Path,
    session: SessionId,
) -> (AuthorityHandle, tokio::task::JoinHandle<Result<(), DaemonError>>) {
    let database = state.join("preview-authority.sqlite3");
    let store = StoreId::new([0xc0; 16]).expect("authority store");
    let mut journal =
        SqliteJournal::open(&database, store, SqliteJournalOptions::default()).expect("journal");
    journal
        .bind_application_principal(NewApplicationPrincipal::new(
            peritus_types::Sha256Digest::new([0xc1; 32]),
            ApplicationPrincipalKind::UnixPeer,
            super::super::actor(),
            peritus_types::Sha256Digest::new([0xc2; 32]),
        ))
        .expect("principal");
    journal
        .open_application_session(
            NewApplicationSession::new(session, super::super::actor(), 1, 100, [0xc3; 16], 1, 0)
                .expect("session facts"),
        )
        .expect("application session");
    let artifacts = ArtifactStore::open(
        StoreConfig::new(state.join("preview-artifacts"), 16 * 1_024 * 1_024, 32 * 1_024 * 1_024)
            .and_then(|config| config.with_database_path(&database))
            .expect("artifact config"),
    )
    .expect("artifact store");
    let mut lifecycle = DaemonLifecycle::starting();
    for phase in [
        StartupPhase::Lock,
        StartupPhase::Migrate,
        StartupPhase::Journal,
        StartupPhase::Artifacts,
        StartupPhase::Evidence,
        StartupPhase::Projections,
        StartupPhase::AuthorityEpoch,
        StartupPhase::DomainRecovery,
        StartupPhase::EffectRecovery,
        StartupPhase::AppRecovery,
        StartupPhase::Outbox,
        StartupPhase::Ipc,
        StartupPhase::Ready,
    ] {
        lifecycle.advance(phase).expect("startup phase");
    }
    AuthorityOwner::spawn(journal, lifecycle, artifacts, 16 * 1_024 * 1_024, 32, 1, 64)
        .expect("authority owner")
}
