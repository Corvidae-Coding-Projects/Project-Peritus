//! One-command native H0 shard operator.

mod aggregate;
mod aggregate_args;
mod args;

use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    CancellationToken, CaseReport, HostFingerprint, IntegratedCandidate, NativeProbeFactory,
    ProbeId, ProbeSpec, QualificationLimits, QualificationPlatform, QualificationRunner,
    QualificationShard, parse_candidate_json,
};

use self::args::Options;

pub use aggregate::{H0AggregateStatus, run_from_env as run_aggregate_from_env};

const MAX_HOST_FACT_BYTES: u64 = 1024 * 1024;
const MAX_CANDIDATE_BYTES: u64 = 256 * 1024;
const H0_WORKER_COUNT: usize = 12;

struct WorkUnitQueue {
    next: AtomicUsize,
    order: Vec<usize>,
}

impl WorkUnitQueue {
    fn new(work_units: impl IntoIterator<Item = usize>) -> Self {
        let work_units = work_units.into_iter().collect::<Vec<_>>();
        let mut order = Vec::with_capacity(work_units.len());
        for front in 0..work_units.len().div_ceil(2) {
            order.push(work_units[front]);
            let back = work_units.len() - front - 1;
            if back != front {
                order.push(work_units[back]);
            }
        }
        Self { next: AtomicUsize::new(0), order }
    }

    fn claim(&self) -> Option<usize> {
        let position = self.next.fetch_add(1, Ordering::Relaxed);
        self.order.get(position).copied()
    }
}

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
    let work_unit_count =
        ProbeSpec::h0_production().iter().filter(|spec| platform.owns(spec.target())).count();
    let warmup_work_unit = serial_warmup_work_unit(platform);
    let mut cases = Vec::new();
    if let Some(work_unit) = warmup_work_unit {
        let mut factory = NativeProbeFactory::new(
            controller.clone(),
            candidate_root.clone(),
            scratch.clone(),
            artifacts.clone(),
            host,
        )?;
        cases.extend(QualificationRunner::run_shard_partition(
            &mut factory,
            candidate,
            limits,
            &CancellationToken::new(),
            platform,
            work_unit,
            work_unit_count,
        ));
    }
    let work_units =
        WorkUnitQueue::new((0..work_unit_count).filter(|unit| Some(*unit) != warmup_work_unit));
    let partitions = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(H0_WORKER_COUNT);
        for _ in 0..H0_WORKER_COUNT {
            let controller = controller.clone();
            let candidate_root = candidate_root.clone();
            let scratch = scratch.clone();
            let artifacts = artifacts.clone();
            let work_units = &work_units;
            handles.push(scope.spawn(move || {
                let mut factory =
                    NativeProbeFactory::new(controller, candidate_root, scratch, artifacts, host)?;
                let mut cases = Vec::new();
                while let Some(work_unit) = work_units.claim() {
                    cases.extend(QualificationRunner::run_shard_partition(
                        &mut factory,
                        candidate,
                        limits,
                        &CancellationToken::new(),
                        platform,
                        work_unit,
                        work_unit_count,
                    ));
                }
                Ok::<Vec<CaseReport>, crate::QualificationError>(cases)
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
    cases.extend(partitions.into_iter().flatten());
    cases.sort_by_key(|case| case.spec().id());
    Ok(QualificationShard::new(candidate, limits, platform, cases)?)
}

fn serial_warmup_work_unit(platform: QualificationPlatform) -> Option<usize> {
    ProbeSpec::h0_production()
        .iter()
        .filter(|spec| platform.owns(spec.target()))
        .position(|spec| spec.id() == ProbeId::UnsafeInventory)
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

#[cfg(test)]
mod tests {
    use super::{H0_WORKER_COUNT, WorkUnitQueue, serial_warmup_work_unit};
    use crate::{ProbeId, ProbeSpec, QualificationPlatform};

    #[test]
    fn work_units_alternate_between_catalog_edges() {
        let even = WorkUnitQueue::new(0..6);
        assert_eq!((0..6).map(|_| even.claim()).collect::<Vec<_>>(), [0, 5, 1, 4, 2, 3].map(Some));

        let odd = WorkUnitQueue::new(0..5);
        assert_eq!((0..5).map(|_| odd.claim()).collect::<Vec<_>>(), [0, 4, 1, 3, 2].map(Some));
    }

    #[test]
    fn only_linux_serializes_the_workspace_compiler_probe() {
        let work_unit = serial_warmup_work_unit(QualificationPlatform::Linux)
            .expect("Linux owns the tier-one unsafe inventory probe");
        let spec = ProbeSpec::h0_production()
            .iter()
            .filter(|spec| QualificationPlatform::Linux.owns(spec.target()))
            .nth(work_unit)
            .expect("warm-up work unit");
        assert_eq!(spec.id(), ProbeId::UnsafeInventory);
        assert_eq!(serial_warmup_work_unit(QualificationPlatform::Macos), None);
        assert_eq!(serial_warmup_work_unit(QualificationPlatform::Windows), None);
    }

    #[test]
    fn linux_warmup_and_parallel_queue_cover_every_work_unit_once() {
        let count = ProbeSpec::h0_production()
            .iter()
            .filter(|spec| QualificationPlatform::Linux.owns(spec.target()))
            .count();
        let warmup = serial_warmup_work_unit(QualificationPlatform::Linux)
            .expect("Linux compiler warm-up work unit");
        let work_units = WorkUnitQueue::new((0..count).filter(|unit| *unit != warmup));
        let mut claimed = vec![warmup];
        while let Some(work_unit) = work_units.claim() {
            claimed.push(work_unit);
        }
        claimed.sort_unstable();
        assert_eq!(claimed, (0..count).collect::<Vec<_>>());
    }

    #[test]
    fn concurrent_workers_claim_every_linux_probe_exactly_once() {
        let count = ProbeSpec::h0_production()
            .iter()
            .filter(|spec| QualificationPlatform::Linux.owns(spec.target()))
            .count();
        let work_units = WorkUnitQueue::new(0..count);
        let mut claimed = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(H0_WORKER_COUNT);
            for _ in 0..H0_WORKER_COUNT {
                let work_units = &work_units;
                handles.push(scope.spawn(move || {
                    let mut claimed = Vec::new();
                    while let Some(index) = work_units.claim() {
                        claimed.push(index);
                    }
                    claimed
                }));
            }
            handles
                .into_iter()
                .flat_map(|handle| handle.join().expect("work-unit worker"))
                .collect::<Vec<_>>()
        });
        claimed.sort_unstable();
        assert_eq!(claimed, (0..count).collect::<Vec<_>>());
    }
}
