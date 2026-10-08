//! Resumable indexed observation of a changing workspace.

use std::{
    collections::{BTreeMap, btree_map::Entry},
    fs::{self, ReadDir},
    io,
    ops::Bound::{Excluded, Unbounded},
    path::PathBuf,
};

#[cfg(test)]
use std::{fs::DirEntry, path::Path};
#[cfg(test)]
use crate::ProcessError;
#[cfg(test)]
use super::resource_error;

/// Result of one physically bounded filesystem-observation slice.
pub(super) enum DiskObservation {
    /// More entries remain in the current verification pass.
    Pending,
    /// One complete walk and stale-entry sweep established the current logical byte total.
    Complete(u64),
    /// This pass could not establish a complete total. A later slice repairs the live index.
    Unavailable,
}

/// One live file index updated and verified without a second workspace-sized replacement map.
pub(super) struct DiskIndex {
    root: PathBuf,
    files: BTreeMap<PathBuf, IndexedFile>,
    total: u64,
    generation: u128,
    scan: Option<Scan>,
}

impl DiskIndex {
    pub(super) fn new(root: PathBuf) -> Self {
        Self { root, files: BTreeMap::new(), total: 0, generation: 0, scan: None }
    }

    /// Advances verification by at most `entry_budget` directory entries.
    ///
    /// The budget limits work in one sampler slice; it never limits the number of slices or the
    /// number of workspace entries that can eventually be verified.
    pub(super) fn step(&mut self, entry_budget: usize) -> DiskObservation {
        if self.scan.is_none() {
            self.generation = self.generation.saturating_add(1);
            self.scan = Some(Scan::new(self.root.clone(), self.generation));
        }
        let outcome = self.scan.as_mut().expect("scan was initialized").advance(
            entry_budget,
            &mut self.files,
            &mut self.total,
        );
        match outcome {
            ScanOutcome::Pending => DiskObservation::Pending,
            ScanOutcome::Complete => {
                self.scan = None;
                DiskObservation::Complete(self.total)
            }
            ScanOutcome::Unavailable => {
                self.scan = None;
                DiskObservation::Unavailable
            }
        }
    }
}

struct IndexedFile {
    bytes: u64,
    observed_generation: u128,
}

struct Scan {
    root: PathBuf,
    pending: Vec<PathBuf>,
    active: Option<ReadDir>,
    generation: u128,
    phase: ScanPhase,
}

impl Scan {
    fn new(root: PathBuf, generation: u128) -> Self {
        Self {
            root: root.clone(),
            pending: vec![root],
            active: None,
            generation,
            phase: ScanPhase::Walk,
        }
    }

    fn advance(
        &mut self,
        entry_budget: usize,
        files: &mut BTreeMap<PathBuf, IndexedFile>,
        total: &mut u64,
    ) -> ScanOutcome {
        let mut visited = 0_usize;
        while visited < entry_budget {
            match &mut self.phase {
                ScanPhase::Walk => {
                    if self.active.is_none() {
                        let Some(directory) = self.pending.pop() else {
                            self.phase = ScanPhase::Prune { after: None };
                            continue;
                        };
                        visited = visited.saturating_add(1);
                        match fs::read_dir(&directory) {
                            Ok(entries) => self.active = Some(entries),
                            Err(error)
                                if error.kind() == io::ErrorKind::NotFound
                                    && directory != self.root => {}
                            Err(_) => return ScanOutcome::Unavailable,
                        }
                        continue;
                    }
                    visited = visited.saturating_add(1);
                    let entry = self.active.as_mut().and_then(Iterator::next);
                    let Some(entry) = entry else {
                        self.active = None;
                        continue;
                    };
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(_) => return ScanOutcome::Unavailable,
                    };
                    let path = entry.path();
                    let metadata = match fs::symlink_metadata(&path) {
                        Ok(metadata) => metadata,
                        Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                        Err(_) => return ScanOutcome::Unavailable,
                    };
                    if metadata.file_type().is_symlink() {
                        continue;
                    }
                    if metadata.is_dir() {
                        self.pending.push(path);
                    } else if metadata.is_file() {
                        observe_file(files, total, path, metadata.len(), self.generation);
                    }
                }
                ScanPhase::Prune { after } => {
                    let next = match after.as_ref() {
                        Some(path) => files
                            .range((Excluded(path.clone()), Unbounded))
                            .next()
                            .map(|(path, file)| (path.clone(), file.observed_generation)),
                        None => files
                            .iter()
                            .next()
                            .map(|(path, file)| (path.clone(), file.observed_generation)),
                    };
                    let Some((path, observed_generation)) = next else {
                        return ScanOutcome::Complete;
                    };
                    visited = visited.saturating_add(1);
                    *after = Some(path.clone());
                    if observed_generation != self.generation
                        && let Some(removed) = files.remove(&path)
                    {
                        *total = total.saturating_sub(removed.bytes);
                    }
                }
            }
        }
        ScanOutcome::Pending
    }
}

enum ScanPhase {
    Walk,
    Prune { after: Option<PathBuf> },
}

fn observe_file(
    files: &mut BTreeMap<PathBuf, IndexedFile>,
    total: &mut u64,
    path: PathBuf,
    bytes: u64,
    generation: u128,
) {
    match files.entry(path) {
        Entry::Occupied(mut entry) => {
            *total = total.saturating_sub(entry.get().bytes).saturating_add(bytes);
            *entry.get_mut() = IndexedFile { bytes, observed_generation: generation };
        }
        Entry::Vacant(entry) => {
            *total = total.saturating_add(bytes);
            entry.insert(IndexedFile { bytes, observed_generation: generation });
        }
    }
}

enum ScanOutcome {
    Pending,
    Complete,
    Unavailable,
}

// Retain the direct helpers for their existing narrow unit coverage. Production sampling uses the
// resumable index above and never calls these whole-tree functions from the process owner.
#[cfg(test)]
pub(super) fn disk_usage(root: &Path) -> Result<u64, ProcessError> {
    let entries = fs::read_dir(root)
        .map_err(|_| resource_error("workspace disk usage cannot be observed"))?;
    let mut pending = Vec::new();
    let total = entries_usage(entries, &mut pending)?;
    descendants_usage(pending, total)
}

#[cfg(test)]
fn descendants_usage(mut pending: Vec<PathBuf>, mut total: u64) -> Result<u64, ProcessError> {
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return Err(resource_error("workspace disk usage cannot be observed")),
        };
        total = total.saturating_add(entries_usage(entries, &mut pending)?);
    }
    Ok(total)
}

#[cfg(test)]
fn entries_usage(
    entries: impl IntoIterator<Item = io::Result<DirEntry>>,
    pending: &mut Vec<PathBuf>,
) -> Result<u64, ProcessError> {
    let mut total = 0_u64;
    for entry in entries {
        let entry = entry.map_err(|_| resource_error("workspace entry cannot be observed"))?;
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return Err(resource_error("workspace metadata cannot be observed")),
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            pending.push(entry.path());
        } else if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests;
