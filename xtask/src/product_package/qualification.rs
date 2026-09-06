//! Native H2 execution over freshly built or same-run prepared package artifacts.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::{
    H2_SHARD_COUNT, assemble, build_debug_binaries, build_h2_binaries, debug_binary, host_os,
    host_version, package_path, qualification_report, qualification_run_root, require_success,
};
use crate::XtaskError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QualificationInput {
    Build,
    Prepared,
}

/// Assembles and archives checked debug artifacts without rebuilding them or invoking Cargo.
pub(crate) fn prepare(root: &Path) -> Result<PathBuf, XtaskError> {
    super::native_artifacts::restore(root)?;
    let status = Command::new(debug_binary(root, "peritus-package"))
        .current_dir(root)
        .arg("--use-debug-artifacts")
        .status()
        .map_err(|error| XtaskError::io("assemble prepared H2 package in", root, error))?;
    require_success(status.success(), "prepared H2 package assembly failed")?;
    require_prepared_inputs(root)?;
    let archive = root.join("target/h2-prepared.tar");
    let status = Command::new("tar")
        .current_dir(root)
        .arg("-cf")
        .arg(&archive)
        .arg(package_path(Path::new(".")))
        .arg(debug_binary(Path::new("."), "peritus-h2"))
        .arg(debug_binary(Path::new("."), "peritus-h2-controller"))
        .status()
        .map_err(|error| XtaskError::io("archive prepared H2 package in", root, error))?;
    require_success(status.success(), "prepared H2 archive creation failed")?;
    Ok(archive)
}

/// Restores the platform-specific artifact downloaded by this workflow run.
pub(crate) fn restore(root: &Path) -> Result<PathBuf, XtaskError> {
    let archive = root.join("target/prepared-h2/h2-prepared.tar");
    let status =
        Command::new("tar").current_dir(root).arg("-xf").arg(&archive).status().map_err(
            |error| XtaskError::io("restore same-run prepared H2 archive", &archive, error),
        )?;
    require_success(status.success(), "prepared H2 archive restoration failed")?;
    require_prepared_inputs(root)?;
    Ok(package_path(root))
}

pub(crate) fn qualify(root: &Path) -> Result<PathBuf, XtaskError> {
    let qualification = prepare_qualification(root, QualificationInput::Build)?;
    let report = qualification.run_root.join("report.json");
    let status = h2_command(root, &qualification, &report)?.status().map_err(|error| {
        XtaskError::io("run complete native H2 qualification from", &qualification.package, error)
    })?;
    if !report.is_file() {
        return Err(XtaskError::metadata(format!(
            "native H2 qualification exited with {status} without retaining its report at {}",
            report.display()
        )));
    }
    if !status.success() {
        let reasons = qualification_report::not_ready_reasons(&report)?;
        return Err(XtaskError::metadata(format!(
            "native H2 qualification did not reach Ready; retained report: {}; reasons: {reasons}",
            report.display()
        )));
    }
    Ok(report)
}

pub(crate) fn qualify_shard(
    root: &Path,
    index: usize,
    input: QualificationInput,
) -> Result<PathBuf, XtaskError> {
    if index >= H2_SHARD_COUNT {
        return Err(XtaskError::invocation(format!(
            "H2 qualification shard index must be from 0 through {}",
            H2_SHARD_COUNT - 1
        )));
    }
    let qualification = prepare_qualification(root, input)?;
    let reports = qualification.run_root.join(format!("shard-{index}"));
    let status = h2_command(root, &qualification, &reports)?
        .args(["--shard", &index.to_string()])
        .status()
        .map_err(|error| {
            XtaskError::io("run native H2 qualification shard from", &qualification.package, error)
        })?;
    let report_count = fs::read_dir(&reports)
        .map_err(|error| XtaskError::io("read H2 shard reports from", &reports, error))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .count();
    if report_count != 1 {
        return Err(XtaskError::metadata(format!(
            "native H2 shard {index} retained {report_count} reports instead of 1 at {}",
            reports.display()
        )));
    }
    require_success(status.success(), "native H2 qualification shard did not reach Ready")?;
    Ok(reports)
}

struct PreparedQualification {
    package: PathBuf,
    run_root: PathBuf,
    scratch: PathBuf,
    artifacts: PathBuf,
}

fn prepare_qualification(
    root: &Path,
    input: QualificationInput,
) -> Result<PreparedQualification, XtaskError> {
    if input == QualificationInput::Build {
        build_debug_binaries(root)?;
        build_h2_binaries(root)?;
        assemble(root, true)?;
    }
    require_prepared_inputs(root)?;
    let package = package_path(root);
    let run_root = qualification_run_root(root)?;
    let scratch = run_root.join("scratch");
    let artifacts = run_root.join("artifacts");
    fs::create_dir_all(&scratch).map_err(|error| {
        XtaskError::io("create H2 qualification scratch root at", &scratch, error)
    })?;
    fs::create_dir_all(&artifacts).map_err(|error| {
        XtaskError::io("create H2 qualification artifact root at", &artifacts, error)
    })?;
    Ok(PreparedQualification { package, run_root, scratch, artifacts })
}

fn require_prepared_inputs(root: &Path) -> Result<(), XtaskError> {
    for path in [
        package_path(root).join("manifest.toml"),
        debug_binary(root, "peritus-h2"),
        debug_binary(root, "peritus-h2-controller"),
    ] {
        if !path.is_file() {
            return Err(XtaskError::metadata(format!(
                "required prepared H2 artifact is missing: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn h2_command(
    root: &Path,
    qualification: &PreparedQualification,
    report: &Path,
) -> Result<Command, XtaskError> {
    let mut command = Command::new(debug_binary(root, "peritus-h2"));
    command
        .current_dir(root)
        .args(["--controller"])
        .arg(debug_binary(root, "peritus-h2-controller"))
        .args(["--package"])
        .arg(&qualification.package)
        .args(["--manifest"])
        .arg(qualification.package.join("manifest.toml"))
        .args(["--scratch"])
        .arg(&qualification.scratch)
        .args(["--artifacts"])
        .arg(&qualification.artifacts)
        .args(["--report"])
        .arg(report)
        .args(["--platform", host_os(), "--architecture", std::env::consts::ARCH])
        .args(["--version", &host_version::detect(host_os())?]);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{error::ErrorCode, product_package::SmokeSubject};

    #[test]
    fn prepared_mode_rejects_missing_artifacts_without_building_or_creating_a_run() {
        let subject = SmokeSubject::new().expect("subject");
        let root = subject.path();
        let error =
            qualify_shard(root, 0, QualificationInput::Prepared).expect_err("missing inputs");
        assert_eq!(error.code(), ErrorCode::Metadata);
        assert!(error.render().contains("required prepared H2 artifact is missing"));
        assert!(!root.join("target").exists());
        for input in [QualificationInput::Build, QualificationInput::Prepared] {
            let error = qualify_shard(root, H2_SHARD_COUNT, input).expect_err("invalid shard");
            assert_eq!(error.code(), ErrorCode::Invocation);
            assert!(!root.join("target").exists());
        }
    }

    #[test]
    fn each_prepared_input_is_required() {
        let subject = SmokeSubject::new().expect("subject");
        let root = subject.path();
        let inputs = [
            package_path(root).join("manifest.toml"),
            debug_binary(root, "peritus-h2"),
            debug_binary(root, "peritus-h2-controller"),
        ];
        for path in &inputs {
            fs::create_dir_all(path.parent().expect("parent")).expect("input directory");
            fs::write(path, b"input presence fixture").expect("input");
        }
        require_prepared_inputs(root).expect("complete input inventory");
        for path in &inputs {
            fs::remove_file(path).expect("remove exact fixture input");
            assert!(require_prepared_inputs(root).is_err());
            fs::write(path, b"input presence fixture").expect("restore input");
        }
    }
}
