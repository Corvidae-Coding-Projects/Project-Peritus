use super::*;
use std::os::unix::fs::symlink;

#[test]
fn fifo_socket_and_directory_reject_as_non_regular_files_without_waiting_for_a_writer() {
    let (directory, reader) = fixture();
    nix::unistd::mkfifo(
        &directory.path().join("pipe"),
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )
    .expect("FIFO");
    let _socket =
        std::os::unix::net::UnixListener::bind(directory.path().join("socket")).expect("socket");
    fs::create_dir(directory.path().join("subdir")).expect("directory");
    for name in ["pipe", "socket", "subdir"] {
        assert!(reader.read_file(&path(name), FileReadSelection::all(), 100).is_err());
    }
}

#[test]
fn an_open_capability_rejects_a_replaced_root_before_publishing_source_bytes() {
    let directory = tempfile::tempdir().expect("parent");
    let selected = directory.path().join("selected");
    fs::create_dir(&selected).expect("selected");
    fs::write(selected.join("source"), b"original").expect("source");
    let identity = FolderIdentity::observe(&selected).expect("identity");
    let reader = FolderInspection::open(&identity).expect("reader");
    fs::rename(&selected, directory.path().join("old")).expect("rename");
    fs::create_dir(&selected).expect("new root");
    fs::write(selected.join("source"), b"replacement").expect("replacement");
    assert!(reader.read_file(&path("source"), FileReadSelection::all(), 100).is_err());
}

#[test]
fn concurrent_intermediate_symlink_swaps_never_publish_outside_bytes() {
    let (directory, reader) = fixture();
    let outside = tempfile::tempdir().expect("outside");
    fs::write(outside.path().join("source"), b"outside-only").expect("outside bytes");
    let selected = directory.path().join("selected");
    let parked = directory.path().join("parked");
    fs::create_dir(&selected).expect("selected");
    fs::write(selected.join("source"), b"inside-only").expect("inside bytes");
    let requested = path("selected/source");
    assert_eq!(
        reader.read_file(&requested, FileReadSelection::all(), 100).expect("control").bytes(),
        b"inside-only"
    );
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            for _ in 0..256 {
                fs::rename(&selected, &parked).expect("park");
                symlink(outside.path(), &selected).expect("replace with symlink");
                std::thread::yield_now();
                fs::remove_file(&selected).expect("remove fixture symlink");
                fs::rename(&parked, &selected).expect("restore directory");
            }
        });
        for _ in 0..512 {
            if let Ok(result) = reader.read_file(&requested, FileReadSelection::all(), 100) {
                assert_eq!(result.bytes(), b"inside-only");
            }
        }
        worker.join().expect("observed worker");
    });
    assert_eq!(
        reader.read_file(&requested, FileReadSelection::all(), 100).expect("restored").bytes(),
        b"inside-only"
    );
}
