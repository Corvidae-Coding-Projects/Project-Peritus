//! One-command native H0 shard operator.

mod aggregate;
mod aggregate_args;
mod args;

use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::path::Path;

use crate::{
    CancellationToken, HostFingerprint, NativeProbeFactory, QualificationLimits,
    QualificationRunner, parse_candidate_json,
};

use self::args::Options;

pub use aggregate::{H0AggregateStatus, run_from_env as run_aggregate_from_env};

const MAX_HOST_FACT_BYTES: u64 = 1024 * 1024;
const MAX_CANDIDATE_BYTES: u64 = 256 * 1024;

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
    let mut factory = NativeProbeFactory::new(
        &options.controller,
        &options.candidate_root,
        &options.scratch,
        &options.artifacts,
        HostFingerprint::from_document(&host_facts),
    )?;
    let shard = QualificationRunner.run_shard(
        &mut factory,
        candidate,
        QualificationLimits::production(),
        &CancellationToken::new(),
        options.platform,
    )?;
    let passed = shard.cases().iter().all(|case| case.outcome() == crate::CaseOutcome::Passed);
    publish_report(&options.report, &shard.canonical_json()?)?;
    Ok(if passed { H0OperatorStatus::Passed } else { H0OperatorStatus::Failed })
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
