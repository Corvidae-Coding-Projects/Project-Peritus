//! Atomic, no-overwrite publication for normalized campaign evidence.

use std::{ffi::OsString, fs, io::Write as _, path::Path};

use super::model::CampaignReport;
use crate::BenchmarkError;

pub(super) fn write(output: &Path, report: &CampaignReport) -> Result<(), BenchmarkError> {
    if output.exists() {
        return Err(BenchmarkError::Workspace(format!(
            "report output already exists: {}",
            output.display()
        )));
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(BenchmarkError::Workspace(format!(
            "report output parent is not a directory: {}",
            parent.display()
        )));
    }
    let temporary = temporary_path(output);
    let bytes = serde_json::to_vec_pretty(report)?;
    let result = publish_bytes(output, &temporary, &bytes);
    if result.is_err() {
        let _ignored = fs::remove_file(&temporary);
    }
    result
}

fn publish_bytes(output: &Path, temporary: &Path, bytes: &[u8]) -> Result<(), BenchmarkError> {
    let mut file =
        fs::OpenOptions::new().write(true).create_new(true).open(temporary).map_err(|error| {
            BenchmarkError::filesystem("create temporary report", temporary, error)
        })?;
    file.write_all(bytes)
        .map_err(|error| BenchmarkError::filesystem("write temporary report", temporary, error))?;
    file.write_all(b"\n")
        .map_err(|error| BenchmarkError::filesystem("finish temporary report", temporary, error))?;
    file.sync_all()
        .map_err(|error| BenchmarkError::filesystem("sync temporary report", temporary, error))?;
    fs::rename(temporary, output)
        .map_err(|error| BenchmarkError::filesystem("publish campaign report", output, error))?;
    Ok(())
}

fn temporary_path(output: &Path) -> std::path::PathBuf {
    let mut name: OsString = output.as_os_str().to_owned();
    name.push(".new");
    name.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_report_is_a_sibling() {
        assert_eq!(
            temporary_path(Path::new("/state/report.json")),
            Path::new("/state/report.json.new")
        );
    }
}
