//! Strict `peritus-h1` command-line arguments.

use std::ffi::OsString;
use std::path::PathBuf;

pub struct OperatorOptions {
    pub controller: PathBuf,
    pub candidate: PathBuf,
    pub scratch: PathBuf,
    pub artifacts: PathBuf,
    pub report: PathBuf,
    pub subject_id: String,
    pub implementation: String,
    pub scenario_id: Option<String>,
}

impl OperatorOptions {
    pub(super) fn parse(arguments: &[OsString]) -> Result<Self, &'static str> {
        if !matches!(arguments.len(), 14 | 16) {
            return Err(usage());
        }
        let mut controller = None;
        let mut candidate = None;
        let mut scratch = None;
        let mut artifacts = None;
        let mut report = None;
        let mut subject_id = None;
        let mut implementation = None;
        let mut scenario_id = None;
        for pair in arguments.chunks_exact(2) {
            let name = pair[0].to_str().ok_or_else(usage)?;
            match name {
                "--controller" => set_once(&mut controller, PathBuf::from(&pair[1]))?,
                "--candidate" => set_once(&mut candidate, PathBuf::from(&pair[1]))?,
                "--scratch" => set_once(&mut scratch, PathBuf::from(&pair[1]))?,
                "--artifacts" => set_once(&mut artifacts, PathBuf::from(&pair[1]))?,
                "--report" => set_once(&mut report, PathBuf::from(&pair[1]))?,
                "--subject-id" => set_once(&mut subject_id, text(&pair[1])?)?,
                "--implementation" => set_once(&mut implementation, text(&pair[1])?)?,
                "--diagnostic-scenario" => set_once(&mut scenario_id, text(&pair[1])?)?,
                _ => return Err(usage()),
            }
        }
        Ok(Self {
            controller: controller.ok_or_else(usage)?,
            candidate: candidate.ok_or_else(usage)?,
            scratch: scratch.ok_or_else(usage)?,
            artifacts: artifacts.ok_or_else(usage)?,
            report: report.ok_or_else(usage)?,
            subject_id: subject_id.ok_or_else(usage)?,
            implementation: implementation.ok_or_else(usage)?,
            scenario_id,
        })
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), &'static str> {
    if slot.replace(value).is_some() {
        return Err(usage());
    }
    Ok(())
}

fn text(value: &OsString) -> Result<String, &'static str> {
    value.to_str().map(str::to_owned).ok_or_else(usage)
}

const fn usage() -> &'static str {
    "usage: peritus-h1 --controller FILE --candidate FILE --scratch DIR --artifacts DIR --report FILE --subject-id ID --implementation TEXT [--diagnostic-scenario ID]"
}

#[cfg(test)]
mod tests {
    use super::OperatorOptions;
    use std::ffi::OsString;

    #[test]
    fn exact_option_set_is_order_independent() {
        let arguments = [
            "--report",
            "report.json",
            "--candidate",
            "peritusd",
            "--controller",
            "controller",
            "--artifacts",
            "artifacts",
            "--implementation",
            "release",
            "--scratch",
            "scratch",
            "--subject-id",
            "peritus.release",
        ]
        .map(OsString::from);
        let parsed = OperatorOptions::parse(&arguments).expect("parse exact H1 options");
        assert_eq!(parsed.subject_id, "peritus.release");
        assert_eq!(parsed.report, std::path::Path::new("report.json"));
        assert_eq!(parsed.scenario_id, None);
    }

    #[test]
    fn focused_diagnostic_scenario_is_explicit() {
        let arguments = [
            "--report",
            "report.json",
            "--candidate",
            "peritusd",
            "--controller",
            "controller",
            "--artifacts",
            "artifacts",
            "--implementation",
            "release",
            "--scratch",
            "scratch",
            "--subject-id",
            "peritus.release",
            "--diagnostic-scenario",
            "h1.crash.journal.after-before-ack",
        ]
        .map(OsString::from);
        let parsed = OperatorOptions::parse(&arguments).expect("parse focused H1 options");
        assert_eq!(parsed.scenario_id.as_deref(), Some("h1.crash.journal.after-before-ack"));
    }

    #[test]
    fn duplicate_or_unknown_options_are_rejected() {
        let arguments = [
            "--report",
            "one",
            "--report",
            "two",
            "--candidate",
            "peritusd",
            "--controller",
            "controller",
            "--artifacts",
            "artifacts",
            "--implementation",
            "release",
            "--scratch",
            "scratch",
        ]
        .map(OsString::from);
        assert!(OperatorOptions::parse(&arguments).is_err());
    }
}
