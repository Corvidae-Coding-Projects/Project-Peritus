//! Owned QEMU guest and bounded SSH operations for actual host-reboot qualification.

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::media;
use super::parse;
use crate::native_controller::args::ControllerPaths;

const BOOT_TIMEOUT: Duration = Duration::from_mins(5);
const REBOOT_TIMEOUT: Duration = Duration::from_mins(3);
const CHECKPOINT_TIMEOUT: Duration = Duration::from_mins(1);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const SSH_READY_POLL: Duration = Duration::from_secs(10);
const GUEST_STATE: &str = "/var/lib/peritus-h1";

pub(super) struct Guest {
    child: Child,
    port: u16,
    private_key: PathBuf,
    root: PathBuf,
    qemu_stderr: PathBuf,
    boot_id: String,
    version: String,
}

pub(super) struct RemoteFile {
    pub(super) sha256: String,
    pub(super) bytes: u64,
}

impl Guest {
    pub(super) fn launch(
        paths: &ControllerPaths,
        runtime_root: &Path,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let media = media::create(paths, runtime_root)?;
        let port = available_port()?;
        let qemu_stdout = super::super::process::create_output(&runtime_root.join("qemu.stdout"))?;
        let qemu_stderr_path = runtime_root.join("qemu.stderr");
        let qemu_stderr = super::super::process::create_output(&qemu_stderr_path)?;
        let console = runtime_root.join("qemu.console");
        let mut command = Command::new("qemu-system-x86_64");
        command
            .arg("-name")
            .arg("peritus-h1-reboot")
            .arg("-m")
            .arg("1024")
            .arg("-smp")
            .arg("2")
            .arg("-display")
            .arg("none")
            .arg("-monitor")
            .arg("none")
            .arg("-serial")
            .arg(format!("file:{}", console.display()))
            .arg("-drive")
            .arg(format!("file={},if=virtio,format=qcow2", media.overlay.display()))
            .arg("-drive")
            .arg(format!("file={},media=cdrom,readonly=on", media.seed_iso.display()))
            .arg("-drive")
            .arg(format!("file={},media=cdrom,readonly=on", media.payload_iso.display()))
            .arg("-netdev")
            .arg(format!("user,id=peritusnet,restrict=on,hostfwd=tcp:127.0.0.1:{port}-:22"))
            .arg("-device")
            .arg("e1000,netdev=peritusnet")
            .current_dir(runtime_root)
            .stdin(Stdio::null())
            .stdout(Stdio::from(qemu_stdout))
            .stderr(Stdio::from(qemu_stderr));
        if kvm_available() {
            command.arg("-enable-kvm").arg("-cpu").arg("host");
        } else {
            command.arg("-accel").arg("tcg,thread=multi").arg("-cpu").arg("max");
        }
        let child = command.spawn()?;
        let mut guest = Self {
            child,
            port,
            private_key: media.private_key,
            root: runtime_root.to_path_buf(),
            qemu_stderr: qemu_stderr_path,
            boot_id: String::new(),
            version: String::new(),
        };
        let (boot_id, version) = guest.wait_ready(None, BOOT_TIMEOUT)?;
        guest.boot_id = boot_id;
        guest.version = version;
        Ok(guest)
    }

    pub(super) fn boot_id(&self) -> &str {
        &self.boot_id
    }

    pub(super) fn version(&self) -> &str {
        &self.version
    }

    pub(super) fn start_checkpoint(
        &mut self,
        command: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let remote = format!(
            "rm -f {GUEST_STATE}/stage.stdout {GUEST_STATE}/stage.stderr; \
             nohup /usr/local/bin/peritusd {command} --config {GUEST_STATE}/peritus.toml \
             >{GUEST_STATE}/stage.stdout 2>{GUEST_STATE}/stage.stderr </dev/null &"
        );
        let output = self.ssh(&remote)?;
        if !output.status.success() || !output.stderr.is_empty() {
            return Err(remote_failure("start reboot checkpoint", &output));
        }
        let started = Instant::now();
        loop {
            self.require_running()?;
            let output = self.ssh(&format!(
                "if [ -s {GUEST_STATE}/stage.stdout ]; then cat {GUEST_STATE}/stage.stdout; \
                 elif [ -s {GUEST_STATE}/stage.stderr ]; then cat {GUEST_STATE}/stage.stderr >&2; exit 2; \
                 else exit 1; fi"
            ))?;
            if output.status.success() {
                return one_line(&output.stdout, "guest reboot checkpoint");
            }
            if output.status.code() == Some(2) {
                return Err(remote_failure("reach reboot checkpoint", &output));
            }
            if started.elapsed() >= CHECKPOINT_TIMEOUT {
                return Err("guest candidate did not reach its reboot checkpoint".into());
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    pub(super) fn reboot(&mut self) -> Result<(String, String), Box<dyn std::error::Error>> {
        let previous = self.boot_id.clone();
        let _ = self.ssh("sync; reboot -f");
        let (current, version) = self.wait_ready(Some(&previous), REBOOT_TIMEOUT)?;
        if version != self.version {
            return Err("guest candidate version changed across host reboot".into());
        }
        self.boot_id.clone_from(&current);
        Ok((previous, current))
    }

    pub(super) fn run_candidate(
        &self,
        command: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let output = self.ssh(&format!(
            "/usr/local/bin/peritusd {command} --config {GUEST_STATE}/peritus.toml"
        ))?;
        if !output.status.success() || !output.stderr.is_empty() {
            return Err(remote_failure("run guest candidate recovery", &output));
        }
        one_line(&output.stdout, "guest candidate recovery")
    }

    pub(super) fn file(&self, path: &str) -> Result<RemoteFile, Box<dyn std::error::Error>> {
        let output = self.ssh(&format!("sha256sum {path}; stat -c %s {path}"))?;
        if !output.status.success() || !output.stderr.is_empty() {
            return Err(remote_failure("inspect guest file", &output));
        }
        let text = std::str::from_utf8(&output.stdout)?.trim_end_matches(['\r', '\n']);
        let mut lines = text.lines();
        let digest_line = lines.next().ok_or("guest file digest is missing")?;
        let bytes_line = lines.next().ok_or("guest file size is missing")?;
        if lines.next().is_some() {
            return Err("guest file inspection returned extra output".into());
        }
        let sha256 = digest_line
            .split_whitespace()
            .next()
            .filter(|value| {
                value.len() == 64
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
            .ok_or("guest file digest is malformed")?
            .to_owned();
        let bytes = bytes_line.parse::<u64>()?;
        if bytes == 0 {
            return Err("guest file is empty".into());
        }
        Ok(RemoteFile { sha256, bytes })
    }

    pub(super) fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        let _ = self.ssh("sync; poweroff -f");
        let started = Instant::now();
        while self.child.try_wait()?.is_none() {
            if started.elapsed() >= Duration::from_secs(30) {
                self.child.kill()?;
                self.child.wait()?;
                return Ok(());
            }
            thread::sleep(POLL_INTERVAL);
        }
        Ok(())
    }

    fn wait_ready(
        &mut self,
        previous: Option<&str>,
        timeout: Duration,
    ) -> Result<(String, String), Box<dyn std::error::Error>> {
        let started = Instant::now();
        thread::sleep(SSH_READY_POLL);
        loop {
            self.require_running()?;
            let output = self.ssh(&format!(
                "test -f {GUEST_STATE}/ready && cat /proc/sys/kernel/random/boot_id && /usr/local/bin/peritusd --version"
            ))?;
            if output.status.success() && output.stderr.is_empty() {
                let text = std::str::from_utf8(&output.stdout)?.trim_end_matches(['\r', '\n']);
                let mut lines = text.lines();
                let boot_id = parse::boot_id(lines.next().unwrap_or_default())?;
                let version = lines.next().ok_or("guest candidate version is missing")?.to_owned();
                if lines.next().is_none()
                    && version.starts_with("peritusd ")
                    && previous.is_none_or(|value| value != boot_id)
                {
                    return Ok((boot_id, version));
                }
            }
            if started.elapsed() >= timeout {
                return Err("disposable guest did not become ready before its deadline".into());
            }
            thread::sleep(SSH_READY_POLL);
        }
    }

    fn ssh(&self, remote: &str) -> Result<Output, std::io::Error> {
        Command::new("ssh")
            .arg("-i")
            .arg(&self.private_key)
            .arg("-p")
            .arg(self.port.to_string())
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("IdentitiesOnly=yes")
            .arg("-o")
            .arg("StrictHostKeyChecking=no")
            .arg("-o")
            .arg("UserKnownHostsFile=/dev/null")
            .arg("-o")
            .arg("LogLevel=ERROR")
            .arg("-o")
            .arg("ConnectTimeout=2")
            .arg("-o")
            .arg("ConnectionAttempts=1")
            .arg("root@127.0.0.1")
            .arg(remote)
            .current_dir(&self.root)
            .output()
    }

    fn require_running(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(status) = self.child.try_wait()? {
            let diagnostics = fs::read_to_string(&self.qemu_stderr).unwrap_or_default();
            return Err(format!(
                "disposable guest exited with {status}: {}",
                diagnostics.trim_end()
            )
            .into());
        }
        Ok(())
    }
}

impl Drop for Guest {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

fn available_port() -> Result<u16, std::io::Error> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.local_addr().map(|address| address.port())
}

fn kvm_available() -> bool {
    fs::OpenOptions::new().read(true).write(true).open("/dev/kvm").is_ok()
}

fn one_line(bytes: &[u8], label: &str) -> Result<String, Box<dyn std::error::Error>> {
    let text = std::str::from_utf8(bytes)?.trim_end_matches(['\r', '\n']);
    if text.is_empty() || text.contains(['\r', '\n']) {
        Err(format!("{label} is not one nonempty line").into())
    } else {
        Ok(text.to_owned())
    }
}

fn remote_failure(operation: &str, output: &Output) -> Box<dyn std::error::Error> {
    format!(
        "{operation} failed with {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim_end()
    )
    .into()
}
