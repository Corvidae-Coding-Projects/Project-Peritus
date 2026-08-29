//! Tag-bound GitHub release publication behind direct reviewed Cargo commands.

use std::{
    env, fs,
    fs::File,
    io::Read as _,
    path::Path,
    process::{Command, ExitStatus},
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest as _, Sha256};

use crate::XtaskError;

pub(crate) fn bootstrap_smoke(root: &Path) -> Result<std::path::PathBuf, XtaskError> {
    let package = crate::product_package::smoke(root)?;
    let fixture = TemporaryDirectory::new("peritus-public-installer")?;
    let version = format!("v{}", workspace_version(root)?);
    let release_root = fixture.path().join("releases").join(&version);
    fs::create_dir_all(&release_root).map_err(|error| {
        XtaskError::io("create public installer release fixture at", &release_root, error)
    })?;
    let name = package
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| XtaskError::metadata("native package directory has no UTF-8 name"))?;
    let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
    let archive = release_root.join(format!("{name}.{extension}"));
    archive_package(root, &package, &archive)?;
    let checksum = archive.with_file_name(format!("{name}.{extension}.sha256"));
    fs::write(&checksum, format!("{}\n", digest(&archive)?))
        .map_err(|error| XtaskError::io("write bootstrap fixture checksum at", &checksum, error))?;

    let subject = TemporaryDirectory::new("peritus-public-installer-subject")?;
    let release_base = file_url(&fixture.path().join("releases"));
    let mut command = bootstrap_command(root);
    command.env("PERITUS_VERSION", &version).env("PERITUS_RELEASE_BASE_URL", release_base);
    set_subject_home(&mut command, subject.path());
    run(&mut command, "run public release bootstrap")?;
    verify_bootstrap_install(subject.path(), &version)?;

    fs::write(&checksum, format!("{}\n", "0".repeat(64))).map_err(|error| {
        XtaskError::io("replace bootstrap fixture checksum at", &checksum, error)
    })?;
    let invalid_subject = TemporaryDirectory::new("peritus-public-installer-invalid")?;
    let mut invalid = bootstrap_command(root);
    invalid
        .env("PERITUS_VERSION", &version)
        .env("PERITUS_RELEASE_BASE_URL", file_url(&fixture.path().join("releases")));
    set_subject_home(&mut invalid, invalid_subject.path());
    require_failure(&mut invalid, "public release bootstrap accepted a bad checksum")?;
    Ok(package)
}

pub(crate) fn create(root: &Path) -> Result<(), XtaskError> {
    let tag = environment("GITHUB_REF_NAME")?;
    let sha = environment("GITHUB_SHA")?;
    let expected = format!("v{}", workspace_version(root)?);
    if tag != expected {
        return Err(XtaskError::metadata(format!(
            "release tag {tag} does not match workspace version {expected}"
        )));
    }
    run(
        Command::new("git").current_dir(root).args(["fetch", "origin", "main", "--no-tags"]),
        "fetch the authoritative main branch",
    )?;
    run(
        Command::new("git").current_dir(root).args([
            "merge-base",
            "--is-ancestor",
            &sha,
            "origin/main",
        ]),
        "prove the release tag belongs to main history",
    )?;
    let exists = Command::new("gh")
        .current_dir(root)
        .args(["release", "view", &tag])
        .status()
        .map_err(|error| XtaskError::io("query GitHub release from", root, error))?
        .success();
    if !exists {
        run(
            Command::new("gh").current_dir(root).args([
                "release",
                "create",
                &tag,
                "--draft",
                "--verify-tag",
                "--generate-notes",
                "--title",
                &format!("Peritus {tag}"),
            ]),
            "create draft GitHub release",
        )?;
    }
    Ok(())
}

pub(crate) fn package_upload(root: &Path) -> Result<(), XtaskError> {
    let tag = environment("GITHUB_REF_NAME")?;
    let package = crate::product_package::build(root)?;
    let name = package
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| XtaskError::metadata("native package directory has no UTF-8 name"))?;
    let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
    let archive = root.join("dist").join(format!("{name}.{extension}"));
    archive_package(root, &package, &archive)?;
    let checksum = archive.with_file_name(format!("{name}.{extension}.sha256"));
    fs::write(&checksum, format!("{}\n", digest(&archive)?))
        .map_err(|error| XtaskError::io("write release archive checksum at", &checksum, error))?;
    run(
        Command::new("gh")
            .current_dir(root)
            .args(["release", "upload", &tag])
            .arg(&archive)
            .arg(&checksum)
            .arg("--clobber"),
        "upload native release archive",
    )
}

pub(crate) fn publish() -> Result<(), XtaskError> {
    let tag = environment("GITHUB_REF_NAME")?;
    run(
        Command::new("gh").args(["release", "edit", &tag, "--draft=false", "--latest"]),
        "publish complete GitHub release",
    )
}

#[cfg(windows)]
fn archive_package(root: &Path, package: &Path, archive: &Path) -> Result<(), XtaskError> {
    run(
        Command::new("powershell")
            .current_dir(root)
            .args([
                "-NoProfile",
                "-Command",
                "Compress-Archive -LiteralPath $args[0] -DestinationPath $args[1] -Force",
            ])
            .arg(package)
            .arg(archive),
        "archive Windows release package",
    )
}

#[cfg(not(windows))]
fn archive_package(root: &Path, package: &Path, archive: &Path) -> Result<(), XtaskError> {
    let parent = package
        .parent()
        .ok_or_else(|| XtaskError::metadata("native package directory has no parent"))?;
    let name = package
        .file_name()
        .ok_or_else(|| XtaskError::metadata("native package directory has no name"))?;
    run(
        Command::new("tar")
            .current_dir(root)
            .arg("-C")
            .arg(parent)
            .arg("-czf")
            .arg(archive)
            .arg(name),
        "archive Unix release package",
    )
}

fn bootstrap_command(root: &Path) -> Command {
    if cfg!(windows) {
        let mut command = Command::new("powershell");
        command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
        command.arg(root.join("install.ps1"));
        command
    } else {
        let mut command = Command::new("sh");
        command.arg(root.join("install.sh"));
        command
    }
}

fn set_subject_home(command: &mut Command, subject: &Path) {
    if cfg!(windows) {
        command.env("LOCALAPPDATA", subject);
    } else {
        command.env("HOME", subject);
    }
}

fn verify_bootstrap_install(subject: &Path, version: &str) -> Result<(), XtaskError> {
    let executable = if cfg!(windows) {
        subject.join("Programs/Peritus/bin/peritus.exe")
    } else {
        subject.join(".local/bin/peritus")
    };
    let output = Command::new(&executable).arg("--version").output().map_err(|error| {
        XtaskError::io("run bootstrap-installed command at", &executable, error)
    })?;
    let observed = String::from_utf8(output.stdout)
        .map_err(|_| XtaskError::metadata("bootstrap-installed version is not UTF-8"))?;
    let expected = format!("peritus {}", version.trim_start_matches('v'));
    if output.status.success() && observed.trim_end_matches(['\r', '\n']) == expected {
        Ok(())
    } else {
        Err(XtaskError::metadata("bootstrap-installed command has the wrong version"))
    }
}

fn file_url(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) { format!("file:///{normalized}") } else { format!("file://{normalized}") }
}

fn require_failure(command: &mut Command, detail: &'static str) -> Result<(), XtaskError> {
    let status = command.status().map_err(|error| {
        XtaskError::io(
            "run negative bootstrap qualification from",
            Path::new("<bootstrap-command>"),
            error,
        )
    })?;
    if status.success() { Err(XtaskError::metadata(detail)) } else { Ok(()) }
}

fn workspace_version(root: &Path) -> Result<String, XtaskError> {
    let manifest = root.join("Cargo.toml");
    let text = fs::read_to_string(&manifest)
        .map_err(|error| XtaskError::io("read workspace manifest from", &manifest, error))?;
    let value: toml::Value = toml::from_str(&text)
        .map_err(|error| XtaskError::metadata(format!("workspace manifest is invalid: {error}")))?;
    value
        .get("workspace")
        .and_then(|value| value.get("package"))
        .and_then(|value| value.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| XtaskError::metadata("workspace package version is unavailable"))
}

fn digest(path: &Path) -> Result<String, XtaskError> {
    let mut file =
        File::open(path).map_err(|error| XtaskError::io("open release archive at", path, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| XtaskError::io("read release archive at", path, error))?;
        if count == 0 {
            return Ok(hex(hasher.finalize().into()));
        }
        hasher.update(&buffer[..count]);
    }
}

fn hex(bytes: [u8; 32]) -> String {
    use core::fmt::Write as _;

    let mut output = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn environment(name: &'static str) -> Result<String, XtaskError> {
    env::var(name).map_err(|_| XtaskError::metadata(format!("{name} is required for release work")))
}

fn run(command: &mut Command, operation: &'static str) -> Result<(), XtaskError> {
    let status = command
        .status()
        .map_err(|error| XtaskError::io(operation, Path::new("<release-command>"), error))?;
    require_success(status, operation)
}

fn require_success(status: ExitStatus, operation: &'static str) -> Result<(), XtaskError> {
    if status.success() {
        Ok(())
    } else {
        Err(XtaskError::metadata(format!("{operation} failed with status {status}")))
    }
}

struct TemporaryDirectory {
    path: std::path::PathBuf,
}

impl TemporaryDirectory {
    fn new(label: &str) -> Result<Self, XtaskError> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| XtaskError::metadata("system clock is before the Unix epoch"))?
            .as_nanos();
        let path = env::temp_dir().join(format!("{label}-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).map_err(|error| {
            XtaskError::io("create release qualification directory at", &path, error)
        })?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_release_version_is_exact() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
        assert_eq!(workspace_version(root).expect("version"), "0.0.0");
    }
}
