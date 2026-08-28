//! Host-native package assembly behind the thin `peritus-package` binary.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    Architecture, ArtifactRole, InstallPath, ManifestArtifact, PackageManifest, PackageVersion,
    Platform, RelativePackagePath, ReleaseLayout, digest_file,
};

const MAX_ARTIFACT_BYTES: u64 = 1024 * 1024 * 1024;

/// Builds a package from the process arguments and prints its output directory.
///
/// # Errors
///
/// Returns a checked assembly, filesystem, manifest, platform, or child-build error.
pub fn run_from_env() -> Result<(), Box<dyn std::error::Error>> {
    let root = workspace_root()?;
    let platform = host_platform()?;
    let architecture = host_architecture()?;
    let use_debug_artifacts = parse_arguments()?;
    if !use_debug_artifacts {
        build_binaries(&root, platform)?;
    }
    let output =
        root.join("dist").join(format!("peritus-{}-{}", platform.as_str(), architecture.as_str()));
    if output.exists() {
        fs::remove_dir_all(&output)?;
    }
    fs::create_dir_all(output.join("bin"))?;
    fs::create_dir_all(output.join("libexec"))?;
    fs::create_dir_all(output.join("share/peritus"))?;
    let executable_suffix = if platform == Platform::Windows { ".exe" } else { "" };
    let target = root.join(if use_debug_artifacts { "target/debug" } else { "target/release" });
    let helper = helper_name(platform);
    let mut artifacts = stage_binaries(&target, &output, helper, executable_suffix)?;
    artifacts.extend(stage_packaging_assets(&root, &output, platform)?);
    let home = InstallPath::new(
        platform,
        match platform {
            Platform::Linux | Platform::Macos => "/home/peritus",
            Platform::Windows => "C:/Users/peritus",
        },
    )?;
    let layout = ReleaseLayout::production(platform, &home)?;
    let manifest = PackageManifest::new(
        PackageVersion::new(env!("CARGO_PKG_VERSION"))?,
        platform,
        architecture,
        layout.digest(),
        artifacts,
    )?;
    fs::write(output.join("manifest.toml"), manifest.canonical_bytes())?;
    fs::write(output.join("SHA256SUMS"), manifest.checksums())?;
    println!("{}", output.display());
    Ok(())
}

fn stage_binaries(
    target: &Path,
    output: &Path,
    helper: &str,
    executable_suffix: &str,
) -> Result<Vec<ManifestArtifact>, Box<dyn std::error::Error>> {
    Ok(vec![
        stage(
            &target.join(format!("peritusd{executable_suffix}")),
            &output.join(format!("bin/peritusd{executable_suffix}")),
            output,
            ArtifactRole::Daemon,
            true,
        )?,
        stage(
            &target.join(format!("peritus{executable_suffix}")),
            &output.join(format!("bin/peritus{executable_suffix}")),
            output,
            ArtifactRole::Cli,
            true,
        )?,
        stage(
            &target.join(format!("peritus-tui{executable_suffix}")),
            &output.join(format!("bin/peritus-tui{executable_suffix}")),
            output,
            ArtifactRole::Tui,
            true,
        )?,
        stage(
            &target.join(format!("{helper}{executable_suffix}")),
            &output.join(format!("libexec/{helper}{executable_suffix}")),
            output,
            ArtifactRole::SandboxHelper,
            true,
        )?,
    ])
}

fn parse_arguments() -> Result<bool, &'static str> {
    let mut arguments = env::args().skip(1);
    match (arguments.next().as_deref(), arguments.next()) {
        (None, None) => Ok(false),
        (Some("--use-debug-artifacts"), None) => Ok(true),
        _ => Err("usage: peritus-package [--use-debug-artifacts]"),
    }
}

fn build_binaries(root: &Path, platform: Platform) -> Result<(), Box<dyn std::error::Error>> {
    let helper_package = match platform {
        Platform::Linux => "peritus-sandbox-linux",
        Platform::Macos => "peritus-sandbox-macos",
        Platform::Windows => "peritus-sandbox-windows",
    };
    let status = Command::new("cargo")
        .current_dir(root)
        .env("CARGO_BUILD_JOBS", "2")
        .args([
            "build",
            "--release",
            "--locked",
            "-p",
            "peritus-cli",
            "-p",
            "peritus-daemon",
            "-p",
            "peritus-tui",
            "-p",
            helper_package,
        ])
        .status()?;
    if !status.success() {
        return Err("release binary build failed".into());
    }
    Ok(())
}

fn stage_packaging_assets(
    root: &Path,
    output: &Path,
    platform: Platform,
) -> Result<Vec<ManifestArtifact>, Box<dyn std::error::Error>> {
    let (directory, installer, uninstaller, upgrader, service) = match platform {
        Platform::Linux => (
            "linux",
            "Install-Peritus.sh",
            "Uninstall-Peritus.sh",
            "Upgrade-Peritus.sh",
            "peritus.service",
        ),
        Platform::Macos => (
            "macos",
            "Install-Peritus.sh",
            "Uninstall-Peritus.sh",
            "Upgrade-Peritus.sh",
            "com.corvidae.peritus.plist.in",
        ),
        Platform::Windows => (
            "windows",
            "Install-Peritus.ps1",
            "Uninstall-Peritus.ps1",
            "Upgrade-Peritus.ps1",
            "Peritus.Task.xml.in",
        ),
    };
    let source = root.join("packaging").join(directory);
    let service_target = output.join("share/peritus").join(service);
    Ok(vec![
        stage(
            &source.join(service),
            &service_target,
            output,
            ArtifactRole::ServiceDefinition,
            false,
        )?,
        stage(
            &source.join(installer),
            &output.join(installer),
            output,
            ArtifactRole::Installer,
            true,
        )?,
        stage(
            &source.join(uninstaller),
            &output.join(uninstaller),
            output,
            ArtifactRole::Uninstaller,
            true,
        )?,
        stage(
            &source.join(upgrader),
            &output.join(upgrader),
            output,
            ArtifactRole::Upgrader,
            true,
        )?,
    ])
}

fn stage(
    source: &Path,
    target: &Path,
    package_root: &Path,
    role: ArtifactRole,
    executable: bool,
) -> Result<ManifestArtifact, Box<dyn std::error::Error>> {
    fs::copy(source, target)?;
    #[cfg(unix)]
    if executable {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(target, fs::Permissions::from_mode(0o755))?;
    }
    let relative = target.strip_prefix(package_root)?;
    let relative = relative.to_string_lossy().replace('\\', "/");
    Ok(ManifestArtifact::new(
        role,
        RelativePackagePath::new(relative)?,
        digest_file(target, MAX_ARTIFACT_BYTES)?,
        executable,
    )?)
}

fn workspace_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut root = env::current_dir()?.canonicalize()?;
    while !root.join("architecture.toml").is_file() {
        if !root.pop() {
            return Err("workspace root was not found".into());
        }
    }
    Ok(root)
}

const fn helper_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Linux => "peritus-linux-sandbox-helper",
        Platform::Macos => "peritus-macos-sandbox-helper",
        Platform::Windows => "peritus-windows-sandbox-helper",
    }
}

const fn host_platform() -> Result<Platform, &'static str> {
    if cfg!(target_os = "linux") {
        Ok(Platform::Linux)
    } else if cfg!(target_os = "macos") {
        Ok(Platform::Macos)
    } else if cfg!(target_os = "windows") {
        Ok(Platform::Windows)
    } else {
        Err("host platform is unsupported")
    }
}

fn host_architecture() -> Result<Architecture, &'static str> {
    match env::consts::ARCH {
        "x86_64" => Ok(Architecture::X86_64),
        "aarch64" => Ok(Architecture::Aarch64),
        _ => Err("host architecture is unsupported"),
    }
}
