//! Real immutable-worktree and inert-patch filesystem tool tests.

mod support;

use peritus_patch::{FileMode, LineEndingPolicy, Preimage, WorkspacePath};
use peritus_tools_fs::{
    CompiledMutation, CreateInput, DiscoverInput, FileContent, FsReadService, MetadataInput,
    PatchEdit, PatchInput, ReadInput, RenderedOutput, SearchInput, WorkspaceVersion,
    descriptor_catalog, descriptor_digest,
};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};

#[test]
fn immutable_discover_read_metadata_and_search_are_real_and_bounded() {
    let fixture = support::read_fixture("fs-read");
    let service = FsReadService::new(&fixture.workspace);
    let discovered = service
        .discover(&DiscoverInput::new(None, 8, 100).expect("discover input"))
        .expect("discover");
    let paths = discovered
        .entries()
        .iter()
        .map(|entry| entry.metadata().path().as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["README.md", "blob.bin", "src", "src/lib.rs"]);
    assert!(
        !RenderedOutput::discover_page(&discovered, 0, 100, 64 * 1024).expect("render").truncated()
    );

    let metadata = service
        .metadata(&MetadataInput::new("README.md").expect("metadata input"))
        .expect("metadata");
    assert_eq!(metadata.size(), 11);
    let read = service.read(&ReadInput::new("README.md", 1024).expect("read input")).expect("read");
    assert_eq!(read.content(), &FileContent::Utf8("Alpha\nbeta\n".to_owned()));
    let binary = service
        .read(&ReadInput::new("blob.bin", 1024).expect("binary input"))
        .expect("binary read");
    assert!(matches!(binary.content(), FileContent::Base64(_)));

    let search = service
        .search(
            &SearchInput::new(None, "alpha".to_owned(), false, 8, 4096, 10).expect("search input"),
        )
        .expect("search");
    assert_eq!(search.matches().len(), 2);
    assert_eq!(search.matches()[0].path().as_str(), "README.md");
    assert_eq!(search.matches()[1].path().as_str(), "src/lib.rs");
}

#[test]
fn path_fragment_rendering_rejects_a_budget_that_cannot_fit_progress() {
    let fixture = support::read_fixture("fs-small-output");
    let discovered = FsReadService::new(&fixture.workspace)
        .discover(&DiscoverInput::new(None, 8, 100).expect("discover input"))
        .expect("discover");

    let rendered = RenderedOutput::discover_page_with_path_range(&discovered, 0, 1, Some(0), 1);
    assert!(rendered.is_err());
}

#[test]
fn search_subpage_finishes_after_the_last_retained_match() {
    let fixture = support::read_fixture("fs-search-subpage");
    let observation = FsReadService::new(&fixture.workspace)
        .search(&SearchInput::new(None, "alpha".to_owned(), false, 8, 4096, 10).expect("input"))
        .expect("search");
    assert_eq!(observation.matches().len(), 2);
    let page =
        RenderedOutput::search_page(&observation, 1, 10, 1, 64 * 1024).expect("last match page");
    assert!(!page.truncated(), "the final match must not advertise another page");
}

#[test]
fn search_fragments_preserve_the_requested_match_cursor() {
    let fixture = support::read_fixture("fs-search-fragment-cursor");
    std::fs::write(
        fixture.workspace.root().join("src/lib.rs"),
        format!("alpha{}\n", "x".repeat(2000)),
    )
    .expect("long match preview");
    let observation = FsReadService::new(&fixture.workspace)
        .search(&SearchInput::new(None, "alpha".to_owned(), false, 8, 4096, 10).expect("input"))
        .expect("search");
    let page =
        RenderedOutput::search_page_with_field_ranges(&observation, 1, 1, 1, None, None, 600)
            .expect("second match fragment");
    let json = std::str::from_utf8(page.structured().canonical_bytes()).expect("JSON");
    assert!(json.contains("\"text\":\"src/lib.rs\""), "{json}");
    let omission =
        RenderedOutput::search_page_with_field_ranges(&observation, 2, 1, 0, None, Some(0), 600)
            .expect("omission after all matches");
    let json = std::str::from_utf8(omission.structured().canonical_bytes()).expect("JSON");
    assert!(json.contains("\"next_match_offset\":null"), "{json}");
}

#[cfg(unix)]
#[test]
fn immutable_inspection_omits_symlinks_without_following_external_targets() {
    use peritus_tools_fs::OmissionReason;
    use std::os::unix::fs::symlink;

    let fixture = support::read_fixture("fs-symlink");
    let target = fixture.root.parent().expect("fixture parent").join("outside.txt");
    std::fs::write(&target, b"private outside bytes").expect("outside target");
    let target_before = std::fs::read(&target).expect("capture target");
    symlink(&target, fixture.root.join("linked.txt")).expect("test symlink");
    let service = FsReadService::new(&fixture.workspace);
    assert!(service.metadata(&MetadataInput::new("linked.txt").expect("input")).is_err());
    let discovered = service
        .discover(&DiscoverInput::new(None, 4, 100).expect("input"))
        .expect("discovery records unsafe entries as omissions");
    assert!(discovered.omissions().iter().any(|omission| {
        omission.path().is_none()
            && omission.native_path_bytes() == b"linked.txt"
            && omission.reason() == OmissionReason::UnsupportedType
    }));
    assert!(
        service.read(&ReadInput::new("linked.txt", 1024).expect("read input")).is_err(),
        "inspection must not expose bytes through the symlink",
    );
    assert_eq!(
        std::fs::read(&target).expect("outside target remains available to its owner"),
        target_before,
        "no-follow inspection must leave the external target unchanged",
    );
}

#[test]
fn every_mutation_form_compiles_to_one_canonical_patch_set() {
    let version = WorkspaceVersion::new(
        WorkspaceId::new([41; 16]).expect("workspace"),
        Generation::first(),
        RevisionNumber::first(),
    );
    let create = CreateInput::new(
        "new.txt",
        b"new\n".to_vec(),
        FileMode::Regular,
        LineEndingPolicy::Preserve,
    )
    .expect("create");
    let compiled = CompiledMutation::create(version, create.clone()).expect("compiled create");
    assert_eq!(compiled.patch_set().operations().len(), 1);
    assert_eq!(compiled.patch_set().operations()[0].path().as_str(), "new.txt");

    let existing = Preimage::from_bytes(b"old\n", FileMode::Regular);
    let replacement = peritus_tools_fs::ReplaceInput::new(
        "old.txt",
        existing,
        b"replacement\n".to_vec(),
        FileMode::Regular,
        LineEndingPolicy::Lf,
    )
    .expect("replacement");
    let patch = PatchInput::new(vec![
        PatchEdit::Create(create),
        PatchEdit::Replace(replacement),
        PatchEdit::Remove(
            peritus_tools_fs::RemoveInput::new("gone.txt", existing).expect("remove"),
        ),
    ])
    .expect("patch input");
    let compiled = CompiledMutation::patch(version, patch).expect("compiled patch");
    assert_eq!(compiled.patch_set().operations().len(), 3);
    assert_eq!(
        compiled.patch_set().operations()[0].path(),
        &WorkspacePath::new("gone.txt").expect("path")
    );
}

#[test]
fn descriptor_catalog_is_complete_canonical_and_deterministic() {
    let first = descriptor_catalog().expect("catalog");
    let second = descriptor_catalog().expect("catalog");
    assert_eq!(first.len(), 9);
    assert_eq!(
        first.iter().map(|value| value.name().as_str()).collect::<Vec<_>>(),
        [
            "fs.create",
            "fs.discover",
            "fs.metadata",
            "fs.patch",
            "fs.read",
            "fs.remove",
            "fs.replace",
            "fs.search",
            "fs.write",
        ]
    );
    assert_eq!(
        first
            .iter()
            .map(peritus_tool_protocol::ToolDescriptor::canonical_bytes)
            .collect::<Vec<_>>(),
        second
            .iter()
            .map(peritus_tool_protocol::ToolDescriptor::canonical_bytes)
            .collect::<Vec<_>>()
    );
    assert_eq!(descriptor_digest().expect("digest"), descriptor_digest().expect("digest"));
}

#[cfg(unix)]
#[test]
fn discovery_and_search_render_exact_native_paths_and_causes_for_unsupported_children() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use std::os::unix::fs::symlink;
    #[cfg(target_os = "linux")]
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt as _};

    let fixture = support::read_fixture("fs-symlink-diagnostics");
    let root = fixture.root.clone();
    std::fs::write(root.join("searchable.txt"), b"needle\n").expect("ordinary sibling");
    #[cfg(target_os = "linux")]
    let invalid_name = {
        let name = b"invalid-\xff-name";
        std::fs::write(root.join(OsStr::from_bytes(name)), b"unsupported name\n")
            .expect("unsupported child name");
        name
    };
    symlink("searchable.txt", root.join("linked.txt")).expect("unsupported symlink child");

    let service = FsReadService::new(&fixture.workspace);
    let discover_input =
        DiscoverInput::new(None, 4, 100).expect("discover input").with_omission_offset(0);
    let discovered = service.discover(&discover_input).expect("partial discovery");
    let discover_json = RenderedOutput::discover_page(&discovered, 0, 100, 64 * 1024)
        .expect("discover diagnostics render");
    let discover_json =
        std::str::from_utf8(discover_json.structured().canonical_bytes()).expect("JSON");
    #[cfg(target_os = "linux")]
    assert!(discover_json.contains(&STANDARD.encode(invalid_name)), "{discover_json}");
    assert!(discover_json.contains(&STANDARD.encode(b"linked.txt")), "{discover_json}");
    assert!(discover_json.contains("unsupported_name"), "{discover_json}");
    assert!(discover_json.contains("unsupported_type"), "{discover_json}");
    let discovery_page = RenderedOutput::discover_page(&discovered, 0, 1, 64 * 1024)
        .expect("first discovery omission page");
    let first_page =
        std::str::from_utf8(discovery_page.structured().canonical_bytes()).expect("JSON");
    assert!(first_page.contains("\"next_omission_offset\":1"), "{first_page}");
    let next_discover = service
        .discover(
            &DiscoverInput::new(None, 4, 100).expect("next discovery").with_omission_offset(1),
        )
        .expect("next diagnostic page");
    assert_eq!(next_discover.omissions().len(), 1);

    let search_input =
        SearchInput::new(None, "needle".to_owned(), true, 4, 1024, 100).expect("search");
    let search = service.search(&search_input).expect("search with diagnostic omissions");
    let search_json = RenderedOutput::search_page(&search, 0, 100, 0, 64 * 1024)
        .expect("search diagnostics render");
    let search_json =
        std::str::from_utf8(search_json.structured().canonical_bytes()).expect("JSON");
    #[cfg(target_os = "linux")]
    assert!(search_json.contains(&STANDARD.encode(invalid_name)), "{search_json}");
    assert!(search_json.contains(&STANDARD.encode(b"linked.txt")), "{search_json}");
    assert!(search_json.contains("unsupported_name"), "{search_json}");
    assert!(search_json.contains("unsupported_type"), "{search_json}");

    let first_search_page = RenderedOutput::search_page(&search, 0, 1, 0, 64 * 1024)
        .expect("first search omission page");
    let first_search_page =
        std::str::from_utf8(first_search_page.structured().canonical_bytes()).expect("JSON");
    assert!(first_search_page.contains("\"next_omission_offset\":1"), "{first_search_page}");
    let next_search = service
        .search(
            &SearchInput::page(None, "needle".to_owned(), true, 4, 1024, 1)
                .expect("next search page")
                .with_continuation_offsets(0, 1),
        )
        .expect("next search diagnostic page");
    assert_eq!(next_search.omissions().len(), 1);
}

#[cfg(unix)]
#[test]
fn wide_directory_discovery_counts_each_unsupported_child_once() {
    #[cfg(target_os = "linux")]
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::symlink;
    let fixture = support::read_fixture("fs-wide-diagnostics");
    for index in 0..260 {
        std::fs::write(fixture.root.join(format!("file-{index:03}")), b"x")
            .expect("supported child");
    }
    symlink("README.md", fixture.root.join("zzzz-link")).expect("unsupported symlink");
    #[cfg(target_os = "linux")]
    std::fs::write(fixture.root.join(std::ffi::OsStr::from_bytes(b"invalid-\xff")), b"x")
        .expect("non-UTF-8 child");
    let service = FsReadService::new(&fixture.workspace);
    let input = DiscoverInput::new(None, 4, 1000).expect("input");
    let result = service.discover(&input).expect("discovery");
    assert_eq!(result.observed_count(), 264);
    #[cfg(target_os = "linux")]
    assert_eq!(result.omission_count(), 2);
    #[cfg(not(target_os = "linux"))]
    assert_eq!(result.omission_count(), 1);
    assert_eq!(
        result.omissions().len(),
        usize::try_from(result.omission_count()).expect("fixture omission count fits usize")
    );
    let names = result
        .omissions()
        .iter()
        .map(peritus_tools_fs::ScopeOmission::native_path_bytes)
        .collect::<std::collections::BTreeSet<_>>();
    #[cfg(target_os = "linux")]
    assert_eq!(
        names,
        std::collections::BTreeSet::from([b"invalid-\xff".as_slice(), b"zzzz-link".as_slice(),])
    );
    #[cfg(not(target_os = "linux"))]
    assert_eq!(names, std::collections::BTreeSet::from([b"zzzz-link".as_slice()]));
    assert_eq!(result.digest(), service.discover(&input).expect("repeat discovery").digest());
}
