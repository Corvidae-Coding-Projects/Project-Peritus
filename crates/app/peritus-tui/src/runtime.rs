//! Orderly terminal ownership and asynchronous application runtime.

mod candidate;
mod clipboard;
mod connection;
mod files;
mod images;
mod interrupts;
mod product;
mod state;
mod terminal;

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use peritus_app_protocol::ProtocolId;
use peritus_types::SessionId;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use crate::{
    TuiError,
    action::{Action, Effect},
    client::ClientEvent,
    model::AppModel,
};
use connection::Connection;
use terminal::{InputPump, TerminalOwner};

pub use product::{ProductLaunchContext, ProductProviderOption};
pub use state::TuiState;

const UI_TICK: Duration = Duration::from_millis(250);
const CLOSE_GRACE: Duration = Duration::from_secs(8);

#[derive(Default)]
struct LocalReads {
    clipboard: clipboard::ClipboardWrites,
    files: files::FileReads,
    images: images::ImageReads,
    interrupts: Option<interrupts::Interrupts>,
}

/// Runtime configuration for one interactive client process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TuiConfig {
    endpoint: PathBuf,
    requested_session: Option<SessionId>,
    connection_timeout: Option<Duration>,
    product: Option<ProductLaunchContext>,
}

impl TuiConfig {
    /// Creates a configuration for one exact local daemon endpoint.
    #[must_use]
    pub fn new(endpoint: impl Into<PathBuf>) -> Self {
        Self {
            endpoint: endpoint.into(),
            requested_session: None,
            connection_timeout: None,
            product: None,
        }
    }

    /// Requests resumption of an existing durable application session.
    #[must_use]
    pub const fn with_session(mut self, session: SessionId) -> Self {
        self.requested_session = Some(session);
        self
    }

    /// Applies a caller-selected bound to each daemon connection attempt.
    #[must_use]
    pub const fn with_connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = Some(timeout);
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

    /// Returns the caller-selected daemon connection bound, if any.
    #[must_use]
    pub const fn connection_timeout(&self) -> Option<Duration> {
        self.connection_timeout
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
    /// Open a durable conversation using its launcher-resolved workspace facts.
    OpenConversation(peritus_app_protocol::WorkbenchQuery),
    /// Open a saved run using its launcher-resolved workspace facts.
    OpenRun {
        /// Exact run selected from the dashboard.
        run: peritus_types::RunId,
        /// Workspace observed on that run.
        workspace: peritus_types::WorkspaceId,
    },
}

/// Runs the interactive TUI until the user exits.
///
/// # Errors
///
/// Returns [`TuiError`] when terminal ownership cannot be established/restored or an orderly
/// connection shutdown reports failure. Live daemon disconnects are presented in the UI and may
/// be retried without ending the process.
pub async fn run(config: TuiConfig) -> Result<ExitReason, TuiError> {
    run_with_state(config, &mut TuiState::default()).await
}

/// Runs the interface while retaining its local conversation and drafts across daemon recovery.
///
/// # Errors
/// Returns the same terminal and connection-cleanup errors as [`run`].
pub async fn run_with_state(
    config: TuiConfig,
    state: &mut TuiState,
) -> Result<ExitReason, TuiError> {
    let seed = process_seed(config.endpoint());
    let mut reads = LocalReads { interrupts: Some(interrupts::listen()?), ..LocalReads::default() };
    let mut terminal = TerminalOwner::enter()?;
    let (input_tx, mut input_rx) = mpsc::channel(128);
    let mut input = InputPump::start(input_tx.clone())?;
    let (client_events_tx, mut client_events_rx) = mpsc::channel(512);
    let mut model = state.take_model(&config, seed);
    let mut connection = Connection::new(client_events_tx);
    connection.start(&config, &mut model);

    let mut tick = tokio::time::interval(UI_TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let result = loop {
        terminal.draw(&mut model)?;
        let effects = match next_effects(
            &mut model,
            &mut input_rx,
            &mut client_events_rx,
            &mut tick,
            &mut reads,
            &mut connection,
        )
        .await
        {
            Ok(effects) => effects,
            Err(error) => break Err(error),
        };
        match apply_effects(effects, &config, &mut model, &mut connection, &mut reads) {
            ControlFlow::Continue => {}
            ControlFlow::RunCandidate { workspace, instruction, candidate_digest } => {
                input.stop()?;
                terminal.suspend()?;
                let interrupts = reads.interrupts.as_mut().ok_or_else(|| {
                    TuiError::Task("terminal interrupt listener is unavailable".into())
                })?;
                let outcome =
                    candidate::execute(workspace, instruction, candidate_digest, interrupts).await;
                terminal.resume()?;
                input = InputPump::start(input_tx.clone())?;
                model.candidate_run_finished(outcome);
            }
            ControlFlow::Quit => break Ok(ExitReason::UserQuit),
            ControlFlow::RecoverDaemon => break Ok(ExitReason::RecoverDaemon),
            ControlFlow::OpenConversation(query) => break Ok(ExitReason::OpenConversation(query)),
            ControlFlow::OpenRun { run, workspace } => {
                break Ok(ExitReason::OpenRun { run, workspace });
            }
        }
    };

    input.stop()?;
    let cleanup = model.cleanup_messages();
    let cleanup_result = connection.close(cleanup).await;
    if matches!(
        result,
        Ok(ExitReason::RecoverDaemon
            | ExitReason::OpenConversation(_)
            | ExitReason::OpenRun { .. })
    ) {
        if let Err(error) = cleanup_result {
            model.update(Action::Disconnected(error.to_string()));
        }
        state.retain(config, model);
    } else {
        cleanup_result?;
    }
    result
}

fn client_action(
    event: ClientEvent,
    current: Option<peritus_app_protocol::ProtocolContext>,
) -> Option<Action> {
    if Some(event.context()) != current {
        return None;
    }
    Some(match event {
        ClientEvent::Message { message, .. } => Action::Message(message),
        ClientEvent::Disconnected { error, .. } => Action::Disconnected(error),
    })
}

enum ControlFlow {
    Continue,
    RunCandidate {
        workspace: PathBuf,
        instruction: String,
        candidate_digest: peritus_types::Sha256Digest,
    },
    Quit,
    RecoverDaemon,
    OpenConversation(peritus_app_protocol::WorkbenchQuery),
    OpenRun {
        run: peritus_types::RunId,
        workspace: peritus_types::WorkspaceId,
    },
}

fn apply_effects(
    effects: Vec<Effect>,
    config: &TuiConfig,
    model: &mut AppModel,
    connection: &mut Connection,
    reads: &mut LocalReads,
) -> ControlFlow {
    for effect in effects {
        match effect {
            Effect::CopyText { operation, text } => {
                if !reads.clipboard.start(operation, text) {
                    let _ = model.update(Action::ClipboardWritten {
                        operation,
                        result: Err("Another clipboard write is still finishing".to_owned()),
                    });
                }
            }
            Effect::ReadFile { operation, path, range } => {
                if !reads.files.start(operation, path, range) {
                    let _ = model.update(Action::FileRead {
                        operation,
                        result: Err(
                            "Another explicit text read is still finishing; try again shortly.",
                        ),
                    });
                }
            }
            Effect::ReadImage { operation, path } => {
                if !reads.images.start(operation, path) {
                    let _ = model.update(Action::ImageRead {
                        operation,
                        result: Err(
                            "Another explicit file read is still finishing; try again shortly.",
                        ),
                    });
                }
            }
            Effect::Send(message) => {
                if let Err(error) = connection.send(message) {
                    connection.sent(Err(error), model);
                }
            }
            Effect::Reconnect => {
                if config.product().is_some() {
                    return ControlFlow::RecoverDaemon;
                }
                connection.start(config, model);
            }
            Effect::OpenConversation(query) => return ControlFlow::OpenConversation(query),
            Effect::OpenRun { run, workspace } => return ControlFlow::OpenRun { run, workspace },
            Effect::RunCandidate { workspace, instruction, candidate_digest } => {
                return ControlFlow::RunCandidate { workspace, instruction, candidate_digest };
            }
            Effect::Quit => return ControlFlow::Quit,
        }
    }
    ControlFlow::Continue
}

async fn next_effects(
    model: &mut AppModel,
    input: &mut mpsc::Receiver<crossterm::event::Event>,
    events: &mut mpsc::Receiver<ClientEvent>,
    tick: &mut tokio::time::Interval,
    reads: &mut LocalReads,
    connection: &mut Connection,
) -> Result<Vec<Effect>, TuiError> {
    loop {
        let action = tokio::select! {
            () = interrupts::next(&mut reads.interrupts) => return Ok(vec![Effect::Quit]),
            input = input.recv() => match input {
                Some(event) => Action::TerminalEvent(event),
                None => return Err(TuiError::Task("terminal input worker stopped".to_owned())),
            },
            event = events.recv() => match event {
                Some(event) => match client_action(event, model.protocol_context()) {
                    Some(action) => action,
                    None => continue,
                },
                None => Action::Disconnected("all daemon client tasks stopped".to_owned()),
            },
            effects = connection.next(model), if connection.active() || connection.sending() => return Ok(effects),
            _ = tick.tick() => Action::Tick(std::time::Instant::now()),
            file = reads.files.next(), if reads.files.active() => file,
            image = reads.images.next(), if reads.images.active() => image,
            clipboard = reads.clipboard.next(), if reads.clipboard.active() => match clipboard {
                Some(action) => action,
                None => continue,
            },
        };
        return Ok(model.update(action));
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

#[cfg(test)]
mod tests;
