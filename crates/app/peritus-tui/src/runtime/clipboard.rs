//! One owned clipboard write at a time, using desktop helpers or terminal OSC 52.

use crossterm::{Command, clipboard::CopyToClipboard};
use std::io::{self, Write};

use crate::action::{Action, ClipboardDestination};
use peritus_app_protocol::ControlOperationId;
use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command as ProcessCommand, task::JoinSet};

#[derive(Default)]
pub(super) struct ClipboardWrites {
    jobs: JoinSet<io::Result<ClipboardDestination>>,
    operation: Option<ControlOperationId>,
}

impl ClipboardWrites {
    pub(super) fn active(&self) -> bool {
        !self.jobs.is_empty()
    }

    pub(super) fn start(&mut self, operation: ControlOperationId, text: String) -> bool {
        if self.active() {
            return false;
        }
        self.operation = Some(operation);
        self.jobs.spawn(async move { copy(&text).await });
        true
    }

    pub(super) async fn next(&mut self) -> Option<Action> {
        let result = match self.jobs.join_next().await? {
            Ok(result) => result.map_err(|error| error.to_string()),
            Err(error) => Err(format!("Clipboard task failed: {error}")),
        };
        self.operation.take().map(|operation| Action::ClipboardWritten { operation, result })
    }
}

async fn copy(text: &str) -> io::Result<ClipboardDestination> {
    if let Some((program, arguments)) = desktop_helper() {
        match native_command(program, arguments, text, Duration::from_secs(2)).await {
            Ok(()) => return Ok(ClipboardDestination::Desktop),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    write(&mut io::stdout().lock(), text)?;
    Ok(ClipboardDestination::Terminal)
}

fn desktop_helper() -> Option<(&'static str, &'static [&'static str])> {
    if cfg!(target_os = "linux") {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return Some(("wl-copy", &["--type", "text/plain;charset=utf-8"]));
        }
        if std::env::var_os("DISPLAY").is_some() {
            return Some(("xclip", &["-selection", "clipboard", "-in"]));
        }
    }
    if cfg!(target_os = "macos") {
        return Some(("pbcopy", &[]));
    }
    None
}

async fn native_command(
    program: impl AsRef<std::ffi::OsStr>,
    arguments: &[&str],
    text: &str,
    timeout: Duration,
) -> io::Result<()> {
    let mut child = ProcessCommand::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let work = async {
        let mut input =
            child.stdin.take().ok_or_else(|| io::Error::other("clipboard input unavailable"))?;
        input.write_all(text.as_bytes()).await?;
        input.shutdown().await?;
        drop(input);
        let status = child.wait().await?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!("desktop clipboard helper exited with {status}")))
        }
    };
    tokio::time::timeout(timeout, work).await.map_err(|_| {
        io::Error::new(io::ErrorKind::TimedOut, "desktop clipboard helper timed out")
    })?
}

fn write(writer: &mut impl Write, text: &str) -> io::Result<()> {
    let mut command = String::new();
    CopyToClipboard::to_clipboard_from(text)
        .write_ansi(&mut command)
        .map_err(|error| io::Error::other(error.to_string()))?;
    // Write ANSI directly on all platforms; native WinAPI clipboard dispatch is unsupported.
    writer.write_all(command.as_bytes())?;
    writer.flush()
}
