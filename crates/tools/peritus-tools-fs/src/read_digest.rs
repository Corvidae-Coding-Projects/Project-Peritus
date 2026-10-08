//! Canonical filesystem observation digests.

use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use peritus_workspace::WorkspaceEntryKind;

use crate::{DiscoverExclusion, MetadataObservation, SearchObservation};
use crate::{exclusion::TraversalOmission, read::WalkRecord};

pub(crate) fn discover_digest(
    root: Option<&WorkspacePath>,
    records: &[WalkRecord],
) -> Sha256Digest {
    let mut bytes = b"PERITUS-FS-DISCOVER-V3\0".to_vec();
    put_bytes(&mut bytes, root.map_or("", WorkspacePath::as_str));
    bytes.extend_from_slice(&(records.len() as u64).to_be_bytes());
    for record in records {
        match record {
            WalkRecord::Entry(metadata, depth) => {
                bytes.push(1);
                put_metadata(&mut bytes, metadata);
                bytes.extend_from_slice(&depth.to_be_bytes());
            }
            WalkRecord::Exclusion(value) => {
                bytes.push(2);
                put_exclusion(&mut bytes, value);
            }
            WalkRecord::TraversalOmission(value) => {
                bytes.push(3);
                put_traversal_omission(&mut bytes, value);
            }
        }
    }
    peritus_codec::sha256(&bytes)
}

pub(crate) fn search_digest(observation: &SearchObservation) -> Sha256Digest {
    let mut bytes = b"PERITUS-FS-SEARCH-V3\0".to_vec();
    bytes.extend_from_slice(&observation.scanned_files().to_be_bytes());
    bytes.extend_from_slice(&observation.scanned_bytes().to_be_bytes());
    bytes.extend_from_slice(&observation.match_count().to_be_bytes());
    for value in observation.matches() {
        put_bytes(&mut bytes, value.path().as_str());
        bytes.extend_from_slice(&value.line().to_be_bytes());
        bytes.extend_from_slice(&value.column_bytes().to_be_bytes());
        put_bytes(&mut bytes, value.preview());
    }
    bytes.extend_from_slice(&observation.exclusion_count().to_be_bytes());
    for value in observation.exclusions() {
        put_exclusion(&mut bytes, value);
    }
    bytes.extend_from_slice(&observation.traversal_omission_count().to_be_bytes());
    for value in observation.traversal_omissions() {
        put_traversal_omission(&mut bytes, value);
    }
    bytes.extend_from_slice(&observation.omission_count().to_be_bytes());
    for value in observation.omissions() {
        put_bytes(&mut bytes, value.path().as_str());
        put_bytes(&mut bytes, value.reason().as_str());
    }
    peritus_codec::sha256(&bytes)
}

fn put_exclusion(bytes: &mut Vec<u8>, value: &DiscoverExclusion) {
    put_bytes(bytes, value.directory().map_or("", WorkspacePath::as_str));
    put_bytes(bytes, value.reason().as_str());
    bytes.extend_from_slice(&value.depth().to_be_bytes());
    bytes.push(match value.name().encoding() {
        peritus_workspace::NativeNameEncoding::UnixBytes => 1,
        peritus_workspace::NativeNameEncoding::WindowsWide => 2,
        peritus_workspace::NativeNameEncoding::Utf8 => 3,
    });
    let native = value.name().encoded_bytes();
    bytes.extend_from_slice(&(native.len() as u64).to_be_bytes());
    bytes.extend_from_slice(native);
}

fn put_traversal_omission(bytes: &mut Vec<u8>, value: &TraversalOmission) {
    put_bytes(bytes, value.path().as_str());
    bytes.extend_from_slice(&value.depth().to_be_bytes());
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
