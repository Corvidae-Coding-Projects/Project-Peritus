//! One-command H3 production qualification composition.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use peritus_benchmarks::{
    DatasetLimits, QualificationDataset, RunnerDescriptor, Sha256Digest, StableId,
    baseline_from_json,
};

use crate::{
    CampaignCoordinator, CampaignEvidenceWriter, CampaignMode, CampaignRequest, MachineProbe,
    OperatorError, PublishedEvidence, sha256_file,
};

const MAX_PROFILE_BYTES: u64 = 256 * 1024;
const MAX_WORKLOAD_BYTES: u64 = 512 * 1024;
const MAX_BASELINE_BYTES: u64 = 512 * 1024;

/// Human-facing command syntax for the H3 operator binary.
pub const OPERATOR_USAGE: &str = "\
Usage: peritus-h3 <load|full> \\
  --daemon <peritusd> \\
  --profile <profile.json> \\
  --workloads <workloads.json> \\
  [--baseline <accepted-baseline.json>] \\
  [--accept-baseline-sha256 <reviewed-document-digest>] \\
  --evidence <new-output-directory> \\
  --storage-class <stable-id> \\
  --revision <source-revision>\n\
\n\
load runs only sub-hour workloads. full runs the load workloads and then the four concurrent\n\
long-horizon workloads. The evidence destination must not already exist.\n";

/// Validated command-line inputs for one H3 operator execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorOptions {
    mode: CampaignMode,
    daemon: PathBuf,
    profile: PathBuf,
    workloads: PathBuf,
    baseline: Option<PathBuf>,
    baseline_digest: Option<Sha256Digest>,
    evidence: PathBuf,
    storage_class: StableId,
    revision: String,
}

impl OperatorOptions {
    /// Parses strict, duplicate-free command arguments.
    ///
    /// # Errors
    ///
    /// Returns [`OperatorError::HelpRequested`] for `--help` and [`OperatorError::Usage`] for a
    /// missing, duplicate, non-UTF-8, or unknown command argument.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, OperatorError> {
        let mut args = args.into_iter();
        let mode = match text(args.next(), "mode")?.as_str() {
            "load" => CampaignMode::Load,
            "full" => CampaignMode::Full,
            "--help" | "-h" => return Err(OperatorError::HelpRequested),
            value => return Err(usage(format!("unknown mode `{value}`"))),
        };
        let mut daemon = None;
        let mut profile = None;
        let mut workloads = None;
        let mut baseline = None;
        let mut baseline_digest = None;
        let mut evidence = None;
        let mut storage_class = None;
        let mut revision = None;
        while let Some(argument) = args.next() {
            let flag = argument
                .into_string()
                .map_err(|_| usage("option names must be UTF-8".to_owned()))?;
            if matches!(flag.as_str(), "--help" | "-h") {
                return Err(OperatorError::HelpRequested);
            }
            let value = args.next().ok_or_else(|| usage(format!("{flag} requires a value")))?;
            match flag.as_str() {
                "--daemon" => assign(&mut daemon, PathBuf::from(value), &flag)?,
                "--profile" => assign(&mut profile, PathBuf::from(value), &flag)?,
                "--workloads" => assign(&mut workloads, PathBuf::from(value), &flag)?,
                "--baseline" => assign(&mut baseline, PathBuf::from(value), &flag)?,
                "--accept-baseline-sha256" => {
                    let value = value
                        .into_string()
                        .map_err(|_| usage("--accept-baseline-sha256 must be UTF-8".to_owned()))?;
                    assign(&mut baseline_digest, Sha256Digest::parse(value)?, &flag)?;
                }
                "--evidence" => assign(&mut evidence, PathBuf::from(value), &flag)?,
                "--storage-class" => {
                    let value = value
                        .into_string()
                        .map_err(|_| usage("--storage-class must be UTF-8".to_owned()))?;
                    assign(&mut storage_class, StableId::new(value)?, &flag)?;
                }
                "--revision" => {
                    let value = value
                        .into_string()
                        .map_err(|_| usage("--revision must be UTF-8".to_owned()))?;
                    if value.trim().is_empty() || value.len() > 200 {
                        return Err(usage(
                            "--revision must contain 1 through 200 bytes".to_owned(),
                        ));
                    }
                    assign(&mut revision, value, &flag)?;
                }
                _ => return Err(usage(format!("unknown option `{flag}`"))),
            }
        }
        if baseline.is_some() != baseline_digest.is_some() {
            return Err(usage(
                "--baseline and --accept-baseline-sha256 must be supplied together".to_owned(),
            ));
        }
        Ok(Self {
            mode,
            daemon: required(daemon, "--daemon")?,
            profile: required(profile, "--profile")?,
            workloads: required(workloads, "--workloads")?,
            baseline,
            baseline_digest,
            evidence: required(evidence, "--evidence")?,
            storage_class: required(storage_class, "--storage-class")?,
            revision: required(revision, "--revision")?,
        })
    }

    /// Executes the selected campaign and publishes its exact evidence bundle.
    ///
    /// # Errors
    ///
    /// Returns [`OperatorError`] for bounded input loading, host probing, subject execution,
    /// evaluation, executable identity, or atomic evidence failures.
    pub fn execute(self) -> Result<PublishedEvidence, OperatorError> {
        let profile_document = read_document(&self.profile, MAX_PROFILE_BYTES, "read profile")?;
        let workload_document =
            read_document(&self.workloads, MAX_WORKLOAD_BYTES, "read workload catalog")?;
        let baseline_document = self
            .baseline
            .as_deref()
            .map(|path| read_document(path, MAX_BASELINE_BYTES, "read accepted baseline"))
            .transpose()?;
        if let (Some(document), Some(expected)) = (&baseline_document, &self.baseline_digest) {
            let observed = Sha256Digest::of_bytes(document.as_bytes());
            if &observed != expected {
                return Err(OperatorError::BaselineDigestMismatch {
                    expected: expected.to_string(),
                    observed: observed.to_string(),
                });
            }
        }
        let limits = DatasetLimits::production_defaults();
        let dataset =
            QualificationDataset::from_json(&profile_document, &workload_document, limits)?;
        let baseline = baseline_document
            .as_deref()
            .map(|document| baseline_from_json(document, limits))
            .transpose()?;
        let machine = MachineProbe::observe(self.storage_class)?;
        let runner_executable = std::env::current_exe().map_err(|source| {
            OperatorError::io("resolve qualification runner", Path::new("peritus-h3"), source)
        })?;
        let runner = RunnerDescriptor::new(
            StableId::new("peritus-h3")?,
            env!("CARGO_PKG_VERSION"),
            sha256_file(&runner_executable).map_err(|source| {
                OperatorError::io("digest qualification runner", &runner_executable, source)
            })?,
        )?;
        let mut request = CampaignRequest::new(
            dataset,
            self.daemon,
            self.revision,
            run_id()?,
            runner,
            machine,
            self.mode,
        );
        if let Some(baseline) = baseline {
            request = request.baseline(baseline);
        }
        let outcome = CampaignCoordinator::run(request)?;
        Ok(CampaignEvidenceWriter::publish(
            &self.evidence,
            &profile_document,
            &workload_document,
            baseline_document.as_deref(),
            &runner_executable,
            &outcome,
        )?)
    }
}

fn read_document(
    path: &Path,
    limit: u64,
    operation: &'static str,
) -> Result<String, OperatorError> {
    let metadata =
        fs::metadata(path).map_err(|source| OperatorError::io(operation, path, source))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(usage(format!(
            "{} must be a regular file no larger than {limit} bytes",
            path.display()
        )));
    }
    fs::read_to_string(path).map_err(|source| OperatorError::io(operation, path, source))
}

fn run_id() -> Result<StableId, OperatorError> {
    let micros = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros();
    Ok(StableId::new(format!("h3-{micros}-{}", std::process::id()))?)
}

fn text(value: Option<OsString>, name: &str) -> Result<String, OperatorError> {
    value
        .ok_or_else(|| usage(format!("missing {name}")))?
        .into_string()
        .map_err(|_| usage(format!("{name} must be UTF-8")))
}

fn assign<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), OperatorError> {
    if slot.replace(value).is_some() {
        Err(usage(format!("{flag} may be supplied only once")))
    } else {
        Ok(())
    }
}

fn required<T>(value: Option<T>, flag: &str) -> Result<T, OperatorError> {
    value.ok_or_else(|| usage(format!("missing required {flag}")))
}

const fn usage(message: String) -> OperatorError {
    OperatorError::Usage(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_command_is_parsed_without_environment_state() {
        let options = OperatorOptions::parse(arguments()).expect("options");
        assert_eq!(options.mode, CampaignMode::Full);
        assert_eq!(options.daemon, PathBuf::from("peritusd"));
        assert_eq!(options.storage_class.as_str(), "nvme-gen4");
    }

    #[test]
    fn duplicate_and_unknown_options_are_rejected() {
        let mut duplicate = arguments();
        duplicate.extend([OsString::from("--daemon"), OsString::from("other")]);
        assert!(matches!(
            OperatorOptions::parse(duplicate),
            Err(OperatorError::Usage(message)) if message.contains("only once")
        ));
        assert!(matches!(
            OperatorOptions::parse(["load", "--unknown", "value"].map(OsString::from)),
            Err(OperatorError::Usage(message)) if message.contains("unknown option")
        ));
    }

    #[test]
    fn help_is_not_reported_as_a_failure() {
        assert!(matches!(
            OperatorOptions::parse([OsString::from("--help")]),
            Err(OperatorError::HelpRequested)
        ));
    }

    #[test]
    fn baseline_bytes_must_match_the_explicitly_accepted_digest() {
        let temporary = tempfile::tempdir().expect("temporary");
        let profile = temporary.path().join("profile.json");
        let workloads = temporary.path().join("workloads.json");
        let baseline = temporary.path().join("baseline.json");
        fs::write(&profile, "{}").expect("profile");
        fs::write(&workloads, "{}").expect("workloads");
        fs::write(&baseline, "observed").expect("baseline");
        let options = OperatorOptions {
            mode: CampaignMode::Load,
            daemon: PathBuf::from("peritusd"),
            profile,
            workloads,
            baseline: Some(baseline),
            baseline_digest: Some(Sha256Digest::of_bytes(b"different")),
            evidence: temporary.path().join("evidence"),
            storage_class: StableId::new("storage").expect("storage"),
            revision: "revision".to_owned(),
        };
        assert!(matches!(options.execute(), Err(OperatorError::BaselineDigestMismatch { .. })));
    }

    fn arguments() -> Vec<OsString> {
        [
            "full",
            "--daemon",
            "peritusd",
            "--profile",
            "profile.json",
            "--workloads",
            "workloads.json",
            "--baseline",
            "baseline.json",
            "--accept-baseline-sha256",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "--evidence",
            "evidence",
            "--storage-class",
            "nvme-gen4",
            "--revision",
            "revision",
        ]
        .map(OsString::from)
        .to_vec()
    }
}
