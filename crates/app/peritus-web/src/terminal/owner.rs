//! Independent PTY owner process. Gateway shutdown never drops these handles.

use super::{
    identity::{child_binding, current_start_token},
    record::{
        CloseDisposition, ConsoleRecord, LaunchManifest, OUTPUT_FILE, Phase, ProcessBinding,
        RECORD_FILE,
    },
};
#[cfg(windows)]
use super::identity::WindowsJob;
use crate::error::{Result, problem};
use portable_pty::{CommandBuilder, MasterPty, PtySize};
use std::{
    fs::OpenOptions,
    io::ErrorKind,
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, TryRecvError},
    },
    thread,
    time::Duration,
};

mod control;
use control::{capture_output, handle, join_finished, terminate_child};

struct Controls {
    writer: control::WriterLanes,
    master: Mutex<Box<dyn MasterPty + Send>>,
    #[cfg(windows)]
    job: WindowsJob,
    child: ProcessBinding,
    #[cfg(windows)]
    interrupt: Mutex<()>,
    input_settlement: Mutex<()>,
    stopping: AtomicBool,
}

struct LaunchGuard(Option<Box<dyn portable_pty::Child + Send + Sync>>);

impl Drop for LaunchGuard {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(super) fn run(manifest_path: &Path) -> Result<()> {
    let manifest = LaunchManifest::read(manifest_path)?;
    let directory = manifest_path
        .parent()
        .ok_or_else(|| problem("Console launch path has no parent"))?;
    let record_path = directory.join(RECORD_FILE);
    let output_path = directory.join(OUTPUT_FILE);
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;

    #[cfg(windows)]
    let job = WindowsJob::create(&manifest.token)?;

    let pair = portable_pty::native_pty_system()
        .openpty(PtySize { rows: 30, cols: 100, pixel_width: 0, pixel_height: 0 })
        .map_err(problem)?;
    let mut command = CommandBuilder::new(&manifest.executable);
    command.args(manifest.arguments.clone());
    command.cwd(&manifest.working_directory);
    command.env("TERM", "xterm-256color");
    let child = pair.slave.spawn_command(command).map_err(problem)?;
    drop(pair.slave);
    let mut child = LaunchGuard(Some(child));

    let child_pid = child
        .0
        .as_ref()
        .expect("launch guard owns the unpublished child")
        .process_id()
        .ok_or_else(|| problem("PTY child did not publish a process identifier"))?;
    #[cfg(unix)]
    let child_binding = child_binding(child_pid)?;
    #[cfg(windows)]
    let child_binding = child_binding(child_pid, &job)?;
    let owner_binding = ProcessBinding {
        pid: std::process::id(),
        start_token: current_start_token(std::process::id())
            .ok_or_else(|| problem("Console owner birth identity is unavailable"))?,
        process_group: None,
        #[cfg(unix)]
        job: None,
        #[cfg(windows)]
        job: Some(job.name.clone()),
        complete_containment: cfg!(windows),
    };
    let reader = pair.master.try_clone_reader().map_err(problem)?;
    let writer = control::start_writer(pair.master.take_writer().map_err(problem)?)?;
    let controls = Arc::new(Controls {
        writer,
        master: Mutex::new(pair.master),
        #[cfg(windows)]
        job,
        child: child_binding.clone(),
        #[cfg(windows)]
        interrupt: Mutex::new(()),
        input_settlement: Mutex::new(()),
        stopping: AtomicBool::new(false),
    });
    let record = Arc::new(Mutex::new(ConsoleRecord::running(
        &manifest,
        listener.local_addr()?,
        owner_binding,
        child_binding,
    )));

    OpenOptions::new().write(true).create_new(true).open(&output_path)?.sync_all()?;
    record.lock().map_err(problem)?.publish(&record_path)?;
    let _ = std::fs::remove_file(manifest_path);

    let (reader_sender, reader_receiver) = mpsc::channel();
    let reader_record = Arc::clone(&record);
    let reader_record_path = record_path.clone();
    let reader_thread = thread::Builder::new()
        .name("peritus-console-output".into())
        .spawn(move || {
            let result = capture_output(reader, &output_path, &reader_record_path, &reader_record);
            let _ = reader_sender.send(result);
        })
        .map_err(problem)?;
    let (wait_sender, wait_receiver) = mpsc::channel();
    let mut child = child.0.take().expect("published console transfers child to its waiter");
    let wait_thread = match thread::Builder::new()
        .name("peritus-console-wait".into())
        .spawn(move || {
            let _ = wait_sender.send(child.wait().map_err(problem));
        })
    {
        Ok(thread) => thread,
        Err(error) => {
            controls.stopping.store(true, Ordering::Release);
            let _ = terminate_child(&controls);
            return Err(problem(error));
        }
    };

    let mut handlers = Vec::new();
    let mut exit = None;
    let mut output = None;
    while exit.is_none() || output.is_none() {
        match listener.accept() {
            Ok((stream, _)) => match handle(
                stream,
                manifest.token.clone(),
                Arc::clone(&controls),
                Arc::clone(&record),
                record_path.clone(),
                directory.to_owned(),
            ) {
                Ok(handler) => handlers.push(handler),
                Err(error) => eprintln!("peritus web console: control capacity unavailable: {error}"),
            },
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
        if exit.is_none() {
            exit = match wait_receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(problem(
                    "PTY child waiter ended without publishing an outcome",
                ))),
            };
            if let Some(Err(error)) = exit.as_ref() {
                let mut locked = record.lock().map_err(problem)?;
                locked.error = Some(format!("PTY child wait failed: {error}"));
                locked.publish(&record_path)?;
                drop(locked);
                controls.stopping.store(true, Ordering::Release);
                let _ = terminate_child(&controls);
            }
        }
        if output.is_none() {
            output = match reader_receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(problem(
                    "PTY output reader ended without publishing an outcome",
                ))),
            };
            if let Some(Err(error)) = output.as_ref() {
                let mut locked = record.lock().map_err(problem)?;
                locked.error = Some(format!("PTY output capture failed: {error}"));
                locked.publish(&record_path)?;
                drop(locked);
                controls.stopping.store(true, Ordering::Release);
                let _ = terminate_child(&controls);
            }
        }
        join_finished(&mut handlers);
        if exit.is_none() || output.is_none() {
            thread::sleep(Duration::from_millis(10));
        }
    }

    controls.stopping.store(true, Ordering::Release);
    let exit = exit.expect("loop observes child exit before completion");
    let output = output.expect("loop observes reader exit before completion");
    {
        let mut locked = record.lock().map_err(problem)?;
        match exit {
            Ok(status) => {
                locked.exit_code = Some(status.exit_code());
                locked.exit_signal = status.signal().map(str::to_owned);
                locked.phase = Phase::Exited;
                #[cfg(unix)]
                if locked.close_disposition == Some(CloseDisposition::Terminate) {
                    locked.close_confirmed = true;
                }
            }
            Err(error) => {
                locked.phase = Phase::Failed;
                locked.error = Some(format!("PTY child wait failed: {error}"));
            }
        }
        if let Err(error) = output {
            locked.phase = Phase::Failed;
            locked.error = Some(format!("PTY output capture failed: {error}"));
        }
        locked.publish(&record_path)?;
    }
    reader_thread.join().map_err(|_| problem("PTY reader thread panicked"))?;
    wait_thread.join().map_err(|_| problem("PTY wait thread panicked"))?;
    for handler in handlers {
        let _ = handler.join();
    }
    Ok(())
}
