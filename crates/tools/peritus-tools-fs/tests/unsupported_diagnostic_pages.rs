//! Large unsupported-entry sets remain bounded and continue across result pages.

#![cfg(unix)]

mod support;

#[cfg(unix)]
#[test]
fn discovery_and_search_stream_unsupported_diagnostics_across_pages() {
    use peritus_tools_fs::{DiscoverInput, FsReadService, OmissionReason, SearchInput};
    use std::{collections::BTreeSet, os::unix::fs::symlink};

    let fixture = support::read_fixture("fs-unsupported-pages");
    for index in 0..270 {
        symlink("README.md", fixture.root.join(format!("link-{index:03}")))
            .expect("unsupported symlink");
    }
    let service = FsReadService::new(&fixture.workspace);

    let mut discovery_names = BTreeSet::new();
    let mut discovery_digest = None;
    for offset in [0, 40, 80, 120, 160, 200, 240, 280] {
        let observation = service
            .discover(
                &DiscoverInput::new(None, 4, 40)
                    .expect("discover input")
                    .with_omission_offset(offset),
            )
            .expect("discover page");
        assert_eq!(observation.omission_count(), 270);
        assert!(observation.omissions().len() <= 40);
        if let Some(digest) = discovery_digest {
            assert_eq!(observation.digest(), digest);
        } else {
            discovery_digest = Some(observation.digest());
        }
        for omission in observation.omissions() {
            assert_eq!(omission.reason(), OmissionReason::UnsupportedType);
            assert!(discovery_names.insert(omission.native_path_bytes().to_vec()));
        }
    }
    assert_eq!(discovery_names.len(), 270);
    assert!(discovery_names.contains(b"link-000".as_slice()));
    assert!(discovery_names.contains(b"link-269".as_slice()));

    let mut search_names = BTreeSet::new();
    for offset in [0, 40, 80, 120, 160, 200, 240, 280] {
        let observation = service
            .search(
                &SearchInput::page(None, "absent-token".to_owned(), false, 4, 1024, 40)
                    .expect("search input")
                    .with_continuation_offsets(0, offset),
            )
            .expect("search page");
        // The repository's pre-existing binary fixture is a separate search omission.
        assert_eq!(observation.omission_count(), 271);
        assert!(observation.omissions().len() <= 40);
        for omission in observation.omissions() {
            if omission.reason() == OmissionReason::UnsupportedType {
                assert!(search_names.insert(omission.native_path_bytes().to_vec()));
            }
        }
    }
    assert_eq!(search_names.len(), 270);
    assert!(search_names.contains(b"link-000".as_slice()));
    assert!(search_names.contains(b"link-269".as_slice()));
}
