//! Strict arguments for native H0 candidate preparation.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::QualificationPlatform;

use super::PreparationError;

pub(super) struct Options {
    pub(super) candidate_root: PathBuf,
    pub(super) controller: PathBuf,
    pub(super) output: PathBuf,
    pub(super) platform: QualificationPlatform,
}

impl Options {
    pub(super) fn parse(arguments: &[OsString]) -> Result<Self, PreparationError> {
        if arguments.len() != 8 {
            return Err(PreparationError::Arguments(usage()));
        }
        let mut candidate_root = None;
        let mut controller = None;
        let mut output = None;
        let mut platform = None;
        for pair in arguments.chunks_exact(2) {
            let name = pair[0].to_str().ok_or(PreparationError::Arguments(usage()))?;
            match name {
                "--candidate-root" => set_once(&mut candidate_root, PathBuf::from(&pair[1]))?,
                "--controller" => set_once(&mut controller, PathBuf::from(&pair[1]))?,
                "--output" => set_once(&mut output, PathBuf::from(&pair[1]))?,
                "--platform" => set_once(&mut platform, parse_platform(&pair[1])?)?,
                _ => return Err(PreparationError::Arguments(usage())),
            }
        }
        Ok(Self {
            candidate_root: candidate_root.ok_or(PreparationError::Arguments(usage()))?,
            controller: controller.ok_or(PreparationError::Arguments(usage()))?,
            output: output.ok_or(PreparationError::Arguments(usage()))?,
            platform: platform.ok_or(PreparationError::Arguments(usage()))?,
        })
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), PreparationError> {
    if slot.replace(value).is_some() {
        return Err(PreparationError::Arguments(usage()));
    }
    Ok(())
}

fn parse_platform(value: &OsString) -> Result<QualificationPlatform, PreparationError> {
    match value.to_str() {
        Some("linux") => Ok(QualificationPlatform::Linux),
        Some("macos") => Ok(QualificationPlatform::Macos),
        Some("windows") => Ok(QualificationPlatform::Windows),
        _ => Err(PreparationError::Arguments(usage())),
    }
}

const fn usage() -> &'static str {
    "usage: peritus-h0-prepare --candidate-root DIR --controller FILE --output DIR --platform linux|macos|windows"
}
