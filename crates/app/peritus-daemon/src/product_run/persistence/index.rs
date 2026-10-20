//! Disposable metadata indexes. Full projections and authoritative controls are checked on demand.

use super::{PersistedRecord, ProductRunServiceError, RunRecord};
use peritus_types::RunId;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Stamp {
    length: u64,
    modified: u128,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}
impl Stamp {
    fn read(path: &Path) -> std::io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() {
            return Err(std::io::Error::other("run projection is not a regular file"));
        }
        Ok(Self {
            length: metadata.len(),
            modified: metadata
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(std::io::Error::other)?
                .as_nanos(),
            #[cfg(unix)]
            identity: {
                use std::os::unix::fs::MetadataExt as _;
                (metadata.dev(), metadata.ino(), metadata.ctime(), metadata.ctime_nsec())
            },
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    version: u16,
    stamp: Stamp,
    hint: PersistedRecord,
}

fn location(path: &Path) -> Option<PathBuf> {
    Some(path.parent()?.join(".index").join(path.file_name()?))
}

fn compact(mut value: PersistedRecord) -> PersistedRecord {
    value.interaction.strip_history();
    value.resume_artifact = None;
    value.resume_state = None;
    value.task_baseline = None;
    value.preview_page = None;
    value.preview_outputs.clear();
    value.preview_operations.clear();
    value.message_launches.clear();
    for text in [
        &mut value.execution_task,
        &mut value.task,
        &mut value.status,
        &mut value.diff,
        &mut value.gates,
        &mut value.review,
        &mut value.summary,
        &mut value.finding_state,
        &mut value.interruption_cause,
    ] {
        if text.len() > 1024 {
            *text = text.chars().take(256).collect();
        }
    }
    value.remaining_work.clear();
    if let Some(deliverable) = &mut value.deliverable {
        deliverable.changed_paths.truncate(1);
        deliverable.successful_commands.clear();
        deliverable.run_instructions = "Select the run to load its exact instructions".to_owned();
    }
    value
}

pub(super) fn write(path: &Path, record: PersistedRecord) -> Result<(), ProductRunServiceError> {
    let Some(index) = location(path) else { return Err(ProductRunServiceError::Unavailable) };
    let value = Index {
        version: 1,
        stamp: Stamp::read(path)
            .map_err(|e| ProductRunServiceError::persistence("inspect run index source", e))?,
        hint: compact(record),
    };
    let parent = index.parent().ok_or(ProductRunServiceError::Unavailable)?;
    fs::create_dir_all(parent)
        .map_err(|e| ProductRunServiceError::persistence("create run index", e))?;
    let file = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| ProductRunServiceError::persistence("create run index temporary", e))?;
    serde_json::to_writer(file.as_file(), &value)
        .map_err(|e| ProductRunServiceError::persistence("serialize run index", e))?;
    // Source is already durable. A lost index is rebuilt, so no extra fsync is required.
    file.persist(index).map_err(|e| ProductRunServiceError::persistence("publish run index", e))?;
    Ok(())
}

fn checked_hint(path: &Path, value: PersistedRecord) -> Option<RunRecord> {
    let record = value.into_record_with_context(Some("")).ok()?;
    super::record_path_matches(path, record.request.run_id()).then_some(record)
}

fn hint(path: &Path) -> Option<RunRecord> {
    let stamp = Stamp::read(path).ok()?;
    let index = location(path)
        .and_then(|index| fs::File::open(index).ok())
        .and_then(|file| serde_json::from_reader::<_, Index>(std::io::BufReader::new(file)).ok());
    if let Some(index) = index {
        if index.version == 1 && index.stamp == stamp {
            if let Some(record) = checked_hint(path, index.hint) {
                return Some(record);
            }
        }
    }
    // A disposable index can never cause an authoritative record to be quarantined. Rebuild
    // from the source if the cache is absent, stale, malformed, or semantically invalid.
    let value: PersistedRecord =
        serde_json::from_reader(std::io::BufReader::new(fs::File::open(path).ok()?)).ok()?;
    let encoded = serde_json::to_vec(&compact(value)).ok()?;
    let record = checked_hint(path, serde_json::from_slice(&encoded).ok()?)?;
    if let Ok(copy) = serde_json::from_slice(&encoded) {
        let _ = write(path, copy);
    }
    Some(record)
}

pub(in crate::product_run) fn scan(root: &Path) -> Vec<(RunId, RunRecord, PathBuf)> {
    let directory = root.join("runs");
    let quarantine = directory.join(".quarantine");
    let mut paths = super::workbench::projection_paths(&directory);
    paths.extend(
        super::workbench::projection_paths(&quarantine)
            .into_iter()
            .filter(|path| path.file_name().is_some_and(|name| !directory.join(name).exists())),
    );
    paths
        .into_iter()
        .filter_map(|path| match hint(&path) {
            Some(record) => Some((record.request.run_id(), record, path)),
            None => {
                super::quarantine_record(
                    &path,
                    "run metadata could not be indexed; original retained for recovery",
                    None,
                );
                None
            }
        })
        .collect()
}
