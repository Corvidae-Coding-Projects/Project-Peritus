//! Bounded native PTY exercise for the installed interactive TUI.

mod screen;

use std::ffi::OsStr;
use std::io::{self, Read as _, Write as _};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

use screen::TerminalScreen;

const DEADLINE: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const MAX_TRANSCRIPT_BYTES: usize = 2 * 1024 * 1024;
const CONTROL_Q: u8 = 0x11;
const MAX_CURSOR_REPORTS: usize = 16;

const LEAVE_ALTERNATE_SCREEN: &[u8] = b"\x1b[?1049l";
const SHOW_CURSOR: &[u8] = b"\x1b[?25h";
const DISABLE_BRACKETED_PASTE: &[u8] = b"\x1b[?2004l";
const CURSOR_POSITION_QUERY: &[u8] = b"\x1b[6n";
const CURSOR_POSITION_REPORT: &[u8] = b"\x1b[1;1R";
const CONNECTED_NOTICE: &[u8] = b"connected to daemon";
const ONLINE_STATUS: &[u8] = b"online";
const READY_READ_WRITE: &[u8] = b"ReadyReadWrite";
const LIVE_EVENT_STREAM: &[u8] = b"live event stream resumed";
const RECONNECTED_STATUS: &str = "online #2";
const RECONNECTED_NOTICE: &str = "reconnected to";

pub(super) struct TuiObservation {
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
    let mut help_requested = false;
    let mut reconnect_requested = false;
    let mut cursor_reports = 0_usize;
    let mut screen = TerminalScreen::new(30, 100);
    let mut rendered_bytes = 0_usize;
    let mut milestones = ScreenMilestones::default();

    let status = loop {
        let state = transcript.lock().map_err(|_| "native TUI transcript lock was poisoned")?;
        screen.feed(&state.bytes[rendered_bytes..]);
        rendered_bytes = state.bytes.len();
        milestones.observe(&screen, &state.bytes);
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
        if milestones.rendered && connected && !help_requested {
            writer.write_all(b"?")?;
            writer.flush()?;
            help_requested = true;
        } else if milestones.navigated && !reconnect_requested {
            writer.write_all(b"R")?;
            writer.flush()?;
            reconnect_requested = true;
        } else if reconnect_requested && milestones.reconnected && !quit_sent {
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
                "native TUI did not complete its connected user journey within {} seconds: rendered={} connected={connected} navigated={} help_requested={help_requested} reconnect_requested={reconnect_requested} reconnected={} quit_sent={quit_sent} cursor_reports={cursor_reports}; screen: {}; transcript tail: {diagnostic}",
                DEADLINE.as_secs(),
                milestones.rendered,
                milestones.navigated,
                milestones.reconnected,
                screen.diagnostic()
            )
            .into());
        }
        thread::sleep(POLL_INTERVAL);
    };

    drop(writer);
    drop(pair.master);
    join_reader(reader_thread)?;
    let state = transcript.lock().map_err(|_| "native TUI transcript lock was poisoned")?;
    let result = validate_journey(&status, &state.bytes, milestones, quit_sent, cursor_reports);
    drop(state);
    result
}

fn validate_journey(
    status: &portable_pty::ExitStatus,
    transcript: &[u8],
    milestones: ScreenMilestones,
    quit_sent: bool,
    cursor_reports: usize,
) -> Result<TuiObservation, Box<dyn std::error::Error>> {
    let diagnostic = diagnostic_tail(transcript);
    let connected = connected(transcript);
    let restored = terminal_restored(transcript);
    if !status.success() {
        return Err(format!(
            "native TUI exited unsuccessfully with code {}; transcript tail: {diagnostic}",
            status.exit_code()
        )
        .into());
    }
    if !quit_sent
        || !milestones.rendered
        || !connected
        || !milestones.navigated
        || !milestones.reconnected
        || !restored
    {
        return Err(format!(
            "native TUI journey was incomplete: quit={quit_sent} rendered={} connected={connected} navigated={} reconnected={} restored={restored}",
            milestones.rendered, milestones.navigated, milestones.reconnected
        )
        .into());
    }
    Ok(TuiObservation { cursor_reports: u64::try_from(cursor_reports).unwrap_or(u64::MAX) })
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

fn connected(bytes: &[u8]) -> bool {
    contains(bytes, CONNECTED_NOTICE)
        || (contains(bytes, ONLINE_STATUS)
            && contains(bytes, READY_READ_WRITE)
            && contains(bytes, LIVE_EVENT_STREAM))
}

#[derive(Clone, Copy, Debug, Default)]
struct ScreenMilestones {
    rendered: bool,
    navigated: bool,
    reconnected: bool,
}

impl ScreenMilestones {
    fn observe(&mut self, screen: &TerminalScreen, transcript: &[u8]) {
        self.rendered |= frame_rendered(screen);
        self.navigated |= help_rendered(screen);
        self.reconnected |= screen.contains(RECONNECTED_STATUS)
            || screen.contains(RECONNECTED_NOTICE)
            || contains(transcript, RECONNECTED_NOTICE.as_bytes());
    }
}

fn frame_rendered(screen: &TerminalScreen) -> bool {
    screen.contains("Peritus") && screen.contains("Ctrl-Q quit")
}

fn help_rendered(screen: &TerminalScreen) -> bool {
    screen.contains("Key reference")
        || (screen.contains("Navigation") && screen.contains("Live connection"))
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
    fn screen_milestones_accept_detail_views_and_survive_later_frames() {
        let mut detail = TerminalScreen::new(4, 100);
        let mut milestones = ScreenMilestones::default();
        detail.feed(
            b"\x1b[2J\x1b[1;1HPeritus - online\x1b[2;1HEvent credential-registry-event\x1b[4;1H? help - Ctrl-Q quit",
        );
        milestones.observe(&detail, &[]);
        assert!(milestones.rendered);
        assert!(!detail.contains("Runs"));

        detail.feed(b"\x1b[2J\x1b[1;1HKey r f rence\x1b[2;1HNavigation\x1b[3;1HLive connection");
        milestones.observe(&detail, &[]);
        assert!(milestones.navigated);

        detail.feed(b"\x1b[2J\x1b[1;1Honl ne #2");
        milestones.observe(&detail, b"differential frame reconnected to\x1b[1Cdaemon");
        assert!(milestones.reconnected);

        detail.feed(b"\x1b[2J");
        milestones.observe(&detail, &[]);
        assert!(!frame_rendered(&detail));
        assert!(milestones.rendered);
        assert!(milestones.navigated);
        assert!(milestones.reconnected);

        let mut incomplete = TerminalScreen::new(4, 100);
        incomplete
            .feed(b"\x1b[2J\x1b[1;1HPeritus - online\x1b[2;1HEvent credential-registry-event");
        let mut incomplete_milestones = ScreenMilestones::default();
        incomplete_milestones.observe(&incomplete, &[]);
        assert!(!incomplete_milestones.rendered);
        assert!(!incomplete_milestones.navigated);
        assert!(!incomplete_milestones.reconnected);
    }

    #[test]
    fn transcript_requires_render_connection_and_complete_restoration() {
        let transcript =
            b"\x1b[?1049h Peritus Runs connected to daemon \x1b[?25h\x1b[?2004l\x1b[?1049l";
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
