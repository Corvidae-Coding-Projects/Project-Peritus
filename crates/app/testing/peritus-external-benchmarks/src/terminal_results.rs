//! Deterministic aggregation of retained Harbor Terminal-Bench results.

mod cli;
mod model;
mod parse;
mod publish;
#[cfg(test)]
mod tests;
mod validation;

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

pub use model::PublishedSummary;

use model::{CampaignReport, ReportRequest};

use crate::BenchmarkError;

/// Parses one report command, validates the retained Harbor job, and atomically publishes its
/// normalized report.
///
/// # Errors
///
/// Returns a typed error for invalid arguments, incomplete or inconsistent job evidence, malformed
/// result files, identity mismatches, arithmetic overflow, or failed atomic publication.
pub fn run_cli<I>(arguments: I) -> Result<PublishedSummary, BenchmarkError>
where
    I: IntoIterator<Item = OsString>,
{
    let request = cli::parse(arguments)?;
    let report = build(&request)?;
    publish::write(&request.output, &report)?;
    Ok(PublishedSummary::new(&request.output, &report))
}

/// Builds a normalized report without publishing it.
///
/// # Errors
///
/// Returns a typed error when the retained job is malformed, inconsistent, incomplete in final
/// mode, or does not match the declared campaign identity.
fn build(request: &ReportRequest) -> Result<CampaignReport, BenchmarkError> {
    request.validate()?;
    let job_directory = canonical_directory(&request.job_directory, "Terminal-Bench job")?;
    let pin_file = canonical_file(&request.pin_file, "Terminal-Bench pin")?;
    let state = parse::job_state(&job_directory.join("result.json"))?;
    let trials = parse::trials(&job_directory)?;
    CampaignReport::assemble(request, &job_directory, &pin_file, state, trials)
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, BenchmarkError> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| BenchmarkError::filesystem("canonicalize directory", path, error))?;
    if !canonical.is_dir() {
        return Err(BenchmarkError::Workspace(format!(
            "{label} path is not a directory: {}",
            path.display()
        )));
    }
    Ok(canonical)
}

fn canonical_file(path: &Path, label: &str) -> Result<PathBuf, BenchmarkError> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| BenchmarkError::filesystem("canonicalize file", path, error))?;
    if !canonical.is_file() {
        return Err(BenchmarkError::Workspace(format!(
            "{label} path is not a file: {}",
            path.display()
        )));
    }
    Ok(canonical)
}
