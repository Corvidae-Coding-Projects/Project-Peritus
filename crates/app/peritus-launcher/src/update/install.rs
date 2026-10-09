//! Transactional native installer invocation and installed-version verification.

use std::{env, path::PathBuf, process::Command};

use crate::LauncherError;

use super::release::Release;

#[cfg(windows)]
#[path = "install/windows_parent.rs"]
mod windows_parent;
#[cfg(windows)]
use windows_parent::ParentIdentity;

#[cfg(not(windows))]
pub(super) async fn apply(
    package: &std::path::Path,
    release: &Release,
) -> Result<(), LauncherError> {
    let target = installed_command()?;
    immediate_unix(package, release, &target).await
}

#[cfg(windows)]
pub(super) fn apply(
    package: &std::path::Path,
    release: &Release,
) -> std::future::Ready<Result<(), LauncherError>> {
    std::future::ready(
        installed_command().and_then(|target| deferred_windows(package, release, &target)),
    )
}

#[cfg(not(windows))]
async fn immediate_unix(
    package: &std::path::Path,
    release: &Release,
    target: &std::path::Path,
) -> Result<(), LauncherError> {
    let installation_owner = installation_owner(target)?;
    let outcome = package.parent().unwrap_or(package).join("update-outcome.json");
    write_outcome(&outcome, release, "running", "native installation started")?;
    let mut receipt = InstallReceipt { path: outcome, release, finished: false };
    let script =
        package.join(if target.exists() { "Upgrade-Peritus.sh" } else { "Install-Peritus.sh" });
    let result = async {
        let mut command = Command::new("sh");
        command.arg(&script).arg(package);
        inherit_installation_owner(&mut command, &installation_owner);
        let status = super::process::status(&mut command, "run native updater").await?;
        if !status.success() {
            return Err(LauncherError::Update(format!(
                "native updater failed with status {status}"
            )));
        }
        verify(target, release).await
    }
    .await;
    let (state, detail) = match &result {
        Ok(()) => ("succeeded", "installed CLI and daemon versions verified".to_owned()),
        Err(error) => ("failed", error.to_string()),
    };
    write_outcome(&receipt.path, release, state, &detail)?;
    receipt.finished = true;
    result
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "child-only descriptor inheritance requires POSIX pre_exec and fcntl"
)]
fn inherit_installation_owner(command: &mut Command, owner: &std::fs::File) {
    use std::os::{fd::AsRawFd as _, unix::process::CommandExt as _};
    let descriptor = owner.as_raw_fd();
    // SAFETY: the owner remains open through spawn. The post-fork callback only calls
    // async-signal-safe fcntl; it clears CLOEXEC in the child, never in the parent.
    // The inherited open-file description retains flock ownership until native
    // installation descendants exit, including after cancellation drops our future.
    unsafe {
        command.pre_exec(move || {
            let flags = nix::libc::fcntl(descriptor, nix::libc::F_GETFD);
            if flags < 0
                || nix::libc::fcntl(descriptor, nix::libc::F_SETFD, flags & !nix::libc::FD_CLOEXEC)
                    < 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(windows))]
fn installation_owner(target: &std::path::Path) -> Result<std::fs::File, LauncherError> {
    // This path is installation-wide, independent of release and staging attempt.
    let directory = target
        .parent()
        .ok_or_else(|| LauncherError::Update("installed CLI has no parent directory".into()))?;
    std::fs::create_dir_all(directory).map_err(|error| {
        LauncherError::filesystem("create installation directory", directory, error)
    })?;
    let path = directory.join(".peritus-update.lock");
    let owner = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| LauncherError::filesystem("open installation owner", &path, error))?;
    owner.try_lock().map_err(|error| {
        LauncherError::Update(format!("another updater owns this installation: {error}"))
    })?;
    Ok(owner)
}

#[cfg(not(windows))]
struct InstallReceipt<'a> {
    path: PathBuf,
    release: &'a Release,
    finished: bool,
}

#[cfg(not(windows))]
impl Drop for InstallReceipt<'_> {
    fn drop(&mut self) {
        if !self.finished {
            let _ = write_outcome(
                &self.path,
                self.release,
                "interrupted",
                "installation owner ended before verification; reconcile installed CLI and daemon before retrying",
            );
        }
    }
}

#[cfg(windows)]
fn deferred_windows(
    package: &std::path::Path,
    release: &Release,
    target: &std::path::Path,
) -> Result<(), LauncherError> {
    use std::os::windows::process::CommandExt as _;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

    let helper = package.parent().unwrap_or(package).join("finish-update.ps1");
    let installer =
        package.join(if target.exists() { "Upgrade-Peritus.ps1" } else { "Install-Peritus.ps1" });
    let outcome = helper.with_file_name("update-outcome.json");
    let daemon = target.with_file_name("peritusd.exe");
    let script = deferred_script(
        package,
        release,
        target,
        &daemon,
        &installer,
        &outcome,
        ParentIdentity::current()?,
    )?;
    // Windows PowerShell 5.1 needs a BOM to decode non-ASCII literal paths as UTF-8.
    let mut encoded = vec![0xEF, 0xBB, 0xBF];
    encoded.extend_from_slice(script.as_bytes());
    persist(&helper, &encoded)?;
    write_outcome(&outcome, release, "pending", "waiting for launcher exit")?;
    if let Err(error) = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&helper)
        // Deferred installation must survive this launcher. An enclosing job that disallows
        // breakaway rejects creation; record failure instead of scheduling a helper it will kill.
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB)
        .spawn()
    {
        write_outcome(&outcome, release, "failed", &error.to_string())?;
        return Err(LauncherError::Update(format!("start deferred native updater: {error}")));
    }
    Ok(())
}

fn installed_command() -> Result<PathBuf, LauncherError> {
    if cfg!(windows) {
        environment("LOCALAPPDATA").map(|root| root.join("Programs/Peritus/bin/peritus.exe"))
    } else {
        environment("HOME").map(|root| root.join(".local/bin/peritus"))
    }
}

fn environment(name: &'static str) -> Result<PathBuf, LauncherError> {
    let value =
        env::var_os(name).ok_or_else(|| LauncherError::Update(format!("{name} is unavailable")))?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(LauncherError::Update(format!("{name} must contain an absolute path")));
    }
    Ok(path)
}

#[cfg(not(windows))]
async fn verify(target: &std::path::Path, release: &Release) -> Result<(), LauncherError> {
    let (status, stdout) = super::process::stdout(
        Command::new(target).arg("--version"),
        "run installed version check",
    )
    .await?;
    let expected = format!("peritus {}\n", release.version());
    if status.success() && stdout == expected.as_bytes() {
        // macOS publishes a CLI symlink in ~/.local/bin; its daemon lives beside the
        // resolved executable in the application bundle's bin directory.
        let installed = target
            .canonicalize()
            .map_err(|error| LauncherError::filesystem("resolve installed CLI", target, error))?;
        let daemon = installed.with_file_name("peritusd");
        let (status, stdout) = super::process::stdout(
            Command::new(&daemon).arg("--version"),
            "verify installed daemon",
        )
        .await?;
        if !status.success() || stdout != format!("peritusd {}\n", release.version()).as_bytes() {
            return Err(LauncherError::Update(
                "installed daemon version verification failed".to_owned(),
            ));
        }
        Ok(())
    } else {
        Err(LauncherError::Update("installed version verification failed".to_owned()))
    }
}

#[cfg(windows)]
fn quote(path: &std::path::Path) -> Result<String, LauncherError> {
    path.to_str().map(|path| path.replace('\'', "''")).ok_or_else(|| {
        LauncherError::Update("PowerShell updater requires a Unicode path".to_owned())
    })
}

fn write_outcome(
    path: &std::path::Path,
    release: &Release,
    state: &str,
    detail: &str,
) -> Result<(), LauncherError> {
    let value = serde_json::json!({"version": release.version(), "state": state, "detail": detail});
    persist(path, value.to_string().as_bytes())
}

#[cfg(windows)]
fn deferred_script(
    package: &std::path::Path,
    release: &Release,
    target: &std::path::Path,
    daemon: &std::path::Path,
    installer: &std::path::Path,
    outcome: &std::path::Path,
    parent: ParentIdentity,
) -> Result<String, LauncherError> {
    let installation_lock = windows_installation_lock(target)?;
    let installation_directory = installation_lock.parent().expect("lock has checked parent");
    Ok(format!(
        r"$ErrorActionPreference='Stop'
$outcome = '{outcome}'
function Write-Outcome([string]$state, [string]$detail) {{
    $json = @{{version='{version}';state=$state;detail=$detail}} | ConvertTo-Json -Compress
    $bytes = [Text.Encoding]::UTF8.GetBytes($json)
    $temporary = $outcome + '.' + [guid]::NewGuid().ToString('N')
    $stream = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {{ $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true) }} finally {{ $stream.Dispose() }}
    if ([IO.File]::Exists($outcome)) {{ [IO.File]::Replace($temporary, $outcome, $null) }} else {{ [IO.File]::Move($temporary, $outcome) }}
}}
function Wait-ExactParent {{
    $process = $null
    try {{
        try {{
            $process = [Diagnostics.Process]::GetProcessById({parent})
            # Cache the native handle before inspecting birth time or waiting, pinning one object.
            [void]$process.Handle
        }} catch {{
            $cause = $_.Exception
            while ($null -ne $cause.InnerException) {{ $cause = $cause.InnerException }}
            if ($cause -is [ArgumentException] -or ($cause -is [ComponentModel.Win32Exception] -and $cause.NativeErrorCode -eq 87)) {{ return }}
            if ($cause -is [InvalidOperationException] -and $null -ne $process -and $process.HasExited) {{ return }}
            throw
        }}
        if ($process.StartTime.ToUniversalTime().ToFileTimeUtc() -eq [Int64]{created}) {{ $process.WaitForExit() }}
    }} finally {{ if ($null -ne $process) {{ $process.Dispose() }} }}
}}
$installationOwner = $null
try {{
    Wait-ExactParent
    [void][IO.Directory]::CreateDirectory('{installation_directory}')
    Write-Outcome 'pending' 'waiting for installation owner'
    while ($null -eq $installationOwner) {{
        try {{
            $installationOwner = [IO.File]::Open('{installation_lock}', [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        }} catch {{
            $cause = $_.Exception
            while ($null -ne $cause.InnerException) {{ $cause = $cause.InnerException }}
            $code = $cause.HResult -band 0xffff
            if ($cause -isnot [IO.IOException] -or ($code -ne 32 -and $code -ne 33)) {{ throw }}
            Start-Sleep -Milliseconds 100
        }}
    }}
    Write-Outcome 'running' 'native installation started'
    & '{installer}' -BundleRoot '{package}'
    $version = & '{target}' --version
    if ($LASTEXITCODE -ne 0 -or @($version).Count -ne 1 -or [string]$version -cne 'peritus {version}') {{ throw 'installed CLI version verification failed' }}
    $daemonVersion = & '{daemon}' --version
    if ($LASTEXITCODE -ne 0 -or @($daemonVersion).Count -ne 1 -or [string]$daemonVersion -cne 'peritusd {version}') {{ throw 'installed daemon version verification failed' }}
    Write-Outcome 'succeeded' 'installed CLI and daemon versions verified'
    Remove-Item -LiteralPath $MyInvocation.MyCommand.Path -Force -ErrorAction SilentlyContinue
}} catch {{
    Write-Outcome 'failed' $_.Exception.Message
    exit 1
}} finally {{
    if ($null -ne $installationOwner) {{ $installationOwner.Dispose() }}
}}
",
        outcome = quote(outcome)?,
        version = release.version(),
        parent = parent.id,
        created = parent.created,
        installation_directory = quote(installation_directory)?,
        installation_lock = quote(&installation_lock)?,
        installer = quote(installer)?,
        package = quote(package)?,
        target = quote(target)?,
        daemon = quote(daemon)?,
    ))
}

#[cfg(windows)]
fn windows_installation_lock(target: &std::path::Path) -> Result<PathBuf, LauncherError> {
    // Upgrade-Peritus.ps1 backs up and can replace the whole program root, including bin.
    // Keep its exclusive owner outside that tree so backup and rollback remain possible.
    let program_root = target
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or_else(|| LauncherError::Update("installed CLI has no program root".into()))?;
    let directory = program_root
        .parent()
        .ok_or_else(|| LauncherError::Update("installation has no lock directory".into()))?;
    let name = program_root
        .file_name()
        .ok_or_else(|| LauncherError::Update("installation has no stable directory name".into()))?;
    let mut lock_name = std::ffi::OsString::from(".");
    lock_name.push(name);
    lock_name.push(".peritus-update.lock");
    Ok(directory.join(lock_name))
}

fn persist(path: &std::path::Path, bytes: &[u8]) -> Result<(), LauncherError> {
    let previous = crate::persistence::read_exact_or_publish(path, bytes)?;
    if previous != bytes {
        crate::persistence::replace_recovery_file(path, bytes)?;
    }
    Ok(())
}

#[cfg(all(test, windows))]
#[path = "install/windows_tests.rs"]
mod windows_tests;

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    async fn wait_for_installation_release(target: &std::path::Path) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if installation_owner(target).is_ok() {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "installation ownership leaked after child exit"
            );
            // Concurrent test processes can briefly inherit CLOEXEC descriptors between
            // fork and exec. Require eventual release, preserving active-owner rejection.
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn installation_owner_rejects_other_releases_and_survives_parent_handle_drop() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("peritus");
        let owner = installation_owner(&target).unwrap();
        let mut command = Command::new("sh");
        command.args(["-c", "exec sleep 30"]);
        inherit_installation_owner(&mut command, &owner);
        let mut child = command.spawn().unwrap();
        drop(owner);
        assert!(installation_owner(&target).is_err(), "child must retain installation custody");
        child.kill().unwrap();
        child.wait().unwrap();
        wait_for_installation_release(&target).await;
    }

    #[tokio::test]
    async fn native_install_receipt_requires_both_versions_and_survives_cancellation() {
        let directory = tempfile::tempdir().unwrap();
        let package = directory.path().join("bundle");
        std::fs::create_dir(&package).unwrap();
        let target = directory.path().join("peritus");
        let daemon = directory.path().join("peritusd");
        for (path, text) in [(&target, "peritus 1.2.3"), (&daemon, "peritusd 1.2.3")] {
            std::fs::write(path, format!("#!/bin/sh\nprintf '{text}\\n'\n")).unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let script = package.join("Upgrade-Peritus.sh");
        std::fs::write(&script, "exit 0\n").unwrap();
        let release = Release::from_tag("v1.2.3").unwrap();
        immediate_unix(&package, &release, &target).await.unwrap();
        let outcome = directory.path().join("update-outcome.json");
        let read = || {
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&outcome).unwrap()).unwrap()
        };
        assert_eq!(read()["state"], "succeeded");
        wait_for_installation_release(&target).await;
        let command_directory = directory.path().join("commands");
        std::fs::create_dir(&command_directory).unwrap();
        let alias = command_directory.join("peritus");
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        verify(&alias, &release).await.unwrap();
        std::fs::write(&daemon, "#!/bin/sh\nprintf 'peritusd 1.2.2\\n'\n").unwrap();
        assert!(immediate_unix(&package, &release, &target).await.is_err());
        assert_eq!(read()["state"], "failed");
        wait_for_installation_release(&target).await;
        std::fs::write(&script, "sleep 30\n").unwrap();
        {
            let installation = immediate_unix(&package, &release, &target);
            tokio::pin!(installation);
            tokio::select! {
                result = &mut installation => panic!("unexpected install completion: {result:?}"),
                () = tokio::time::sleep(std::time::Duration::from_millis(50)) => {},
            }
            let competing_release = Release::from_tag("v1.2.4").unwrap();
            let competing = immediate_unix(&package, &competing_release, &target).await;
            assert!(competing.unwrap_err().to_string().contains("another updater owns"));
            assert_eq!(read()["state"], "running");
            assert_eq!(read()["version"], "1.2.3");
        }
        assert_eq!(read()["state"], "interrupted");
        wait_for_installation_release(&target).await;
    }
}
