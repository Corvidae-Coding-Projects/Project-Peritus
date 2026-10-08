//! Streamed PTY I/O and independently dispatched owner controls.

use super::Controls;
use super::super::{
    protocol::{self, Operation},
    receipt::{self, InputReceipt, InputState},
    record::{CloseDisposition, ConsoleRecord},
};
#[cfg(unix)]
use super::super::identity::signal_group;
#[cfg(windows)]
use super::super::identity::interrupt_child;
use crate::error::{Result, problem};
#[cfg(windows)]
use crate::error::uncertain;
#[cfg(unix)]
use peritus_process::{NativeProcessProbe, ProbeObservation, ProcessProbe};
use portable_pty::PtySize;
use sha2::{Digest as _, Sha256};
use std::{fs::OpenOptions, io::{ErrorKind, Read, Write}, net::TcpStream, path::{Path, PathBuf}, sync::{Arc, Mutex, atomic::Ordering, mpsc}, thread::{self, JoinHandle}, time::Duration};

struct WriteRequest {
    bytes: Vec<u8>,
    reply: mpsc::SyncSender<std::result::Result<(), String>>,
}

#[derive(Clone)]
pub(super) struct WriterLanes {
    input: mpsc::Sender<WriteRequest>,
}

impl WriterLanes {
    fn write(&self, bytes: Vec<u8>) -> Result<()> {
        dispatch(&self.input, bytes)
    }

}

fn dispatch(lane: &mpsc::Sender<WriteRequest>, bytes: Vec<u8>) -> Result<()> {
    let (reply, result) = mpsc::sync_channel(1);
    lane.send(WriteRequest { bytes, reply }).map_err(problem)?;
    result.recv().map_err(problem)?.map_err(problem)
}

pub(super) fn start_writer(mut writer: Box<dyn Write + Send>) -> Result<WriterLanes> {
    let (input, inputs) = mpsc::channel::<WriteRequest>();
    thread::Builder::new()
        .name("peritus-console-input".into())
        .spawn(move || while let Ok(request) = inputs.recv() {
            let result = writer
                .write_all(&request.bytes)
                .and_then(|()| writer.flush())
                .map_err(|error| error.to_string());
            let _ = request.reply.send(result);
        })
        .map_err(problem)?;
    Ok(WriterLanes { input })
}

pub(super) fn capture_output(
    mut reader: Box<dyn Read + Send>,
    output_path: &Path,
    record_path: &Path,
    record: &Mutex<ConsoleRecord>,
) -> Result<()> {
    let mut output = OpenOptions::new().append(true).open(output_path)?;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        output.write_all(&buffer[..count])?;
        output.sync_data()?;
        let mut locked = record.lock().map_err(problem)?;
        locked.output_bytes = locked
            .output_bytes
            .checked_add(u64::try_from(count).map_err(problem)?)
            .ok_or_else(|| problem("Console output offset overflowed"))?;
        locked.publish(record_path)?;
    }
    Ok(())
}

pub(super) fn handle(
    mut stream: TcpStream,
    token: String,
    controls: Arc<Controls>,
    record: Arc<Mutex<ConsoleRecord>>,
    record_path: PathBuf,
    console_directory: PathBuf,
) -> Result<JoinHandle<Result<()>>> {
    thread::Builder::new()
        .name("peritus-console-control".into())
        .spawn(move || {
            stream.set_read_timeout(Some(Duration::from_millis(250)))?;
            let acknowledgement = match protocol::read_header(&mut stream, &token, || {
                controls.stopping.load(Ordering::Acquire)
            })? {
                Operation::Input if !controls.stopping.load(Ordering::Acquire) => {
                    input(&mut stream, &controls, &console_directory)?
                }
                Operation::Resize if !controls.stopping.load(Ordering::Acquire) => {
                    resize(&mut stream, &controls)?;
                    protocol::ACK
                }
                Operation::Terminate => {
                    terminate(&controls, &record, &record_path)?;
                    protocol::ACK
                }
                Operation::Interrupt if !controls.stopping.load(Ordering::Acquire) => {
                    interrupt(&controls)?;
                    protocol::ACK
                }
                Operation::Input | Operation::Resize | Operation::Interrupt => {
                    return Err(problem("Console lifecycle settlement is already in progress"));
                }
            };
            protocol::respond(&mut stream, acknowledgement)
        })
        .map_err(problem)
}

fn input(stream: &mut TcpStream, controls: &Controls, console: &Path) -> Result<u8> {
    let (input, digest) = protocol::read_input_identity(stream, || {
        controls.stopping.load(Ordering::Acquire)
    })?;
    let _settlement = controls.input_settlement.lock().map_err(problem)?;
    let mut receipt = InputReceipt::read(console, &input)?
        .ok_or_else(|| problem("The exact retained terminal input receipt is unavailable"))?;
    if receipt.digest != digest {
        return Err(problem("Terminal input identity was reused with another body digest"));
    }
    match receipt.state {
        InputState::Settled => return Ok(protocol::ACK),
        InputState::Unknown => return Ok(protocol::UNKNOWN),
        InputState::Pending => {}
    }
    let body_path = receipt::body_path(console, &input)?;
    let mut body = std::fs::File::open(&body_path)?;
    if body.metadata()?.len() != receipt.bytes {
        return Err(problem("Retained terminal input length conflicts with its receipt"));
    }
    let mut hasher = Sha256::new();
    std::io::copy(&mut body, &mut HashWriter(&mut hasher))?;
    let observed = crate::state::hex(&hasher.finalize());
    if observed != receipt.digest {
        return Err(problem("Retained terminal input digest does not match its receipt"));
    }
    receipt.state = InputState::Unknown;
    receipt.publish(console)?;
    let mut body = std::fs::File::open(body_path)?;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = body.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        controls.writer.write(buffer[..count].to_vec())?;
    }
    receipt.state = InputState::Settled;
    receipt.publish(console)?;
    Ok(protocol::ACK)
}

struct HashWriter<'a>(&'a mut Sha256);

impl Write for HashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

#[cfg(unix)]
fn interrupt(controls: &Controls) -> Result<()> {
    let mut probe = NativeProcessProbe::new();
    match probe.observe(controls.child.identity()).map_err(problem)? {
        ProbeObservation::ExactLive => signal_group(&controls.child, libc::SIGINT),
        ProbeObservation::ExactAbsent => Err(problem("Console process has already exited")),
        ProbeObservation::Mismatched | ProbeObservation::Unverifiable => {
            Err(problem("Console process identity cannot be verified for interruption"))
        }
    }
}

#[cfg(windows)]
fn interrupt(controls: &Controls) -> Result<()> {
    let _interrupt = controls.interrupt.lock().map_err(problem)?;
    interrupt_child(&controls.child, &controls.job)
}

fn resize(stream: &mut TcpStream, controls: &Controls) -> Result<()> {
    let mut bytes = [0_u8; 8];
    protocol::read_fully(stream, &mut bytes, || controls.stopping.load(Ordering::Acquire))?;
    controls
        .master
        .lock()
        .map_err(problem)?
        .resize(PtySize {
            cols: u16::from_be_bytes([bytes[0], bytes[1]]),
            rows: u16::from_be_bytes([bytes[2], bytes[3]]),
            pixel_width: u16::from_be_bytes([bytes[4], bytes[5]]),
            pixel_height: u16::from_be_bytes([bytes[6], bytes[7]]),
        })
        .map_err(problem)
}

fn terminate(
    controls: &Controls,
    record: &Mutex<ConsoleRecord>,
    record_path: &Path,
) -> Result<()> {
    {
        let mut locked = record.lock().map_err(problem)?;
        if locked.close_disposition == Some(CloseDisposition::Dismiss) {
            return Err(problem("A dismissed console cannot be terminated"));
        }
        if locked.close_disposition == Some(CloseDisposition::Terminate)
            && locked.close_confirmed
        {
            #[cfg(unix)]
            {
                return Ok(());
            }
            #[cfg(windows)]
            {
                if locked.ended() {
                    return Ok(());
                }
                // Older Windows owners could publish confirmation before TerminateJobObject. A
                // live record is an intent that still requires exact process-tree quiescence.
                locked.close_confirmed = false;
            }
        }
        locked.close_disposition = Some(CloseDisposition::Terminate);
        locked.publish(record_path)?;
    }
    #[cfg(windows)]
    controls.stopping.store(true, Ordering::Release);
    match terminate_child(controls) {
        Ok(()) => {
            #[cfg(unix)]
            {
                let mut locked = record.lock().map_err(problem)?;
                locked.close_confirmed = true;
                locked.publish(record_path)?;
                drop(locked);
                controls.stopping.store(true, Ordering::Release);
                Ok(())
            }
            #[cfg(windows)]
            {
                // The exact Job includes this owner, so it cannot truthfully publish or
                // acknowledge quiescence after requesting its own termination. Gateway recovery
                // confirms the durable intent only after Job accounting reaches zero.
                Err(uncertain(
                    "Windows console termination is awaiting exact Job Object quiescence",
                ))
            }
        }
        Err(error) => {
            let mut locked = record.lock().map_err(problem)?;
            locked.error = Some(error.0.clone());
            locked.publish(record_path)?;
            Err(error)
        }
    }
}

#[cfg(unix)]
pub(super) fn terminate_child(controls: &Controls) -> Result<()> {
    let mut probe = NativeProcessProbe::new();
    match probe.observe(controls.child.identity()).map_err(problem)? {
        ProbeObservation::ExactLive => probe.terminate(controls.child.identity()).map_err(problem),
        ProbeObservation::ExactAbsent => Err(problem(
            "Console root exited before exact process-group termination could be requested",
        )),
        ProbeObservation::Mismatched | ProbeObservation::Unverifiable => {
            Err(problem("Console process identity cannot be verified for termination"))
        }
    }
}

#[cfg(windows)]
pub(super) fn terminate_child(controls: &Controls) -> Result<()> {
    let _termination = controls.interrupt.lock().map_err(problem)?;
    controls.job.terminate()
}

pub(super) fn join_finished(handlers: &mut Vec<JoinHandle<Result<()>>>) {
    let mut index = 0;
    while index < handlers.len() {
        if handlers[index].is_finished() {
            let _ = handlers.swap_remove(index).join();
        } else {
            index += 1;
        }
    }
}
