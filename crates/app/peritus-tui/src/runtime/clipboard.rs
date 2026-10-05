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
        match native_command(program, arguments, text, None).await {
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
    timeout: Option<Duration>,
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
    match timeout {
        Some(timeout) => tokio::time::timeout(timeout, work).await.map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "desktop clipboard helper reached its caller-selected deadline",
            )
        })?,
        None => work.await,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn clipboard_command_transports_the_exact_unicode_selection_as_inert_data() {
        let text = "λ界\nline two\x1b[2J";
        let mut bytes = Vec::new();
        write(&mut bytes, text).unwrap();
        let expected =
            format!("\x1b]52;c;{}\x1b\\", base64::engine::general_purpose::STANDARD.encode(text));
        assert_eq!(bytes, expected.as_bytes());
    }

    #[test]
    fn clipboard_transport_reports_write_and_flush_failures() {
        struct Broken {
            flush: bool,
        }
        impl Write for Broken {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.flush { Ok(bytes.len()) } else { Err(io::ErrorKind::BrokenPipe.into()) }
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
        }
        for flush in [false, true] {
            assert_eq!(
                write(&mut Broken { flush }, "text").unwrap_err().kind(),
                io::ErrorKind::BrokenPipe
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn native_helper_receives_exact_bytes_and_reports_rejection_or_timeout() {
        use std::os::unix::fs::PermissionsExt;
        let temporary = tempfile::tempdir().unwrap();
        let helper = temporary.path().join("clipboard-helper");
        let output = temporary.path().join("output");
        std::fs::write(&helper, "#!/bin/sh\ncase \"$1\" in\ncopy) cat > \"$2\";;\nreject) exit 7;;\nwait) exec sleep 10;;\nesac\n").unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let text = "λ界\nquoted ' text; $(inert)\n";
        native_command(
            &helper,
            &["copy", output.to_str().unwrap()],
            text,
            Some(Duration::from_secs(1)),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), text.as_bytes());
        let rejected = native_command(&helper, &["reject"], "", Some(Duration::from_secs(1)))
            .await
            .unwrap_err();
        assert!(rejected.to_string().contains('7'));
        let started = std::time::Instant::now();
        let timed_out = native_command(&helper, &["wait"], "", Some(Duration::from_millis(100)))
            .await
            .unwrap_err();
        assert_eq!(timed_out.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(1));
        let missing = native_command(
            temporary.path().join("absent"),
            &[],
            text,
            Some(Duration::from_secs(1)),
        )
        .await
        .unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
    }

    #[tokio::test]
    async fn an_owned_clipboard_job_rejects_duplicates_and_is_aborted_on_drop() {
        struct OnDrop(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl Drop for OnDrop {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::Release);
            }
        }
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let guard = OnDrop(std::sync::Arc::clone(&dropped));
        let mut writes = ClipboardWrites::default();
        writes.jobs.spawn(async move {
            let _guard = guard;
            std::future::pending::<()>().await;
            Ok(ClipboardDestination::Desktop)
        });
        assert!(!writes.start(ControlOperationId::new([85; 16]).unwrap(), "second".to_owned()));
        drop(writes);
        tokio::time::timeout(Duration::from_secs(1), async {
            while !dropped.load(std::sync::atomic::Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
