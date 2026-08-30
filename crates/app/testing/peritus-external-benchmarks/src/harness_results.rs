//! Deterministic aggregation of retained `HarnessBench` campaign results.

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

/// Selects one retained result per upstream task, validates the complete campaign, and atomically
/// publishes a normalized aggregate plus its exact selection manifest.
///
/// # Errors
///
/// Returns a typed error for invalid arguments, incomplete task coverage, malformed evidence,
/// inconsistent identities or task names, invalid scores, or failed atomic publication.
pub fn run_cli<I>(arguments: I) -> Result<PublishedSummary, BenchmarkError>
where
    I: IntoIterator<Item = OsString>,
{
    let request = cli::parse(arguments)?;
    let report = build(&request)?;
    publish::write(&request.output, &report)?;
    Ok(PublishedSummary::new(&request.output, &report))
}

fn build(request: &ReportRequest) -> Result<CampaignReport, BenchmarkError> {
    validation::request(request)?;
    let campaign_directory = canonical_directory(&request.campaign_directory, "campaign")?;
    let task_catalog = canonical_directory(&request.task_catalog, "task catalog")?;
    let pin_file = canonical_file(&request.pin_file, "pin")?;
    let task_names = parse::task_catalog(&task_catalog)?;
    let selected = parse::select_latest(&campaign_directory)?;
    CampaignReport::assemble(
        request,
        &campaign_directory,
        &task_catalog,
        &pin_file,
        &task_names,
        selected,
    )
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, BenchmarkError> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| BenchmarkError::filesystem("canonicalize directory", path, error))?;
    if !canonical.is_dir() {
        return Err(BenchmarkError::Workspace(format!(
            "HarnessBench {label} is not a directory: {}",
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
            "HarnessBench {label} is not a file: {}",
            path.display()
        )));
    }
    Ok(canonical)
}
