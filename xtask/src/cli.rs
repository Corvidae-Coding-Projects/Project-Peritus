use crate::api_contract;
use crate::architecture;
use crate::error::XtaskError;
use crate::metadata;
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
  release-bootstrap-smoke Qualify the public download, checksum, and install entry point
  release-create         Validate a tag and create its retained draft GitHub release
  release-package-stage Build, archive, checksum, and record this host's native package
  release-package-assemble Assemble a native package from separately built release binaries
  release-publish        Publish a draft only after every native package job passes
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
    ProductNativeQualificationShard { index: usize },
    ReleaseBootstrapSmoke,
    ReleaseCreate,
    ReleasePackageStage,
    ReleasePackageAssemble,
    ReleasePublish,
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
        | Command::ProductNativeQualificationShard { .. } => {
            execute_product(command, root, output)?;
        }
        Command::ReleaseCreate => crate::release::create(root)?,
        Command::ReleaseBootstrapSmoke => {
            let package = crate::release::bootstrap_smoke(root)?;
            write_output(
                output,
                &format!("public release bootstrap passed: {}\n", package.display()),
            )?;
        }
        Command::ReleasePackageStage => crate::release::package_stage(root)?,
        Command::ReleasePackageAssemble => crate::release::package_assemble(root)?,
        Command::ReleasePublish => crate::release::publish()?,
        Command::Help => {}
    }
    Ok(())
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

fn execute_product(
    command: Command,
    root: &Path,
    output: &mut dyn Write,
) -> Result<(), XtaskError> {
    let (package, message) = match command {
        Command::ProductPackage => (crate::product_package::build(root)?, "product package ready"),
        Command::ProductInstall => {
            (crate::product_package::install(root)?, "product installed; start it with `peritus`")
        }
        Command::ProductPackageSmoke => {
            (crate::product_package::smoke(root)?, "native product lifecycle passed")
        }
        Command::ProductNativeQualification => (
            crate::product_package::qualify(root)?,
            "native H2 qualification passed; retained report",
        ),
        Command::ProductNativeQualificationShard { index } => (
            crate::product_package::qualify_shard(root, index)?,
            "native H2 qualification shard passed; retained reports",
        ),
        _ => return Err(XtaskError::invocation("command is not a product packaging operation")),
    };
    write_output(output, &format!("{message}: {}\n", package.display()))
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
    match first.as_deref().and_then(|value| value.to_str()) {
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
        Some("release-bootstrap-smoke") => Ok(Command::ReleaseBootstrapSmoke),
        Some("release-create") => Ok(Command::ReleaseCreate),
        Some("release-package-stage") => Ok(Command::ReleasePackageStage),
        Some("release-package-assemble") => Ok(Command::ReleasePackageAssemble),
        Some("release-publish") => Ok(Command::ReleasePublish),
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

#[cfg(test)]
mod tests {
    use super::{Command, discover_workspace_root, parse};
    use crate::error::ErrorCode;
    use std::ffi::OsString;
    use std::fs;

    #[test]
    fn empty_arguments_show_help() {
        assert_eq!(parse(Vec::<OsString>::new()).expect("empty args are valid"), Command::Help);
    }

    #[test]
    fn unknown_command_has_stable_typed_error() {
        let error = parse([OsString::from("unknown")]).expect_err("unknown command must fail");
        assert_eq!(error.code(), ErrorCode::Invocation);
        assert!(error.render().contains("cargo xtask help"));
    }

    #[test]
    fn native_qualification_command_is_first_class() {
        assert_eq!(
            parse([OsString::from("product-native-qualification")])
                .expect("native qualification command must parse"),
            Command::ProductNativeQualification
        );
    }

    #[test]
    fn workspace_root_is_discovered_from_the_xtask_directory() {
        let crate_root = fs::canonicalize(env!("CARGO_MANIFEST_DIR"))
            .expect("xtask manifest directory must be canonicalizable");
        let workspace = discover_workspace_root(&crate_root)
            .expect("xtask must be nested under the Peritus workspace root");
        assert_eq!(
            workspace.join("xtask").canonicalize().expect("discovered xtask must canonicalize"),
            crate_root
        );
        assert!(workspace.join("architecture.toml").is_file());
    }

    #[cfg(windows)]
    #[test]
    fn discovered_workspace_root_is_safe_to_pass_back_to_child_processes() {
        use std::path::{Component, Prefix};

        let crate_root = fs::canonicalize(env!("CARGO_MANIFEST_DIR"))
            .expect("xtask manifest directory must be canonicalizable");
        let workspace = discover_workspace_root(&crate_root)
            .expect("xtask must be nested under the Peritus workspace root");
        assert!(!matches!(
            workspace.components().next(),
            Some(Component::Prefix(prefix))
                if matches!(prefix.kind(), Prefix::VerbatimDisk(_) | Prefix::VerbatimUNC(_, _))
        ));
    }
}
mod shard_args;
