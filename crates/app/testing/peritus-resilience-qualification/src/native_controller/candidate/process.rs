//! Bounded child-process execution for the staged H1 daemon.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::Read as _;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub(super) const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const PROCESS_POLL: Duration = Duration::from_millis(20);
pub(super) const MAX_OUTPUT_BYTES: u64 = 64 * 1024;

pub(super) struct CommandOutput {
    pub(super) status: ExitStatus,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
}

pub(super) fn bounded_command<'a>(
    executable: &Path,
    arguments: impl IntoIterator<Item = &'a OsStr>,
    cwd: &Path,
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<CommandOutput, Box<dyn std::error::Error>> {
    let stdout = create_output(stdout_path)?;
    let stderr = create_output(stderr_path)?;
    let mut command = candidate_command(executable, cwd);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    let mut child = command.spawn()?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= PROCESS_TIMEOUT {
            terminate(&mut child)?;
            return Err("staged peritusd command exceeded 30 seconds".into());
        }
        thread::sleep(PROCESS_POLL);
    };
    Ok(CommandOutput {
        status,
        stdout: read_bounded(stdout_path)?,
        stderr: read_bounded(stderr_path)?,
    })
}

pub(super) fn candidate_command(executable: &Path, cwd: &Path) -> Command {
    let mut command = Command::new(executable);
    command.current_dir(cwd).env_clear();
    for name in ["PATH", "SYSTEMROOT", "WINDIR", "TEMP", "TMP", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

pub(super) fn terminate(child: &mut Child) -> Result<(), std::io::Error> {
    if child.try_wait()?.is_none() {
        child.kill()?;
    }
    child.wait().map(|_| ())
}

pub(super) fn create_output(path: &Path) -> Result<File, std::io::Error> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(path)
}

pub(super) fn read_bounded(path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_OUTPUT_BYTES {
        return Err("staged peritusd output exceeded its byte limit".into());
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len())?);
    File::open(path)?.take(MAX_OUTPUT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_OUTPUT_BYTES {
        return Err("staged peritusd output exceeded its byte limit".into());
    }
    Ok(bytes)
}

pub(super) fn one_line(bytes: &[u8], label: &str) -> Result<String, Box<dyn std::error::Error>> {
    let text = std::str::from_utf8(bytes)?.trim_end_matches(['\r', '\n']);
    if text.is_empty() || text.contains(['\r', '\n']) {
        return Err(format!("{label} is not one nonempty line").into());
    }
    Ok(text.to_owned())
}
