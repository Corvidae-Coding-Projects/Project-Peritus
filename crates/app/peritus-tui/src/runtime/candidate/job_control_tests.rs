//! Real interactive-shell suspension, backgrounding, and foreground-resume regression.

use std::{
    io::{Read as _, Write as _},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

const PROMPT: &str = "peritus-shell-ready> ";

#[test]
fn stopped_candidate_returns_to_shell_and_resumes_only_in_foreground() {
    let workspace = tempfile::tempdir().unwrap();
    let mut fixture = ShellFixture::new(workspace.path());
    fixture.until(PROMPT);
    let binary = std::env::current_exe().unwrap().to_str().unwrap().replace('\'', "'\\''");
    fixture.send(&format!(
        "'{binary}' --exact {} --ignored --nocapture\n",
        super::interrupt_tests::FIXTURE
    ));
    let output = fixture.until("peritus-candidate-input");
    let owner = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("peritus-fixture-owner=")?.parse::<i32>().ok())
        .expect("fixture owner process group");
    fixture.groups.push(Pid::from_raw(owner));
    fixture.send("Ada\n");
    fixture.until("peritus-candidate-ready");
    let candidate = fixture.master.process_group_leader().unwrap();
    fixture.groups.push(Pid::from_raw(candidate));
    let shell = i32::try_from(fixture.child.process_id().unwrap()).unwrap();

    for background in [false, true] {
        fixture.send("\u{1a}");
        fixture.until(PROMPT);
        fixture.shell_barrier();
        assert_eq!(fixture.master.process_group_leader(), Some(shell));
        if background {
            fixture.send("bg\n");
            fixture.shell_barrier();
            thread::sleep(Duration::from_millis(100));
            assert_eq!(fixture.master.process_group_leader(), Some(shell));
        }
        fixture.send("fg\n");
        let deadline = Instant::now() + Duration::from_secs(10);
        while fixture.master.process_group_leader() != Some(candidate) {
            if Instant::now() >= deadline {
                for bytes in fixture.output.try_iter() {
                    fixture.buffered.extend(bytes);
                }
                panic!(
                    "fg did not restore the candidate terminal (background={background}): {}",
                    String::from_utf8_lossy(&fixture.buffered)
                );
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
    fixture.send("\u{3}");
    fixture.until("peritus-client-survived");
    fixture.until(PROMPT);
    fixture.shell_barrier();
    fixture.groups.clear();
    fixture.send("exit\n");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = fixture.child.try_wait().unwrap() {
            assert!(status.success(), "shell fixture failed: {status:?}");
            break;
        }
        assert!(Instant::now() < deadline, "shell did not exit");
        thread::sleep(Duration::from_millis(5));
    }
}

struct ShellFixture {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    input: Box<dyn std::io::Write + Send>,
    output: mpsc::Receiver<Vec<u8>>,
    reader: Option<thread::JoinHandle<()>>,
    buffered: Vec<u8>,
    groups: Vec<Pid>,
}

impl ShellFixture {
    fn new(workspace: &std::path::Path) -> Self {
        let pair = NativePtySystem::default().openpty(PtySize::default()).unwrap();
        let mut command = CommandBuilder::new("sh");
        command.arg("-i");
        command.env("PS1", PROMPT);
        command.env("ENV", "");
        command.env("PERITUS_TUI_INTERRUPT_WORKSPACE", workspace);
        command.env("PERITUS_TUI_INTERRUPT_CHECK_UI", "0");
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let input = pair.master.take_writer().unwrap();
        let mut output = pair.master.try_clone_reader().unwrap();
        let (sender, receiver) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut bytes = [0; 4096];
            while let Ok(length) = output.read(&mut bytes) {
                if length == 0 || sender.send(bytes[..length].to_vec()).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            master: pair.master,
            input,
            output: receiver,
            reader: Some(reader),
            buffered: Vec::new(),
            groups: Vec::new(),
        }
    }

    fn send(&mut self, value: &str) {
        self.input.write_all(value.as_bytes()).unwrap();
        self.input.flush().unwrap();
    }

    fn shell_barrier(&mut self) {
        // Echoed input and duplicate prompts are not evidence that the shell is running.
        self.send("printf 'peritus-%s\\n' shell-barrier\n");
        self.until("peritus-shell-barrier");
    }

    fn until(&mut self, marker: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(start) =
                self.buffered.windows(marker.len()).position(|part| part == marker.as_bytes())
            {
                return String::from_utf8_lossy(
                    &self.buffered.drain(..start + marker.len()).collect::<Vec<_>>(),
                )
                .into_owned();
            }
            match self.output.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(bytes) => self.buffered.extend(bytes),
                Err(error) => panic!(
                    "shell did not reach {marker}: {error}; {}",
                    String::from_utf8_lossy(&self.buffered)
                ),
            }
        }
    }
}

impl Drop for ShellFixture {
    fn drop(&mut self) {
        for group in &self.groups {
            let _ = killpg(*group, Signal::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
