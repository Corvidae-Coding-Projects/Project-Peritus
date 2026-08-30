//! Bounded parsing and deterministic selection of `HarnessBench` evidence.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use super::model::{NativeInvocation, PinEvidence, SelectedResult, UpstreamReport};
use crate::BenchmarkError;

const MAX_RESULT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_INVOCATION_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PIN_BYTES: u64 = 64 * 1024;
const MAX_RESULT_FILES: usize = 4_096;
const MAX_DIRECTORY_DEPTH: usize = 8;

pub(super) fn task_catalog(directory: &Path) -> Result<BTreeSet<String>, BenchmarkError> {
    let entries = fs::read_dir(directory).map_err(|error| {
        BenchmarkError::filesystem("list HarnessBench task catalog", directory, error)
    })?;
    let mut tasks = BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            BenchmarkError::filesystem("read HarnessBench task catalog entry", directory, error)
        })?;
        if entry
            .file_type()
            .map_err(|error| {
                BenchmarkError::filesystem("inspect HarnessBench task entry", entry.path(), error)
            })?
            .is_dir()
        {
            let name = entry.file_name().into_string().map_err(|_| {
                BenchmarkError::Workspace("HarnessBench task name is not UTF-8".to_owned())
            })?;
            tasks.insert(name);
        }
    }
    Ok(tasks)
}

pub(super) fn select_latest(
    campaign_directory: &Path,
) -> Result<Vec<SelectedResult>, BenchmarkError> {
    let results_directory = campaign_directory.join("results");
    if !results_directory.is_dir() {
        return Err(BenchmarkError::Workspace(format!(
            "HarnessBench results directory is missing: {}",
            results_directory.display()
        )));
    }
    let mut paths = Vec::new();
    collect_json(&results_directory, 0, &mut paths)?;
    paths.sort();

    let mut selected: BTreeMap<String, Candidate> = BTreeMap::new();
    for path in paths {
        let bytes = read_bounded(&path, MAX_RESULT_BYTES, "HarnessBench result")?;
        let identity: TaskIdentity = decode_json(&path, &bytes, "HarnessBench result identity")?;
        validate_file_name(&path, &identity.task_id)?;
        let relative_path = below(campaign_directory, &path)?;
        let modified_unix_ns = modified_ns(&path)?;
        let candidate = Candidate::new(path, relative_path, modified_unix_ns, identity.task_id);
        match selected.get_mut(&candidate.task_id) {
            Some(current) => {
                current.candidates += 1;
                if prefer(&candidate, current) {
                    let count = current.candidates;
                    *current = candidate;
                    current.candidates = count;
                }
            }
            None => {
                selected.insert(candidate.task_id.clone(), candidate);
            }
        }
    }
    selected.into_values().map(|candidate| finish(campaign_directory, candidate)).collect()
}

pub(super) fn pin_evidence(path: &Path) -> Result<PinEvidence, BenchmarkError> {
    let bytes = read_bounded(path, MAX_PIN_BYTES, "HarnessBench pin")?;
    let contents = String::from_utf8(bytes.clone()).map_err(|_| {
        BenchmarkError::Workspace(format!("HarnessBench pin is not UTF-8: {}", path.display()))
    })?;
    Ok(PinEvidence { path: path.to_path_buf(), sha256: digest(&bytes), contents })
}

#[derive(Debug)]
struct Candidate {
    path: PathBuf,
    relative_path: PathBuf,
    modified_unix_ns: u128,
    candidates: usize,
    task_id: String,
}

impl Candidate {
    const fn new(
        path: PathBuf,
        relative_path: PathBuf,
        modified_unix_ns: u128,
        task_id: String,
    ) -> Self {
        Self { path, relative_path, modified_unix_ns, candidates: 1, task_id }
    }
}

#[derive(serde::Deserialize)]
struct TaskIdentity {
    task_id: String,
}

fn finish(
    campaign_directory: &Path,
    candidate: Candidate,
) -> Result<SelectedResult, BenchmarkError> {
    let bytes = read_bounded(&candidate.path, MAX_RESULT_BYTES, "selected HarnessBench result")?;
    let report: UpstreamReport =
        decode_json(&candidate.path, &bytes, "selected HarnessBench result")?;
    if report.task_id != candidate.task_id {
        return Err(BenchmarkError::Workspace(format!(
            "selected HarnessBench result identity changed while reading {}",
            candidate.path.display()
        )));
    }
    let invocation = invocation(campaign_directory, &report.workspace)?;
    Ok(SelectedResult {
        relative_path: candidate.relative_path,
        modified_unix_ns: candidate.modified_unix_ns,
        candidate_results: candidate.candidates,
        sha256: digest(&bytes),
        report,
        invocation_path: invocation.as_ref().map(|(path, _)| path.clone()),
        invocation: invocation.map(|(_, value)| value),
    })
}

fn invocation(
    campaign_directory: &Path,
    workspace: &Path,
) -> Result<Option<(PathBuf, NativeInvocation)>, BenchmarkError> {
    let Some(sandbox) = workspace.parent() else {
        return Ok(None);
    };
    let path = sandbox.join("peritus-benchmark/invocation.json");
    if !path.is_file() {
        return Ok(None);
    }
    let relative = below(campaign_directory, &path)?;
    Ok(Some((relative, read_json(&path, MAX_INVOCATION_BYTES, "native invocation")?)))
}

fn collect_json(
    directory: &Path,
    depth: usize,
    paths: &mut Vec<PathBuf>,
) -> Result<(), BenchmarkError> {
    if depth > MAX_DIRECTORY_DEPTH {
        return Err(BenchmarkError::Workspace(format!(
            "HarnessBench results exceed the {MAX_DIRECTORY_DEPTH}-level directory bound"
        )));
    }
    let entries = fs::read_dir(directory).map_err(|error| {
        BenchmarkError::filesystem("list HarnessBench results", directory, error)
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            BenchmarkError::filesystem("read HarnessBench result entry", directory, error)
        })?;
        let file_type = entry.file_type().map_err(|error| {
            BenchmarkError::filesystem("inspect HarnessBench result entry", entry.path(), error)
        })?;
        if file_type.is_dir() {
            collect_json(&entry.path(), depth + 1, paths)?;
        } else if file_type.is_file()
            && entry.path().extension().is_some_and(|value| value == "json")
        {
            paths.push(entry.path());
            if paths.len() > MAX_RESULT_FILES {
                return Err(BenchmarkError::Workspace(format!(
                    "HarnessBench results exceed the {MAX_RESULT_FILES}-file bound"
                )));
            }
        }
    }
    Ok(())
}

fn prefer(candidate: &Candidate, current: &Candidate) -> bool {
    (candidate.modified_unix_ns, &candidate.relative_path)
        > (current.modified_unix_ns, &current.relative_path)
}

fn validate_file_name(path: &Path, task_id: &str) -> Result<(), BenchmarkError> {
    let stem = path.file_stem().and_then(|value| value.to_str()).ok_or_else(|| {
        BenchmarkError::Workspace(format!(
            "HarnessBench result name is not UTF-8: {}",
            path.display()
        ))
    })?;
    if stem != task_id {
        return Err(BenchmarkError::Workspace(format!(
            "HarnessBench result {} contains task {task_id:?}",
            path.display()
        )));
    }
    Ok(())
}

fn modified_ns(path: &Path) -> Result<u128, BenchmarkError> {
    let modified = fs::metadata(path).and_then(|value| value.modified()).map_err(|error| {
        BenchmarkError::filesystem("read result modification time", path, error)
    })?;
    modified.duration_since(UNIX_EPOCH).map(|value| value.as_nanos()).map_err(|_| {
        BenchmarkError::Workspace(format!(
            "HarnessBench result predates the Unix epoch: {}",
            path.display()
        ))
    })
}

fn below(root: &Path, path: &Path) -> Result<PathBuf, BenchmarkError> {
    path.strip_prefix(root).map(Path::to_path_buf).map_err(|_| {
        BenchmarkError::Workspace(format!(
            "HarnessBench evidence escaped campaign directory: {}",
            path.display()
        ))
    })
}

fn read_json<T: DeserializeOwned>(
    path: &Path,
    maximum: u64,
    label: &str,
) -> Result<T, BenchmarkError> {
    let bytes = read_bounded(path, maximum, label)?;
    decode_json(path, &bytes, label)
}

fn decode_json<T: DeserializeOwned>(
    path: &Path,
    bytes: &[u8],
    label: &str,
) -> Result<T, BenchmarkError> {
    serde_json::from_slice(bytes).map_err(|error| {
        BenchmarkError::Workspace(format!("{label} is malformed at {}: {error}", path.display()))
    })
}

fn read_bounded(path: &Path, maximum: u64, label: &str) -> Result<Vec<u8>, BenchmarkError> {
    let metadata = fs::metadata(path)
        .map_err(|error| BenchmarkError::filesystem("inspect evidence file", path, error))?;
    if !metadata.is_file() || metadata.len() > maximum {
        return Err(BenchmarkError::Workspace(format!(
            "{label} is not a regular file within the {maximum}-byte bound: {}",
            path.display()
        )));
    }
    fs::read(path).map_err(|error| BenchmarkError::filesystem("read evidence file", path, error))
}

fn digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_prefers_time_then_path() {
        let earlier = Candidate::new(
            PathBuf::from("/state/results/z/task.json"),
            PathBuf::from("results/z/task.json"),
            10,
            "task".to_owned(),
        );
        let later = Candidate::new(
            PathBuf::from("/state/results/a/task.json"),
            PathBuf::from("results/a/task.json"),
            11,
            "task".to_owned(),
        );
        assert!(prefer(&later, &earlier));
        let same_time_later_path = Candidate::new(
            PathBuf::from("/state/results/zz/task.json"),
            PathBuf::from("results/zz/task.json"),
            10,
            "task".to_owned(),
        );
        assert!(prefer(&same_time_later_path, &earlier));
    }
}
