//! Exercises real archive preparation and transport with explicitly non-executable fixtures.

use super::*;
use crate::product_package::{self, qualification};
use crate::release::TemporaryDirectory;

#[test]
fn archive_preparation_and_transport_retain_release_bytes_without_debug_product_inputs() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
    let fixture = TemporaryDirectory::new("peritus-staged-release-test").expect("fixture");
    let root = fixture.path().join("producer");
    let package = product_package::package_path(&root);
    let name = package.file_name().expect("package name");
    let source = fixture.path().join("source").join(name);
    fs::create_dir_all(&source).expect("source tree");
    fs::write(source.join("manifest.toml"), b"explicit archive fixture").expect("manifest");
    let inputs = root.join("target/release-input");
    fs::create_dir_all(&inputs).expect("download directory");
    fs::create_dir_all(root.join("packaging")).expect("adapter directory");
    fs::copy(workspace.join("packaging/qualification.py"), root.join("packaging/qualification.py"))
        .expect("actual adapter");
    let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
    let archive = inputs.join(name).with_extension(extension);
    run(
        Command::new(if cfg!(windows) { "python" } else { "python3" })
            .arg(workspace.join("packaging/archive.py"))
            .arg(&source)
            .arg(&archive)
            .arg("1788740000"),
        "create explicit release archive fixture",
    )
    .expect("archive");
    fs::write(
        archive.with_extension(format!("{}sha256", if cfg!(windows) { "zip." } else { "gz." })),
        format!("{}\n", digest(&archive).expect("archive digest")),
    )
    .expect("checksum");
    for name in ["peritus-h2", "peritus-h2-controller"] {
        let path = product_package::debug_binary(&root, name);
        fs::create_dir_all(path.parent().expect("tool directory")).expect("tool directory");
        fs::write(path, b"non-executable tool presence fixture").expect("tool presence");
    }
    prepare(&root).expect("prepare without Cargo or debug product binaries");
    assert!(!product_package::debug_binary(&root, "peritus").exists());
    let consumer = fixture.path().join("consumer");
    fs::create_dir_all(consumer.join("target/prepared-h2")).expect("consumer download");
    fs::copy(
        root.join("target/h2-prepared.tar"),
        consumer.join("target/prepared-h2/h2-prepared.tar"),
    )
    .expect("transport same-run bundle");
    qualification::restore(&consumer).expect("restore executable permissions and exact archive");
    let restored = consumer.join("dist").join(archive.file_name().expect("archive name"));
    assert_eq!(
        fs::read(restored).expect("restored bytes"),
        fs::read(&archive).expect("source bytes")
    );
    assert_eq!(
        fs::read(product_package::package_path(&consumer).join("manifest.toml")).expect("manifest"),
        b"explicit archive fixture"
    );
}

#[test]
fn staged_bootstrap_copies_original_bytes_and_rejects_a_changed_checksum() {
    let fixture = TemporaryDirectory::new("peritus-staged-bootstrap-test").expect("fixture");
    let root = fixture.path();
    fs::create_dir(root.join("dist")).expect("dist");
    let source = root.join("dist/fixture.tar.gz");
    let original = b"original archive fixture, not reassembled";
    fs::write(&source, original).expect("archive");
    fs::write(
        root.join("dist/fixture.tar.gz.sha256"),
        format!("{}\n", digest(&source).expect("digest")),
    )
    .expect("checksum");
    let destination = root.join("fixture.tar.gz");
    let checksum = root.join("fixture.tar.gz.sha256");
    copy_archive(root, &destination, &checksum).expect("copy exact bytes");
    assert_eq!(fs::read(&destination).expect("copy"), original);
    fs::write(root.join("dist/fixture.tar.gz.sha256"), "0".repeat(64)).expect("tamper");
    assert!(copy_archive(root, &destination, &checksum).is_err());
    assert_eq!(fs::read(destination).expect("unchanged copy"), original);
}
