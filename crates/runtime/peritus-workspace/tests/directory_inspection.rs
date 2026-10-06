//! Native siblings, durable continuation and source-independent accepted directory history.

use peritus_workspace::{
    DirectoryCursor, FolderIdentity, FolderInspection, ObservedDirectory, RetainedDirectory,
};
use std::io::{Seek as _, SeekFrom, Write as _};
use std::{collections::BTreeSet, fs::OpenOptions};

fn open(root: &std::path::Path) -> FolderInspection {
    FolderInspection::open(&FolderIdentity::observe(root).unwrap()).unwrap()
}

#[test]
fn retained_listing_resumes_exact_names_after_source_and_reader_are_gone() {
    let source = tempfile::tempdir().unwrap();
    let owner = tempfile::tempdir().unwrap();
    let names = (0..1_025).map(|index| format!("child-{index:04}")).collect::<BTreeSet<_>>();
    for name in &names {
        std::fs::write(source.path().join(name), []).unwrap();
    }
    let backing = owner.path().join("accepted-listing");
    let file = OpenOptions::new().read(true).write(true).create_new(true).open(&backing).unwrap();
    let inspection = open(source.path());
    let mut reader = inspection.capture_directory(None, file).unwrap();
    let metadata = reader.observation().to_record();
    let initial = reader.cursor().unwrap();
    assert!(reader.read_page(initial, 0).is_err());
    assert_eq!(reader.cursor(), Some(initial));
    let first = reader.read_page(initial, 1).unwrap();
    let mut seen = first
        .items()
        .iter()
        .map(|item| item.name().native_name().unwrap().into_string().unwrap())
        .collect::<BTreeSet<_>>();
    let saved = DirectoryCursor::from_record(&first.next().unwrap().to_record()).unwrap();
    assert!(reader.read_page(initial, 1).is_err());
    drop(reader);
    drop(inspection);
    std::fs::remove_dir_all(source.path()).unwrap();
    let observation = ObservedDirectory::from_record(&metadata).unwrap();
    let mut reader = RetainedDirectory::open(
        std::fs::File::open(&backing).unwrap(),
        observation.clone(),
        Some(saved),
    )
    .unwrap();
    let mut pages = 1;
    while let Some(cursor) = reader.cursor() {
        let page = reader.read_page(cursor, 1).unwrap();
        assert_eq!(page.observation(), &observation);
        assert_eq!(page.start(), cursor.index());
        for item in page.items() {
            assert!(seen.insert(item.name().native_name().unwrap().into_string().unwrap()));
        }
        pages += 1;
    }
    assert_eq!(pages, 1_025);
    assert_eq!(seen, names);
    let mut replay =
        RetainedDirectory::open(std::fs::File::open(backing).unwrap(), observation, Some(initial))
            .unwrap();
    assert_eq!(replay.read_page(initial, 1).unwrap(), first);
}

#[cfg(unix)]
#[test]
fn unsupported_children_have_exact_native_identity_without_blocking_usable_names() {
    use peritus_patch::WorkspacePath;
    use peritus_workspace::DirectoryExclusionReason;
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt as _, fs::symlink, net::UnixListener},
    };
    let source = tempfile::tempdir().unwrap();
    for name in ["good", "a:b", "a\\b", "name.", "NUL", "line\nbreak"] {
        std::fs::write(source.path().join(name), b"accepted").unwrap();
    }
    std::fs::write(source.path().join(OsString::from_vec(b"bad-\xff".to_vec())), b"raw").unwrap();
    std::fs::write(
        source.path().join(OsString::from_vec(b".peritus-txn-\xff".to_vec())),
        b"hidden",
    )
    .unwrap();
    std::fs::create_dir(source.path().join(".git")).unwrap();
    symlink("good", source.path().join("linked")).unwrap();
    let _socket = UnixListener::bind(source.path().join("socket")).unwrap();
    let inspection = open(source.path());
    let listing = inspection.inspect_directory(None).unwrap();
    assert_eq!(listing.items().len(), 9);
    assert_eq!(listing.items().iter().filter(|item| item.metadata().is_some()).count(), 6);
    let raw =
        listing.items().iter().find(|item| item.name().encoded_bytes() == b"bad-\xff").unwrap();
    assert_eq!(raw.exclusion(), Some(DirectoryExclusionReason::UnrepresentableName));
    assert_eq!(raw.name().native_name().unwrap(), OsString::from_vec(b"bad-\xff".to_vec()));
    assert_eq!(raw.name().display_name(), "unix:6261642dff");
    assert!(
        listing
            .items()
            .iter()
            .any(|item| item.exclusion() == Some(DirectoryExclusionReason::SymbolicLink))
    );
    assert!(
        listing
            .items()
            .iter()
            .any(|item| item.exclusion() == Some(DirectoryExclusionReason::SpecialNode))
    );
    assert!(inspection.metadata(&WorkspacePath::new("linked").unwrap()).is_err());
    for allow_missing in [false, true] {
        assert!(
            inspection.check_path(&WorkspacePath::new("linked").unwrap(), allow_missing).is_err()
        );
        assert!(
            inspection
                .check_path(&WorkspacePath::new("linked/child").unwrap(), allow_missing)
                .is_err()
        );
    }
    let missing = WorkspacePath::new("missing/nested/file").unwrap();
    assert!(inspection.check_path(&missing, false).is_err());
    inspection.check_path(&missing, true).unwrap();
    assert!(inspection.inspect_directory(Some(&WorkspacePath::new("linked").unwrap())).is_err());
    let mut retained = inspection.capture_directory(None, tempfile::tempfile().unwrap()).unwrap();
    let page = retained.read_page(retained.cursor().unwrap(), u64::MAX).unwrap();
    assert!(page.next().is_none());
    let direct = listing
        .items()
        .iter()
        .map(|item| (item.name().encoded_bytes().to_vec(), item.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let retained = page
        .items()
        .iter()
        .map(|item| (item.name().encoded_bytes().to_vec(), item.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(direct, retained);
    let escaped =
        listing.items().iter().find(|item| item.name().encoded_bytes() == b"line\nbreak").unwrap();
    assert_eq!(escaped.name().display_name(), "line\\nbreak");
}

#[test]
fn storage_aliases_and_predecessors_reject_before_writes_and_empty_listing_reopens() {
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("owned");
    let file = OpenOptions::new().read(true).write(true).create_new(true).open(&path).unwrap();
    assert!(open(source.path()).capture_directory(None, file).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"");
    let owner = tempfile::tempdir().unwrap();
    let predecessor = owner.path().join("predecessor");
    std::fs::write(&predecessor, b"accepted predecessor").unwrap();
    let file = OpenOptions::new().read(true).write(true).open(&predecessor).unwrap();
    assert!(open(source.path()).capture_directory(None, file).is_err());
    assert_eq!(std::fs::read(&predecessor).unwrap(), b"accepted predecessor");
    std::fs::remove_file(path).unwrap();
    let mut file = tempfile::tempfile().unwrap();
    let clone = file.try_clone().unwrap();
    let retained = open(source.path()).capture_directory(None, clone).unwrap();
    assert!(retained.cursor().is_none());
    let observation = ObservedDirectory::from_record(&retained.observation().to_record()).unwrap();
    assert!(
        RetainedDirectory::open(file.try_clone().unwrap(), observation, None)
            .unwrap()
            .cursor()
            .is_none()
    );
    file.seek(SeekFrom::Start(0)).unwrap();
    file.write_all(b"corrupt!").unwrap();
    assert!(RetainedDirectory::open(file, retained.observation().clone(), None).is_err());
}

#[test]
fn cursor_boundaries_foreign_observations_and_record_corruption_reject() {
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("first"), b"x").unwrap();
    std::fs::write(source.path().join("second"), b"y").unwrap();
    let backing = tempfile::tempfile().unwrap();
    let reader = open(source.path()).capture_directory(None, backing.try_clone().unwrap()).unwrap();
    let observation = reader.observation().clone();
    let cursor = reader.cursor().unwrap();
    let records = [observation.to_record(), cursor.to_record()];
    for record in records {
        for length in 0..record.len() {
            if record.starts_with(b"PDOB") {
                assert!(ObservedDirectory::from_record(&record[..length]).is_err());
            } else {
                assert!(DirectoryCursor::from_record(&record[..length]).is_err());
            }
        }
    }
    let mut forged = cursor.to_record();
    forged[55] += 1;
    reseal(&mut forged);
    let forged = DirectoryCursor::from_record(&forged).unwrap();
    assert!(
        RetainedDirectory::open(backing.try_clone().unwrap(), observation.clone(), Some(forged))
            .is_err()
    );
    let other = tempfile::tempdir().unwrap();
    std::fs::write(other.path().join("first"), b"x").unwrap();
    let foreign = open(other.path())
        .capture_directory(None, tempfile::tempfile().unwrap())
        .unwrap()
        .cursor()
        .unwrap();
    assert!(
        RetainedDirectory::open(backing.try_clone().unwrap(), observation.clone(), Some(foreign))
            .is_err()
    );
    let mut forged = observation.to_record();
    forged[8] = 255;
    reseal(&mut forged);
    assert!(ObservedDirectory::from_record(&forged).is_err());
    let mut file = backing;
    file.seek(SeekFrom::End(-1)).unwrap();
    file.write_all(&[99]).unwrap();
    assert!(RetainedDirectory::open(file, observation, None).is_err());
}

fn reseal(record: &mut [u8]) {
    let length = record.len() - 32;
    let checksum = peritus_codec::sha256(&record[..length]);
    record[length..].copy_from_slice(checksum.as_bytes());
}

#[test]
fn handle_relative_listing_and_metadata_cross_former_path_and_depth_limits() {
    let source = tempfile::tempdir().unwrap();
    let mut directory =
        cap_std::fs::Dir::open_ambient_dir(source.path(), cap_std::ambient_authority()).unwrap();
    let component = "directory-component";
    for _ in 0..257 {
        directory.create_dir(component).unwrap();
        directory = directory.open_dir(component).unwrap();
    }
    directory.write("usable", b"yes").unwrap();
    let path = peritus_patch::WorkspacePath::new(vec![component; 257].join("/")).unwrap();
    assert!(path.as_str().len() > 4_096);
    let inspection = open(source.path());
    let listing = inspection.inspect_directory(Some(&path)).unwrap();
    assert_eq!(listing.items().len(), 1);
    let metadata = listing.items()[0].metadata().unwrap();
    assert_eq!(metadata.size(), 3);
    assert_eq!(inspection.metadata(metadata.path()).unwrap(), *metadata);
    inspection.check_path(metadata.path(), false).unwrap();
    let file = inspection
        .read_file(metadata.path(), peritus_workspace::FileReadSelection::all(), 3)
        .unwrap();
    assert_eq!(file.bytes(), b"yes");
    let retained = inspection
        .capture_file(
            metadata.path(),
            peritus_workspace::FileReadSelection::all(),
            tempfile::tempfile().unwrap(),
        )
        .unwrap();
    let encoded = retained.observation().encode();
    assert!(encoded.starts_with(b"PINSv002"));
    assert_eq!(
        peritus_workspace::InspectedSelection::decode(&encoded).unwrap(),
        *retained.observation()
    );
    let mut foreign = encoded.clone();
    foreign[8] = 255;
    reseal(&mut foreign);
    assert!(peritus_workspace::InspectedSelection::decode(&foreign).is_err());
    let mut legacy = encoded;
    legacy[..8].copy_from_slice(b"PINSv001");
    legacy.remove(8);
    reseal(&mut legacy);
    assert!(peritus_workspace::InspectedSelection::decode(&legacy).is_err());
    let mut reader =
        inspection.capture_directory(Some(&path), tempfile::tempfile().unwrap()).unwrap();
    assert_eq!(reader.read_page(reader.cursor().unwrap(), 1).unwrap().items(), listing.items());
}

#[test]
fn retained_storage_changes_reject_without_advancing_the_cursor() {
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("first"), b"x").unwrap();
    let mut backing = tempfile::tempfile().unwrap();
    let mut reader =
        open(source.path()).capture_directory(None, backing.try_clone().unwrap()).unwrap();
    let cursor = reader.cursor().unwrap();
    backing.seek(SeekFrom::End(-1)).unwrap();
    backing.write_all(&[99]).unwrap();
    backing.sync_all().unwrap();
    assert!(reader.read_page(cursor, 1).is_err());
    assert_eq!(reader.cursor(), Some(cursor));

    let retained = open(source.path())
        .capture_file(
            &peritus_patch::WorkspacePath::new("first").unwrap(),
            peritus_workspace::FileReadSelection::all(),
            tempfile::tempfile().unwrap(),
        )
        .unwrap();
    let mut forged = retained.observation().encode();
    assert!(forged.starts_with(b"PINSv001"));
    forged[..8].copy_from_slice(b"PINSv002");
    #[cfg(unix)]
    let platform = 1;
    #[cfg(windows)]
    let platform = 2;
    #[cfg(not(any(unix, windows)))]
    let platform = 3;
    forged.insert(8, platform);
    reseal(&mut forged);
    assert!(peritus_workspace::InspectedSelection::decode(&forged).is_err());
}
