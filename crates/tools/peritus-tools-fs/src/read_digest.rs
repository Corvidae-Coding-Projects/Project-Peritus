//! Canonical filesystem observation digests.

use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use peritus_workspace::WorkspaceEntryKind;

use crate::{DiscoverEntry, MetadataObservation, SearchObservation};

pub fn discover_digest(root: Option<&WorkspacePath>, entries: &[DiscoverEntry]) -> Sha256Digest {
    let mut bytes = b"PERITUS-FS-DISCOVER-V1\0".to_vec();
    put_bytes(&mut bytes, root.map_or("", WorkspacePath::as_str));
    bytes.extend_from_slice(&(entries.len() as u64).to_be_bytes());
    for entry in entries {
        put_metadata(&mut bytes, entry.metadata());
        bytes.extend_from_slice(&entry.depth().to_be_bytes());
    }
    peritus_codec::sha256(&bytes)
}

pub fn search_digest(observation: &SearchObservation) -> Sha256Digest {
    let mut bytes = b"PERITUS-FS-SEARCH-V1\0".to_vec();
    bytes.extend_from_slice(&observation.scanned_files().to_be_bytes());
    bytes.extend_from_slice(&observation.scanned_bytes().to_be_bytes());
    bytes.extend_from_slice(&(observation.matches().len() as u64).to_be_bytes());
    for value in observation.matches() {
        put_bytes(&mut bytes, value.path().as_str());
        bytes.extend_from_slice(&value.line().to_be_bytes());
        bytes.extend_from_slice(&value.column_bytes().to_be_bytes());
        put_bytes(&mut bytes, value.preview());
    }
    peritus_codec::sha256(&bytes)
}

fn put_metadata(bytes: &mut Vec<u8>, value: &MetadataObservation) {
    put_bytes(bytes, value.path().as_str());
    bytes.push(match value.kind() {
        WorkspaceEntryKind::File => 1,
        WorkspaceEntryKind::Directory => 2,
    });
    bytes.extend_from_slice(&value.size().to_be_bytes());
    bytes.push(u8::from(value.executable()));
}

fn put_bytes(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
}
