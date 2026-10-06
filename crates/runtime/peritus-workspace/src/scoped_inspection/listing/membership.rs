//! Exact native membership, independent of directory timestamp propagation or iteration order.

use super::{NativeEntryName, WorkspaceError, changed, snapshot_io};
use cap_std::fs::Dir;

struct Member {
    name: NativeEntryName,
    visited: bool,
}

/// Name-only evidence for one capture. It grants no authority and is never persisted.
/// Memory grows with actual native names; there is no synthetic entry allowance.
pub(super) struct DirectoryMembership {
    members: Vec<Member>,
}

impl DirectoryMembership {
    pub(super) fn observe(directory: &Dir) -> Result<Self, WorkspaceError> {
        let mut members = Vec::new();
        for entry in directory.entries().map_err(snapshot_io)? {
            let name = NativeEntryName::observed(&entry.map_err(snapshot_io)?.file_name())?;
            if name.protected() {
                continue;
            }
            members.try_reserve(1).map_err(|error| {
                snapshot_io(std::io::Error::new(std::io::ErrorKind::OutOfMemory, error))
            })?;
            members.push(Member { name, visited: false });
        }
        members.sort_unstable_by(|left, right| {
            left.name.encoded_bytes().cmp(right.name.encoded_bytes())
        });
        if members.windows(2).any(|pair| pair[0].name == pair[1].name) {
            return Err(changed());
        }
        Ok(Self { members })
    }

    pub(super) fn visit(&mut self, name: &NativeEntryName) -> Result<(), WorkspaceError> {
        let index = self
            .members
            .binary_search_by(|member| member.name.encoded_bytes().cmp(name.encoded_bytes()))
            .map_err(|_| changed())?;
        if std::mem::replace(&mut self.members[index].visited, true) {
            return Err(changed());
        }
        Ok(())
    }

    pub(super) fn finish_pass(&mut self) -> Result<(), WorkspaceError> {
        if self.members.iter().any(|member| !member.visited) {
            return Err(changed());
        }
        for member in &mut self.members {
            member.visited = false;
        }
        Ok(())
    }

    pub(super) fn verify(&mut self, directory: &Dir) -> Result<(), WorkspaceError> {
        for entry in directory.entries().map_err(snapshot_io)? {
            let name = NativeEntryName::observed(&entry.map_err(snapshot_io)?.file_name())?;
            if !name.protected() {
                self.visit(&name)?;
            }
        }
        self.finish_pass()
    }
}
