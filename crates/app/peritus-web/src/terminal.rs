//! Durable PTY references backed by independent owner processes and append-only output.

mod identity;
mod owner;
mod protocol;
pub(crate) mod receipt;
mod record;
mod registry;

use crate::{
    error::{Result, problem, uncertain},
    state::App,
};
use base64::Engine;
use peritus_process::{NativeProcessProbe, ProbeObservation, ProcessProbe};
pub(crate) use protocol::Operation;
pub(crate) use record::CloseDisposition;
pub(crate) use registry::TerminalRegistry;
use record::{
    ConsoleRecord, OUTPUT_FILE, Phase, RECORD_FILE,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    net::{Shutdown, TcpStream},
    path::{Path, PathBuf},
};

const OUTPUT_PAGE_BYTES: u64 = 256 * 1024;
pub(crate) const OWNER_ACK: u8 = protocol::ACK;
pub(crate) const OWNER_UNKNOWN: u8 = protocol::UNKNOWN;

pub(crate) struct Terminal {
    pub(super) directory: PathBuf,
    pub(super) workspace: String,
}

pub(crate) struct OwnerConnection {
    pub(crate) endpoint: std::net::SocketAddr,
    pub(crate) header: Vec<u8>,
}

impl Terminal {
    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }
    pub(crate) fn start(
        app: &App,
        operation: &str,
        console: crate::consoles::Console,
        arguments: Vec<String>,
    ) -> Result<()> {
        let project = app.project(&console.project)?;
        app.terminals.launch(
            operation,
            console,
            &app.options.cli,
            &project.root,
            arguments,
        )
    }

    pub(crate) fn summary(&self) -> Result<Value> {
        let record = reconcile(&self.record_path())?;
        let available = owner_live(&record);
        let mut value = serde_json::to_value(&record.console)?;
        value["workspace"] = json!(self.workspace);
        value["ended"] = json!(record.ended());
        value["available"] = json!(available || record.ended());
        if record.error.is_some() || !available && !record.ended() {
            value["recovery"] = json!(record.error.unwrap_or_else(|| {
                "The durable process identity remains live, but its PTY owner is unavailable"
                    .into()
            }));
        }
        Ok(value)
    }

    pub(crate) fn read(&self, after: u64, recover_input: bool) -> Result<Value> {
        let record = reconcile(&self.record_path())?;
        let mut output = std::fs::File::open(self.directory.join(OUTPUT_FILE))?;
        let retained = output.metadata()?.len();
        if after > retained {
            return Err(problem("Console output offset is beyond the retained stream"));
        }
        output.seek(SeekFrom::Start(after))?;
        let mut bytes = Vec::new();
        output.take(OUTPUT_PAGE_BYTES).read_to_end(&mut bytes)?;
        let next = after
            .checked_add(u64::try_from(bytes.len()).map_err(problem)?)
            .ok_or_else(|| problem("Console output offset overflowed"))?;
        let (recoverable_input, input_recovery_error) = if recover_input {
            match receipt::InputReceipt::recoverable(&self.directory) {
                Ok(value) => (value, None),
                Err(error) => (None, Some(error.0)),
            }
        } else {
            (None, None)
        };
        Ok(json!({
            "data":base64::engine::general_purpose::STANDARD.encode(&bytes),
            "next":next.to_string(),
            "lost":false,
            "ended":record.ended(),
            "available":owner_live(&record) || record.ended(),
            "error":record.error,
            "recoverableInput":recoverable_input,
            "inputRecoveryError":input_recovery_error,
        }))
    }

    pub(crate) fn connection(&self, operation: Operation) -> Result<OwnerConnection> {
        let record = reconcile(&self.record_path())?;
        owner_connection(&record, operation)
    }

    pub(crate) fn input_connection(&self, input: &str, digest: &str) -> Result<OwnerConnection> {
        let record = reconcile(&self.record_path())?;
        if record.legacy() {
            return Err(problem(
                "This recovered legacy console cannot accept exact retained input; terminate it or wait for it to exit",
            ));
        }
        let mut connection = owner_connection(&record, Operation::Input)?;
        connection.header = protocol::input_header(&connection.header, input, digest)?;
        Ok(connection)
    }

    pub(crate) fn close(&self, disposition: CloseDisposition) -> Result<()> {
        let record_path = self.record_path();
        let mut record = reconcile(&record_path)?;
        while record.ended() && owner_live(&record) {
            std::thread::sleep(std::time::Duration::from_millis(10));
            record = ConsoleRecord::read(&record_path)?;
        }
        if record.ended() {
            record = reconcile(&record_path)?;
        }
        if record.close_disposition == Some(CloseDisposition::Terminate)
            && disposition != CloseDisposition::Terminate
        {
            return Err(problem(
                "A retained console termination intent cannot be replaced by dismissal",
            ));
        }
        match disposition {
            CloseDisposition::Dismiss if !record.ended() => {
                return Err(problem("Only an ended console can be dismissed"));
            }
            CloseDisposition::Dismiss => {
                record.close_disposition = Some(CloseDisposition::Dismiss);
                record.close_confirmed = true;
                record.publish(&record_path)
            }
            CloseDisposition::Terminate if record.ended() => {
                #[cfg(windows)]
                {
                    terminate_recovered(&mut record, &record_path)
                }
                #[cfg(unix)]
                {
                    record.close_disposition = Some(CloseDisposition::Terminate);
                    record.close_confirmed = true;
                    record.publish(&record_path)
                }
            }
            CloseDisposition::Terminate if owner_live(&record) => {
                let mut stream = TcpStream::connect(record.endpoint()?)?;
                if let Err(error) = stream.write_all(&protocol::header(
                    Operation::Terminate,
                    &record.token,
                )?) {
                    return self.reconcile_termination(error);
                }
                if let Err(error) = stream.shutdown(Shutdown::Write) {
                    return self.reconcile_termination(error);
                }
                let mut acknowledgement = [0_u8; 1];
                if let Err(error) = stream.read_exact(&mut acknowledgement) {
                    return self.reconcile_termination(error);
                }
                if acknowledgement != [protocol::ACK] {
                    return self.reconcile_termination("Console owner acknowledgement is invalid");
                }
                Ok(())
            }
            CloseDisposition::Terminate => terminate_recovered(&mut record, &record_path),
        }
    }

    fn record_path(&self) -> PathBuf {
        self.directory.join(RECORD_FILE)
    }

    fn reconcile_termination(&self, error: impl std::fmt::Display) -> Result<()> {
        let record = reconcile(&self.record_path())?;
        if record.close_confirmed
            && record.close_disposition == Some(CloseDisposition::Terminate)
        {
            return Ok(());
        }
        Err(uncertain(error))
    }
}

fn owner_connection(record: &ConsoleRecord, operation: Operation) -> Result<OwnerConnection> {
    if record.ended() || record.closed() {
        return Err(problem("Console process has ended"));
    }
    if !owner_live(record) {
        return Err(problem(
            "Console PTY owner is unavailable; the exact process identity was retained",
        ));
    }
    Ok(OwnerConnection {
        endpoint: record.endpoint()?,
        header: protocol::header(operation, &record.token)?,
    })
}

pub(crate) fn run_owner(path: &Path) -> Result<()> {
    owner::run(path)
}

pub(crate) fn resize_payload(
    cols: u64,
    rows: u64,
    pixel_width: u64,
    pixel_height: u64,
) -> Result<[u8; 8]> {
    let cols = u16::try_from(cols).map_err(|_| problem("PTY column count exceeds OS bounds"))?;
    let rows = u16::try_from(rows).map_err(|_| problem("PTY row count exceeds OS bounds"))?;
    let pixel_width = u16::try_from(pixel_width)
        .map_err(|_| problem("PTY pixel width exceeds OS bounds"))?;
    let pixel_height = u16::try_from(pixel_height)
        .map_err(|_| problem("PTY pixel height exceeds OS bounds"))?;
    let mut bytes = [0_u8; 8];
    bytes[..2].copy_from_slice(&cols.to_be_bytes());
    bytes[2..4].copy_from_slice(&rows.to_be_bytes());
    bytes[4..6].copy_from_slice(&pixel_width.to_be_bytes());
    bytes[6..].copy_from_slice(&pixel_height.to_be_bytes());
    Ok(bytes)
}

fn reconcile(record_path: &Path) -> Result<ConsoleRecord> {
    let mut record = ConsoleRecord::read(record_path)?;
    let output_path = record_path
        .parent()
        .ok_or_else(|| problem("Console record has no parent"))?
        .join(OUTPUT_FILE);
    let retained = std::fs::metadata(output_path)?.len();
    if retained < record.output_bytes {
        return Err(problem("Durable console output is shorter than its recorded offset"));
    }
    let owner_available = owner_live(&record);
    let mut changed = false;
    if !owner_available && retained != record.output_bytes {
        record.output_bytes = retained;
        changed = true;
    }
    #[cfg(windows)]
    if record.phase == Phase::Running
        && record.close_disposition == Some(CloseDisposition::Terminate)
    {
        if child_tree_absent(&record)? {
            record.phase = Phase::Failed;
            record.close_confirmed = true;
            record.error = Some(
                "The exact Windows console Job Object is quiescent after its retained termination intent"
                    .into(),
            );
            changed = true;
        } else if record.close_confirmed {
            // Repair confirmations written by the former pre-TerminateJobObject path. A live Job
            // is exact evidence that the durable termination intent has not settled yet.
            record.close_confirmed = false;
            record.error = Some(
                "The retained Windows console termination intent is not yet quiescent".into(),
            );
            changed = true;
        }
    }
    if record.phase != Phase::Running || owner_available {
        if changed {
            record.publish(record_path)?;
        }
        return Ok(record);
    }
    let mut probe = NativeProcessProbe::new();
    match probe.observe(record.child.identity()).map_err(problem)? {
        ProbeObservation::ExactAbsent => {
            if child_tree_absent(&record)? {
                record.phase = Phase::Failed;
                if record.close_disposition == Some(CloseDisposition::Terminate) {
                    record.close_confirmed = true;
                }
                record.error = Some(
                    "The PTY owner ended before it published the child process result; the exact child process group is absent"
                        .into(),
                );
            } else {
                record.error = Some(
                    "The PTY root is absent but its process tree is not provably quiescent; no lifecycle action was taken"
                        .into(),
                );
            }
        }
        ProbeObservation::ExactLive => {
            record.error = Some(
                "The exact child process is live, but its PTY owner is unavailable; terminate it explicitly or preserve it for inspection"
                    .into(),
            );
        }
        ProbeObservation::Mismatched => {
            record.error = Some(
                "The retained child PID now names a different process; no lifecycle action was taken"
                    .into(),
            );
        }
        ProbeObservation::Unverifiable => {
            record.error = Some(
                "The retained child process identity cannot be verified; no lifecycle action was taken"
                    .into(),
            );
        }
    }
    record.publish(record_path)?;
    Ok(record)
}

#[cfg(unix)]
fn terminate_recovered(record: &mut ConsoleRecord, record_path: &Path) -> Result<()> {
    if record.close_disposition.is_some()
        && record.close_disposition != Some(CloseDisposition::Terminate)
    {
        return Err(problem(
            "A retained console close disposition cannot be replaced by termination",
        ));
    }
    record.close_disposition = Some(CloseDisposition::Terminate);
    record.publish(record_path)?;
    let mut probe = NativeProcessProbe::new();
    let result = match probe.observe(record.child.identity()).map_err(problem)? {
        ProbeObservation::ExactLive => probe.terminate(record.child.identity()).map_err(problem),
        ProbeObservation::ExactAbsent if child_tree_absent(record)? => {
            record.phase = Phase::Failed;
            Ok(())
        }
        ProbeObservation::ExactAbsent => {
            Err(problem(
                "Console root exited before recovered process-group termination could be proven safe",
            ))
        }
        ProbeObservation::Mismatched | ProbeObservation::Unverifiable => {
            Err(problem(
                "Console process identity cannot be verified for recovered termination",
            ))
        }
    };
    if let Err(error) = result {
        record.error = Some(error.0.clone());
        record.publish(record_path)?;
        return Err(error);
    }
    record.close_confirmed = true;
    record.publish(record_path)
}

#[cfg(windows)]
fn terminate_recovered(record: &mut ConsoleRecord, record_path: &Path) -> Result<()> {
    record.close_disposition = Some(CloseDisposition::Terminate);
    record.close_confirmed = false;
    record.publish(record_path)?;
    if child_tree_absent(record)? {
        if record.phase == Phase::Running {
            record.phase = Phase::Failed;
        }
        record.close_confirmed = true;
        record.error = Some(
            "The exact Windows console Job Object is quiescent after its retained termination intent"
                .into(),
        );
        return record.publish(record_path);
    }
    let name = record
        .child
        .job
        .as_deref()
        .ok_or_else(|| problem("The Windows console has no exact Job Object identity"))?;
    let job = identity::WindowsJob::open(name)?;
    if identity::current_start_token(record.child.pid) == Some(record.child.start_token)
        && !job.contains(record.child.pid)?
    {
        return Err(problem("The exact console root is outside its retained Job Object"));
    }
    if let Err(error) = job.terminate() {
        record.error = Some(error.0.clone());
        record.publish(record_path)?;
        return Err(error);
    }
    let active = match job.active_processes() {
        Ok(active) => active,
        Err(error) => {
            record.error = Some(error.0.clone());
            record.publish(record_path)?;
            return Err(uncertain(error));
        }
    };
    if active != 0 {
        let detail = format!(
            "Windows console termination was requested, but {active} exact Job Object processes remain active"
        );
        record.error = Some(detail.clone());
        record.publish(record_path)?;
        return Err(uncertain(detail));
    }
    record.phase = Phase::Failed;
    record.close_confirmed = true;
    record.error = Some(
        "The exact Windows console Job Object is quiescent after recovered termination".into(),
    );
    record.publish(record_path)
}

fn owner_live(record: &ConsoleRecord) -> bool {
    identity::current_start_token(record.owner.pid) == Some(record.owner.start_token)
}

#[cfg(unix)]
fn child_tree_absent(record: &ConsoleRecord) -> Result<bool> {
    identity::process_group_absent(&record.child)
}

#[cfg(windows)]
fn child_tree_absent(record: &ConsoleRecord) -> Result<bool> {
    let name = record
        .child
        .job
        .as_deref()
        .ok_or_else(|| problem("The Windows console has no exact Job Object identity"))?;
    match identity::WindowsJob::open_optional(name)? {
        Some(job) => Ok(job.active_processes()? == 0),
        None => Ok(identity::current_start_token(record.child.pid).is_none()),
    }
}

#[cfg(test)]
mod tests;
