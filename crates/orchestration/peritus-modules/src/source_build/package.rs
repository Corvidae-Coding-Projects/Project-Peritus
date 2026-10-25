//! Pinned toolchain preparation, offline compilation, and atomic package publication.

use super::{process, source::PreparedSource};
use crate::{
    Definition, Error, Registry, SourceEntry, digest, invalid, market::source::CatalogSet, publish,
};
use peritus_plugin_sdk::LocalPackageIdentity;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(super) struct BuildContext {
    source: PathBuf,
    cargo_home: PathBuf,
    target_dir: PathBuf,
    toolchain: PathBuf,
    scratch: PathBuf,
}

impl BuildContext {
    pub(super) fn prepare(
        registry: &Registry,
        entry: &SourceEntry,
        source: &PreparedSource,
        temporary: &Path,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<Self, Error> {
        if cancelled.load(Ordering::Acquire) {
            return Err(invalid("source build was cancelled before dependency preparation"));
        }
        validate_lock_sources(&source.root.join("Cargo.lock"))?;
        let toolchain = resolve_toolchain(&entry.toolchain, &entry.target)?;
        let cargo_home = temporary.join("cargo-home");
        let target_dir = temporary.join("target");
        fs::create_dir(&cargo_home)?;
        fs::create_dir(&target_dir)?;
        let context = Self {
            source: source.root.clone(),
            cargo_home,
            target_dir,
            toolchain,
            scratch: temporary.to_path_buf(),
        };
        let arguments = context.bwrap_arguments(
            false,
            &[
                "fetch".into(),
                "--locked".into(),
                "--target".into(),
                entry.target.clone(),
                "--config".into(),
                "net.git-fetch-with-cli=false".into(),
                "--config".into(),
                "registries.crates-io.protocol=\"sparse\"".into(),
            ],
        )?;
        process::run_scoped(
            registry,
            &entry.id,
            &super::build_job_id(registry, &entry.id)?,
            &process::ScopedRun {
                stage: "prepare",
                program: Path::new("/usr/bin/bwrap"),
                arguments: &arguments,
                environment: &[],
                scratch: &context.scratch,
            },
            cancelled,
        )?;
        Ok(context)
    }

    pub(super) fn compile(
        &self,
        registry: &Registry,
        entry: &SourceEntry,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<PathBuf, Error> {
        let arguments = self.bwrap_arguments(
            true,
            &[
                "build".into(),
                "--locked".into(),
                "--release".into(),
                "--jobs".into(),
                "2".into(),
                "--package".into(),
                entry.package.clone(),
                "--target".into(),
                entry.target.clone(),
                "--offline".into(),
            ],
        )?;
        process::run_scoped(
            registry,
            &entry.id,
            &super::build_job_id(registry, &entry.id)?,
            &process::ScopedRun {
                stage: "compile",
                program: Path::new("/usr/bin/bwrap"),
                arguments: &arguments,
                environment: &[],
                scratch: &self.scratch,
            },
            cancelled,
        )?;
        let executable =
            self.target_dir.join(&entry.target).join("release").join(&entry.executable);
        if !fs::metadata(&executable).is_ok_and(|metadata| metadata.is_file()) {
            return Err(invalid(format!(
                "source build did not produce the declared executable {}",
                executable.display()
            )));
        }
        Ok(executable)
    }

    fn bwrap_arguments(
        &self,
        deny_network: bool,
        cargo_arguments: &[String],
    ) -> Result<Vec<String>, Error> {
        if !Path::new("/usr/bin/bwrap").is_file() {
            return Err(invalid(
                "source builds require Bubblewrap at /usr/bin/bwrap; install the platform package and retry",
            ));
        }
        let mut arguments = vec![
            "--die-with-parent".into(),
            "--new-session".into(),
            "--unshare-user".into(),
            "--unshare-pid".into(),
            "--unshare-ipc".into(),
            "--unshare-uts".into(),
            "--clearenv".into(),
            "--proc".into(),
            "/proc".into(),
            "--dev".into(),
            "/dev".into(),
            "--tmpfs".into(),
            "/tmp".into(),
            "--dir".into(),
            "/home".into(),
            "--dir".into(),
            "/nonexistent".into(),
            "--ro-bind".into(),
            self.source.display().to_string(),
            "/source".into(),
            "--bind".into(),
            self.cargo_home.display().to_string(),
            "/cargo-home".into(),
            "--bind".into(),
            self.target_dir.display().to_string(),
            "/target".into(),
            "--ro-bind".into(),
            self.toolchain.display().to_string(),
            "/toolchain".into(),
        ];
        if deny_network {
            arguments.push("--unshare-net".into());
        }
        for path in ["/usr", "/lib", "/lib64"] {
            if Path::new(path).exists() {
                arguments.extend(["--ro-bind".into(), path.into(), path.into()]);
            }
        }
        arguments.extend(["--dir".into(), "/etc".into()]);
        for path in [
            "/etc/hosts",
            "/etc/nsswitch.conf",
            "/etc/resolv.conf",
            "/etc/ld.so.cache",
            "/etc/pki",
            "/etc/ssl",
        ] {
            let Ok(source) = Path::new(path).canonicalize() else { continue };
            arguments.extend(["--ro-bind".into(), source.display().to_string(), path.into()]);
        }
        arguments.extend([
            "--chdir".into(),
            "/source".into(),
            "--setenv".into(),
            "HOME".into(),
            "/nonexistent".into(),
            "--setenv".into(),
            "CARGO_HOME".into(),
            "/cargo-home".into(),
            "--setenv".into(),
            "CARGO_TARGET_DIR".into(),
            "/target".into(),
            "--setenv".into(),
            "PATH".into(),
            "/toolchain/bin:/usr/bin:/bin".into(),
            "--setenv".into(),
            "CARGO_NET_GIT_FETCH_WITH_CLI".into(),
            "false".into(),
            "--".into(),
            "/toolchain/bin/cargo".into(),
        ]);
        arguments.extend_from_slice(cargo_arguments);
        Ok(arguments)
    }
}

pub(super) fn install(
    registry: &Registry,
    set: &CatalogSet,
    entry: &SourceEntry,
    source: &PreparedSource,
    context: &BuildContext,
    executable: &Path,
) -> Result<LocalPackageIdentity, Error> {
    let package =
        tempfile::Builder::new().prefix("package-").tempdir_in(context.scratch.as_path())?;
    let template = fs::read_to_string(source.root.join(&entry.manifest_template))?;
    let executable_relative = format!("bin/{}/{}", entry.target, entry.executable);
    if template.matches("@TARGET@").count() != 1 || template.matches("@EXECUTABLE@").count() != 2 {
        return Err(invalid("source package manifest template tokens are invalid"));
    }
    let manifest =
        template.replace("@TARGET@", &entry.target).replace("@EXECUTABLE@", &executable_relative);
    fs::write(package.path().join("module.toml"), manifest)?;
    let parsed = crate::Manifest::parse(&fs::read_to_string(package.path().join("module.toml"))?)?;
    if parsed.id != entry.id || parsed.version != entry.version {
        return Err(invalid("source package manifest does not match its catalog identity"));
    }
    for resource in &parsed.resources {
        let destination = package.path().join(resource);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        if resource == &executable_relative {
            fs::copy(executable, &destination)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                fs::set_permissions(&destination, fs::Permissions::from_mode(0o755))?;
            }
        } else {
            let source_path = source.root.join(entry.path.as_str()).join(resource);
            if !fs::metadata(&source_path).is_ok_and(|metadata| metadata.is_file()) {
                return Err(invalid(format!(
                    "declared package resource is absent from source: {resource}"
                )));
            }
            fs::copy(source_path, destination)?;
        }
    }
    let definition = Definition::from_directory(package.path())?;
    let executable_digest = file_digest(executable)?;
    let recipe_digest = digest(&serde_json::to_vec(&json!({
        "schema":1,
        "recipe":entry.recipe,
        "package":entry.package,
        "executable":entry.executable,
        "template":entry.manifest_template,
        "toolchain":entry.toolchain,
        "target":entry.target,
        "lockfile":entry.lockfile_digest,
    }))?);
    let identity = LocalPackageIdentity {
        schema: 1,
        module_id: entry.id.clone(),
        source_digest: entry.source_digest.clone(),
        recipe_digest,
        target: entry.target.clone(),
        executable_digest,
        package_digest: definition.digest,
    };
    let result = registry.install(package.path())?;
    if result["digest"] != identity.package_digest || result["enabled"] != false {
        return Err(invalid("installed source package identity or disabled state is inconsistent"));
    }
    let origin = json!({
        "schema":1,
        "repository":crate::MARKET_REPOSITORY,
        "catalog_commit":set.commit,
        "path":entry.path,
        "source_archive_digest":source.archive_digest,
        "identity":identity,
    });
    publish(
        &registry
            .root()
            .join("origins")
            .join(format!("{}-{}.json", entry.id, identity.package_digest)),
        &serde_json::to_vec_pretty(&origin)?,
    )?;
    Ok(identity)
}

fn resolve_toolchain(toolchain: &str, target: &str) -> Result<PathBuf, Error> {
    let cargo = Command::new("rustup")
        .args(["which", "cargo", "--toolchain", toolchain])
        .output()
        .map_err(|error| invalid(format!("rustup is required for source builds: {error}")))?;
    if !cargo.status.success() {
        return Err(invalid(format!(
            "pinned Rust toolchain {toolchain} is unavailable; install it with rustup and retry"
        )));
    }
    let cargo = PathBuf::from(
        std::str::from_utf8(&cargo.stdout)
            .map_err(|_| invalid("rustup cargo path is not UTF-8"))?
            .trim(),
    )
    .canonicalize()?;
    let toolchain_root = cargo
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| invalid("pinned cargo path has no toolchain root"))?
        .to_path_buf();
    let rustc = toolchain_root.join("bin/rustc");
    let version = Command::new(&rustc).arg("-vV").output()?;
    let text = std::str::from_utf8(&version.stdout)
        .map_err(|_| invalid("pinned rustc version is not UTF-8"))?;
    let host = text
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| invalid("pinned rustc did not report a host target"))?;
    if !version.status.success() || host != target {
        return Err(invalid(format!(
            "source package target {target} does not match pinned toolchain host {host}"
        )));
    }
    Ok(toolchain_root)
}

fn validate_lock_sources(path: &Path) -> Result<(), Error> {
    let lock: toml::Value = toml::from_str(&fs::read_to_string(path)?)?;
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| invalid("Cargo.lock has no package inventory"))?;
    for package in packages {
        if let Some(source) = package.get("source").and_then(toml::Value::as_str)
            && source != "registry+https://github.com/rust-lang/crates.io-index"
        {
            return Err(invalid(format!(
                "source build recipe version 1 supports only locked crates.io dependencies; found {source}"
            )));
        }
        if package.get("source").is_some()
            && package.get("checksum").and_then(toml::Value::as_str).is_none()
        {
            return Err(invalid("locked registry dependency has no checksum"));
        }
    }
    Ok(())
}

fn file_digest(path: &Path) -> Result<String, Error> {
    Ok(digest(&fs::read(path)?))
}
