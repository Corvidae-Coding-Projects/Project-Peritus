//! Orderly terminal ownership and asynchronous application runtime.

mod candidate;
mod clipboard;
mod connection;
mod diagnostics;
mod editor;
mod files;
mod images;
mod interrupts;
mod local;
mod modules;
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
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Default)]
struct LocalReads {
    local: local::LocalOperations,
    archive: local::TranscriptWrites,
    editor_active: bool,
    transcript_search: Option<(u64, String, String)>,
    modules: modules::ModuleOperations,
    clipboard: clipboard::ClipboardWrites,
    files: files::FileReads,
    images: images::ImageReads,
    interrupts: Option<interrupts::Interrupts>,
}

impl LocalReads {
    fn start_pending_search(&mut self) {
        if self.archive.active() || self.editor_active {
            return;
        }
        let Some((generation, scope, query)) = self.transcript_search.as_ref() else { return };
        if self.local.start(
            *generation,
            crate::local::LocalOperation::SearchTranscript {
                scope: scope.clone(),
                query: query.clone(),
            },
        ) {
            self.transcript_search = None;
        }
    }
}

/// Runtime configuration for one interactive client process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TuiConfig {
    endpoint: PathBuf,
    requested_session: Option<SessionId>,
    product: Option<ProductLaunchContext>,
    diagnostic_log: Option<PathBuf>,
}

impl TuiConfig {
    /// Creates a configuration for one exact local daemon endpoint.
    #[must_use]
    pub fn new(endpoint: impl Into<PathBuf>) -> Self {
        Self {
            endpoint: endpoint.into(),
            requested_session: None,
            product: None,
            diagnostic_log: None,
        }
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

    /// Records transport failures in an application-owned private diagnostic file.
    /// Message bodies, drafts and credentials are never included.
    #[must_use]
    pub fn with_diagnostic_log(mut self, path: PathBuf) -> Self {
        self.diagnostic_log = Some(path);
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
    let mut connection = Connection::new(client_events_tx, config.diagnostic_log.clone());
    connection.start(&config, &mut model);

    let mut tick = tokio::time::interval(UI_TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let result = loop {
        reads.archive.observe(&model);
        terminal.draw(&mut model)?;
        let effects = match next_effects(
            &mut model,
            &mut input_rx,
            &mut client_events_rx,
            &mut tick,
            &mut reads,
            &mut connection,
            &config,
        )
        .await
        {
            Ok(effects) => effects,
            Err(error) => break Err(error),
        };
        match apply_effects(effects, &config, &mut model, &mut connection, &mut reads) {
            ControlFlow::Continue => {}
            ControlFlow::EditDraft { generation, text } => {
                input.stop()?;
                // Input already queued for the composer belongs to the old terminal
                // ownership interval, never to the editor or a restored draft.
                while input_rx.try_recv().is_ok() {}
                terminal.suspend()?;
                reads.editor_active = true;
                let mut editing = Box::pin(editor::execute(text));
                let mut deferred = ControlFlow::Continue;
                let outcome = loop {
                    tokio::select! {
                        result = &mut editing => break result,
                        effects = background_effects(&mut model, &mut client_events_rx,
                            &mut tick, &mut reads, &mut connection, &config) => {
                            reads.archive.observe(&model);
                            let flow = apply_effects(effects, &config, &mut model, &mut connection, &mut reads);
                            if !matches!(flow, ControlFlow::Continue) {
                                deferred = flow;
                            }
                        }
                    }
                };
                reads.editor_active = false;
                if let Some(interrupts) = reads.interrupts.as_mut() {
                    interrupts::drain(interrupts);
                }
                // An editor may leave keystrokes in either queue. Flush before the new
                // input worker starts so closing it can never submit a prompt by accident.
                while input_rx.try_recv().is_ok() {}
                terminal.resume()?;
                drain_editor_input();
                input = InputPump::start(input_tx.clone())?;
                let effects = model.update(Action::LocalResult { generation, result: outcome });
                let flow = apply_effects(effects, &config, &mut model, &mut connection, &mut reads);
                let flow = if matches!(flow, ControlFlow::Continue) { deferred } else { flow };
                match flow {
                    ControlFlow::Quit => break Ok(ExitReason::UserQuit),
                    ControlFlow::RecoverDaemon => break Ok(ExitReason::RecoverDaemon),
                    ControlFlow::OpenConversation(query) => {
                        break Ok(ExitReason::OpenConversation(query));
                    }
                    ControlFlow::OpenRun { run, workspace } => {
                        break Ok(ExitReason::OpenRun { run, workspace });
                    }
                    ControlFlow::Continue => {}
                    ControlFlow::RunCandidate { .. } | ControlFlow::EditDraft { .. } => {
                        // Interactive foreground actions originate in user input, which
                        // is suspended for the editor. Never start a second one implicitly.
                        let _ = model.update(Action::LocalResult { generation,
                            result: Err("Another foreground request arrived while editing; repeat it explicitly.".into()) });
                    }
                }
            }
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
    // Preserve the last public page before normal process exit. Archive work has one
    // writer and one coalesced pending page, independent of daemon cleanup.
    let _ = tokio::time::timeout(Duration::from_secs(2), reads.archive.finish()).await;
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
    EditDraft {
        generation: u64,
        text: String,
    },
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
            Effect::LocalOperation { generation, operation } => {
                if reads.editor_active {
                    let _ = model.update(Action::LocalResult { generation,
                        result: Err("The external editor still owns the terminal; finish or close it first.".into()) });
                } else if let crate::local::LocalOperation::SearchTranscript { scope, query } =
                    operation
                {
                    // Persist all public activity observed before this query, then wait
                    // beside normal input/transport for the archive writer to drain.
                    reads.archive.observe(model);
                    reads.local.cancel_search();
                    reads.transcript_search = Some((generation, scope, query));
                    reads.start_pending_search();
                } else if let crate::local::LocalOperation::EditDraft { text } = operation {
                    if reads.local.active() {
                        let _ = model.update(Action::LocalResult { generation,
                            result: Err("A local operation is still finishing; try the editor again shortly.".into()) });
                    } else {
                        return ControlFlow::EditDraft { generation, text };
                    }
                } else {
                    reads.transcript_search = None;
                    if !reads.local.start(generation, operation) {
                        let _ = model.update(Action::LocalResult {
                            generation,
                            result: Err(
                                "Another local operation is still finishing; try again shortly."
                                    .into(),
                            ),
                        });
                    }
                }
            }
            Effect::ModuleOperation { generation, operation } => {
                if !reads.modules.start(generation, operation) {
                    let _ = model.update(Action::ModuleResult { generation, result: Err("A module operation is still finishing. Reopen the menu after it completes.".into()) });
                }
            }
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
    config: &TuiConfig,
) -> Result<Vec<Effect>, TuiError> {
    loop {
        reads.start_pending_search();
        let action = tokio::select! {
            () = interrupts::next(&mut reads.interrupts) => return Ok(vec![Effect::Quit]),
            input = input.recv() => match input {
                Some(event) => Action::TerminalEvent(event),
                None => return Err(TuiError::Task("terminal input worker stopped".to_owned())),
            },
            event = events.recv() => match event {
                Some(event) => match client_action(event, model.protocol_context()) {
                    Some(Action::Disconnected(error)) => return Ok(connection.disconnected("read/write", error, model)),
                    Some(action) => action,
                    None => continue,
                },
                None => Action::Disconnected("all daemon client tasks stopped".to_owned()),
            },
            effects = connection.next(config, model), if connection.active() || connection.sending() => return Ok(effects),
            _ = tick.tick() => Action::Tick(std::time::Instant::now()),
            local = reads.local.next(), if reads.local.active() => local,
            archive = reads.archive.next(), if reads.archive.active() => match archive {
                Some(action) => action,
                None => continue,
            },
            file = reads.files.next(), if reads.files.active() => file,
            modules = reads.modules.next(), if reads.modules.active() => modules,
            image = reads.images.next(), if reads.images.active() => image,
            clipboard = reads.clipboard.next(), if reads.clipboard.active() => match clipboard {
                Some(action) => action,
                None => continue,
            },
        };
        return Ok(model.update(action));
    }
}

/// Like the foreground loop, without reading or drawing over the external editor.
async fn background_effects(
    model: &mut AppModel,
    events: &mut mpsc::Receiver<ClientEvent>,
    tick: &mut tokio::time::Interval,
    reads: &mut LocalReads,
    connection: &mut Connection,
    config: &TuiConfig,
) -> Vec<Effect> {
    loop {
        reads.start_pending_search();
        let action = tokio::select! {
            event = events.recv() => match event {
                Some(event) => match client_action(event, model.protocol_context()) {
                    Some(Action::Disconnected(error)) => return connection.disconnected("read/write", error, model),
                    Some(action) => action,
                    None => continue,
                },
                None => Action::Disconnected("all daemon client tasks stopped".into()),
            },
            effects = connection.next(config, model), if connection.active() || connection.sending() => return effects,
            _ = tick.tick() => Action::Tick(std::time::Instant::now()),
            local = reads.local.next(), if reads.local.active() => local,
            archive = reads.archive.next(), if reads.archive.active() => match archive {
                Some(action) => action,
                None => continue,
            },
            file = reads.files.next(), if reads.files.active() => file,
            modules = reads.modules.next(), if reads.modules.active() => modules,
            image = reads.images.next(), if reads.images.active() => image,
            clipboard = reads.clipboard.next(), if reads.clipboard.active() => match clipboard {
                Some(action) => action,
                None => continue,
            },
        };
        return model.update(action);
    }
}

fn drain_editor_input() {
    #[cfg(unix)]
    {
        // Discard unread OS input after restoring raw mode. Crossterm's parsed
        // event queue is separate and is drained below before the worker restarts.
        let _ = nix::sys::termios::tcflush(std::io::stdin(), nix::sys::termios::FlushArg::TCIFLUSH);
    }
    // Bounded by both event count and elapsed time; no background clipboard polling.
    let until = std::time::Instant::now() + Duration::from_millis(20);
    for _ in 0..1024 {
        if std::time::Instant::now() >= until
            || !matches!(crossterm::event::poll(Duration::ZERO), Ok(true))
        {
            break;
        }
        if crossterm::event::read().is_err() {
            break;
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
