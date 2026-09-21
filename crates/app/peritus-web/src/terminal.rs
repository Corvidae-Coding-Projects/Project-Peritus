//! Retained PTY sessions expose the installed CLI, including interactive setup.

use crate::{
    error::{Result, problem},
    state::App,
};
use base64::Engine;
use portable_pty::{CommandBuilder, MasterPty, PtySize};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    sync::{Arc, Mutex},
};

struct Output {
    bytes: VecDeque<u8>,
    start: u64,
    ended: bool,
}
pub struct Terminal {
    pub(crate) console: crate::consoles::Console,
    output: Arc<Mutex<Output>>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
}
impl Terminal {
    pub(crate) fn start(
        app: &App,
        console: crate::consoles::Console,
        args: Vec<String>,
    ) -> Result<()> {
        if args.len() > 128 || args.iter().any(|arg| arg.len() > 32768 || arg.contains('\0')) {
            return Err(problem("CLI arguments exceed the allowed bounds"));
        }
        let mut terminals = app.terminals.lock().map_err(problem)?;
        if terminals.len() >= 24 {
            return Err(problem("Close a console before opening another (24-console limit)"));
        }
        let project = app.project(&console.project)?;
        let pair = portable_pty::native_pty_system()
            .openpty(PtySize { rows: 30, cols: 100, pixel_width: 0, pixel_height: 0 })
            .map_err(problem)?;
        let mut command = CommandBuilder::new(&app.options.cli);
        command.args(args);
        command.cwd(project.root);
        command.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(command).map_err(problem)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(problem)?;
        let writer = pair.master.take_writer().map_err(problem)?;
        let output =
            Arc::new(Mutex::new(Output { bytes: VecDeque::new(), start: 0, ended: false }));
        let reading = Arc::clone(&output);
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        let Ok(mut output) = reading.lock() else {
                            return;
                        };
                        output.bytes.extend(&buffer[..count]);
                        while output.bytes.len() > 1024 * 1024 {
                            output.bytes.pop_front();
                            output.start += 1;
                        }
                    }
                }
            }
            if let Ok(mut output) = reading.lock() {
                output.ended = true;
            }
        });
        terminals.insert(
            console.id.clone(),
            Arc::new(Self {
                console,
                output,
                writer: Mutex::new(writer),
                master: Mutex::new(pair.master),
                child: Mutex::new(child),
            }),
        );
        drop(terminals);
        Ok(())
    }
    pub(crate) fn finished(&self) -> Result<bool> {
        Ok(self.child.lock().map_err(problem)?.try_wait()?.is_some())
    }
    pub(crate) fn read(&self, after: u64) -> Result<Value> {
        let output = self.output.lock().map_err(problem)?;
        let offset = usize::try_from(after.saturating_sub(output.start)).unwrap_or(usize::MAX);
        let bytes: Vec<u8> = output.bytes.iter().skip(offset).take(65536).copied().collect();
        Ok(
            json!({"data":base64::engine::general_purpose::STANDARD.encode(&bytes),"next":after.max(output.start) + bytes.len() as u64,"lost":after < output.start,"ended":output.ended}),
        )
    }
    pub(crate) fn input(&self, text: &str) -> Result<()> {
        if text.len() > 65536 {
            return Err(problem("Terminal input is too large"));
        }
        let mut writer = self.writer.lock().map_err(problem)?;
        writer.write_all(text.as_bytes())?;
        writer.flush()?;
        drop(writer);
        Ok(())
    }
    pub(crate) fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.master
            .lock()
            .map_err(problem)?
            .resize(PtySize {
                rows: rows.clamp(2, 200),
                cols: cols.clamp(10, 400),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(problem)
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        if let Ok(child) = self.child.get_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
