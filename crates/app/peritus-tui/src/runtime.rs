//! Orderly terminal ownership and asynchronous application runtime.

mod product;

use std::{
    io::{self, Stdout},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use peritus_app_protocol::ProtocolId;
use peritus_types::SessionId;
use ratatui::{Terminal, backend::CrosstermBackend};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use crate::{
    TuiError,
    action::{Action, Effect},
    client::{ClientEvent, ClientSession},
    model::AppModel,
    render,
};

pub use product::{ProductLaunchContext, ProductProviderOption};

const INPUT_POLL: Duration = Duration::from_millis(100);
const UI_TICK: Duration = Duration::from_millis(250);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// Runtime configuration for one interactive client process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TuiConfig {
    endpoint: PathBuf,
    requested_session: Option<SessionId>,
    product: Option<ProductLaunchContext>,
}

impl TuiConfig {
    /// Creates a configuration for one exact local daemon endpoint.
    #[must_use]
    pub fn new(endpoint: impl Into<PathBuf>) -> Self {
        Self { endpoint: endpoint.into(), requested_session: None, product: None }
    }

    /// Requests resumption of an existing durable application session.
    #[must_use]
    pub const fn with_session(mut self, session: SessionId) -> Self {
        self.requested_session = Some(session);
        self
    }

    /// Supplies launcher-resolved product workspace and provider choices.
    #[must_use]
    pub fn with_product(mut self, product: ProductLaunchContext) -> Self {
        self.product = Some(product);
        self
    }

    /// Borrows the exact Unix-socket or Windows named-pipe endpoint.
    #[must_use]
    pub fn endpoint(&self) -> &Path {
        &self.endpoint
    }

    /// Returns the optional durable session requested at first connection.
    #[must_use]
    pub const fn requested_session(&self) -> Option<SessionId> {
        self.requested_session
    }

    /// Borrows launcher-resolved product context when entered through `peritus`.
    #[must_use]
    pub const fn product(&self) -> Option<&ProductLaunchContext> {
        self.product.as_ref()
    }
}

/// Truthful reason the interactive runtime returned normally.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitReason {
    /// The user explicitly requested orderly client exit.
    UserQuit,
    /// The product launcher must restore daemon readiness and reopen the interface.
    RecoverDaemon,
}

/// Runs the interactive TUI until the user exits.
///
/// # Errors
///
/// Returns [`TuiError`] when terminal ownership cannot be established/restored or an orderly
/// connection shutdown reports failure. Live daemon disconnects are presented in the UI and may
/// be retried without ending the process.
pub async fn run(config: TuiConfig) -> Result<ExitReason, TuiError> {
    let seed = process_seed(config.endpoint());
    let mut terminal = TerminalOwner::enter()?;
    let (input_tx, mut input_rx) = mpsc::channel(128);
    let input = InputPump::start(input_tx)?;
    let (client_events_tx, mut client_events_rx) = mpsc::channel(512);
    let mut model = AppModel::with_product(seed, config.product().cloned());
    let mut client = None;
    let mut connection_generation = 0_u64;
    connect(&config, &mut model, &mut client, &client_events_tx, &mut connection_generation).await;

    let mut tick = tokio::time::interval(UI_TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let result = loop {
        terminal.draw(&model)?;
        let action = tokio::select! {
            input = input_rx.recv() => match input {
                Some(event) => Action::TerminalEvent(event),
                None => {
                    break Err(TuiError::Task("terminal input worker stopped".to_owned()));
                }
            },
            event = client_events_rx.recv() => match event {
                Some(ClientEvent::Message(message)) => Action::Message(message),
                Some(ClientEvent::Disconnected(error)) => Action::Disconnected(error),
                None => Action::Disconnected("all daemon client tasks stopped".to_owned()),
            },
            _ = tick.tick() => Action::Tick,
        };
        let effects = model.update(action);
        match apply_effects(
            effects,
            &config,
            &mut model,
            &mut client,
            &client_events_tx,
            &mut connection_generation,
        )
        .await
        {
            Ok(ControlFlow::Continue) => {}
            Ok(ControlFlow::Quit) => break Ok(ExitReason::UserQuit),
            Ok(ControlFlow::RecoverDaemon) => break Ok(ExitReason::RecoverDaemon),
            Err(error) => break Err(error),
        }
    };

    input.stop()?;
    let cleanup = model.cleanup_messages();
    if let Some(session) = client {
        session.close(cleanup).await?;
    }
    result
}

enum ControlFlow {
    Continue,
    Quit,
    RecoverDaemon,
}

async fn apply_effects(
    effects: Vec<Effect>,
    config: &TuiConfig,
    model: &mut AppModel,
    client: &mut Option<ClientSession>,
    events: &mpsc::Sender<ClientEvent>,
    generation: &mut u64,
) -> Result<ControlFlow, TuiError> {
    for effect in effects {
        match effect {
            Effect::Send(message) => {
                if let Some(session) = client {
                    if let Err(error) = session.send(message).await {
                        let _ = model.update(Action::Disconnected(error.to_string()));
                    }
                } else {
                    let _ = model.update(Action::ConnectionFailed(
                        "request could not be sent while disconnected".to_owned(),
                    ));
                }
            }
            Effect::Reconnect => {
                if let Some(session) = client.take() {
                    let cleanup = model.cleanup_messages();
                    let _ = session.close(cleanup).await;
                }
                if config.product().is_some() {
                    return Ok(ControlFlow::RecoverDaemon);
                }
                connect(config, model, client, events, generation).await;
            }
            Effect::Quit => return Ok(ControlFlow::Quit),
        }
    }
    Ok(ControlFlow::Continue)
}

async fn connect(
    config: &TuiConfig,
    model: &mut AppModel,
    client: &mut Option<ClientSession>,
    events: &mpsc::Sender<ClientEvent>,
    generation: &mut u64,
) {
    *generation = generation.saturating_add(1);
    let protocol_id = match protocol_id(process_seed(config.endpoint()), *generation) {
        Ok(protocol_id) => protocol_id,
        Err(error) => {
            let _ = model.update(Action::ConnectionFailed(error.to_string()));
            return;
        }
    };
    let requested = model.retained_session().or_else(|| config.requested_session());
    let attempt = tokio::time::timeout(
        CONNECT_TIMEOUT,
        ClientSession::connect(config.endpoint(), protocol_id, requested, events.clone()),
    )
    .await;
    match attempt {
        Ok(Ok(session)) => {
            let established = session.established().clone();
            *client = Some(session);
            let effects = model.update(Action::Connected {
                context: established.context,
                limits: established.limits,
                server: established.server,
                downgraded: established.downgraded,
            });
            for effect in effects {
                if let Effect::Send(message) = effect
                    && let Some(session) = client
                    && let Err(error) = session.send(message).await
                {
                    let _ = model.update(Action::Disconnected(error.to_string()));
                    break;
                }
            }
        }
        Ok(Err(error)) => {
            let _ = model.update(Action::ConnectionFailed(error.to_string()));
        }
        Err(_) => {
            let _ = model.update(Action::ConnectionFailed(format!(
                "connection timed out after {} seconds",
                CONNECT_TIMEOUT.as_secs()
            )));
        }
    }
}

fn protocol_id(seed: [u8; 32], generation: u64) -> Result<ProtocolId, TuiError> {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/tui-protocol/v1\0");
    hasher.update(seed);
    hasher.update(generation.to_be_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[0] |= 1;
    ProtocolId::new(bytes).map_err(|error| TuiError::InvalidValue(format!("{error:?}")))
}

fn process_seed(endpoint: &Path) -> [u8; 32] {
    let now =
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0_u128, |duration| duration.as_nanos());
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/tui-process/v1\0");
    hasher.update(std::process::id().to_be_bytes());
    hasher.update(now.to_be_bytes());
    hasher.update(endpoint.as_os_str().as_encoded_bytes());
    hasher.finalize().into()
}

struct TerminalOwner {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalOwner {
    fn enter() -> Result<Self, TuiError> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableBracketedPaste, Hide) {
            let _ = disable_raw_mode();
            return Err(TuiError::Io(error));
        }
        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(mut terminal) => {
                terminal.clear()?;
                Ok(Self { terminal })
            }
            Err(error) => {
                let mut stdout = io::stdout();
                let _ = execute!(stdout, Show, DisableBracketedPaste, LeaveAlternateScreen);
                let _ = disable_raw_mode();
                Err(TuiError::Io(error))
            }
        }
    }

    fn draw(&mut self, model: &AppModel) -> Result<(), TuiError> {
        self.terminal.draw(|frame| render::draw(frame, model))?;
        Ok(())
    }
}

impl Drop for TerminalOwner {
    fn drop(&mut self) {
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

struct InputPump {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl InputPump {
    fn start(sender: mpsc::Sender<Event>) -> Result<Self, TuiError> {
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

    fn stop(mut self) -> Result<(), TuiError> {
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

#[cfg(test)]
mod tests;
