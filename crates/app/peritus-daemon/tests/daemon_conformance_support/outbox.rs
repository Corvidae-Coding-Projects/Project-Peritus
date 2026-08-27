//! Effect-before-ack crash qualification through real `peritusd` subprocesses.

use std::fs;
use std::io::{self, BufRead, BufReader, Read};
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use peritus_conformance::{DaemonConformanceObservation, DaemonOutboxObservation};

use super::process::{TestEnvironment, peritusd_executable};

const PROCESS_BOUND: Duration = Duration::from_secs(10);
const OUTPUT_BOUND: usize = 4_096;
const STAGE_PREFIX: &str = "peritus-qualification outbox-stage ";
const QUALIFICATION_TOKEN: &str = "peritus-qualification";
const RECOVERY_TOKEN: &str = "outbox-recover";
const EFFECT_FILE: &str = "delivery-00000000000000000000000000000001.effect";

/// Exercises the public post-effect checkpoint and restart recovery boundary.
pub(super) fn crash_recovery() -> io::Result<DaemonConformanceObservation> {
    let environment = TestEnvironment::new()?;
    let mut daemon = environment.start()?;
    daemon.kill_for_restart()?;
    stage_and_kill(&environment)?;
    let recovery = recover(&environment)?;
    Ok(DaemonConformanceObservation::Outbox(recovery))
}

fn stage_and_kill(environment: &TestEnvironment) -> io::Result<()> {
    let child = Command::new(peritusd_executable()?)
        .arg("qualify-outbox-stage")
        .arg("--config")
        .arg(environment.config_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut child = ChildGuard::new(child);
    let stdout = child.take_stdout()?;
    let (checkpoint_tx, checkpoint_rx) = mpsc::sync_channel(1);
    let reader = thread::Builder::new().name("peritus-outbox-checkpoint-reader".to_owned()).spawn(
        move || {
            let result = read_checkpoint_line(stdout);
            let _ = checkpoint_tx.send(result);
        },
    )?;

    let checkpoint_result =
        checkpoint_rx.recv_timeout(PROCESS_BOUND).map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => io::Error::new(
                io::ErrorKind::TimedOut,
                "outbox checkpoint line exceeded time bound",
            ),
            mpsc::RecvTimeoutError::Disconnected => {
                io::Error::other("outbox checkpoint reader disconnected")
            }
        });
    let premature = child.kill_at_checkpoint();
    let joined = reader.join().map_err(|_| io::Error::other("outbox checkpoint reader panicked"));

    joined?;
    let line = checkpoint_result??;
    let premature = premature?;
    let stderr = child.read_stderr()?;
    if let Some(status) = premature {
        return Err(io::Error::other(format!(
            "outbox stage exited before the adapter kill with {status}: {stderr}",
        )));
    }
    validate_checkpoint(environment, &line)
}

fn recover(environment: &TestEnvironment) -> io::Result<DaemonOutboxObservation> {
    let child = Command::new(peritusd_executable()?)
        .arg("qualify-outbox-recover")
        .arg("--config")
        .arg(environment.config_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut child = ChildGuard::new(child);
    let status = TestEnvironment::wait_for_exit(child.child_mut()?)?;
    let stdout = child.read_stdout()?;
    let stderr = child.read_stderr()?;
    if !status.success() {
        return Err(io::Error::other(format!("outbox recovery exited with {status}: {stderr}")));
    }
    let line = one_output_line(&stdout)?;
    parse_recovery(line)
}

fn read_checkpoint_line(stdout: ChildStdout) -> io::Result<String> {
    let mut bytes = Vec::new();
    let mut bounded = BufReader::new(stdout).take((OUTPUT_BOUND + 1) as u64);
    bounded.read_until(b'\n', &mut bytes)?;
    if bytes.is_empty() || bytes.len() > OUTPUT_BOUND || bytes.last() != Some(&b'\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "outbox checkpoint output is empty, oversized, or unterminated",
        ));
    }
    bytes.pop();
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn validate_checkpoint(environment: &TestEnvironment, line: &str) -> io::Result<()> {
    let body = line.strip_prefix(STAGE_PREFIX).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "outbox checkpoint prefix differs")
    })?;
    let (effect, fence) = body.rsplit_once(" claim_fence=").ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "outbox checkpoint fields are incomplete")
    })?;
    let effect_path =
        environment.state_root().join("outbox-crash-qualification-v1").join(EFFECT_FILE);
    if effect != format!("effect_path={}", effect_path.display()) || parse_u64(fence)? != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "outbox checkpoint did not name the exact effect and first live fence",
        ));
    }
    if !fs::symlink_metadata(effect_path)?.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "outbox checkpoint effect is not a regular file",
        ));
    }
    Ok(())
}

fn parse_recovery(line: &str) -> io::Result<DaemonOutboxObservation> {
    let mut fields = line.split_ascii_whitespace();
    require_token(fields.next(), QUALIFICATION_TOKEN)?;
    require_token(fields.next(), RECOVERY_TOKEN)?;
    let destination_reconciled = parse_bool_field(fields.next(), "destination_reconciled")?;
    let external_effects = parse_u64_field(fields.next(), "external_effects")?;
    let duplicate_effects = parse_u64_field(fields.next(), "duplicate_effects")?;
    let exact_fence_acknowledged = parse_bool_field(fields.next(), "exact_fence_acknowledged")?;
    let pending_claims = parse_u64_field(fields.next(), "pending_claims")?;
    if fields.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "outbox recovery output contains unknown fields",
        ));
    }
    Ok(DaemonOutboxObservation::new(
        destination_reconciled,
        external_effects,
        duplicate_effects,
        exact_fence_acknowledged,
        pending_claims,
    ))
}

fn require_token(observed: Option<&str>, expected: &str) -> io::Result<()> {
    if observed == Some(expected) {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, "outbox recovery output prefix differs"))
    }
}

fn parse_bool_field(field: Option<&str>, name: &str) -> io::Result<bool> {
    match field_value(field, name)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "outbox recovery boolean is not canonical",
        )),
    }
}

fn parse_u64_field(field: Option<&str>, name: &str) -> io::Result<u64> {
    parse_u64(field_value(field, name)?)
}

fn field_value<'a>(field: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let field = field.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "outbox recovery field is missing")
    })?;
    let (observed_name, value) = field.split_once('=').ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "outbox recovery field has no value")
    })?;
    if observed_name != name || value.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "outbox recovery field name or value differs",
        ));
    }
    Ok(value)
}

fn parse_u64(value: &str) -> io::Result<u64> {
    value.parse().map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn one_output_line(bytes: &[u8]) -> io::Result<&str> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut lines = text.lines();
    let line = lines.next().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "outbox recovery output is empty")
    })?;
    if lines.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "outbox recovery emitted more than one observation line",
        ));
    }
    Ok(line)
}

struct ChildGuard {
    child: Option<Child>,
}

impl ChildGuard {
    const fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn child_mut(&mut self) -> io::Result<&mut Child> {
        self.child
            .as_mut()
            .ok_or_else(|| io::Error::other("qualification child is no longer owned"))
    }

    fn take_stdout(&mut self) -> io::Result<ChildStdout> {
        self.child_mut()?
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("qualification child has no stdout pipe"))
    }

    fn kill_at_checkpoint(&mut self) -> io::Result<Option<ExitStatus>> {
        let child = self.child_mut()?;
        let premature = child.try_wait()?;
        if premature.is_none() {
            child.kill()?;
            let _ = child.wait()?;
        }
        Ok(premature)
    }

    fn read_stdout(&mut self) -> io::Result<Vec<u8>> {
        let stdout = self
            .child_mut()?
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("qualification child has no stdout pipe"))?;
        read_bounded(stdout)
    }

    fn read_stderr(&mut self) -> io::Result<String> {
        let stderr = self
            .child_mut()?
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("qualification child has no stderr pipe"))?;
        let bytes = read_bounded(stderr)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn read_bounded(reader: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take((OUTPUT_BOUND + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_BOUND {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "qualification subprocess output exceeded its bound",
        ));
    }
    Ok(bytes)
}
