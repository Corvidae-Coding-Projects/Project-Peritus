use crate::api_contract;
use crate::architecture;
use crate::error::XtaskError;
use crate::metadata;
use crate::product_package::qualification::{self, QualificationInput};
use crate::reproducibility;
use crate::source;
use crate::toolchain;
use crate::trust;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const HELP: &str = "Peritus workspace policy tool

Usage: cargo xtask <command>

Commands:
  all                    Run all locally executable repository policy checks
  architecture-check     Validate packages, layers, ownership, and source layout
  docs-check             Validate maintained Markdown structure and local links
  format-check           Check every workspace package without one oversized command line
  ordinary-api-check     Validate formal APIs callable from ordinary safe Rust
  source-layout-check    Validate module names, crate roots, and source budgets
  reproducibility-check  Validate toolchain pins, lock policy, and immutable CI inputs
  toolchain-check        Probe installed Rust, Verus, vstd metadata, and bundled Z3
  verify-trust           Reject trusted Verus constructs outside approved roots
  ci-shard OPERATION SHARD Run one reviewed package shard for hosted Rust or Verus CI
  product-package        Build a host-native checked Peritus package in dist/
  product-install        Build and install Peritus for the current user
  product-package-smoke  Qualify native install, repeat launch, upgrade, and uninstall
  product-native-qualification Run and retain all 18 native H2 package scenarios
  product-native-qualification-shard INDEX Run one of 18 single-scenario H2 shards
  product-native-qualification-prepare Assemble previously built native H2 artifacts
  product-native-qualification-restore Restore this platform's same-run native H2 archive
  product-native-qualification-prepared-shard INDEX Qualify same-run artifacts without Cargo
  release-bootstrap-smoke Qualify the public download, checksum, and install entry point
  release-bootstrap-prepared-smoke Qualify the public installer using same-run native artifacts
  release-bootstrap-staged-smoke Qualify the public installer using the exact staged archive
  release-qualification-prepare Restore the staged release archive and retain H2 inputs
  release-rebuild-record Retain actual source, environment, and native assembly observations
  release-rebuild-compare Require a compatible byte-identical independent native rebuild
  release-daemon-library Compile and retain a same-run native daemon library tree
  release-daemon-binary Compile the final daemon from its verified same-role library tree
  release-cli-binary     Compile the CLI from its verified same-role daemon libraries
  release-windows-binary Compile a selected native Windows x86-64 binary with pinned C tooling
  release-windows-sqlite-check Compare two fresh native Windows bundled SQLite compilations
  release-create         Validate a tag and create its retained draft GitHub release
  release-package-stage Build, archive, checksum, and record this host's native package
  release-package-assemble Assemble a native package from separately built release binaries
  release-stage          Complete and validate the release draft without publishing it
  distro-image           Build a pinned Debian or Fedora package-builder Docker image
  distro-image-save      Retain the exact builder image for same-run package jobs
  distro-image-restore   Restore the same-run builder image without rebuilding it
  distro-build           Build real source and binary distribution packages offline
  distro-compile         Retain a candidate-bound native distribution compilation
  distro-compile-checks  Retain native RPM check compilation for final recipe execution
  distro-package-compiled  Run full recipes and tests on the same-run compiled tree
  distro-sign            Sign distribution packages using the local OpenPGP agent
  distro-sign-ci         Sign with explicitly provisioned protected-environment CI secrets
  distro-verify          Verify package signatures and disposable install/remove lifecycle
  distro-upload          Upload a verified package set to the exact existing release draft
  distro-test            Run distribution package tooling regression tests
  help                   Print this help
";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Command {
    All,
    Architecture,
    Documentation,
    Formatting,
    OrdinaryApi,
    SourceLayout,
    Reproducibility,
    Toolchain,
    Trust,
    CiShard { operation: crate::ci_shard::Operation, shard: &'static str },
    ProductPackage,
    ProductInstall,
    ProductPackageSmoke,
    ProductNativeQualification,
    ProductNativeQualificationPrepare,
    ProductNativeQualificationRestore,
    ProductNativeQualificationShard { index: usize, input: QualificationInput },
    ReleaseBootstrapSmoke { input: QualificationInput },
    ReleaseCreate,
    ReleasePackageStage,
    ReleasePackageAssemble,
    ReleaseQualificationPrepare,
    ReleaseRebuild { operation: crate::release::rebuild::Operation },
    ReleaseStage,
    Distro { operation: crate::distro::Operation },
    Help,
}

/// Runs `xtask` using process arguments, the current directory, and standard output.
///
/// # Errors
///
/// Returns a typed error when invocation, filesystem access, Cargo metadata, or a policy check
/// fails. Diagnostics include a stable category and recovery guidance.
pub fn run_from_env() -> Result<(), XtaskError> {
    let current = env::current_dir().map_err(|error| {
        XtaskError::io("determine current directory from", Path::new("."), error)
    })?;
    let root = discover_workspace_root(&current)?;
    execute(env::args_os().skip(1), &root, &mut io::stdout().lock())
}

fn discover_workspace_root(start: &Path) -> Result<PathBuf, XtaskError> {
    let canonical = fs::canonicalize(start)
        .map_err(|error| XtaskError::io("canonicalize workspace search path", start, error))?;
    let mut candidate = child_process_path(canonical);
    loop {
        if candidate.join("Cargo.toml").is_file() && candidate.join("architecture.toml").is_file() {
            return Ok(candidate);
        }
        if !candidate.pop() {
            return Err(XtaskError::metadata(format!(
                "could not locate the Peritus workspace above {}",
                start.display()
            )));
        }
    }
}

#[cfg(not(windows))]
const fn child_process_path(path: PathBuf) -> PathBuf {
    path
}

#[cfg(windows)]
fn child_process_path(path: PathBuf) -> PathBuf {
    use std::path::{Component, Prefix};

    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return path;
    };
    let mut ordinary = match prefix.kind() {
        Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", char::from(drive))),
        Prefix::VerbatimUNC(server, share) => {
            let mut value = OsString::from(r"\\");
            value.push(server);
            value.push(r"\");
            value.push(share);
            PathBuf::from(value)
        }
        _ => return path,
    };
    for component in components {
        if !matches!(component, Component::RootDir) {
            ordinary.push(component.as_os_str());
        }
    }
    ordinary
}

pub(crate) fn execute(
    args: impl IntoIterator<Item = OsString>,
    root: &Path,
    output: &mut dyn Write,
) -> Result<(), XtaskError> {
    let command = parse(args)?;
    if command == Command::Help {
        write_output(output, HELP)?;
        return Ok(());
    }

    match command {
        Command::All => execute_all(root, output)?,
        Command::Architecture => {
            let policy = metadata::architecture_policy(root)?;
            let (packages, files) = architecture::check(root, &policy)?;
            write_output(
                output,
                &format!(
                    "architecture-check passed: {packages} package(s), {files} source file(s)\n"
                ),
            )?;
        }
        Command::Documentation => {
            let files = crate::documentation::check(root)?;
            write_output(output, &format!("docs-check passed: {files} documentation file(s)\n"))?;
        }
        Command::Formatting => {
            let packages = crate::formatting::check(root)?;
            write_output(
                output,
                &format!("format-check passed: {packages} workspace package(s)\n"),
            )?;
        }
        Command::OrdinaryApi => {
            let policy = metadata::architecture_policy(root)?;
            let report = api_contract::check(root, &policy)?;
            write_output(
                output,
                &format!(
                    "ordinary-api-check passed: {} formal-boundary file(s), {} ordinary-safe executable entry point(s)\n",
                    report.files, report.executable_entry_points
                ),
            )?;
        }
        Command::SourceLayout => {
            let policy = metadata::architecture_policy(root)?;
            let cargo = metadata::cargo_metadata(root)?;
            let files = source::check(root, &policy, &cargo)?;
            write_output(output, &format!("source-layout-check passed: {files} source file(s)\n"))?;
        }
        Command::Reproducibility => {
            let tools = metadata::toolchain_policy(root)?;
            let actions = reproducibility::check(root, &tools)?;
            write_output(
                output,
                &format!("reproducibility-check passed: {actions} immutable action reference(s)\n"),
            )?;
        }
        Command::Toolchain => {
            let tools = metadata::toolchain_policy(root)?;
            toolchain::check(root, &tools)?;
            write_output(
                output,
                "toolchain-check passed: Rust, Verus, vstd metadata, and bundled Z3 match\n",
            )?;
        }
        Command::Trust => {
            let policy = metadata::architecture_policy(root)?;
            let files = trust::check(root, &policy)?;
            write_output(
                output,
                &format!("verify-trust passed: {files} source file(s) scanned\n"),
            )?;
        }
        Command::CiShard { operation, shard } => {
            let packages = crate::ci_shard::run(root, operation, shard)?;
            write_output(
                output,
                &format!("CI shard `{shard}` passed {operation:?} for {packages} package(s)\n"),
            )?;
        }
        Command::ProductPackage
        | Command::ProductInstall
        | Command::ProductPackageSmoke
        | Command::ProductNativeQualification
        | Command::ProductNativeQualificationPrepare
        | Command::ProductNativeQualificationRestore
        | Command::ProductNativeQualificationShard { .. } => {
            execute_product(command, root, output)?;
        }
        Command::ReleaseCreate => crate::release::create(root)?,
        Command::ReleaseBootstrapSmoke { input } => {
            execute_release_bootstrap(root, input, output)?;
        }
        Command::ReleasePackageStage => crate::release::package_stage(root)?,
        Command::ReleasePackageAssemble => crate::release::package_assemble(root)?,
        Command::ReleaseQualificationPrepare => crate::release::qualification_prepare(root)?,
        Command::ReleaseRebuild { operation } => crate::release::rebuild::run(root, operation)?,
        Command::ReleaseStage => execute_release_stage(root, output)?,
        Command::Distro { operation } => crate::distro::run(root, operation)?,
        Command::Help => {}
    }
    Ok(())
}

fn execute_release_stage(root: &Path, output: &mut dyn Write) -> Result<(), XtaskError> {
    crate::release::stage_draft(root)?;
    write_output(
        output,
        "Release draft is complete and remains unpublished; H4 approval and separate publication authorization are required.\n",
    )
}

fn execute_release_bootstrap(
    root: &Path,
    input: QualificationInput,
    output: &mut dyn Write,
) -> Result<(), XtaskError> {
    let package = crate::release::bootstrap_smoke(root, input)?;
    write_output(output, &format!("public release bootstrap passed: {}\n", package.display()))
}

fn execute_all(root: &Path, output: &mut dyn Write) -> Result<(), XtaskError> {
    let policy = metadata::architecture_policy(root)?;
    let (packages, files) = architecture::check(root, &policy)?;
    let api = api_contract::check(root, &policy)?;
    let documentation = crate::documentation::check(root)?;
    let trust_files = trust::check_local(root, &policy)?;
    let tools = metadata::toolchain_policy(root)?;
    let actions = reproducibility::check(root, &tools)?;
    write_output(
        output,
        &format!(
            "all checks passed: {packages} package(s), {files} source file(s), \
             {} formal-boundary file(s), {} ordinary-safe executable entry point(s), \
             {trust_files} trust-scanned file(s), {documentation} documentation file(s), \
             {actions} pinned action(s)\n",
            api.files, api.executable_entry_points
        ),
    )
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, XtaskError> {
    let mut args = args.into_iter();
    let first = args.next();
    if let Some(command) = shard_args::parse(first.as_ref(), &mut args)? {
        return Ok(command);
    }
    if args.next().is_some() {
        return Err(XtaskError::invocation(
            "expected exactly one command; run `cargo xtask help` for the supported interface",
        ));
    }
    let name = first.as_deref().and_then(|value| value.to_str());
    if let Some(operation) = name.and_then(crate::release::rebuild::Operation::parse) {
        return Ok(Command::ReleaseRebuild { operation });
    }
    match name {
        Some("all") => Ok(Command::All),
        Some("architecture-check") => Ok(Command::Architecture),
        Some("docs-check") => Ok(Command::Documentation),
        Some("format-check") => Ok(Command::Formatting),
        Some("ordinary-api-check") => Ok(Command::OrdinaryApi),
        Some("source-layout-check") => Ok(Command::SourceLayout),
        Some("reproducibility-check") => Ok(Command::Reproducibility),
        Some("toolchain-check") => Ok(Command::Toolchain),
        Some("verify-trust") => Ok(Command::Trust),
        Some("product-package") => Ok(Command::ProductPackage),
        Some("product-install") => Ok(Command::ProductInstall),
        Some("product-package-smoke") => Ok(Command::ProductPackageSmoke),
        Some("product-native-qualification") => Ok(Command::ProductNativeQualification),
        Some("product-native-qualification-prepare") => {
            Ok(Command::ProductNativeQualificationPrepare)
        }
        Some("product-native-qualification-restore") => {
            Ok(Command::ProductNativeQualificationRestore)
        }
        Some("release-bootstrap-smoke") => {
            Ok(Command::ReleaseBootstrapSmoke { input: QualificationInput::Build })
        }
        Some("release-bootstrap-prepared-smoke") => {
            Ok(Command::ReleaseBootstrapSmoke { input: QualificationInput::Prepared })
        }
        Some("release-create") => Ok(Command::ReleaseCreate),
        Some("release-bootstrap-staged-smoke") => {
            Ok(Command::ReleaseBootstrapSmoke { input: QualificationInput::Release })
        }
        Some("release-qualification-prepare") => Ok(Command::ReleaseQualificationPrepare),
        Some("release-package-stage") => Ok(Command::ReleasePackageStage),
        Some("release-package-assemble") => Ok(Command::ReleasePackageAssemble),
        Some("release-stage") => Ok(Command::ReleaseStage),
        Some("distro-image") => Ok(Command::Distro { operation: crate::distro::Operation::Image }),
        Some("distro-image-save") => {
            Ok(Command::Distro { operation: crate::distro::Operation::ImageSave })
        }
        Some("distro-image-restore") => {
            Ok(Command::Distro { operation: crate::distro::Operation::ImageRestore })
        }
        Some("distro-build") => Ok(Command::Distro { operation: crate::distro::Operation::Build }),
        Some("distro-compile") => {
            Ok(Command::Distro { operation: crate::distro::Operation::Compile })
        }
        Some("distro-compile-checks") => {
            Ok(Command::Distro { operation: crate::distro::Operation::CompileChecks })
        }
        Some("distro-package-compiled") => {
            Ok(Command::Distro { operation: crate::distro::Operation::PackageCompiled })
        }
        Some("distro-sign") => Ok(Command::Distro { operation: crate::distro::Operation::Sign }),
        Some("distro-sign-ci") => {
            Ok(Command::Distro { operation: crate::distro::Operation::SignCi })
        }
        Some("distro-verify") => {
            Ok(Command::Distro { operation: crate::distro::Operation::Verify })
        }
        Some("distro-upload") => {
            Ok(Command::Distro { operation: crate::distro::Operation::Upload })
        }
        Some("distro-test") => Ok(Command::Distro { operation: crate::distro::Operation::Test }),
        Some("help" | "-h" | "--help") | None => Ok(Command::Help),
        Some(command) => Err(XtaskError::invocation(format!(
            "unknown command `{command}`; run `cargo xtask help` for the supported interface"
        ))),
    }
}

fn write_output(output: &mut dyn Write, message: &str) -> Result<(), XtaskError> {
    output
        .write_all(message.as_bytes())
        .map_err(|error| XtaskError::io("write", Path::new("<stdout>"), error))
}

mod product;
mod shard_args;
use product::execute_product;
#[cfg(test)]
mod tests;
