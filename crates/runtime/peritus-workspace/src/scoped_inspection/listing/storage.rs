//! Streaming body codec; exact native units and offsets are verified before cursor adoption.

use super::super::{NativeEntryName, WorkspaceError, invalid, snapshot_io};
use super::{DirectoryExclusionReason, DirectoryItem, ObservedDirectory};
use crate::{WorkspaceEntryKind, WorkspaceMetadata};
use cap_std::fs::File;
use sha2::{Digest as _, Sha256};
use std::io::{self, Read as _, Write as _};

pub(super) struct BodyReader<'a> {
    pub(super) file: &'a mut File,
    pub(super) position: u64,
    pub(super) end: u64,
    pub(super) hasher: Option<Sha256>,
}
impl io::Read for BodyReader<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count =
            bytes.len().min(usize::try_from(self.end - self.position).unwrap_or(usize::MAX));
        let count = self.file.read(&mut bytes[..count])?;
        if let Some(hasher) = &mut self.hasher {
            hasher.update(&bytes[..count]);
        }
        self.position += count as u64;
        Ok(count)
    }
}
impl BodyReader<'_> {
    pub(super) fn fixed<const N: usize>(&mut self) -> Result<[u8; N], WorkspaceError> {
        let mut bytes = [0; N];
        self.read_exact(&mut bytes).map_err(snapshot_io)?;
        Ok(bytes)
    }
    pub(super) fn item(
        &mut self,
        observation: &ObservedDirectory,
        index: u64,
    ) -> Result<DirectoryItem, WorkspaceError> {
        let start = self.position;
        if u64::from_be_bytes(self.fixed::<8>()?) != index
            || u64::from_be_bytes(self.fixed::<8>()?) != start
        {
            return Err(invalid("directory record ordinal or offset changed"));
        }
        let tag = self.fixed::<1>()?[0];
        let length = u64::from_be_bytes(self.fixed::<8>()?);
        if length == 0 || length > self.end - self.position {
            return Err(invalid("native child length exceeds retained bytes"));
        }
        let mut bytes = vec![
            0;
            usize::try_from(length)
                .map_err(|_| invalid("native child length is not representable"))?
        ];
        self.read_exact(&mut bytes).map_err(snapshot_io)?;
        let name = NativeEntryName::from_encoded(tag, bytes)?;
        if name.protected() {
            return Err(invalid("retained listing exposes protected metadata"));
        }
        let observation = match self.fixed::<1>()?[0] {
            kind @ (1 | 2) => {
                let size = u64::from_be_bytes(self.fixed::<8>()?);
                let executable = match self.fixed::<1>()?[0] {
                    0 => false,
                    1 => true,
                    _ => return Err(invalid("invalid executable observation")),
                };
                if kind == 2 && (size != 0 || executable) {
                    return Err(invalid("invalid directory metadata"));
                }
                let native = name.native_name()?;
                let text =
                    native.to_str().ok_or_else(|| invalid("supported child name is not UTF-8"))?;
                let path =
                    peritus_patch::WorkspacePath::new(observation.path.as_ref().map_or_else(
                        || text.to_owned(),
                        |parent| format!("{}/{text}", parent.as_str()),
                    ))
                    .map_err(|_| invalid("supported child authority path is invalid"))?;
                Ok(WorkspaceMetadata::inspected(
                    path,
                    if kind == 1 {
                        WorkspaceEntryKind::File
                    } else {
                        WorkspaceEntryKind::Directory
                    },
                    size,
                    executable,
                ))
            }
            tag => Err(match tag {
                16 => DirectoryExclusionReason::UnrepresentableName,
                17 => DirectoryExclusionReason::SymbolicLink,
                18 => DirectoryExclusionReason::ReparsePoint,
                19 => DirectoryExclusionReason::SpecialNode,
                20 => DirectoryExclusionReason::MetadataUnavailable,
                21 => DirectoryExclusionReason::Changed,
                _ => return Err(invalid("unknown directory outcome tag")),
            }),
        };
        Ok(DirectoryItem { name, observation })
    }
}

pub(super) struct BodyWriter<'a> {
    pub(super) file: &'a mut File,
    pub(super) position: u64,
    pub(super) hasher: Sha256,
}
impl io::Write for BodyWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self.file.write(bytes)?;
        self.position = self.position.checked_add(count as u64).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "retained body offset overflow")
        })?;
        self.hasher.update(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
impl BodyWriter<'_> {
    pub(super) fn item(&mut self, item: &DirectoryItem, index: u64) -> Result<(), WorkspaceError> {
        let start = self.position;
        self.write_all(&index.to_be_bytes()).map_err(snapshot_io)?;
        self.write_all(&start.to_be_bytes()).map_err(snapshot_io)?;
        self.write_all(&[item.name.tag()]).map_err(snapshot_io)?;
        self.write_all(&(item.name.encoded_bytes().len() as u64).to_be_bytes())
            .map_err(snapshot_io)?;
        self.write_all(item.name.encoded_bytes()).map_err(snapshot_io)?;
        match &item.observation {
            Ok(metadata) => {
                self.write_all(&[if metadata.kind() == WorkspaceEntryKind::File { 1 } else { 2 }])
                    .map_err(snapshot_io)?;
                self.write_all(&metadata.size().to_be_bytes()).map_err(snapshot_io)?;
                self.write_all(&[u8::from(metadata.executable())]).map_err(snapshot_io)?;
            }
            Err(reason) => self
                .write_all(&[match reason {
                    DirectoryExclusionReason::UnrepresentableName => 16,
                    DirectoryExclusionReason::SymbolicLink => 17,
                    DirectoryExclusionReason::ReparsePoint => 18,
                    DirectoryExclusionReason::SpecialNode => 19,
                    DirectoryExclusionReason::MetadataUnavailable => 20,
                    DirectoryExclusionReason::Changed => 21,
                }])
                .map_err(snapshot_io)?,
        }
        Ok(())
    }
}
