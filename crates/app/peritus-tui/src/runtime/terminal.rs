//! Exclusive terminal ownership and the blocking terminal-input worker.

use std::{
    io::{self, Stdout},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

use crate::{TuiError, model::AppModel, render};

const INPUT_POLL: Duration = Duration::from_millis(100);

pub(super) struct TerminalOwner {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    active: bool,
}

impl TerminalOwner {
    pub(super) fn enter() -> Result<Self, TuiError> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableBracketedPaste, Hide) {
            let _ = disable_raw_mode();
            return Err(TuiError::Io(error));
        }
        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(mut terminal) => {
                terminal.clear()?;
                Ok(Self { terminal, active: true })
            }
            Err(error) => {
                let mut stdout = io::stdout();
                let _ = execute!(stdout, Show, DisableBracketedPaste, LeaveAlternateScreen);
                let _ = disable_raw_mode();
                Err(TuiError::Io(error))
            }
        }
    }

    pub(super) fn draw(&mut self, model: &AppModel) -> Result<(), TuiError> {
        self.terminal.draw(|frame| render::draw(frame, model))?;
        Ok(())
    }

    pub(super) fn suspend(&mut self) -> Result<(), TuiError> {
        self.terminal.show_cursor()?;
        execute!(self.terminal.backend_mut(), Show, DisableBracketedPaste, LeaveAlternateScreen)?;
        disable_raw_mode()?;
        self.active = false;
        Ok(())
    }

    pub(super) fn resume(&mut self) -> Result<(), TuiError> {
        enable_raw_mode()?;
        if let Err(error) =
            execute!(self.terminal.backend_mut(), EnterAlternateScreen, EnableBracketedPaste, Hide)
        {
            let _ = disable_raw_mode();
            return Err(TuiError::Io(error));
        }
        self.active = true;
        self.terminal.clear()?;
        Ok(())
    }
}

impl Drop for TerminalOwner {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let _ = self.terminal.show_cursor();
        let _ = execute!(
            self.terminal.backend_mut(),
            Show,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}

pub(super) struct InputPump {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl InputPump {
    pub(super) fn start(sender: mpsc::Sender<Event>) -> Result<Self, TuiError> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let thread =
            thread::Builder::new().name("peritus-tui-input".to_owned()).spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match event::poll(INPUT_POLL) {
                        Ok(true) => match event::read() {
                            Ok(event) => {
                                if sender.blocking_send(event).is_err() {
                                    return;
                                }
                            }
                            Err(_) => return,
                        },
                        Ok(false) => {}
                        Err(_) => return,
                    }
                }
            })?;
        Ok(Self { stop, thread: Some(thread) })
    }

    pub(super) fn stop(&mut self) -> Result<(), TuiError> {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| TuiError::Task("terminal input worker panicked".to_owned()))?;
        }
        Ok(())
    }
}

impl Drop for InputPump {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
