//! Fixed-memory literal replacement with an exact staged postimage receipt.

use std::{
    collections::VecDeque,
    fs,
    io::{self, BufWriter, Write as _},
    path::Path,
};

use peritus_agent::DeveloperLoopError;
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use peritus_workspace::{FolderIdentity, FolderInspection};
use sha2::{Digest as _, Sha256};

use super::{
    checked,
    checkpoint_observer::{CheckpointFileMode, CheckpointFileVersion},
    tool,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LiteralPatchEvidence {
    digest: Sha256Digest,
    bytes: u64,
    replacements: usize,
    mode: CheckpointFileMode,
}

impl LiteralPatchEvidence {
    pub(super) const fn bytes(self) -> u64 {
        self.bytes
    }

    pub(super) const fn replacements(self) -> usize {
        self.replacements
    }

    pub(super) const fn version(self) -> CheckpointFileVersion {
        CheckpointFileVersion::present(self.digest, self.bytes, self.mode)
    }

    pub(super) fn digest_hex(self) -> String {
        use core::fmt::Write as _;

        let mut value = String::with_capacity(Sha256Digest::LENGTH.saturating_mul(2));
        for byte in self.digest.as_bytes() {
            let _ = write!(value, "{byte:02x}");
        }
        value
    }
}

pub(super) fn inspect_literal_patch(
    root: &Path,
    relative: &str,
    old: &str,
    new: &str,
    replace_all: bool,
) -> Result<LiteralPatchEvidence, DeveloperLoopError> {
    stream_literal_patch(root, relative, old, new, replace_all, io::sink())
        .map(|(evidence, _)| evidence)
}

pub(super) fn apply_literal_patch(
    root: &Path,
    relative: &str,
    target: &Path,
    old: &str,
    new: &str,
    replace_all: bool,
) -> Result<LiteralPatchEvidence, DeveloperLoopError> {
    let parent = target.parent().ok_or_else(|| tool("workspace file has no parent"))?;
    let mut staged =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| tool(error.to_string()))?;
    let (evidence, source) =
        stream_literal_patch(root, relative, old, new, replace_all, &mut staged)?;
    staged
        .as_file()
        .set_permissions(source.permissions())
        .map_err(|error| tool(error.to_string()))?;
    staged.as_file().sync_all().map_err(|error| tool(error.to_string()))?;
    let current = fs::symlink_metadata(target).map_err(|error| tool(error.to_string()))?;
    if !same_metadata(&source, &current) {
        return Err(tool(
            "workspace target changed while staging the patch; reobserve before mutation",
        ));
    }
    staged.persist(target).map_err(|error| tool(error.to_string()))?;
    Ok(evidence)
}

fn stream_literal_patch(
    root: &Path,
    relative: &str,
    old: &str,
    new: &str,
    replace_all: bool,
    output: impl io::Write,
) -> Result<(LiteralPatchEvidence, fs::Metadata), DeveloperLoopError> {
    if old.is_empty() {
        return Err(tool("patch old text is empty"));
    }
    let target = checked(root, relative, false)?;
    let before = fs::symlink_metadata(&target).map_err(|error| tool(error.to_string()))?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(tool("patch target is not a regular file"));
    }
    let identity = FolderIdentity::observe(root).map_err(|error| tool(error.to_string()))?;
    let inspection = FolderInspection::open(&identity).map_err(|error| tool(error.to_string()))?;
    let workspace_path = WorkspacePath::new(relative).map_err(|error| tool(error.to_string()))?;
    let mut transform = LiteralTransform::new(output, old.as_bytes(), new.as_bytes(), replace_all);
    inspection
        .copy_snapshot(&workspace_path, &mut transform)
        .map_err(|error| tool(error.to_string()))?;
    let (digest, bytes, occurrences) = transform.finish().map_err(|error| tool(error.to_string()))?;
    if occurrences == 0 || (!replace_all && occurrences != 1) {
        return Err(tool(format!("patch expected one match but found {occurrences}")));
    }
    let after = fs::symlink_metadata(&target).map_err(|error| tool(error.to_string()))?;
    if !same_metadata(&before, &after) {
        return Err(tool(
            "workspace target changed while preparing the patch; reobserve before mutation",
        ));
    }
    Ok((
        LiteralPatchEvidence {
            digest,
            bytes,
            replacements: occurrences,
            mode: file_mode(&after),
        },
        after,
    ))
}

struct LiteralTransform<W: io::Write> {
    output: BufWriter<W>,
    old: Vec<u8>,
    new: Vec<u8>,
    failure: Vec<usize>,
    pending: VecDeque<u8>,
    matched: usize,
    occurrences: usize,
    replace_all: bool,
    digest: Sha256,
    bytes: u64,
    utf8: Utf8Validator,
}

impl<W: io::Write> LiteralTransform<W> {
    fn new(output: W, old: &[u8], new: &[u8], replace_all: bool) -> Self {
        Self {
            output: BufWriter::with_capacity(64 * 1024, output),
            old: old.to_vec(),
            new: new.to_vec(),
            failure: failure_table(old),
            pending: VecDeque::with_capacity(old.len()),
            matched: 0,
            occurrences: 0,
            replace_all,
            digest: Sha256::new(),
            bytes: 0,
            utf8: Utf8Validator::default(),
        }
    }

    fn accept_byte(&mut self, byte: u8) -> io::Result<()> {
        loop {
            if byte == self.old[self.matched] {
                self.pending.push_back(byte);
                self.matched += 1;
                if self.matched == self.old.len() {
                    self.occurrences = self
                        .occurrences
                        .checked_add(1)
                        .ok_or_else(|| invalid_data("patch match count overflowed"))?;
                    if self.replace_all || self.occurrences == 1 {
                        self.pending.clear();
                        self.emit_new()?;
                    } else {
                        self.emit_pending(self.matched)?;
                    }
                    self.matched = 0;
                }
                return Ok(());
            }
            if self.matched == 0 {
                return self.emit(&[byte]);
            }
            let retained = self.failure[self.matched - 1];
            self.emit_pending(self.matched - retained)?;
            self.matched = retained;
        }
    }

    fn emit_new(&mut self) -> io::Result<()> {
        let new = self.new.clone();
        self.emit(&new)
    }

    fn emit_pending(&mut self, count: usize) -> io::Result<()> {
        if count > self.pending.len() {
            return Err(invalid_data("patch matcher state is inconsistent"));
        }
        {
            let (first, second) = self.pending.as_slices();
            let first_count = count.min(first.len());
            emit(
                &mut self.output,
                &mut self.digest,
                &mut self.bytes,
                &first[..first_count],
            )?;
            let second_count = count - first_count;
            emit(
                &mut self.output,
                &mut self.digest,
                &mut self.bytes,
                &second[..second_count],
            )?;
        }
        drop(self.pending.drain(..count));
        Ok(())
    }

    fn emit(&mut self, bytes: &[u8]) -> io::Result<()> {
        emit(&mut self.output, &mut self.digest, &mut self.bytes, bytes)
    }

    fn finish(mut self) -> io::Result<(Sha256Digest, u64, usize)> {
        self.utf8.finish()?;
        self.emit_pending(self.pending.len())?;
        self.output.flush()?;
        Ok((Sha256Digest::new(self.digest.finalize().into()), self.bytes, self.occurrences))
    }
}

impl<W: io::Write> io::Write for LiteralTransform<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.utf8.accept(bytes)?;
        for byte in bytes {
            self.accept_byte(*byte)?;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

fn failure_table(pattern: &[u8]) -> Vec<usize> {
    let mut table = vec![0; pattern.len()];
    for index in 1..pattern.len() {
        let mut matched = table[index - 1];
        while matched != 0 && pattern[index] != pattern[matched] {
            matched = table[matched - 1];
        }
        if pattern[index] == pattern[matched] {
            matched += 1;
        }
        table[index] = matched;
    }
    table
}

fn emit(
    output: &mut impl io::Write,
    digest: &mut Sha256,
    total: &mut u64,
    bytes: &[u8],
) -> io::Result<()> {
    let count = u64::try_from(bytes.len())
        .map_err(|_| invalid_data("patched file length is not representable"))?;
    let next = total
        .checked_add(count)
        .ok_or_else(|| invalid_data("patched file length overflowed"))?;
    output.write_all(bytes)?;
    digest.update(bytes);
    *total = next;
    Ok(())
}

#[derive(Default)]
struct Utf8Validator {
    pending: Vec<u8>,
    scratch: Vec<u8>,
}

impl Utf8Validator {
    fn accept(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.scratch.clear();
        self.scratch.extend_from_slice(&self.pending);
        self.scratch.extend_from_slice(bytes);
        match std::str::from_utf8(&self.scratch) {
            Ok(_) => self.pending.clear(),
            Err(error) if error.error_len().is_some() => {
                return Err(invalid_data("patch target is not UTF-8"));
            }
            Err(error) => {
                self.pending.clear();
                self.pending.extend_from_slice(&self.scratch[error.valid_up_to()..]);
            }
        }
        Ok(())
    }

    fn finish(self) -> io::Result<()> {
        if self.pending.is_empty() {
            Ok(())
        } else {
            Err(invalid_data("patch target is not UTF-8"))
        }
    }
}

fn invalid_data(detail: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, detail)
}

fn same_metadata(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    let same = left.len() == right.len()
        && left.modified().ok() == right.modified().ok()
        && left.permissions() == right.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        same
            && left.dev() == right.dev()
            && left.ino() == right.ino()
            && left.ctime() == right.ctime()
            && left.ctime_nsec() == right.ctime_nsec()
    }
    #[cfg(not(unix))]
    same
}

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> CheckpointFileMode {
    use std::os::unix::fs::PermissionsExt as _;
    if metadata.permissions().mode() & 0o111 == 0 {
        CheckpointFileMode::Regular
    } else {
        CheckpointFileMode::Executable
    }
}

#[cfg(not(unix))]
const fn file_mode(_metadata: &fs::Metadata) -> CheckpointFileMode {
    CheckpointFileMode::Regular
}
