//! Bounded native PTY exercise for the installed interactive TUI.

use std::ffi::OsStr;
use std::io::{self, Read as _, Write as _};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

const DEADLINE: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const MAX_TRANSCRIPT_BYTES: usize = 2 * 1024 * 1024;
const CONTROL_Q: u8 = 0x11;
const MAX_CURSOR_REPORTS: usize = 8;

const ENTER_ALTERNATE_SCREEN: &[u8] = b"\x1b[?1049h";
const LEAVE_ALTERNATE_SCREEN: &[u8] = b"\x1b[?1049l";
const SHOW_CURSOR: &[u8] = b"\x1b[?25h";
const DISABLE_BRACKETED_PASTE: &[u8] = b"\x1b[?2004l";
const CURSOR_POSITION_QUERY: &[u8] = b"\x1b[6n";
const CURSOR_POSITION_REPORT: &[u8] = b"\x1b[1;1R";
const CONNECTED_NOTICE: &[u8] = b"connected to daemon";
const ONLINE_STATUS: &[u8] = b"online";
const READY_READ_WRITE: &[u8] = b"ReadyReadWrite";
const LIVE_EVENT_STREAM: &[u8] = b"live event stream resumed";

pub(super) struct TuiObservation {
    pub(super) connected: bool,
    pub(super) rendered: bool,
    pub(super) cursor_reports: u64,
}

/// Runs the installed TUI through the host PTY/ConPTY, waits for a real daemon connection and
/// rendered frame, sends the documented quit chord, and verifies terminal restoration bytes.
pub(super) fn exercise(
    executable: &Path,
    endpoint: &OsStr,
) -> Result<TuiObservation, Box<dyn std::error::Error>> {
    let pty = native_pty_system();
    let pair = pty.openpty(PtySize { rows: 30, cols: 100, pixel_width: 0, pixel_height: 0 })?;
    let reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;
    let transcript = Arc::new(Mutex::new(Transcript::default()));
    let reader_thread = drain(reader, Arc::clone(&transcript));

    let mut command = CommandBuilder::new(executable);
    command.arg("--endpoint");
    command.arg(endpoint);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    let child = pair.slave.spawn_command(command)?;
    drop(pair.slave);
    let mut child = OwnedChild::new(child);
    let started = Instant::now();
    let mut quit_sent = false;
    let mut cursor_reports = 0_usize;

    let status = loop {
        let state = transcript.lock().map_err(|_| "native TUI transcript lock was poisoned")?;
        let rendered = rendered(&state.bytes);
        let connected = connected(&state.bytes);
        let cursor_queries = occurrences(&state.bytes, CURSOR_POSITION_QUERY);
        let overflow = state.overflow;
        drop(state);
        if overflow {
            child.terminate()?;
            return Err("native TUI transcript exceeded its hard byte limit".into());
        }
        if cursor_queries > MAX_CURSOR_REPORTS {
            child.terminate()?;
            return Err("native TUI exceeded the bounded cursor-position handshake".into());
        }
        let answered_cursor_query = cursor_reports < cursor_queries;
        while cursor_reports < cursor_queries {
            writer.write_all(CURSOR_POSITION_REPORT)?;
            cursor_reports += 1;
        }
        if answered_cursor_query {
            writer.flush()?;
        }
        if rendered && connected && !quit_sent {
            writer.write_all(&[CONTROL_Q])?;
            writer.flush()?;
            quit_sent = true;
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= DEADLINE {
            let diagnostic = transcript
                .lock()
                .map_err(|_| "native TUI transcript lock was poisoned")
                .map(|state| diagnostic_tail(&state.bytes))?;
            child.terminate()?;
            return Err(format!(
                "native TUI did not complete its connected lifecycle within {} seconds: rendered={rendered} connected={connected} quit_sent={quit_sent} cursor_reports={cursor_reports}; transcript tail: {diagnostic}",
                DEADLINE.as_secs()
            )
            .into());
        }
        thread::sleep(POLL_INTERVAL);
    };

    drop(writer);
    drop(pair.master);
    join_reader(reader_thread)?;
    let state = transcript.lock().map_err(|_| "native TUI transcript lock was poisoned")?;
    let diagnostic = diagnostic_tail(&state.bytes);
    let rendered = rendered(&state.bytes);
    let connected = connected(&state.bytes);
    let restored = terminal_restored(&state.bytes);
    drop(state);
    if !status.success() {
        return Err(format!(
            "native TUI exited unsuccessfully with code {}; transcript tail: {diagnostic}",
            status.exit_code()
        )
        .into());
    }
    if !quit_sent || !rendered || !connected || !restored {
        return Err(format!(
            "native TUI lifecycle was incomplete: quit={quit_sent} rendered={rendered} connected={connected} restored={restored}"
        )
        .into());
    }
    Ok(TuiObservation {
        connected,
        rendered,
        cursor_reports: u64::try_from(cursor_reports).unwrap_or(u64::MAX),
    })
}

#[derive(Default)]
struct Transcript {
    bytes: Vec<u8>,
    overflow: bool,
}

fn drain(
    mut reader: Box<dyn io::Read + Send>,
    transcript: Arc<Mutex<Transcript>>,
) -> JoinHandle<io::Result<()>> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                return Ok(());
            }
            let mut state = transcript.lock().map_err(|_| io::Error::other("transcript lock"))?;
            let remaining = MAX_TRANSCRIPT_BYTES.saturating_sub(state.bytes.len());
            state.bytes.extend_from_slice(&buffer[..count.min(remaining)]);
            let overflow = count > remaining;
            if overflow {
                state.overflow = true;
            }
            drop(state);
            if overflow {
                return Ok(());
            }
        }
    })
}

fn join_reader(reader: JoinHandle<io::Result<()>>) -> Result<(), Box<dyn std::error::Error>> {
    reader.join().map_err(|_| "native TUI transcript reader panicked")??;
    Ok(())
}

fn rendered(bytes: &[u8]) -> bool {
    contains(bytes, ENTER_ALTERNATE_SCREEN)
        && contains(bytes, b"Peritus")
        && contains(bytes, b"Runs")
}

fn connected(bytes: &[u8]) -> bool {
    contains(bytes, CONNECTED_NOTICE)
        || (contains(bytes, ONLINE_STATUS)
            && contains(bytes, READY_READ_WRITE)
            && contains(bytes, LIVE_EVENT_STREAM))
}

fn terminal_restored(bytes: &[u8]) -> bool {
    contains(bytes, LEAVE_ALTERNATE_SCREEN)
        && contains(bytes, SHOW_CURSOR)
        && contains(bytes, DISABLE_BRACKETED_PASTE)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

fn occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    haystack.windows(needle.len()).filter(|window| *window == needle).count()
}

fn diagnostic_tail(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(2 * 1024);
    String::from_utf8_lossy(&bytes[start..])
        .chars()
        .map(|character| {
            if character.is_control() && !matches!(character, '\n' | '\r' | '\t') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

struct OwnedChild {
    child: Option<Box<dyn Child + Send + Sync>>,
}

impl OwnedChild {
    const fn new(child: Box<dyn Child + Send + Sync>) -> Self {
        Self { child: Some(child) }
    }

    fn try_wait(&mut self) -> io::Result<Option<portable_pty::ExitStatus>> {
        let status = self
            .child
            .as_mut()
            .ok_or_else(|| io::Error::other("native TUI child was already reaped"))?
            .try_wait()?;
        if status.is_some() {
            self.child = None;
        }
        Ok(status)
    }

    fn terminate(&mut self) -> io::Result<()> {
        if let Some(mut child) = self.child.take() {
            child.kill()?;
            let _ = child.wait()?;
        }
        Ok(())
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_requires_render_connection_and_complete_restoration() {
        let transcript =
            b"\x1b[?1049h Peritus Runs connected to daemon \x1b[?25h\x1b[?2004l\x1b[?1049l";
        assert!(rendered(transcript));
        assert!(connected(transcript));
        assert!(terminal_restored(transcript));
        assert!(!terminal_restored(b"\x1b[?1049h Peritus Runs connected to daemon"));
        assert_eq!(occurrences(b"\x1b[6ntext\x1b[6n", CURSOR_POSITION_QUERY), 2);
    }

    #[test]
    fn stable_online_state_proves_connection_after_transient_notice_is_replaced() {
        let transcript =
            b"\x1b[?1049h Peritus Runs online ReadyReadWrite live event stream resumed after #0";
        assert!(connected(transcript));
        assert!(!connected(b"online ReadyReadWrite"));
        assert!(!connected(b"ReadyReadWrite live event stream resumed after #0"));
        assert!(!connected(b"online live event stream resumed after #0"));
    }

    #[test]
    fn diagnostic_tail_is_bounded_and_removes_terminal_control_bytes() {
        let mut transcript = vec![b'x'; 3 * 1024];
        transcript.extend_from_slice(b"\x1b[?1049l\nperitus-tui: close failed\n");
        let diagnostic = diagnostic_tail(&transcript);
        assert!(diagnostic.len() <= 2 * 1024);
        assert!(!diagnostic.contains('\u{1b}'));
        assert!(diagnostic.contains("peritus-tui: close failed"));
    }
}
