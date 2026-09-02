//! Caller-derived `HarnessBench` work horizons with report-publication reserve.

use std::{
    env, fs,
    path::{Component, Path},
    time::Duration,
};

use peritus_product_runner::PRODUCT_RUN_MAX_ELAPSED;

use crate::BenchmarkError;

const TASKS_DIR_ENV: &str = "PERITUS_HARNESSBENCH_TASKS_DIR";
const MAX_TASK_BYTES: u64 = 64 * 1_024;

/// Resolves the current task's unchanged outer timeout into a shorter product work horizon.
///
/// # Errors
/// Returns a benchmark boundary error when the catalog location, task identity, or timeout record
/// is missing, malformed, ambiguous, or outside the supported bounds.
pub fn harnessbench_horizon(task_id: &str) -> Result<Duration, BenchmarkError> {
    let tasks_dir = env::var_os(TASKS_DIR_ENV).ok_or_else(|| {
        invalid(format!(
            "{TASKS_DIR_ENV} is required so the native adapter can reserve time before the outer HarnessBench deadline"
        ))
    })?;
    horizon_from_catalog(Path::new(&tasks_dir), task_id)
}

fn horizon_from_catalog(tasks_dir: &Path, task_id: &str) -> Result<Duration, BenchmarkError> {
    let mut components = Path::new(task_id).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(invalid("HarnessBench task identity is not one normal path component"));
    }
    let task_file = tasks_dir.join(task_id).join("task.yaml");
    let metadata = fs::metadata(&task_file).map_err(|error| {
        BenchmarkError::filesystem("inspect HarnessBench task deadline", &task_file, error)
    })?;
    if !metadata.is_file() || metadata.len() > MAX_TASK_BYTES {
        return Err(invalid(format!(
            "HarnessBench task deadline file is not a regular file within {MAX_TASK_BYTES} bytes: {}",
            task_file.display()
        )));
    }
    let text = fs::read_to_string(&task_file).map_err(|error| {
        BenchmarkError::filesystem("read HarnessBench task deadline", &task_file, error)
    })?;
    let outer_seconds = parse_timeout_seconds(&text)?;
    product_horizon(outer_seconds)
}

fn parse_timeout_seconds(text: &str) -> Result<u64, BenchmarkError> {
    let mut timeout = None;
    for raw in text.lines().filter(|line| !line.starts_with([' ', '\t'])) {
        let line = raw.split_once('#').map_or(raw, |(value, _)| value).trim_end();
        let Some(value) = line.strip_prefix("timeout_sec:") else {
            continue;
        };
        let seconds = value
            .trim()
            .parse::<u64>()
            .map_err(|_| invalid("HarnessBench timeout_sec is not a positive integer"))?;
        if seconds == 0 {
            return Err(invalid("HarnessBench timeout_sec is not a positive integer"));
        }
        if timeout.replace(seconds).is_some() {
            return Err(invalid("HarnessBench task declares timeout_sec more than once"));
        }
    }
    timeout.ok_or_else(|| invalid("HarnessBench task does not declare a top-level timeout_sec"))
}

fn product_horizon(outer_seconds: u64) -> Result<Duration, BenchmarkError> {
    let desired_reserve = outer_seconds.div_ceil(10).clamp(90, 300);
    let reserve = desired_reserve.min(outer_seconds / 2);
    let seconds = outer_seconds.saturating_sub(reserve).min(PRODUCT_RUN_MAX_ELAPSED.as_secs());
    if seconds == 0 {
        return Err(invalid("HarnessBench deadline leaves no positive Peritus work horizon"));
    }
    Ok(Duration::from_secs(seconds))
}

fn invalid(detail: impl Into<String>) -> BenchmarkError {
    BenchmarkError::Workspace(detail.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_horizon_from_the_exact_task_catalog_entry() {
        let root = tempfile::tempdir().expect("catalog");
        let task = root.path().join("009-git-pr-merge");
        fs::create_dir(&task).expect("task directory");
        fs::write(task.join("task.yaml"), "name: test\ntimeout_sec: 600\n").expect("task metadata");

        assert_eq!(
            horizon_from_catalog(root.path(), "009-git-pr-merge").expect("horizon"),
            Duration::from_secs(510)
        );
    }

    #[test]
    fn reserve_tracks_short_ordinary_and_long_deadlines() {
        assert_eq!(product_horizon(180).expect("short"), Duration::from_secs(90));
        assert_eq!(product_horizon(900).expect("ordinary"), Duration::from_secs(810));
        assert_eq!(product_horizon(3_600).expect("long"), Duration::from_mins(55));
        assert_eq!(product_horizon(43_200).expect("capped"), PRODUCT_RUN_MAX_ELAPSED);
    }

    #[test]
    fn rejects_ambiguous_or_escaping_task_deadlines() {
        assert!(parse_timeout_seconds("timeout_sec: 600\ntimeout_sec: 900\n").is_err());
        assert!(parse_timeout_seconds("  timeout_sec: 600\n").is_err());
        let root = tempfile::tempdir().expect("catalog");
        assert!(horizon_from_catalog(root.path(), "../escape").is_err());
    }
}
