//! Checksummed snapshot, request, and observation-bound Git continuations.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use peritus_types::Sha256Digest;
use peritus_workspace::ReadOnlyWorkspace;

const CURSOR_MAGIC: &[u8; 8] = b"PGTCv001";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageKind {
    Status,
    Diff,
    History,
}

impl PageKind {
    const fn tag(self) -> u8 {
        match self {
            Self::Status => 1,
            Self::Diff => 2,
            Self::History => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PageCursor {
    pub(crate) snapshot: Sha256Digest,
    pub(crate) request: Sha256Digest,
    pub(crate) observation: Sha256Digest,
    pub(crate) first: u64,
    pub(crate) second: u64,
}

impl PageCursor {
    pub(crate) const fn new(
        snapshot: Sha256Digest,
        request: Sha256Digest,
        observation: Sha256Digest,
        first: u64,
        second: u64,
    ) -> Self {
        Self { snapshot, request, observation, first, second }
    }

    pub(crate) fn encode(self, kind: PageKind) -> String {
        let mut bytes = CURSOR_MAGIC.to_vec();
        bytes.push(kind.tag());
        bytes.extend_from_slice(self.snapshot.as_bytes());
        bytes.extend_from_slice(self.request.as_bytes());
        bytes.extend_from_slice(self.observation.as_bytes());
        bytes.extend_from_slice(&self.first.to_be_bytes());
        bytes.extend_from_slice(&self.second.to_be_bytes());
        seal(&mut bytes);
        STANDARD.encode(bytes)
    }

    pub(crate) fn decode(value: &str, kind: PageKind) -> Result<Self, ()> {
        let bytes = STANDARD.decode(value).map_err(|_| ())?;
        if STANDARD.encode(&bytes) != value {
            return Err(());
        }
        let payload = checked(&bytes)?;
        if payload.len() != 121 || payload[8] != kind.tag() {
            return Err(());
        }
        Ok(Self {
            snapshot: Sha256Digest::new(array(&payload[9..41])?),
            request: Sha256Digest::new(array(&payload[41..73])?),
            observation: Sha256Digest::new(array(&payload[73..105])?),
            first: u64::from_be_bytes(array(&payload[105..113])?),
            second: u64::from_be_bytes(array(&payload[113..121])?),
        })
    }
}

pub(crate) fn snapshot_binding(workspace: &ReadOnlyWorkspace) -> Sha256Digest {
    let snapshot = workspace.snapshot();
    let mut bytes = b"PERITUS-GIT-TOOL-SNAPSHOT-CURSOR-V1\0".to_vec();
    bytes.extend_from_slice(snapshot.workspace_id().as_bytes());
    bytes.extend_from_slice(&snapshot.generation().get().to_be_bytes());
    bytes.extend_from_slice(&snapshot.revision().get().to_be_bytes());
    put(&mut bytes, snapshot.commit().object_id().as_bytes());
    put(&mut bytes, snapshot.tree().object_id().as_bytes());
    peritus_codec::sha256(&bytes)
}

pub(crate) fn status_request() -> Sha256Digest {
    peritus_codec::sha256(b"PERITUS-GIT-TOOL-STATUS-REQUEST-V1\0")
}

pub(crate) fn diff_request(
    base_revision: &str,
    maximum_entries: u32,
    maximum_patch_bytes: u64,
) -> Sha256Digest {
    let mut bytes = b"PERITUS-GIT-TOOL-DIFF-REQUEST-V1\0".to_vec();
    put(&mut bytes, base_revision.as_bytes());
    bytes.extend_from_slice(&maximum_entries.to_be_bytes());
    bytes.extend_from_slice(&maximum_patch_bytes.to_be_bytes());
    peritus_codec::sha256(&bytes)
}

pub(crate) fn history_request(maximum_commits: u16) -> Sha256Digest {
    let mut bytes = b"PERITUS-GIT-TOOL-HISTORY-REQUEST-V1\0".to_vec();
    bytes.extend_from_slice(&maximum_commits.to_be_bytes());
    peritus_codec::sha256(&bytes)
}

fn seal(bytes: &mut Vec<u8>) {
    let digest = peritus_codec::sha256(bytes);
    bytes.extend_from_slice(digest.as_bytes());
}

fn checked(bytes: &[u8]) -> Result<&[u8], ()> {
    let end = bytes.len().checked_sub(32).ok_or(())?;
    let payload = bytes.get(..end).ok_or(())?;
    if !payload.starts_with(CURSOR_MAGIC)
        || peritus_codec::sha256(payload).as_bytes() != &bytes[end..]
    {
        return Err(());
    }
    Ok(payload)
}

fn array<const N: usize>(bytes: &[u8]) -> Result<[u8; N], ()> {
    bytes.try_into().map_err(|_| ())
}

fn put(target: &mut Vec<u8>, value: &[u8]) {
    let length = u64::try_from(value.len()).expect("bounded Git cursor field length fits u64");
    target.extend_from_slice(&length.to_be_bytes());
    target.extend_from_slice(value);
}
