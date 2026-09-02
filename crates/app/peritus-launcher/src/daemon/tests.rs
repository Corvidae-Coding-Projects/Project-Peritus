//! Upgrade and applied-daemon identity behavior.

use super::*;
use crate::{AppLayout, ProductBootstrap};

#[test]
fn applied_identity_changes_when_the_packaged_daemon_changes() {
    let temporary = tempfile::tempdir().expect("temporary application root");
    let daemon = temporary.path().join("peritusd");
    let configuration = temporary.path().join("peritus.toml");
    fs::write(&daemon, b"first daemon build").expect("first daemon fixture");
    let first = applied_identity(&configuration, &daemon).expect("first identity");

    fs::write(&daemon, b"upgraded daemon build").expect("upgraded daemon fixture");
    let upgraded = applied_identity(&configuration, &daemon).expect("upgraded identity");

    assert_ne!(first, upgraded);
    assert!(first.starts_with("peritus-applied-daemon-v2\nconfiguration="));
    assert_ne!(first, format!("{}\n", configuration.display()));
}

#[test]
fn recorded_daemon_is_reused_only_until_its_installed_binary_changes() {
    let temporary = tempfile::tempdir().expect("temporary application root");
    let layout = AppLayout::for_test(temporary.path()).prepare().expect("prepared layout");
    let product = ProductBootstrap::new(layout).prepare().expect("prepared product");
    let application = temporary.path().join("peritus");
    let daemon = temporary.path().join("peritusd");
    fs::write(&application, b"application fixture").expect("application fixture");
    fs::write(&daemon, b"first daemon build").expect("daemon fixture");
    let binaries = SiblingBinaries { application, daemon: daemon.clone() };

    record_applied_configuration(&product, &binaries).expect("record applied identity");
    assert!(applied_configuration_matches(&product, &binaries).expect("matching installed daemon"));

    fs::write(&daemon, b"upgraded daemon build").expect("upgrade daemon fixture");
    assert!(!applied_configuration_matches(&product, &binaries).expect("stale daemon identity"));
}

#[test]
fn replacement_waits_until_the_old_daemon_releases_its_instance_lock() {
    let temporary = tempfile::tempdir().expect("temporary application root");
    let layout = AppLayout::for_test(temporary.path()).prepare().expect("prepared layout");
    let product = ProductBootstrap::new(layout).prepare().expect("prepared product");
    let lock_path = product.daemon_config().paths().state_root().join("daemon.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .expect("daemon lock fixture");
    fs4::FileExt::try_lock(&lock).expect("hold old daemon lock");

    assert!(!instance_lock_available(&product).expect("locked state"));
    fs4::FileExt::unlock(&lock).expect("release old daemon lock");
    assert!(instance_lock_available(&product).expect("released state"));
}
