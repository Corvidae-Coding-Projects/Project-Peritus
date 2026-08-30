//! Strict `peritus-h0` shard-operator arguments.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::QualificationPlatform;

pub(super) struct Options {
    pub(super) controller: PathBuf,
    pub(super) candidate: PathBuf,
    pub(super) host_facts: PathBuf,
    pub(super) scratch: PathBuf,
    pub(super) artifacts: PathBuf,
    pub(super) report: PathBuf,
    pub(super) platform: QualificationPlatform,
}

impl Options {
    pub(super) fn parse(arguments: &[OsString]) -> Result<Self, &'static str> {
        if arguments.len() != 14 {
            return Err(usage());
        }
        let mut controller = None;
        let mut candidate = None;
        let mut host_facts = None;
        let mut scratch = None;
        let mut artifacts = None;
        let mut report = None;
        let mut platform = None;
        for pair in arguments.chunks_exact(2) {
            let name = pair[0].to_str().ok_or_else(usage)?;
            match name {
                "--controller" => set_once(&mut controller, PathBuf::from(&pair[1]))?,
                "--candidate" => set_once(&mut candidate, PathBuf::from(&pair[1]))?,
                "--host-facts" => set_once(&mut host_facts, PathBuf::from(&pair[1]))?,
                "--scratch" => set_once(&mut scratch, PathBuf::from(&pair[1]))?,
                "--artifacts" => set_once(&mut artifacts, PathBuf::from(&pair[1]))?,
                "--report" => set_once(&mut report, PathBuf::from(&pair[1]))?,
                "--platform" => set_once(&mut platform, parse_platform(&pair[1])?)?,
                _ => return Err(usage()),
            }
        }
        Ok(Self {
            controller: controller.ok_or_else(usage)?,
            candidate: candidate.ok_or_else(usage)?,
            host_facts: host_facts.ok_or_else(usage)?,
            scratch: scratch.ok_or_else(usage)?,
            artifacts: artifacts.ok_or_else(usage)?,
            report: report.ok_or_else(usage)?,
            platform: platform.ok_or_else(usage)?,
        })
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), &'static str> {
    if slot.replace(value).is_some() {
        return Err(usage());
    }
    Ok(())
}

fn parse_platform(value: &OsString) -> Result<QualificationPlatform, &'static str> {
    match value.to_str() {
        Some("linux") => Ok(QualificationPlatform::Linux),
        Some("macos") => Ok(QualificationPlatform::Macos),
        Some("windows") => Ok(QualificationPlatform::Windows),
        _ => Err(usage()),
    }
}

pub(super) const fn usage() -> &'static str {
    "usage: peritus-h0 --controller PATH --candidate FILE --host-facts FILE --scratch DIR --artifacts DIR --report FILE --platform linux|macos|windows"
}
