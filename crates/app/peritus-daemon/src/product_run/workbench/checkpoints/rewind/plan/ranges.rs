//! Selected-interval overlay over exact retained preimages, independent of live workspace bytes.

use super::{ControlError, Error, Preimage, patch_input};
use peritus_patch::{SnapshotFile, SnapshotSource};
use peritus_product_runner::control::CheckpointRange;
use sha2::{Digest as _, Sha256};
use std::{
    fs::File,
    io::{self, Read, Seek as _, SeekFrom},
    sync::Arc,
};

pub(super) fn materialize(
    captured: &SnapshotFile,
    current: &SnapshotFile,
    ranges: &[CheckpointRange],
) -> Result<SnapshotFile, Error> {
    let Preimage::Present { size: saved_size, .. } = captured.identity() else {
        return Err(ControlError::InvalidInput.into());
    };
    let Preimage::Present { size: current_size, mode, .. } = current.identity() else {
        return Err(ControlError::InvalidInput.into());
    };
    let mut saved = tempfile::NamedTempFile::new()?;
    let mut before = tempfile::NamedTempFile::new()?;
    patch_input(captured.write_to(&mut saved))?;
    patch_input(current.write_to(&mut before))?;
    saved.seek(SeekFrom::Start(0))?;
    before.seek(SeekFrom::Start(0))?;
    let selections = ranges.iter().map(|range| range.selection()).collect::<Vec<_>>();
    let resolve = crate::product_run::workbench::checkpoints::ranges::resolve_ranges;
    let saved_intervals = resolve(saved.as_file_mut(), &selections, saved_size)?;
    if ranges.iter().map(|range| range.captured_interval()).ne(saved_intervals.iter().copied()) {
        return Err(Error::Corrupt("selected coverage differs from retained source semantics"));
    }
    let current_intervals = resolve(before.as_file_mut(), &selections, current_size)?;
    let groups = overlay_groups(saved_intervals, current_intervals);
    let mut final_file = tempfile::NamedTempFile::new()?;
    let mut offset = 0;
    for group in groups {
        copy_interval(before.as_file_mut(), &mut final_file, offset, group.start)?;
        for (start, end) in union_intervals(group.sources) {
            copy_interval(saved.as_file_mut(), &mut final_file, start, end)?;
        }
        offset = group.end;
    }
    copy_interval(before.as_file_mut(), &mut final_file, offset, current_size)?;
    final_file.seek(SeekFrom::Start(0))?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut chunk = vec![0_u8; 64 * 1024];
    loop {
        let count = final_file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        bytes = bytes.checked_add(count as u64).ok_or(ControlError::Capacity)?;
        hasher.update(&chunk[..count]);
    }
    Ok(SnapshotFile::from_source(
        Arc::new(MergedSource(final_file.into_temp_path())),
        peritus_types::Sha256Digest::new(hasher.finalize().into()),
        bytes,
        mode,
    ))
}

struct OverlayGroup {
    start: u64,
    end: u64,
    sources: Vec<(u64, u64)>,
}
fn overlay_groups(saved: Vec<(u64, u64)>, current: Vec<(u64, u64)>) -> Vec<OverlayGroup> {
    let mut intervals = current.into_iter().zip(saved).collect::<Vec<_>>();
    intervals.sort_unstable();
    let mut groups: Vec<OverlayGroup> = Vec::new();
    for ((start, end), source) in intervals {
        if let Some(group) = groups.last_mut()
            && start <= group.end
        {
            group.end = group.end.max(end);
            group.sources.push(source);
        } else {
            groups.push(OverlayGroup { start, end, sources: vec![source] });
        }
    }
    groups
}
fn union_intervals(mut intervals: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    intervals.sort_unstable();
    let mut union: Vec<(u64, u64)> = Vec::new();
    for (start, end) in intervals {
        if let Some(last) = union.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
        } else {
            union.push((start, end));
        }
    }
    union
}
fn copy_interval(
    input: &mut File,
    output: &mut tempfile::NamedTempFile,
    start: u64,
    end: u64,
) -> Result<(), Error> {
    input.seek(SeekFrom::Start(start))?;
    if io::copy(&mut input.take(end - start), output)? != end - start {
        return Err(Error::StalePreimage);
    }
    Ok(())
}
#[derive(Debug)]
struct MergedSource(tempfile::TempPath);
impl SnapshotSource for MergedSource {
    fn open(&self) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(File::open(&self.0)?))
    }
}

#[cfg(test)]
mod tests;
