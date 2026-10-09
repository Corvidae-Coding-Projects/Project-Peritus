//! Runs the generated deferred helper through native Windows PowerShell.
use super::*;

#[test]
fn deferred_helper_records_success_only_after_verifying_both_binaries() {
    for valid_daemon in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let package = root.path().join("package with spaces Étage");
        std::fs::create_dir(&package).unwrap();
        let bin = root.path().join("Programs/Peritus/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let target = bin.join("peritus.cmd");
        let daemon = bin.join("peritusd.cmd");
        let installer = package.join("install.ps1");
        let marker = package.join("installed");
        let release = Release::from_tag("v9.8.7").unwrap();
        std::fs::write(&target, "@echo peritus 9.8.7\r\n").unwrap();
        std::fs::write(
            &daemon,
            if valid_daemon { "@echo peritusd 9.8.7\r\n" } else { "@echo peritusd 9.8.6\r\n" },
        )
        .unwrap();
        let installer_script = format!(
            "param([string]$BundleRoot)\nSet-Content -LiteralPath '{}' -Value installed",
            quote(&marker).unwrap()
        );
        let mut encoded = vec![0xEF, 0xBB, 0xBF];
        encoded.extend_from_slice(installer_script.as_bytes());
        std::fs::write(&installer, encoded).unwrap();
        let mut parent = Command::new("cmd.exe").args(["/c", "exit", "0"]).spawn().unwrap();
        let parent_id =
            ParentIdentity { id: parent.id(), created: ParentIdentity::current().unwrap().created };
        assert!(parent.wait().unwrap().success());
        let outcome = root.path().join("update-outcome.json");
        let helper = root.path().join("finish.ps1");
        let script =
            deferred_script(&package, &release, &target, &daemon, &installer, &outcome, parent_id)
                .unwrap();
        let mut encoded = vec![0xEF, 0xBB, 0xBF];
        encoded.extend_from_slice(script.as_bytes());
        std::fs::write(&helper, encoded).unwrap();
        write_outcome(&outcome, &release, "pending", "test").unwrap();
        let status = Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&helper)
            .status()
            .unwrap();
        assert_eq!(status.success(), valid_daemon);
        assert!(marker.exists(), "installer did not run");
        let outcome: serde_json::Value =
            serde_json::from_slice(&std::fs::read(outcome).unwrap()).unwrap();
        assert_eq!(outcome["state"], if valid_daemon { "succeeded" } else { "failed" });
        assert_eq!(outcome["version"], "9.8.7");
        if !valid_daemon {
            assert!(outcome["detail"].as_str().unwrap().contains("daemon version"));
        }
    }
}

struct Attempt {
    helper: PathBuf,
    outcome: PathBuf,
}
impl Attempt {
    fn new(
        root: &std::path::Path,
        name: &str,
        version: &str,
        target: &std::path::Path,
        installer_body: &str,
        parent: ParentIdentity,
    ) -> Self {
        let directory = root.join(name);
        let package = directory.join("package");
        std::fs::create_dir_all(&package).unwrap();
        let installer = package.join("install.ps1");
        utf8_script(&installer, installer_body);
        let release = Release::from_tag(version).unwrap();
        let helper = directory.join("finish-update.ps1");
        let outcome = directory.join("update-outcome.json");
        utf8_script(
            &helper,
            &deferred_script(
                &package,
                &release,
                target,
                &target.with_file_name("peritusd.cmd"),
                &installer,
                &outcome,
                parent,
            )
            .unwrap(),
        );
        write_outcome(&outcome, &release, "pending", "test parent wait").unwrap();
        Self { helper, outcome }
    }
    fn outcome(&self) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(&self.outcome).unwrap()).unwrap()
    }
    fn start(&self) -> Helper {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(&self.helper);
        Helper(tokio::spawn(async move {
            super::super::process::status(&mut command, "run generated deferred updater test").await
        }))
    }
}
struct Helper(tokio::task::JoinHandle<Result<std::process::ExitStatus, LauncherError>>);
impl Helper {
    async fn finish(&mut self) -> std::process::ExitStatus {
        tokio::time::timeout(std::time::Duration::from_secs(30), &mut self.0)
            .await
            .expect("helper did not finish")
            .unwrap()
            .unwrap()
    }
}
impl Drop for Helper {
    fn drop(&mut self) {
        self.0.abort();
    }
}
fn utf8_script(path: &std::path::Path, text: &str) {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(text.as_bytes());
    std::fs::write(path, bytes).unwrap();
}
async fn wait_for(mut ready: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !ready() {
        assert!(
            std::time::Instant::now() < deadline,
            "native helper did not reach the expected boundary"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
fn command_shims(root: &std::path::Path) -> PathBuf {
    std::fs::create_dir_all(root).unwrap();
    for (name, script) in [("peritus.cmd", "cli.ps1"), ("peritusd.cmd", "daemon.ps1")] {
        std::fs::write(root.join(name), format!("@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"%~dp0{script}\"\r\n")).unwrap();
    }
    root.join("peritus.cmd")
}
fn serial_installer(target: &std::path::Path, version: &str) -> String {
    format!(
        r"param([string]$BundleRoot)
$root = '{root}'
# Match the real upgrade backup traversal while the installation owner is held.
$backup = Join-Path $BundleRoot 'backup'
Copy-Item -LiteralPath (Split-Path -Parent $root) -Destination $backup -Recurse
Remove-Item -LiteralPath $backup -Recurse -Force
$active = Join-Path $root 'active'
$marker = [IO.File]::Open($active, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
$marker.Dispose()
Add-Content -LiteralPath (Join-Path $root 'events') -Value 'begin:{version}'
while (-not (Test-Path -LiteralPath (Join-Path $root 'gate'))) {{ Start-Sleep -Milliseconds 20 }}
Set-Content -Encoding UTF8 -LiteralPath (Join-Path $root 'cli.ps1') -Value @'
$ErrorActionPreference='Stop'
Add-Content -LiteralPath (Join-Path $PSScriptRoot 'events') -Value 'cli:{version}'
Start-Sleep -Milliseconds 200
Write-Output 'peritus {version}'
'@
Set-Content -Encoding UTF8 -LiteralPath (Join-Path $root 'daemon.ps1') -Value @'
$ErrorActionPreference='Stop'
Add-Content -LiteralPath (Join-Path $PSScriptRoot 'events') -Value 'daemon:{version}'
Start-Sleep -Milliseconds 200
Remove-Item -LiteralPath (Join-Path $PSScriptRoot 'active')
Write-Output 'peritusd {version}'
'@
",
        root = quote(target.parent().unwrap()).unwrap()
    )
}

#[tokio::test]
async fn simultaneous_deferred_attempts_hold_one_installation_owner_through_both_checks() {
    let root = tempfile::tempdir().unwrap();
    let installed = root.path().join("Programs/Peritus with spaces Étage/bin");
    let target = command_shims(&installed);
    let mut parent = ParentIdentity::current().unwrap();
    parent.created += 1; // The current PID denotes a different birth identity than this attempt.
    let first = Attempt::new(
        root.path(),
        "attempt-one",
        "v9.8.7",
        &target,
        &serial_installer(&target, "9.8.7"),
        parent,
    );
    let second = Attempt::new(
        root.path(),
        "attempt-two",
        "v9.8.8",
        &target,
        &serial_installer(&target, "9.8.8"),
        parent,
    );
    let mut first_helper = first.start();
    wait_for(|| installed.join("active").exists()).await;
    let mut second_helper = second.start();
    wait_for(|| second.outcome()["detail"] == "waiting for installation owner").await;
    assert_eq!(
        second.outcome()["state"],
        "pending",
        "second attempt must not enter installation while the first owns it"
    );
    std::fs::write(installed.join("gate"), b"release first installer").unwrap();
    assert!(first_helper.finish().await.success());
    assert!(second_helper.finish().await.success());
    let events = std::fs::read_to_string(installed.join("events")).unwrap();
    assert_eq!(
        events.lines().collect::<Vec<_>>(),
        ["begin:9.8.7", "cli:9.8.7", "daemon:9.8.7", "begin:9.8.8", "cli:9.8.8", "daemon:9.8.8"]
    );
    for (attempt, version) in [(&first, "9.8.7"), (&second, "9.8.8")] {
        assert_eq!(attempt.outcome()["state"], "succeeded");
        assert_eq!(attempt.outcome()["version"], version);
    }
    assert_ne!(first.outcome, second.outcome);
}

#[tokio::test]
async fn mismatched_parent_birth_does_not_wait_for_the_live_reused_pid() {
    let root = tempfile::tempdir().unwrap();
    let target = command_shims(&root.path().join("Programs/Peritus/bin"));
    utf8_script(&target.with_file_name("cli.ps1"), "Write-Output 'peritus 9.8.7'");
    utf8_script(&target.with_file_name("daemon.ps1"), "Write-Output 'peritusd 9.8.7'");
    let mut parent = ParentIdentity::current().unwrap();
    parent.created += 1;
    let attempt = Attempt::new(
        root.path(),
        "attempt",
        "v9.8.7",
        &target,
        "param([string]$BundleRoot)",
        parent,
    );
    assert!(attempt.start().finish().await.success());
    assert_eq!(attempt.outcome()["state"], "succeeded");
    assert_eq!(
        std::process::id(),
        parent.id,
        "the unrelated parent PID is still this live test process"
    );
}
