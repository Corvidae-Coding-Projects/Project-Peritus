use super::*;
use crate::{FinalFile, LineEndingPolicy, PatchSet};
mod extended;
mod pages;

fn patch(paths: impl Iterator<Item = String>) -> PatchSet {
    let empty = FinalFile::new(Vec::new(), FileMode::Regular, LineEndingPolicy::Preserve)
        .expect("empty file");
    PatchSet::from_snapshot(
        WorkspaceId::new([41; 16]).expect("workspace"),
        Generation::first(),
        RevisionNumber::first(),
        paths
            .map(|path| {
                PatchOperation::create(WorkspacePath::new(path).expect("path"), empty.clone())
            })
            .collect(),
    )
    .expect("snapshot metadata has no command payload ceiling")
}

#[test]
fn snapshot_manifest_above_the_old_collection_limit_decodes_exactly() {
    let patch = patch((0..65_536).map(|index| format!("saved-{index:05}")));
    let manifest = Manifest::from_patch(&patch, Vec::new());
    assert_eq!(Manifest::decode(&manifest.encode().expect("encode")).expect("decode"), manifest);
}

#[test]
fn snapshot_manifest_above_the_old_payload_limit_decodes_exactly() {
    let prefix = vec!["d".repeat(255); 15].join("/");
    let patch = patch((0..5_000).map(|index| format!("{prefix}/saved-{index:05}")));
    let manifest = Manifest::from_patch(&patch, Vec::new());
    let encoded = manifest.encode().expect("encode");
    assert!(encoded.len() > CodecLimits::PRODUCTION.max_payload_bytes);
    assert_eq!(Manifest::decode(&encoded).expect("decode"), manifest);
}

#[test]
fn snapshot_count_larger_than_the_available_metadata_rejects_before_allocation() {
    let patch = patch(std::iter::once("saved".to_owned()));
    let mut encoded = Manifest::from_patch(&patch, Vec::new()).encode().expect("encode");
    let offset = MAGIC.len() + 2 + 1 + 16 + 8 + 8 + 32;
    encoded[offset..offset + 8].copy_from_slice(&u64::MAX.to_be_bytes());
    let payload = encoded.len() - 32;
    let digest = peritus_codec::sha256(&encoded[..payload]);
    encoded[payload..].copy_from_slice(digest.as_bytes());
    assert!(Manifest::decode(&encoded).is_err());
}

#[test]
fn directory_manifest_is_versioned_and_cannot_be_reinterpreted_as_legacy_file_metadata() {
    let patch = PatchSet::from_snapshot(
        WorkspaceId::new([41; 16]).expect("workspace"),
        Generation::first(),
        RevisionNumber::first(),
        vec![PatchOperation::create_directory(
            WorkspacePath::new("empty").expect("path"),
            DirectoryMode::new(0o750).expect("mode"),
        )],
    )
    .expect("directory snapshot");
    let manifest = Manifest::from_patch(&patch, Vec::new());
    assert_eq!(manifest.schema, 3);
    let mut encoded = manifest.encode().expect("encode");
    assert_eq!(Manifest::decode(&encoded).expect("decode"), manifest);
    for legacy in [1_u16, 2] {
        encoded[MAGIC.len()..MAGIC.len() + 2].copy_from_slice(&legacy.to_be_bytes());
        let payload = encoded.len() - 32;
        let digest = peritus_codec::sha256(&encoded[..payload]);
        encoded[payload..].copy_from_slice(digest.as_bytes());
        assert!(Manifest::decode(&encoded).is_err());
    }
}

#[test]
fn existing_file_only_manifest_generations_remain_readable() {
    let final_file =
        FinalFile::new(b"saved".to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
            .expect("file");
    let operation = PatchOperation::create(WorkspacePath::new("saved").expect("path"), final_file);
    for (constructor, schema) in
        [(PatchSet::new as fn(_, _, _, _) -> _, 1), (PatchSet::from_snapshot, 2)]
    {
        let patch = constructor(
            WorkspaceId::new([41; 16]).expect("workspace"),
            Generation::first(),
            RevisionNumber::first(),
            vec![operation.clone()],
        )
        .expect("patch");
        let manifest = Manifest::from_patch(&patch, Vec::new());
        assert_eq!(manifest.schema, schema);
        assert_eq!(
            Manifest::decode(&manifest.encode().expect("encode")).expect("decode"),
            manifest
        );
    }
}
