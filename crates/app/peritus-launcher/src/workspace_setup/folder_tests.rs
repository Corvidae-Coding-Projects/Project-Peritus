use super::*;
use crate::AppLayout;
use std::fs;

#[test]
fn ordinary_directory_persists_reopens_and_trusts_without_git_or_a_managed_copy() {
    let temporary = tempfile::tempdir().expect("test root");
    let root = temporary.path().join("chosen-folder");
    fs::create_dir(&root).expect("folder");
    fs::write(root.join("note.txt"), "unchanged").expect("original file");
    let layout = AppLayout::for_test(temporary.path()).prepare().expect("layout");
    let discovered = DiscoveredRepository::open(&root).expect("non-Git discovery");
    assert!(discovered.repository().is_none());
    let restricted = new_profile(&discovered).expect("profile");
    assert!(restricted.is_direct_folder());
    assert_eq!(health(&restricted), WorkspaceHealth::Restricted);
    let first = ProductBootstrap::new(layout.clone())
        .configure_workspace(restricted.clone())
        .expect("restricted configuration");
    assert_eq!(first.daemon_config().folders().len(), 1);
    assert!(!first.daemon_config().folders()[0].writable());
    assert!(first.daemon_config().workspaces().is_empty());
    let original_config = fs::read(first.daemon_config_path()).expect("original config");
    let reopened = ProductBootstrap::new(layout.clone()).prepare().expect("reopen");
    let selected =
        ensure_configured(reopened, Some(&root)).expect("launch remembered folder without prompt");
    assert_eq!(selected.state().generation(), first.state().generation());
    let trusted = trust(&layout, &discovered, restricted).expect("explicit trust");
    assert!(trusted.managed_root().is_none());
    let configured = ProductBootstrap::new(layout.clone())
        .configure_workspace(trusted.clone())
        .expect("trusted configuration");
    assert_eq!(health(&trusted), WorkspaceHealth::Ready);
    assert!(configured.daemon_config().folders()[0].writable());
    assert_eq!(
        configured.daemon_config().folders()[0].root(),
        root.canonicalize().expect("canonical root")
    );
    assert_eq!(
        fs::read(first.daemon_config_path()).expect("retained immutable generation"),
        original_config
    );
    assert!(fs::read_dir(layout.managed_workspaces_root()).expect("managed root").next().is_none());
    assert!(!root.join(".git").exists());
    assert_eq!(fs::read_to_string(root.join("note.txt")).expect("original file"), "unchanged");
}

#[test]
fn non_directory_paths_are_not_workspaces() {
    let temporary = tempfile::tempdir().expect("test root");
    let path = temporary.path().join("file.txt");
    fs::write(&path, "file").expect("file");
    assert!(DiscoveredRepository::open(&path).is_err());
    assert!(DiscoveredRepository::open(&temporary.path().join("absent")).is_err());
}

#[test]
fn configured_folder_starts_and_restarts_the_real_daemon_without_git_registration() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime").block_on(
        async {
            // macOS's default temporary root can exceed sockaddr_un's path bound after the
            // private layout and endpoint name are appended. Match the real-daemon fixtures.
            let base = if cfg!(unix) { std::path::PathBuf::from("/tmp") } else { env::temp_dir() };
            let temporary = tempfile::Builder::new()
                .prefix("pl-")
                .tempdir_in(base)
                .expect("short home-shaped folder");
            let layout = AppLayout::for_test(temporary.path()).prepare().expect("private layout");
            let discovered =
                DiscoveredRepository::open(temporary.path()).expect("folder discovery");
            let profile = trust(&layout, &discovered, new_profile(&discovered).expect("profile"))
                .expect("trust");
            let configured =
                ProductBootstrap::new(layout).configure_workspace(profile).expect("configuration");
            assert!(configured.daemon_config().workspaces().is_empty());
            for _ in 0..2 {
                let daemon =
                    peritus_daemon::DaemonRuntime::start(configured.daemon_config().clone())
                        .await
                        .expect("real folder startup");
                daemon.shutdown().await.expect("shutdown");
            }
            assert!(!temporary.path().join(".git").exists());
        },
    );
}
