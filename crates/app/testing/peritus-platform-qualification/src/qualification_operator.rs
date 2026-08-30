//! One-command native H2 qualification operator.

use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use crate::{
    Architecture, FreshSubjectRunner, NativeControllerLimits, NativePlatformFactory,
    PackageManifest, Platform, PlatformVersion, QualificationReport, QualificationTarget,
    ReadinessVerdict,
};

const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

/// Terminal status of a successfully executed native H2 campaign.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum H2OperatorStatus {
    /// All 18 scenarios passed with complete cleanup.
    Ready,
    /// The campaign completed and retained an honest not-ready report.
    NotReady,
}

/// Parses the operator command, runs all fresh native subjects, and atomically retains the report.
///
/// # Errors
///
/// Returns syntax, filesystem, manifest, controller, scenario, or report-publication failures.
pub fn run_from_env() -> Result<H2OperatorStatus, Box<dyn std::error::Error>> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    run(&arguments)
}

fn run(arguments: &[OsString]) -> Result<H2OperatorStatus, Box<dyn std::error::Error>> {
    let options = Options::parse(arguments)?;
    let manifest = read_manifest(&options.manifest)?;
    let target = QualificationTarget::new(options.platform, options.architecture, options.version);
    let mut factory = NativePlatformFactory::new(
        &options.controller,
        &options.package,
        &options.scratch,
        &options.artifacts,
        NativeControllerLimits::default(),
    )?;
    let run = FreshSubjectRunner.run(&mut factory, target, &manifest)?;
    let report = QualificationReport::evaluate(run);
    let bytes = super::report_json::render(&report)?;
    publish_report(&options.report, &bytes)?;
    Ok(match report.verdict() {
        ReadinessVerdict::Ready(_) => H2OperatorStatus::Ready,
        ReadinessVerdict::NotReady(_) => H2OperatorStatus::NotReady,
    })
}

struct Options {
    controller: PathBuf,
    package: PathBuf,
    manifest: PathBuf,
    scratch: PathBuf,
    artifacts: PathBuf,
    report: PathBuf,
    platform: Platform,
    architecture: Architecture,
    version: PlatformVersion,
}

impl Options {
    fn parse(arguments: &[OsString]) -> Result<Self, Box<dyn std::error::Error>> {
        if arguments.len() != 18 {
            return Err(usage().into());
        }
        let mut controller = None;
        let mut package = None;
        let mut manifest = None;
        let mut scratch = None;
        let mut artifacts = None;
        let mut report = None;
        let mut platform = None;
        let mut architecture = None;
        let mut version = None;
        for pair in arguments.chunks_exact(2) {
            let name = pair[0].to_str().ok_or_else(usage)?;
            match name {
                "--controller" => set_once(&mut controller, PathBuf::from(&pair[1]))?,
                "--package" => set_once(&mut package, PathBuf::from(&pair[1]))?,
                "--manifest" => set_once(&mut manifest, PathBuf::from(&pair[1]))?,
                "--scratch" => set_once(&mut scratch, PathBuf::from(&pair[1]))?,
                "--artifacts" => set_once(&mut artifacts, PathBuf::from(&pair[1]))?,
                "--report" => set_once(&mut report, PathBuf::from(&pair[1]))?,
                "--platform" => set_once(&mut platform, parse_platform(&pair[1])?)?,
                "--architecture" => {
                    set_once(&mut architecture, parse_architecture(&pair[1])?)?;
                }
                "--version" => set_once(&mut version, parse_version(&pair[1])?)?,
                _ => return Err(usage().into()),
            }
        }
        Ok(Self {
            controller: controller.ok_or_else(usage)?,
            package: package.ok_or_else(usage)?,
            manifest: manifest.ok_or_else(usage)?,
            scratch: scratch.ok_or_else(usage)?,
            artifacts: artifacts.ok_or_else(usage)?,
            report: report.ok_or_else(usage)?,
            platform: platform.ok_or_else(usage)?,
            architecture: architecture.ok_or_else(usage)?,
            version: version.ok_or_else(usage)?,
        })
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), &'static str> {
    if slot.replace(value).is_some() {
        return Err(usage());
    }
    Ok(())
}

fn parse_platform(value: &OsString) -> Result<Platform, &'static str> {
    match value.to_str() {
        Some("linux") => Ok(Platform::Linux),
        Some("macos") => Ok(Platform::Macos),
        Some("windows") => Ok(Platform::Windows),
        _ => Err(usage()),
    }
}

fn parse_architecture(value: &OsString) -> Result<Architecture, &'static str> {
    match value.to_str() {
        Some("x86_64") => Ok(Architecture::X86_64),
        Some("aarch64") => Ok(Architecture::Aarch64),
        _ => Err(usage()),
    }
}

fn parse_version(value: &OsString) -> Result<PlatformVersion, &'static str> {
    let text = value.to_str().ok_or_else(usage)?;
    let fields = text.split('.').collect::<Vec<_>>();
    if !(3..=4).contains(&fields.len()) {
        return Err(usage());
    }
    let major = fields[0].parse().map_err(|_| usage())?;
    let minor = fields[1].parse().map_err(|_| usage())?;
    let patch = fields[2].parse().map_err(|_| usage())?;
    let build = fields.get(3).map_or(Ok(0), |field| field.parse()).map_err(|_| usage())?;
    Ok(PlatformVersion::new(major, minor, patch, build))
}

fn read_manifest(path: &Path) -> Result<PackageManifest, Box<dyn std::error::Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        return Err("manifest must be a bounded regular file".into());
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    File::open(path)?.take(MAX_MANIFEST_BYTES + 1).read_to_end(&mut bytes)?;
    Ok(PackageManifest::parse(&bytes)?)
}

fn publish_report(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        return Err("H2 report path already exists".into());
    }
    let parent = path.parent().ok_or("H2 report path has no parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(path)?;
    Ok(())
}

const fn usage() -> &'static str {
    "usage: peritus-h2 --controller PATH --package DIR --manifest FILE --scratch DIR --artifacts DIR --report FILE --platform linux|macos|windows --architecture x86_64|aarch64 --version MAJOR.MINOR.PATCH[.BUILD]"
}
