//! Checksummed, snapshot-bound continuations for stateless filesystem dispatchers.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use peritus_workspace::ReadOnlyWorkspace;

const PAGE_MAGIC: &[u8; 8] = b"PFSCv001";
const PAGE_MEMBERSHIP_MAGIC: &[u8; 8] = b"PFSCv002";
const READ_MAGIC: &[u8; 8] = b"PFSRv001";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageKind {
    Discover,
    Search,
}

impl PageKind {
    const fn tag(self) -> u8 {
        match self {
            Self::Discover => 1,
            Self::Search => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PageCursor {
    pub(crate) snapshot: Sha256Digest,
    pub(crate) request: Sha256Digest,
    pub(crate) first: u64,
    pub(crate) second: u64,
    pub(crate) membership: Option<Sha256Digest>,
    pub(crate) source: Option<Sha256Digest>,
}

impl PageCursor {
    pub(crate) const fn new(
        snapshot: Sha256Digest,
        request: Sha256Digest,
        first: u64,
        second: u64,
    ) -> Self {
        Self { snapshot, request, first, second, membership: None, source: None }
    }

    pub(crate) const fn with_membership(
        mut self,
        membership: Sha256Digest,
        source: Option<Sha256Digest>,
    ) -> Self {
        self.membership = Some(membership);
        self.source = source;
        self
    }

    pub(crate) fn encode(self, kind: PageKind) -> String {
        let mut bytes = if self.membership.is_some() {
            PAGE_MEMBERSHIP_MAGIC.to_vec()
        } else {
            PAGE_MAGIC.to_vec()
        };
        bytes.push(kind.tag());
        bytes.extend_from_slice(self.snapshot.as_bytes());
        bytes.extend_from_slice(self.request.as_bytes());
        if let Some(membership) = self.membership {
            bytes.extend_from_slice(membership.as_bytes());
            bytes.push(u8::from(self.source.is_some()));
            bytes.extend_from_slice(
                self.source.unwrap_or_else(|| Sha256Digest::new([0; 32])).as_bytes(),
            );
        }
        bytes.extend_from_slice(&self.first.to_be_bytes());
        bytes.extend_from_slice(&self.second.to_be_bytes());
        seal(&mut bytes);
        STANDARD.encode(bytes)
    }

    pub(crate) fn decode(value: &str, kind: PageKind) -> Result<Self, ()> {
        let bytes = decode(value)?;
        if bytes.starts_with(PAGE_MEMBERSHIP_MAGIC) {
            let payload = checked(&bytes, PAGE_MEMBERSHIP_MAGIC)?;
            if payload.len() != 154 || payload[8] != kind.tag() || payload[105] > 1 {
                return Err(());
            }
            let source_bytes: [u8; 32] = array(&payload[106..138])?;
            if payload[105] == 0 && source_bytes != [0; 32] {
                return Err(());
            }
            return Ok(Self {
                snapshot: Sha256Digest::new(array(&payload[9..41])?),
                request: Sha256Digest::new(array(&payload[41..73])?),
                membership: Some(Sha256Digest::new(array(&payload[73..105])?)),
                source: (payload[105] == 1).then(|| Sha256Digest::new(source_bytes)),
                first: u64::from_be_bytes(array(&payload[138..146])?),
                second: u64::from_be_bytes(array(&payload[146..154])?),
            });
        }
        let payload = checked(&bytes, PAGE_MAGIC)?;
        if payload.len() != 89 || payload[8] != kind.tag() {
            return Err(());
        }
        Ok(Self {
            snapshot: Sha256Digest::new(array(&payload[9..41])?),
            request: Sha256Digest::new(array(&payload[41..73])?),
            first: u64::from_be_bytes(array(&payload[73..81])?),
            second: u64::from_be_bytes(array(&payload[81..89])?),
            membership: None,
            source: None,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReadCursor {
    pub(crate) snapshot: Sha256Digest,
    pub(crate) path: WorkspacePath,
    pub(crate) source_bytes: u64,
    pub(crate) source_digest: Sha256Digest,
    pub(crate) offset: u64,
}

impl ReadCursor {
    pub(crate) fn encode(&self) -> String {
        let mut bytes = READ_MAGIC.to_vec();
        bytes.extend_from_slice(self.snapshot.as_bytes());
        bytes.extend_from_slice(&self.source_bytes.to_be_bytes());
        bytes.extend_from_slice(self.source_digest.as_bytes());
        bytes.extend_from_slice(&self.offset.to_be_bytes());
        bytes.extend_from_slice(self.path.as_str().as_bytes());
        seal(&mut bytes);
        STANDARD.encode(bytes)
    }

    pub(crate) fn decode(value: &str) -> Result<Self, ()> {
        let bytes = decode(value)?;
        let payload = checked(&bytes, READ_MAGIC)?;
        if payload.len() < 88 {
            return Err(());
        }
        let source_bytes = u64::from_be_bytes(array(&payload[40..48])?);
        let offset = u64::from_be_bytes(array(&payload[80..88])?);
        if offset > source_bytes {
            return Err(());
        }
        let path = std::str::from_utf8(&payload[88..]).map_err(|_| ())?;
        Ok(Self {
            snapshot: Sha256Digest::new(array(&payload[8..40])?),
            path: WorkspacePath::new(path).map_err(|_| ())?,
            source_bytes,
            source_digest: Sha256Digest::new(array(&payload[48..80])?),
            offset,
        })
    }
}

pub(crate) fn snapshot_binding(workspace: &ReadOnlyWorkspace) -> Sha256Digest {
    let snapshot = workspace.snapshot();
    let mut bytes = b"PERITUS-FS-SNAPSHOT-CURSOR-V1\0".to_vec();
    bytes.extend_from_slice(snapshot.workspace_id().as_bytes());
    bytes.extend_from_slice(&snapshot.generation().get().to_be_bytes());
    bytes.extend_from_slice(&snapshot.revision().get().to_be_bytes());
    put(&mut bytes, snapshot.commit().to_string().as_bytes());
    put(&mut bytes, snapshot.tree().to_string().as_bytes());
    peritus_codec::sha256(&bytes)
}

pub(crate) fn discover_request(
    root: Option<&WorkspacePath>,
    maximum_depth: u16,
) -> Sha256Digest {
    let mut bytes = b"PERITUS-FS-DISCOVER-CURSOR-V1\0".to_vec();
    put(&mut bytes, root.map_or(&[][..], |path| path.as_str().as_bytes()));
    bytes.extend_from_slice(&maximum_depth.to_be_bytes());
    peritus_codec::sha256(&bytes)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn search_request(
    root: Option<&WorkspacePath>,
    literal: &str,
    case_sensitive: bool,
    maximum_depth: u16,
    maximum_file_bytes: u64,
    maximum_total_bytes: u64,
) -> Sha256Digest {
    let mut bytes = b"PERITUS-FS-SEARCH-CURSOR-V1\0".to_vec();
    put(&mut bytes, root.map_or(&[][..], |path| path.as_str().as_bytes()));
    put(&mut bytes, literal.as_bytes());
    bytes.push(u8::from(case_sensitive));
    bytes.extend_from_slice(&maximum_depth.to_be_bytes());
    bytes.extend_from_slice(&maximum_file_bytes.to_be_bytes());
    bytes.extend_from_slice(&maximum_total_bytes.to_be_bytes());
    peritus_codec::sha256(&bytes)
}

fn seal(bytes: &mut Vec<u8>) {
    let digest = peritus_codec::sha256(bytes);
    bytes.extend_from_slice(digest.as_bytes());
}

fn decode(value: &str) -> Result<Vec<u8>, ()> {
    let bytes = STANDARD.decode(value).map_err(|_| ())?;
    if STANDARD.encode(&bytes) != value {
        return Err(());
    }
    Ok(bytes)
}

fn checked<'a>(bytes: &'a [u8], magic: &[u8; 8]) -> Result<&'a [u8], ()> {
    let end = bytes.len().checked_sub(32).ok_or(())?;
    let payload = bytes.get(..end).ok_or(())?;
    if !payload.starts_with(magic) || peritus_codec::sha256(payload).as_bytes() != &bytes[end..] {
        return Err(());
    }
    Ok(payload)
}

fn array<const N: usize>(bytes: &[u8]) -> Result<[u8; N], ()> {
    bytes.try_into().map_err(|_| ())
}

fn put(target: &mut Vec<u8>, value: &[u8]) {
    target.extend_from_slice(&(value.len() as u64).to_be_bytes());
    target.extend_from_slice(value);
}
