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
    assert!(!RenderedOutput::discover(&discovered).expect("render").truncated());

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
}

#[cfg(unix)]
#[test]
fn immutable_inspection_refuses_symlink_traversal() {
    use std::os::unix::fs::symlink;

    let fixture = support::read_fixture("fs-symlink");
    symlink("README.md", fixture.root.join("linked.txt")).expect("test symlink");
    let service = FsReadService::new(&fixture.workspace);
    assert!(service.metadata(&MetadataInput::new("linked.txt").expect("input")).is_err());
    assert!(service.discover(&DiscoverInput::new(None, 4, 100).expect("input")).is_err());
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
