use super::*;
use std::io::{Read as _, Seek as _, SeekFrom};

#[test]
fn retained_complete_source_above_64_mib_has_no_cumulative_page_budget() {
    use sha2::{Digest as _, Sha256};
    let (directory, reader) = fixture();
    let size = 64 * 1024 * 1024 + 1;
    fs::File::create(directory.path().join("large")).unwrap().set_len(size).unwrap();
    let mut retained = reader
        .capture_file(&path("large"), FileReadSelection::all(), tempfile::tempfile().unwrap())
        .unwrap();
    assert_eq!(retained.observation().selected_bytes(), size);
    let mut cursor = retained.cursor();
    let mut digest = Sha256::new();
    let mut count = 0;
    let mut pages = 0;
    while let Some(current) = cursor {
        let page = retained.read_page(current, u64::MAX).unwrap().unwrap();
        assert_eq!(page.range().0, count);
        count = page.range().1;
        digest.update(page.bytes());
        cursor = page.next();
        pages += 1;
    }
    assert_eq!(count, size);
    assert_eq!(pages, 1025);
    assert_eq!(Sha256Digest::new(digest.finalize().into()), retained.observation().digest());
    assert_eq!(retained.observation().digest(), retained.observation().source_digest());
}

#[test]
fn retained_pages_cross_former_inclusion_ceiling_and_resume_after_owner_reopen() {
    let (directory, reader) = fixture();
    let storage_directory = tempfile::tempdir().unwrap();
    let storage_path = storage_directory.path().join("retained");
    let source = vec![21; 8 * 1024 * 1024 + 17];
    fs::write(directory.path().join("large"), &source).unwrap();
    let storage =
        fs::OpenOptions::new().create_new(true).read(true).write(true).open(&storage_path).unwrap();
    let mut retained =
        reader.capture_file(&path("large"), FileReadSelection::all(), storage).unwrap();
    let metadata = retained.observation().encode();
    let initial = retained.cursor().unwrap();
    assert!(retained.read_page(initial, 0).is_err());
    let first = retained.read_page(initial, u64::MAX).unwrap().unwrap();
    assert_eq!(first.bytes().len(), 64 * 1024);
    assert_eq!(first.range(), (0, 64 * 1024));
    let mut included = first.bytes().to_vec();
    let cursor_record = first.next().unwrap().encode();
    drop(retained);
    drop(reader);
    fs::write(directory.path().join("large"), b"new ambient version").unwrap();
    let observation = InspectedSelection::decode(&metadata).unwrap();
    assert_eq!(observation.source_bytes(), source.len() as u64);
    assert_eq!(observation.digest(), peritus_codec::sha256(&source));
    assert_eq!(observation.source_digest(), observation.digest());
    let mut retained =
        RetainedInspection::open(fs::File::open(&storage_path).unwrap(), observation).unwrap();
    let mut cursor = Some(InspectionCursor::decode(&cursor_record).unwrap());
    let mut offset = 64 * 1024;
    while let Some(current) = cursor {
        let page = retained.read_page(current, u64::MAX).unwrap().unwrap();
        assert_eq!(page.observation(), retained.observation());
        assert_eq!(page.range(), (offset, offset + page.bytes().len() as u64));
        included.extend_from_slice(page.bytes());
        offset = page.range().1;
        cursor = page.next();
    }
    assert_eq!(included, source);
    let replay = retained.read_page(initial, 3).unwrap().unwrap();
    assert_eq!(replay.bytes(), &[21; 3]);
    assert_eq!(replay.range(), (0, 3));
}

#[test]
fn exact_line_pages_keep_source_identity_and_absolute_intervals() {
    let (directory, reader) = fixture();
    let source = b"excluded\r\nselected\r\nlast\n";
    fs::write(directory.path().join("reference"), source).unwrap();
    let mut retained = reader
        .capture_file(
            &path("reference"),
            FileReadSelection::lines(2, 2).unwrap(),
            tempfile::tempfile().unwrap(),
        )
        .unwrap();
    assert_eq!(retained.observation().folder_digest(), reader.identity().digest());
    assert_eq!(retained.observation().path(), &path("reference"));
    assert_eq!(retained.observation().range(), (10, 20));
    assert_eq!(retained.observation().source_digest(), peritus_codec::sha256(source));
    let mut cursor = retained.cursor();
    let mut bytes = Vec::new();
    let mut position = 10;
    while let Some(current) = cursor {
        let page = retained.read_page(current, 3).unwrap().unwrap();
        assert!(!format!("{page:?}").contains(&format!("{:?}", page.bytes())));
        assert_eq!(page.range().0, position);
        position = page.range().1;
        bytes.extend_from_slice(page.bytes());
        cursor = page.next();
    }
    assert_eq!(position, 20);
    assert_eq!(bytes, b"selected\r\n");
}

#[test]
fn malformed_metadata_cursors_foreign_observations_and_changed_storage_reject() {
    let (directory, reader) = fixture();
    fs::write(directory.path().join("one"), b"one").unwrap();
    fs::write(directory.path().join("two"), b"two").unwrap();
    let storage = tempfile::tempfile().unwrap();
    let mut external = storage.try_clone().unwrap();
    let mut one = reader.capture_file(&path("one"), FileReadSelection::all(), storage).unwrap();
    let two = reader
        .capture_file(&path("two"), FileReadSelection::all(), tempfile::tempfile().unwrap())
        .unwrap();
    assert!(one.read_page(two.cursor().unwrap(), 2).is_err());
    let initial = one.cursor().unwrap();
    for valid in [one.observation().encode(), initial.encode()] {
        for length in 0..valid.len() {
            assert!(InspectedSelection::decode(&valid[..length]).is_err());
            assert!(InspectionCursor::decode(&valid[..length]).is_err());
        }
        let mut corrupted = valid.clone();
        corrupted[10] ^= 1;
        assert!(InspectedSelection::decode(&corrupted).is_err());
        assert!(InspectionCursor::decode(&corrupted).is_err());
        let mut trailing = valid;
        trailing.push(0);
        assert!(InspectedSelection::decode(&trailing).is_err());
        assert!(InspectionCursor::decode(&trailing).is_err());
    }
    let metadata = one.observation().clone();
    external.seek(SeekFrom::Start(0)).unwrap();
    io::Write::write_all(&mut external, b"bad").unwrap();
    external.set_times(fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH)).unwrap();
    assert!(one.read_page(initial, 3).is_err());
    drop(one);
    assert!(RetainedInspection::open(external, metadata).is_err());
}

#[test]
fn capture_rejects_alias_before_truncation_and_empty_content_survives_reopen() {
    let (directory, reader) = fixture();
    let source_path = directory.path().join("reference");
    fs::write(&source_path, b"original").unwrap();
    let alias = fs::OpenOptions::new().read(true).write(true).open(&source_path).unwrap();
    assert!(reader.capture_file(&path("reference"), FileReadSelection::all(), alias).is_err());
    assert_eq!(fs::read(&source_path).unwrap(), b"original");
    fs::write(&source_path, []).unwrap();
    let storage = tempfile::tempfile().unwrap();
    let reopened = storage.try_clone().unwrap();
    let retained =
        reader.capture_file(&path("reference"), FileReadSelection::all(), storage).unwrap();
    assert!(retained.cursor().is_none());
    assert_eq!(retained.observation().range(), (0, 0));
    let metadata = InspectedSelection::decode(&retained.observation().encode()).unwrap();
    drop(retained);
    let retained = RetainedInspection::open(reopened, metadata).unwrap();
    assert!(retained.cursor().is_none());
}

#[test]
fn streaming_source_change_or_storage_failure_returns_no_accepted_observation() {
    struct ChangingWriter(std::path::PathBuf);
    impl io::Write for ChangingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            fs::write(&self.0, b"changed")?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let (directory, reader) = fixture();
    let source_path = directory.path().join("reference");
    fs::write(&source_path, b"original").unwrap();
    assert!(
        reader
            .copy_selection(
                &path("reference"),
                FileReadSelection::all(),
                &mut ChangingWriter(source_path.clone())
            )
            .is_err()
    );
    fs::write(&source_path, b"original").unwrap();
    let read_only = fs::File::open(&source_path).unwrap();
    assert!(reader.capture_file(&path("reference"), FileReadSelection::all(), read_only).is_err());
    let mut wrong_size = tempfile::tempfile().unwrap();
    io::Write::write_all(&mut wrong_size, b"x").unwrap();
    let metadata = reader
        .copy_selection(&path("reference"), FileReadSelection::all(), &mut io::sink())
        .unwrap();
    assert!(RetainedInspection::open(wrong_size, metadata).is_err());
    assert_eq!(fs::read(&source_path).unwrap(), b"original");
    let mut bytes = Vec::new();
    let mut file = fs::File::open(source_path).unwrap();
    file.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"original");
}

#[test]
fn checksummed_but_impossible_metadata_and_out_of_bounds_cursor_still_reject() {
    let (directory, reader) = fixture();
    fs::write(directory.path().join("source"), b"abc").unwrap();
    let mut retained = reader
        .capture_file(&path("source"), FileReadSelection::all(), tempfile::tempfile().unwrap())
        .unwrap();
    let mut bytes = retained.observation().encode();
    bytes[80..88].copy_from_slice(&4_u64.to_be_bytes());
    let end = bytes.len() - 32;
    let digest = peritus_codec::sha256(&bytes[..end]);
    bytes[end..].copy_from_slice(digest.as_bytes());
    assert!(InspectedSelection::decode(&bytes).is_err());
    let mut cursor = retained.cursor().unwrap().encode();
    cursor[40..48].copy_from_slice(&4_u64.to_be_bytes());
    let digest = peritus_codec::sha256(&cursor[..48]);
    cursor[48..].copy_from_slice(digest.as_bytes());
    assert!(retained.read_page(InspectionCursor::decode(&cursor).unwrap(), 3).is_err());
    cursor[40..48].copy_from_slice(&3_u64.to_be_bytes());
    let digest = peritus_codec::sha256(&cursor[..48]);
    cursor[48..].copy_from_slice(digest.as_bytes());
    assert!(retained.read_page(InspectionCursor::decode(&cursor).unwrap(), 3).unwrap().is_none());
}
