//! Resumable Windows workspace accounting for backend-owned disk enforcement.

use std::{
    collections::{BTreeMap, btree_map::Entry},
    fs::{self, ReadDir},
    io,
    ops::Bound::{Excluded, Unbounded},
    path::PathBuf,
};

use peritus_process::{ErrorCode, ProcessError, ProcessOperation, RecoveryClass};

const ENTRIES_PER_POLL: usize = 128;

pub(super) enum DiskObservation {
    Pending,
    Complete(u64),
}

pub(super) struct WindowsDiskObserver {
    root: PathBuf,
    files: BTreeMap<PathBuf, IndexedFile>,
    total: u64,
    generation: u128,
    scan: Option<Scan>,
}

impl core::fmt::Debug for WindowsDiskObserver {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("WindowsDiskObserver")
            .field("root", &self.root)
            .field("indexed_files", &self.files.len())
            .field("total", &self.total)
            .field("generation", &self.generation)
            .field("scan_active", &self.scan.is_some())
            .finish()
    }
}

impl WindowsDiskObserver {
    pub(super) fn new(root: PathBuf) -> Self {
        Self { root, files: BTreeMap::new(), total: 0, generation: 0, scan: None }
    }

    pub(super) fn poll(&mut self) -> Result<DiskObservation, ProcessError> {
        if self.scan.is_none() {
            self.generation = self.generation.saturating_add(1);
            self.scan = Some(Scan::new(self.root.clone(), self.generation));
        }
        let outcome = self
            .scan
            .as_mut()
            .ok_or_else(resource_observation_error)?
            .advance(ENTRIES_PER_POLL, &mut self.files, &mut self.total);
        match outcome {
            ScanOutcome::Pending => Ok(DiskObservation::Pending),
            ScanOutcome::Complete => {
                self.scan = None;
                Ok(DiskObservation::Complete(self.total))
            }
            ScanOutcome::Unavailable => {
                self.scan = None;
                Err(resource_observation_error())
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

const fn resource_observation_error() -> ProcessError {
    ProcessError::new(
        ErrorCode::ResourceLimit,
        ProcessOperation::Wait,
        RecoveryClass::CancelAndReap,
        "Windows workspace disk usage cannot be observed exactly",
    )
}
