//! One-command native H0 shard operator.

mod aggregate;
mod aggregate_args;
mod args;

use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use crate::{
    CancellationToken, CaseReport, HostFingerprint, IntegratedCandidate, NativeProbeFactory,
    QualificationLimits, QualificationPlatform, QualificationRunner, QualificationShard,
    parse_candidate_json,
};

use self::args::Options;

pub use aggregate::{H0AggregateStatus, run_from_env as run_aggregate_from_env};

const MAX_HOST_FACT_BYTES: u64 = 1024 * 1024;
const MAX_CANDIDATE_BYTES: u64 = 256 * 1024;
const H0_WORKER_PARTITIONS: usize = 8;

/// Terminal status of a successfully executed native H0 shard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum H0OperatorStatus {
    /// Every case assigned to this native platform passed with complete cleanup.
    Passed,
    /// The shard completed and retained at least one non-passing case.
    Failed,
}

/// Parses process arguments, runs the canonical native shard, and publishes its report once.
///
/// # Errors
///
/// Returns syntax, filesystem, candidate, controller, campaign, or publication failures.
pub fn run_from_env() -> Result<H0OperatorStatus, Box<dyn std::error::Error>> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    run(&arguments)
}

fn run(arguments: &[OsString]) -> Result<H0OperatorStatus, Box<dyn std::error::Error>> {
    let options = Options::parse(arguments)?;
    let candidate_bytes = read_bounded(&options.candidate, MAX_CANDIDATE_BYTES, "candidate")?;
    let host_facts = read_bounded(&options.host_facts, MAX_HOST_FACT_BYTES, "host facts")?;
    let candidate = parse_candidate_json(&candidate_bytes)?;
    let shard = run_parallel_shard(&ParallelShardInputs {
        candidate,
        limits: QualificationLimits::production(),
        platform: options.platform,
        controller: &options.controller,
        candidate_root: &options.candidate_root,
        scratch: &options.scratch,
        artifacts: &options.artifacts,
        host: HostFingerprint::from_document(&host_facts),
    })?;
    let passed = shard.cases().iter().all(|case| case.outcome() == crate::CaseOutcome::Passed);
    publish_report(&options.report, &shard.canonical_json()?)?;
    Ok(if passed { H0OperatorStatus::Passed } else { H0OperatorStatus::Failed })
}

struct ParallelShardInputs<'a> {
    candidate: IntegratedCandidate,
    limits: QualificationLimits,
    platform: QualificationPlatform,
    controller: &'a Path,
    candidate_root: &'a Path,
    scratch: &'a Path,
    artifacts: &'a Path,
    host: HostFingerprint,
}

fn run_parallel_shard(
    inputs: &ParallelShardInputs<'_>,
) -> Result<QualificationShard, Box<dyn std::error::Error>> {
    let controller = PathBuf::from(inputs.controller);
    let candidate_root = PathBuf::from(inputs.candidate_root);
    let scratch = PathBuf::from(inputs.scratch);
    let artifacts = PathBuf::from(inputs.artifacts);
    let candidate = inputs.candidate;
    let limits = inputs.limits;
    let platform = inputs.platform;
    let host = inputs.host;
    let partitions = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(H0_WORKER_PARTITIONS);
        for partition in 0..H0_WORKER_PARTITIONS {
            let controller = controller.clone();
            let candidate_root = candidate_root.clone();
            let scratch = scratch.clone();
            let artifacts = artifacts.clone();
            handles.push(scope.spawn(move || {
                let mut factory =
                    NativeProbeFactory::new(controller, candidate_root, scratch, artifacts, host)?;
                Ok::<Vec<CaseReport>, crate::QualificationError>(
                    QualificationRunner::run_shard_partition(
                        &mut factory,
                        candidate,
                        limits,
                        &CancellationToken::new(),
                        platform,
                        partition,
                        H0_WORKER_PARTITIONS,
                    ),
                )
            }));
        }
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| "H0 worker partition panicked")?
                    .map_err(Box::<dyn std::error::Error>::from)
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()
    })?;
    let mut cases = partitions.into_iter().flatten().collect::<Vec<_>>();
    cases.sort_by_key(|case| case.spec().id());
    Ok(QualificationShard::new(candidate, limits, platform, cases)?)
}

fn read_bounded(
    path: &Path,
    maximum: u64,
    label: &'static str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > maximum {
        return Err(format!("H0 {label} must be a nonempty bounded regular file").into());
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    File::open(path)?.take(maximum + 1).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum {
        return Err(format!("H0 {label} exceeded its byte bound while reading").into());
    }
    Ok(bytes)
}

fn publish_report(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        return Err("H0 shard report path already exists".into());
    }
    let parent = path.parent().ok_or("H0 shard report path has no parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(path)?;
    Ok(())
}
