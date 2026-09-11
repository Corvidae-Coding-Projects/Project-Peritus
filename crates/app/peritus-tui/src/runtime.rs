//! Orderly terminal ownership and asynchronous application runtime.

mod candidate;
mod files;
mod images;
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
    client::{ClientEvent, ClientSession},
    model::AppModel,
};
use terminal::{InputPump, TerminalOwner};

pub use product::{ProductLaunchContext, ProductProviderOption};
pub use state::TuiState;

const UI_TICK: Duration = Duration::from_millis(250);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Default)]
struct LocalReads {
    files: files::FileReads,
    images: images::ImageReads,
}

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
    let mut terminal = TerminalOwner::enter()?;
    let (input_tx, mut input_rx) = mpsc::channel(128);
    let mut input = InputPump::start(input_tx.clone())?;
    let (client_events_tx, mut client_events_rx) = mpsc::channel(512);
    let mut model = state.take_model(&config, seed);
    let mut client = None;
    let mut reads = LocalReads::default();
    let mut connection_generation = 0_u64;
    connect(&config, &mut model, &mut client, &client_events_tx, &mut connection_generation).await;

    let mut tick = tokio::time::interval(UI_TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let result = loop {
        terminal.draw(&mut model)?;
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
            _ = tick.tick() => Action::Tick(std::time::Instant::now()),
            file = reads.files.next(), if reads.files.active() => file,
            image = reads.images.next(), if reads.images.active() => image,
        };
        let effects = model.update(action);
        match apply_effects(
            effects,
            &config,
            &mut model,
            &mut client,
            &client_events_tx,
            &mut connection_generation,
            &mut reads,
        )
        .await
        {
            Ok(ControlFlow::Continue) => {}
            Ok(ControlFlow::RunCandidate { workspace, instruction, candidate_digest }) => {
                input.stop()?;
                terminal.suspend()?;
                let outcome = candidate::execute(workspace, instruction, candidate_digest).await;
                terminal.resume()?;
                input = InputPump::start(input_tx.clone())?;
                model.candidate_run_finished(outcome);
            }
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
    if matches!(result, Ok(ExitReason::RecoverDaemon)) {
        state.retain(config, model);
    }
    result
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
}

async fn apply_effects(
    effects: Vec<Effect>,
    config: &TuiConfig,
    model: &mut AppModel,
    client: &mut Option<ClientSession>,
    events: &mpsc::Sender<ClientEvent>,
    generation: &mut u64,
    reads: &mut LocalReads,
) -> Result<ControlFlow, TuiError> {
    for effect in effects {
        match effect {
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
            Effect::RunCandidate { workspace, instruction, candidate_digest } => {
                return Ok(ControlFlow::RunCandidate { workspace, instruction, candidate_digest });
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
            let mut effects = model.update(Action::Connected {
                context: established.context,
                limits: established.limits,
                server: established.server,
                downgraded: established.downgraded,
            });
            effects.extend(model.update(Action::NegotiatedFeatures {
                context: established.context,
                features: established.features,
            }));
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

#[cfg(test)]
mod tests;
