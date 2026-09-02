//! Strict `peritus-h0-aggregate` arguments.

use std::ffi::OsString;
use std::path::PathBuf;

pub(super) struct AggregateOptions {
    pub(super) linux: PathBuf,
    pub(super) macos: PathBuf,
    pub(super) windows: PathBuf,
    pub(super) review: PathBuf,
    pub(super) report: PathBuf,
}

impl AggregateOptions {
    pub(super) fn parse(arguments: &[OsString]) -> Result<Self, &'static str> {
        if arguments.len() != 10 {
            return Err(usage());
        }
        let mut linux = None;
        let mut macos = None;
        let mut windows = None;
        let mut review = None;
        let mut report = None;
        for pair in arguments.chunks_exact(2) {
            let name = pair[0].to_str().ok_or_else(usage)?;
            match name {
                "--linux" => set_once(&mut linux, PathBuf::from(&pair[1]))?,
                "--macos" => set_once(&mut macos, PathBuf::from(&pair[1]))?,
                "--windows" => set_once(&mut windows, PathBuf::from(&pair[1]))?,
                "--review" => set_once(&mut review, PathBuf::from(&pair[1]))?,
                "--report" => set_once(&mut report, PathBuf::from(&pair[1]))?,
                _ => return Err(usage()),
            }
        }
        Ok(Self {
            linux: linux.ok_or_else(usage)?,
            macos: macos.ok_or_else(usage)?,
            windows: windows.ok_or_else(usage)?,
            review: review.ok_or_else(usage)?,
            report: report.ok_or_else(usage)?,
        })
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), &'static str> {
    if slot.replace(value).is_some() {
        return Err(usage());
    }
    Ok(())
}

pub(super) const fn usage() -> &'static str {
    "usage: peritus-h0-aggregate --linux FILE --macos FILE --windows FILE --review FILE --report FILE"
}
