//! Bounded parsing of Harbor job, trial, native, and pin evidence.

use std::{collections::BTreeSet, fs, path::Path};

use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use super::model::{
    HarborTrial, JobState, PinEvidence, TrialEvidencePaths, TrialOutcome, TrialReport,
};
use crate::BenchmarkError;

const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PIN_BYTES: u64 = 64 * 1024;

pub(super) fn job_state(path: &Path) -> Result<JobState, BenchmarkError> {
    read_json(path)
}

pub(super) fn trials(job_directory: &Path) -> Result<Vec<TrialReport>, BenchmarkError> {
    let entries = fs::read_dir(job_directory).map_err(|error| {
        BenchmarkError::filesystem("list Terminal-Bench job directory", job_directory, error)
    })?;
    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            BenchmarkError::filesystem("read Terminal-Bench job entry", job_directory, error)
        })?;
        let file_type = entry.file_type().map_err(|error| {
            BenchmarkError::filesystem("inspect Terminal-Bench job entry", entry.path(), error)
        })?;
        if file_type.is_dir() && entry.path().join("result.json").is_file() {
            directories.push(entry.path());
        }
    }
    directories.sort();

    let mut names = BTreeSet::new();
    let mut reports = Vec::with_capacity(directories.len());
    for directory in directories {
        let result_path = directory.join("result.json");
        let trial: HarborTrial = read_json(&result_path)?;
        let directory_name =
            directory.file_name().and_then(|name| name.to_str()).ok_or_else(|| {
                BenchmarkError::Workspace("trial directory name is not UTF-8".to_owned())
            })?;
        if directory_name != trial.trial_name {
            return Err(BenchmarkError::Workspace(format!(
                "trial directory {directory_name:?} contains result for {:?}",
                trial.trial_name
            )));
        }
        if !names.insert(trial.trial_name.clone()) {
            return Err(BenchmarkError::Workspace(format!(
                "duplicate trial identity {:?}",
                trial.trial_name
            )));
        }
        reports.push(project(job_directory, &directory, trial)?);
    }
    Ok(reports)
}

pub(super) fn pin_evidence(path: &Path) -> Result<PinEvidence, BenchmarkError> {
    let bytes = read_bounded(path, MAX_PIN_BYTES, "Terminal-Bench pin")?;
    let contents = String::from_utf8(bytes.clone()).map_err(|_| {
        BenchmarkError::Workspace(format!("Terminal-Bench pin is not UTF-8: {}", path.display()))
    })?;
    Ok(PinEvidence { path: path.to_path_buf(), sha256: digest(&bytes), contents })
}

fn project(
    job_directory: &Path,
    trial_directory: &Path,
    trial: HarborTrial,
) -> Result<TrialReport, BenchmarkError> {
    let reward = trial.verifier_result.and_then(|result| result.rewards.reward);
    let outcome = TrialOutcome::from_reward(reward)?;
    let invocation_path = trial_directory.join("agent/peritus/invocation.json");
    let native = invocation_path.is_file().then(|| read_json(&invocation_path)).transpose()?;
    let relative = crate::report_path::canonical_relative(
        job_directory,
        trial_directory,
        "Terminal-Bench trial directory",
    )?;
    Ok(TrialReport {
        trial_name: trial.trial_name,
        task_name: trial.task_name,
        task_ref: trial.task_id.r#ref,
        source: trial.source,
        task_checksum: trial.task_checksum,
        reward,
        outcome,
        started_at: trial.started_at,
        finished_at: trial.finished_at,
        agent: trial.agent_info,
        usage: trial.agent_result,
        exception: trial.exception_info,
        native,
        evidence: TrialEvidencePaths {
            harbor_result: crate::report_path::join(&relative, "result.json"),
            native_invocation: existing(
                &relative,
                trial_directory,
                "agent/peritus/invocation.json",
            ),
            native_trace: existing(
                &relative,
                trial_directory,
                "agent/peritus/developer-round-0001.trace",
            ),
            native_observation: existing(
                &relative,
                trial_directory,
                "agent/peritus/last-product-observation.json",
            ),
            verifier_output: existing(&relative, trial_directory, "verifier/test-stdout.txt"),
        },
    })
}

fn existing(relative: &str, directory: &Path, suffix: &'static str) -> Option<String> {
    directory.join(suffix).is_file().then(|| crate::report_path::join(relative, suffix))
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, BenchmarkError> {
    let bytes = read_bounded(path, MAX_JSON_BYTES, "Terminal-Bench JSON")?;
    serde_json::from_slice(&bytes).map_err(BenchmarkError::Serialization)
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
    fn digest_is_lowercase_sha256() {
        assert_eq!(
            digest(b"peritus"),
            "e37df4e8bb764f688971c0d821b9c6ceee503134bed29c53c42f3a841d647898"
        );
    }
}
