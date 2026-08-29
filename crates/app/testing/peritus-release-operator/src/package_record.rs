//! Versioned handoff from native packaging to evidence generation.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::error::OperatorError;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRecord {
    schema_version: u32,
    archive: PathBuf,
    checksum: PathBuf,
    build_started_unix: u64,
    build_finished_unix: u64,
}

impl PackageRecord {
    pub fn load(path: &Path) -> Result<Self, OperatorError> {
        let bytes = fs::read(path)
            .map_err(|error| OperatorError::io("read native package record", path, error))?;
        let record: Self = serde_json::from_slice(&bytes)?;
        if record.schema_version != 1 {
            return Err(OperatorError::metadata("native package record schema must be 1"));
        }
        if record.build_finished_unix < record.build_started_unix {
            return Err(OperatorError::metadata("native package finish precedes its start"));
        }
        validate_path(&record.archive)?;
        validate_path(&record.checksum)?;
        Ok(record)
    }

    pub fn archive(&self) -> &Path {
        &self.archive
    }

    pub fn checksum(&self) -> &Path {
        &self.checksum
    }

    pub const fn started(&self) -> u64 {
        self.build_started_unix
    }

    pub const fn finished(&self) -> u64 {
        self.build_finished_unix
    }
}

fn validate_path(path: &Path) -> Result<(), OperatorError> {
    let text = path
        .to_str()
        .ok_or_else(|| OperatorError::metadata("package record paths must be UTF-8"))?;
    if path.is_absolute()
        || path.as_os_str().is_empty()
        || text.bytes().any(|byte| byte.is_ascii_control())
        || path.components().any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(OperatorError::metadata(
            "package record paths must be normalized and relative",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_path;
    use std::path::Path;

    #[test]
    fn package_paths_stay_below_the_workspace() {
        assert!(validate_path(Path::new("dist/peritus.tar.gz")).is_ok());
        assert!(validate_path(Path::new("../peritus.tar.gz")).is_err());
        assert!(validate_path(Path::new("dist/line\nbreak")).is_err());
    }
}
