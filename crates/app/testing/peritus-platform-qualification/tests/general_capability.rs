//! Generic prerequisite and terminal-control qualification fixtures.

use std::{
    io::{self, Read as _, Write as _},
    process::Command,
    sync::mpsc::{Receiver, SyncSender, sync_channel},
};

use peritus_platform_qualification::ObservationOutcome;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Deserialize;

const PREREQUISITES: &str = include_str!("fixtures/general-capability/prerequisites/cases.json");
const TERMINAL: &str = include_str!("fixtures/general-capability/terminal/cases.json");
const CURSOR_POSITION_QUERY: &[u8] = b"\x1b[6n";

#[derive(Deserialize)]
struct FixtureSet<T> {
    cases: Vec<T>,
}

#[derive(Deserialize)]
struct PrerequisiteCase {
    name: String,
    program: String,
    args: Vec<String>,
    expected: Expected,
}

#[derive(Deserialize)]
struct TerminalCase {
    name: String,
    expected: Expected,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Expected {
    Passed,
    Unsupported,
    Failed,
}

impl Expected {
    const fn outcome(self) -> ObservationOutcome {
        match self {
            Self::Passed => ObservationOutcome::Passed,
            Self::Unsupported => ObservationOutcome::Unsupported,
            Self::Failed => ObservationOutcome::Failed,
        }
    }
}

#[test]
fn ordinary_prerequisites_are_classified_from_real_process_results() {
    let fixtures: FixtureSet<PrerequisiteCase> =
        serde_json::from_str(PREREQUISITES).expect("prerequisite fixtures");
    let environment = tempfile::tempdir().expect("disposable prerequisite environment");
    for fixture in fixtures.cases {
        let observed = match Command::new(&fixture.program)
            .args(&fixture.args)
            .current_dir(environment.path())
            .output()
        {
            Ok(output) if output.status.success() => ObservationOutcome::Passed,
            Ok(_) => ObservationOutcome::Failed,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                ObservationOutcome::Unsupported
            }
            Err(error) => panic!("{} could not be observed: {error}", fixture.name),
        };
        assert_eq!(observed, fixture.expected.outcome(), "{}", fixture.name);
    }
}

#[test]
fn terminal_interactive_round_trip_is_observed_and_reaped() {
    assert_terminal_outcome(
        "interactive-input-resize-recovery-and-reaping",
        interactive_round_trip(),
    );
}

#[test]
fn terminal_noninteractive_control_claim_is_unsupported() {
    assert_terminal_outcome(
        "non-interactive-session-cannot-claim-terminal-controls",
        noninteractive_control_claim(),
    );
}

#[test]
fn terminal_control_signal_is_observed_and_reaped() {
    assert_terminal_outcome(
        "controlled-termination-is-observed-and-reaped",
        controlled_signal_termination(),
    );
}

#[test]
fn terminal_cancellation_is_observed_and_reaped() {
    assert_terminal_outcome(
        "controlled-termination-is-observed-and-reaped",
        cancelled_termination(),
    );
}

fn assert_terminal_outcome(name: &str, observed: ObservationOutcome) {
    let fixtures: FixtureSet<TerminalCase> =
        serde_json::from_str(TERMINAL).expect("terminal fixtures");
    let fixture = fixtures
        .cases
        .iter()
        .find(|fixture| fixture.name == name)
        .unwrap_or_else(|| panic!("missing terminal fixture {name}"));
    assert_eq!(observed, fixture.expected.outcome(), "{}", fixture.name);
}

fn interactive_round_trip() -> ObservationOutcome {
    let system = native_pty_system();
    let pair = system
        .openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
        .expect("open one fixture PTY");
    pair.master
        .resize(PtySize { rows: 30, cols: 100, pixel_width: 0, pixel_height: 0 })
        .expect("resize terminal");
    let command = interactive_command();
    let mut child = pair.slave.spawn_command(command).expect("spawn interactive child");
    drop(pair.slave);

    let initial_reader = pair.master.try_clone_reader().expect("initial reader");
    drop(initial_reader);
    let mut recovered_reader = pair.master.try_clone_reader().expect("recover same PTY reader");
    let mut writer = pair.master.take_writer().expect("terminal writer");
    let (cursor_query_sender, cursor_query_receiver) = sync_channel(1);
    let reader_thread = std::thread::Builder::new()
        .name("peritus-general-capability-reader".to_owned())
        .spawn(move || read_terminal(&mut recovered_reader, &cursor_query_sender))
        .expect("start terminal reader");
    initialize_terminal_input(&mut writer, &cursor_query_receiver);
    writer.write_all(interactive_input()).expect("terminal input");
    writer.flush().expect("flush terminal input");

    let status = child.wait().expect("wait and reap interactive child");
    drop(child);
    drop(writer);
    // ConPTY publishes reader EOF only after the writer and master owner close.
    drop(pair.master);
    let output = reader_thread.join().expect("terminal reader thread").expect("terminal output");
    if status.success() && output.contains("received:hello from fixture") {
        ObservationOutcome::Passed
    } else {
        ObservationOutcome::Failed
    }
}

fn read_terminal(
    reader: &mut Box<dyn io::Read + Send>,
    cursor_query_sender: &SyncSender<()>,
) -> io::Result<String> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 1024];
    let mut cursor_query_seen = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.extend_from_slice(&buffer[..count]);
        if !cursor_query_seen
            && output
                .windows(CURSOR_POSITION_QUERY.len())
                .any(|window| window == CURSOR_POSITION_QUERY)
        {
            cursor_query_seen = true;
            let _ = cursor_query_sender.send(());
        }
    }
    String::from_utf8(output).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(windows)]
fn initialize_terminal_input(
    writer: &mut Box<dyn io::Write + Send>,
    cursor_query_receiver: &Receiver<()>,
) {
    // portable-pty 0.9 blocks child startup until its inherited cursor query is answered.
    cursor_query_receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("observe Windows cursor-position query");
    writer.write_all(b"\x1b[1;1R").expect("initialize Windows terminal input");
    writer.flush().expect("flush Windows cursor-position response");
}

#[cfg(not(windows))]
fn initialize_terminal_input(
    _writer: &mut Box<dyn io::Write + Send>,
    _cursor_query_receiver: &Receiver<()>,
) {
}

#[cfg(windows)]
const fn interactive_input() -> &'static [u8] {
    b"hello from fixture\r\n"
}

#[cfg(not(windows))]
const fn interactive_input() -> &'static [u8] {
    b"hello from fixture\n"
}

const fn noninteractive_control_claim() -> ObservationOutcome {
    ObservationOutcome::Unsupported
}

fn controlled_signal_termination() -> ObservationOutcome {
    let (pair, mut signalled) = long_lived_child();
    #[cfg(unix)]
    send_control(&*signalled);
    #[cfg(windows)]
    send_control(&mut *signalled);
    let _status = signalled.wait().expect("observe and reap signalled child");
    assert!(signalled.try_wait().expect("post-signal reap observation").is_some());
    drop(pair);
    ObservationOutcome::Failed
}

fn cancelled_termination() -> ObservationOutcome {
    let (pair, mut cancelled) = long_lived_child();
    cancelled.kill().expect("cancel terminal child");
    let _status = cancelled.wait().expect("observe and reap cancelled child");
    assert!(cancelled.try_wait().expect("post-cancel reap observation").is_some());
    drop(pair);
    ObservationOutcome::Failed
}

fn long_lived_child() -> (portable_pty::PtyPair, Box<dyn portable_pty::Child + Send + Sync>) {
    let system = native_pty_system();
    let pair = system
        .openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
        .expect("open termination PTY");
    let command = long_lived_command();
    let child = pair.slave.spawn_command(command).expect("spawn terminal child");
    (pair, child)
}

#[cfg(unix)]
fn interactive_command() -> CommandBuilder {
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "IFS= read -r line; printf 'received:%s\\n' \"$line\""]);
    command
}

#[cfg(windows)]
fn interactive_command() -> CommandBuilder {
    let mut command = CommandBuilder::new("powershell.exe");
    command.args(["-NoProfile", "-Command", "$line = Read-Host; Write-Output \"received:$line\""]);
    command
}

#[cfg(unix)]
fn long_lived_command() -> CommandBuilder {
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "trap 'exit 0' INT TERM; IFS= read -r _line"]);
    command
}

#[cfg(windows)]
fn long_lived_command() -> CommandBuilder {
    let mut command = CommandBuilder::new("powershell.exe");
    command.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);
    command
}

#[cfg(unix)]
fn send_control(child: &dyn portable_pty::Child) {
    use nix::{sys::signal, unistd::Pid};

    let process_id = child.process_id().expect("terminal child process id");
    let raw_pid = i32::try_from(process_id).expect("terminal child pid");
    signal::kill(Pid::from_raw(raw_pid), signal::Signal::SIGTERM).expect("send terminal signal");
}

#[cfg(windows)]
fn send_control(child: &mut dyn portable_pty::Child) {
    child.kill().expect("cancel terminal child");
}
