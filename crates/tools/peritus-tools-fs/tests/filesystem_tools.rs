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
    let rendered = RenderedOutput::discover(&discovered).expect("render");
    assert!(!rendered.truncated());
    // Captured from the accepted pre-issue-108 source tree, not this implementation.
    assert_eq!(rendered.structured().canonical_bytes(), br#"{"digest":"1942108274dd36288b93b571ff03bb2bd30253b10e3b3383ec4f1bf7b8594e0d","entries":[{"depth":1,"metadata":{"executable":false,"kind":"file","path":"README.md","size":11}},{"depth":1,"metadata":{"executable":false,"kind":"file","path":"blob.bin","size":4}},{"depth":1,"metadata":{"executable":false,"kind":"directory","path":"src","size":0}},{"depth":2,"metadata":{"executable":false,"kind":"file","path":"src/lib.rs","size":27}}],"observed_count":4,"root":null,"truncated":false}"#);

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
            &SearchInput::new(None, "alpha".to_owned(), false, 8, 100, 4096, 16_384, 10)
                .expect("search input"),
        )
        .expect("search");
    assert_eq!(search.matches().len(), 2);
    assert_eq!(search.matches()[0].path().as_str(), "README.md");
    assert_eq!(search.matches()[1].path().as_str(), "src/lib.rs");
    assert_eq!(RenderedOutput::search(&search).unwrap().structured().canonical_bytes(), br#"{"digest":"6c83bd53106f09ce35fcee63edee4430dd645638e47ad25e113df725ab09f5b7","match_count":2,"matches":[{"column_bytes":0,"line":1,"path":"README.md","preview":"Alpha"},{"column_bytes":7,"line":1,"path":"src/lib.rs","preview":"pub fn alpha() -> u8 { 1 }"}],"scanned_bytes":38,"scanned_files":2,"truncated":false}"#);
}

#[cfg(unix)]
#[test]
fn immutable_inspection_refuses_symlink_traversal() {
    use std::os::unix::fs::symlink;

    let fixture = support::read_fixture("fs-symlink");
    symlink("README.md", fixture.root.join("linked.txt")).expect("test symlink");
    let service = FsReadService::new(&fixture.workspace);
    assert!(service.metadata(&MetadataInput::new("linked.txt").expect("input")).is_err());
    let observed = service.discover(&DiscoverInput::new(None, 4, 100).expect("input")).unwrap();
    assert_eq!(observed.exclusions().len(), 1);
    assert_eq!(
        observed.exclusions()[0].reason(),
        peritus_workspace::DirectoryExclusionReason::SymbolicLink
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

#[cfg(unix)]
#[test]
fn unsupported_native_children_do_not_block_usable_discovery_and_search() {
    use std::{ffi::OsStr, os::unix::net::UnixListener};
    let fixture = support::read_fixture("fs-node");
    let _socket = UnixListener::bind(fixture.root.join("node")).unwrap();
    assert_native_exclusions(
        &fixture,
        OsStr::new("node"),
        OsStr::new("other-node"),
        peritus_workspace::DirectoryExclusionReason::SpecialNode,
    );
}

// Darwin's native namespace rejects non-UTF-8 creation before inspection is reached.
// Special-node/link coverage above still runs there; raw-byte identity stays exercised here.
#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn raw_native_names_remain_exact_in_discovery_search_and_exclusion_digests() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt as _};
    let fixture = support::read_fixture("fs-raw");
    let name = OsString::from_vec(b"native-\xff".to_vec());
    std::fs::write(fixture.root.join(&name), b"opaque").unwrap();
    assert_native_exclusions(
        &fixture,
        &name,
        &OsString::from_vec(b"other-\xff".to_vec()),
        peritus_workspace::DirectoryExclusionReason::UnrepresentableName,
    );
}

#[cfg(unix)]
fn assert_native_exclusions(
    fixture: &support::ReadFixture,
    name: &std::ffi::OsStr,
    renamed_name: &std::ffi::OsStr,
    reason: peritus_workspace::DirectoryExclusionReason,
) {
    use std::os::unix::{ffi::OsStrExt as _, fs::symlink};
    symlink("README.md", fixture.root.join("linked.txt")).unwrap();
    let service = FsReadService::new(&fixture.workspace);
    let discovered = service.discover(&DiscoverInput::new(None, 8, 100).unwrap()).unwrap();
    assert_eq!(discovered.exclusions().len(), 2);
    assert!(
        discovered.exclusions().iter().any(
            |value| value.name().encoded_bytes() == name.as_bytes() && value.reason() == reason
        )
    );
    let rendered = RenderedOutput::discover(&discovered).unwrap();
    assert_eq!(rendered.structured().property("excluded_count").unwrap().as_i64(), Some(2));
    assert_eq!(rendered.structured().property("exclusions").unwrap().elements().unwrap().len(), 2);
    assert!(!rendered.truncated());
    assert!(
        discovered.entries().iter().any(|entry| entry.metadata().path().as_str() == "README.md")
    );
    let search = service
        .search(
            &SearchInput::new(None, "alpha".to_owned(), false, 8, 100, 4096, 16_384, 10).unwrap(),
        )
        .unwrap();
    assert_eq!(search.matches().len(), 2);
    assert!(service.metadata(&MetadataInput::new("linked.txt").unwrap()).is_err());
    assert!(service.read(&ReadInput::new("linked.txt", 1024).unwrap()).is_err());
    assert_eq!(search.exclusions(), discovered.exclusions());
    assert_eq!(
        RenderedOutput::search(&search)
            .unwrap()
            .structured()
            .property("excluded_count")
            .unwrap()
            .as_i64(),
        Some(2)
    );
    std::fs::rename(fixture.root.join(name), fixture.root.join(renamed_name)).unwrap();
    let renamed = service.discover(&DiscoverInput::new(None, 8, 100).unwrap()).unwrap();
    assert_eq!(renamed.entries(), discovered.entries());
    assert_eq!(renamed.exclusions().len(), discovered.exclusions().len());
    assert_ne!(renamed.digest(), discovered.digest());
}

#[test]
fn descriptor_path_schema_and_typed_input_accept_extended_authority_paths() {
    use peritus_tool_protocol::{BoundedJson, JsonLimits};
    let path = vec!["directory-component"; 257].join("/");
    assert!(path.len() > 4_096);
    let arguments = BoundedJson::object(
        vec![(
            "path".to_owned(),
            BoundedJson::string(path.clone(), JsonLimits::PRODUCTION).unwrap(),
        )],
        JsonLimits::PRODUCTION,
    )
    .unwrap();
    let catalog = descriptor_catalog().unwrap();
    let metadata = catalog.iter().find(|value| value.name().as_str() == "fs.metadata").unwrap();
    metadata.schema().validate(&arguments).unwrap();
    MetadataInput::new(path).unwrap();
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

#[test]
fn typed_mutations_do_not_reintroduce_patch_body_or_operation_ceilings() {
    let version = WorkspaceVersion::new(
        WorkspaceId::new([41; 16]).unwrap(),
        Generation::first(),
        RevisionNumber::first(),
    );
    let large = CreateInput::new(
        "large",
        vec![7; 8 * 1024 * 1024 + 1],
        FileMode::Regular,
        LineEndingPolicy::Preserve,
    )
    .expect("large typed body");
    let compiled = CompiledMutation::create(version, large).expect("metadata authority");
    assert!(
        matches!(compiled.patch_set().operations()[0].postimage(), Preimage::Present { size, .. } if size == 8 * 1024 * 1024 + 1)
    );
    let edits = (0..1_025)
        .map(|index| {
            PatchEdit::Create(
                CreateInput::new(
                    format!("file-{index:04}"),
                    Vec::new(),
                    FileMode::Regular,
                    LineEndingPolicy::Preserve,
                )
                .unwrap(),
            )
        })
        .collect();
    let compiled =
        CompiledMutation::patch(version, PatchInput::new(edits).expect("many typed edits"))
            .expect("one patch");
    assert_eq!(compiled.patch_set().operations().len(), 1_025);
}
