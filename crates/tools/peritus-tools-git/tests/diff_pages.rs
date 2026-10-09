//! Complete immutable Git content remains reachable through encoded-byte pages.

mod support;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use peritus_git::DiffCursor;
use peritus_test_support::FixturePath;
use peritus_tools_git::{DiffInput, GitReadService, RenderedOutput};
use peritus_types::Sha256Digest;

#[allow(
    clippy::too_many_lines,
    reason = "Keep real diff pagination and exact path/patch reconstruction in one regression."
)]
fn assert_diff_paths_and_patch_reconstruct(label: &str, include_native_name: bool) {
    #[cfg(not(target_os = "linux"))]
    let _ = include_native_name;
    #[cfg(unix)]
    let long_relative_path = format!("{}/long.txt", vec!["p".repeat(200); 2].join("/"));
    let fixture = support::git_fixture_with(label, |source| {
        for index in 0..530 {
            source
                .write_text(
                    &FixturePath::new(format!("many/{index:04}.txt")).expect("path"),
                    "payload\n",
                )
                .expect("write");
        }
        #[cfg(unix)]
        {
            #[cfg(target_os = "linux")]
            use std::os::unix::ffi::OsStrExt;
            // Keep the complete path below macOS PATH_MAX while still forcing a byte-page split.
            let long_path = source.root().join(&long_relative_path);
            std::fs::create_dir_all(long_path.parent().expect("parent")).expect("long directory");
            std::fs::write(long_path, b"long path\n").expect("long path file");
            #[cfg(target_os = "linux")]
            if include_native_name {
                std::fs::write(
                    source.root().join(std::ffi::OsStr::from_bytes(b"native-\xff")),
                    b"native\n",
                )
                .expect("native path");
            }
        }
    });
    let complete =
        fixture.workspace.git_diff(&fixture.first_commit, u32::MAX, u64::MAX).expect("complete");
    let expected_paths =
        complete.entries().iter().map(|entry| entry.path_bytes().to_vec()).collect::<Vec<_>>();
    assert!(expected_paths.len() > 500);
    #[cfg(unix)]
    let long_path_index = expected_paths
        .iter()
        .position(|path| path == long_relative_path.as_bytes())
        .expect("the long relative path is present in the diff");
    let service = GitReadService::new(&fixture.workspace);
    let mut cursor = DiffCursor::default();
    let mut paths: Vec<Vec<u8>> = Vec::new();
    let mut patch = Vec::new();
    #[cfg(unix)]
    let mut continued_path = false;
    #[cfg(not(unix))]
    let continued_path = false;
    for _ in 0..2000 {
        let page = service
            .diff(
                &DiffInput::new(complete.base().to_string(), u32::MAX, u64::MAX)
                    .expect("input")
                    .with_cursor(cursor, complete.digest()),
            )
            .expect("page");
        let rendered = RenderedOutput::diff_with_budget(&page, 1024).expect("encoded page");
        assert!(rendered.structured().canonical_bytes().len() <= 1024);
        let json: serde_json::Value =
            serde_json::from_slice(rendered.structured().canonical_bytes()).expect("JSON");
        for entry in json["entries"].as_array().expect("paths") {
            let index =
                usize::try_from(entry["entry_index"].as_u64().expect("index")).expect("index");
            if index == paths.len() {
                paths.push(Vec::new());
            }
            assert_eq!(
                paths[index].len() as u64,
                entry["path_byte_offset"].as_u64().expect("offset")
            );
            paths[index].extend(
                STANDARD
                    .decode(entry["path_bytes_base64"].as_str().expect("path bytes"))
                    .expect("base64"),
            );
        }
        assert_eq!(patch.len() as u64, json["patch_offset"].as_u64().expect("patch offset"));
        patch.extend(
            STANDARD.decode(json["patch_base64"].as_str().expect("patch bytes")).expect("base64"),
        );
        let next_entry = json["next_entry_offset"].as_u64();
        let next_patch = json["next_patch_offset"].as_u64();
        if next_entry.is_none() && next_patch.is_none() {
            break;
        }
        #[cfg(unix)]
        {
            continued_path |= json["next_entry_offset"].as_u64()
                == Some(u64::try_from(long_path_index).expect("path index"))
                && json["next_path_byte_offset"].as_u64().expect("path offset") != 0;
        }
        let next = DiffCursor {
            entry_offset: next_entry.unwrap_or_else(|| complete.total_entries()),
            patch_offset: next_patch.unwrap_or_else(|| complete.total_patch_bytes()),
            path_byte_offset: json["next_path_byte_offset"].as_u64().expect("path offset"),
        };
        assert_ne!(next, cursor, "every partial page makes progress");
        cursor = next;
    }
    #[cfg(unix)]
    assert!(continued_path, "long path bytes are individually continuable");
    #[cfg(not(unix))]
    let _ = continued_path;
    assert_eq!(paths, expected_paths);
    assert_eq!(patch, complete.patch());
    assert!(
        service
            .diff(
                &DiffInput::new(complete.base().to_string(), 1, 1)
                    .expect("input")
                    .with_cursor(cursor, Sha256Digest::new([1; 32]))
            )
            .is_err()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn diff_paths_and_patch_reconstruct_through_small_encoded_pages_without_native_name() {
    assert_diff_paths_and_patch_reconstruct("diff-pages-base", false);
}

#[cfg(target_os = "linux")]
#[test]
fn diff_paths_and_patch_reconstruct_through_small_encoded_pages_with_native_name() {
    assert_diff_paths_and_patch_reconstruct("diff-pages-native-name", true);
}

#[cfg(not(target_os = "linux"))]
#[test]
fn diff_paths_and_patch_reconstruct_through_small_encoded_pages() {
    assert_diff_paths_and_patch_reconstruct("diff-pages", false);
}

#[test]
fn runtime_patch_larger_than_eight_mib_is_available_without_whole_diff_rejection() {
    let fixture = support::git_fixture_with("large-diff", |source| {
        source
            .write_text(&FixturePath::new("large.txt").expect("path"), &"x".repeat(9 * 1024 * 1024))
            .expect("large file");
    });
    let page =
        fixture.workspace.git_diff(&fixture.first_commit, 1, 5 * 1024 * 1024).expect("first page");
    assert!(page.total_patch_bytes() > 8 * 1024 * 1024);
    assert_eq!(page.entries().len(), 1);
    let continuation = fixture
        .workspace
        .git_diff_page(
            &page.base().to_string(),
            u32::MAX,
            u64::MAX,
            DiffCursor {
                entry_offset: 1,
                patch_offset: page.patch().len() as u64,
                path_byte_offset: 0,
            },
            Some(page.digest()),
        )
        .expect("rest");
    assert_eq!(page.digest(), continuation.digest());
    assert_eq!(
        page.patch().len() as u64 + continuation.patch().len() as u64,
        page.total_patch_bytes()
    );
    assert_eq!(continuation.next_patch_offset(), None);
    assert_eq!(continuation.next_entry_offset(), None);
}
