//! Real archive regressions, independent of build or checkout timestamps.

use std::{fs::FileTimes, time::Duration};

use super::*;

#[test]
fn native_archive_format_and_boundary_regressions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
    let status = Command::new(if cfg!(windows) { "python" } else { "python3" })
        .current_dir(root)
        .args(["-m", "unittest", "discover", "-s", "packaging", "-p", "test_archive.py"])
        .status()
        .expect("run native archive format tests");
    assert!(status.success(), "native tar/ZIP format and failure-boundary regressions");
}

#[test]
fn native_archive_bytes_do_not_depend_on_source_timestamps() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
    let temporary = TemporaryDirectory::new("peritus-archive-reproducibility").expect("fixture");
    let package = temporary.path().join("peritus-fixture");
    fs::create_dir(&package).expect("package");
    let payload = package.join("manifest.toml");
    fs::write(&payload, "schema_version = 1\n").expect("payload");
    let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
    let first = temporary.path().join(format!("first.{extension}"));
    let second = temporary.path().join(format!("second.{extension}"));
    archive_package(root, &package, &first).expect("first archive");
    File::options()
        .write(true)
        .open(&payload)
        .expect("open payload")
        .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(1_234_567_890)))
        .expect("different payload timestamp");
    archive_package(root, &package, &second).expect("second archive");
    assert_eq!(fs::read(first).expect("first bytes"), fs::read(second).expect("second bytes"));
}
