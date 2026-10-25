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
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
        supports_keyboard_enhancement,
    },
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

use crate::{TerminalTitle, TuiError, model::AppModel, render};

const INPUT_POLL: Duration = Duration::from_millis(100);

pub(super) struct TerminalOwner {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    screen: ScreenState,
    keyboard: KeyboardState,
    title: TerminalTitle,
}

#[derive(Default)]
struct ScreenState {
    active: bool,
    selecting_output: bool,
}

#[derive(Default)]
struct KeyboardState {
    supported: bool,
    active: bool,
}

impl TerminalOwner {
    pub(super) fn enter() -> Result<Self, TuiError> {
        let title = TerminalTitle::acquire()?;
        let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        let mut owner = Self {
            terminal,
            screen: ScreenState::default(),
            keyboard: KeyboardState::default(),
            title,
        };
        enable_raw_mode()?;
        owner.screen.active = true;
        // Negotiate before InputPump starts reading. A legacy terminal or a timed-out
        // capability query keeps legacy input; keyboard enhancements are optional.
        owner.keyboard.supported = supports_keyboard_enhancement().unwrap_or(false);
        owner.enter_screen()?;
        Ok(owner)
    }

    fn enter_screen(&mut self) -> Result<(), TuiError> {
        execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture,
            Hide
        )?;
        if self.keyboard.supported {
            // Own the attempted push too, so a partial write is cleaned up on failure.
            self.keyboard.active = true;
            execute!(
                self.terminal.backend_mut(),
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )?;
        }
        self.screen.selecting_output = false;
        self.terminal.clear()?;
        Ok(())
    }

    pub(super) fn draw(&mut self, model: &mut AppModel) -> Result<(), TuiError> {
        if self.screen.selecting_output && model.chat.selecting_output() {
            return Ok(());
        }
        if self.screen.selecting_output {
            execute!(self.terminal.backend_mut(), EnableMouseCapture)?;
            self.screen.selecting_output = false;
            self.terminal.clear()?;
        }
        self.terminal.draw(|frame| {
            model.chat.viewport = Some(frame.area());
            render::draw(frame, model);
        })?;
        if model.chat.selecting_output() {
            execute!(self.terminal.backend_mut(), DisableMouseCapture, Hide)?;
            self.screen.selecting_output = true;
        }
        Ok(())
    }

    pub(super) fn suspend(&mut self) -> Result<(), TuiError> {
        self.restore()?;
        Ok(())
    }

    pub(super) fn resume(&mut self) -> Result<(), TuiError> {
        self.title.activate()?;
        enable_raw_mode()?;
        self.screen.active = true;
        self.enter_screen()
    }

    fn restore(&mut self) -> io::Result<()> {
        // Pop while still on the alternate screen: terminals keep separate keyboard
        // stacks for each screen. Also restore before handing a terminal to a child.
        let keyboard = if std::mem::take(&mut self.keyboard.active) {
            execute!(self.terminal.backend_mut(), PopKeyboardEnhancementFlags)
        } else {
            Ok(())
        };
        let screen = execute!(
            self.terminal.backend_mut(),
            Show,
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let raw = disable_raw_mode();
        let restored = keyboard.and(screen).and(raw);
        // Preserve the owner's final best-effort cleanup attempt if suspension fails.
        // The keyboard pop was already attempted on the alternate screen, so Drop
        // must not pop again after a successful LeaveAlternateScreen.
        self.screen.active = restored.is_err();
        restored
    }
}

impl Drop for TerminalOwner {
    fn drop(&mut self) {
        if self.screen.active {
            let _ = self.restore();
        }
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
                                if !forward_event(&sender, event, &worker_stop) {
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
            thread.thread().unpark();
            thread
                .join()
                .map_err(|_| TuiError::Task("terminal input worker panicked".to_owned()))?;
        }
        Ok(())
    }
}

fn forward_event(sender: &mpsc::Sender<Event>, mut event: Event, stop: &AtomicBool) -> bool {
    loop {
        if stop.load(Ordering::Acquire) {
            return false;
        }
        match sender.try_send(event) {
            Ok(()) => return true,
            Err(mpsc::error::TrySendError::Closed(_)) => return false,
            Err(mpsc::error::TrySendError::Full(pending)) => event = pending,
        }
        // The UI may be shutting down or suspending for a candidate process instead
        // of draining the queue. Retain input while active, but keep stop cancellable.
        thread::park_timeout(INPUT_POLL);
    }
}

impl Drop for InputPump {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}
