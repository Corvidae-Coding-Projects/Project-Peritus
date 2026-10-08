//! Local daemon connection, A3 negotiation, and independently owned frame directions.

use std::path::Path;

use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, ClientHello, NegotiationOutcome, ProtocolContext,
    ProtocolFeatureName, ProtocolId, ServerHello, WellKnownProtocolFeature, decode_app_message,
    encode_app_message,
};
use peritus_codec::{HEADER_LEN, MAGIC};
use peritus_types::SessionId;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::TuiError;

const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(8);
const CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

trait LocalIo: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T> LocalIo for T where T: AsyncRead + AsyncWrite + Send + Unpin {}
type BoxedLocalIo = Box<dyn LocalIo>;

/// Successful connection facts used to initialize the reducer.
#[derive(Clone, Debug)]
pub struct EstablishedConnection {
    pub(crate) features: Vec<ProtocolFeatureName>,
    pub(crate) context: ProtocolContext,
    pub(crate) limits: AppProtocolLimits,
    pub(crate) server: String,
    pub(crate) downgraded: bool,
}

/// An asynchronous observation from the connection reader or writer.
#[allow(
    clippy::large_enum_variant,
    reason = "the reader transfers one bounded A3 frame directly to the reducer"
)]
#[derive(Debug)]
pub enum ClientEvent {
    Message { context: ProtocolContext, message: AppMessage },
    Disconnected { context: ProtocolContext, error: String },
}

impl ClientEvent {
    pub(crate) const fn context(&self) -> ProtocolContext {
        match self {
            Self::Message { context, .. } | Self::Disconnected { context, .. } => *context,
        }
    }
}

#[allow(
    clippy::large_enum_variant,
    reason = "the bounded writer queue owns exact A3 frames until transmission"
)]
enum WriterCommand {
    Message(AppMessage),
    Close { final_messages: Vec<AppMessage>, completed: oneshot::Sender<Result<(), String>> },
}

/// A live, negotiated daemon session with single-owner reader and writer tasks.
#[derive(Debug)]
pub struct ClientSession {
    established: EstablishedConnection,
    writer: mpsc::Sender<WriterCommand>,
    reader_task: JoinHandle<()>,
    writer_task: JoinHandle<()>,
}

impl ClientSession {
    pub(crate) async fn connect(
        endpoint: &Path,
        protocol_id: ProtocolId,
        requested_session: Option<SessionId>,
        events: mpsc::Sender<ClientEvent>,
    ) -> Result<Self, TuiError> {
        let mut io = connect_local(endpoint).await?;
        let limits = AppProtocolLimits::PRODUCTION;
        let hello = client_hello(protocol_id, requested_session, limits)?;
        write_frame(&mut io, &AppMessage::ClientHello(hello.clone()), limits).await?;
        let response = read_frame(&mut io, limits).await?;
        let AppMessage::ServerHello(server_hello) = response else {
            return Err(TuiError::ProtocolViolation(
                "first daemon frame was not ServerHello".to_owned(),
            ));
        };
        if matches!(
            server_hello.outcome(),
            NegotiationOutcome::Compatible(protocol) | NegotiationOutcome::Downgraded(protocol)
                if !hello.accepts(protocol)
        ) {
            return Err(TuiError::ProtocolViolation(
                "daemon selected a version, feature, or capacity outside the client offer"
                    .to_owned(),
            ));
        }
        let established = establish(protocol_id, &server_hello)?;
        let limits = established.limits;
        let context = established.context;
        let (read_half, write_half) = tokio::io::split(io);
        let (writer, writer_rx) = mpsc::channel(256);
        let reader_events = events.clone();
        let reader_task = tokio::spawn(async move {
            reader_loop(read_half, limits, context, reader_events).await;
        });
        let writer_task = tokio::spawn(async move {
            writer_loop(write_half, limits, context, writer_rx, events).await;
        });
        Ok(Self { established, writer, reader_task, writer_task })
    }

    pub(crate) const fn established(&self) -> &EstablishedConnection {
        &self.established
    }

    pub(crate) fn send(
        &self,
        message: AppMessage,
    ) -> impl Future<Output = Result<(), TuiError>> + Send + 'static {
        let writer = self.writer.clone();
        async move {
            writer
                .send(WriterCommand::Message(message))
                .await
                .map_err(|_| TuiError::Task("daemon writer is no longer available".to_owned()))
        }
    }

    pub(crate) async fn close(mut self, final_messages: Vec<AppMessage>) -> Result<(), TuiError> {
        let deadline = tokio::time::Instant::now() + CLOSE_TIMEOUT;
        let (completed_tx, completed_rx) = oneshot::channel();
        let sent = tokio::time::timeout_at(
            deadline,
            self.writer.send(WriterCommand::Close { final_messages, completed: completed_tx }),
        )
        .await;
        let write_result = match sent {
            Ok(Ok(())) => match tokio::time::timeout_at(deadline, completed_rx).await {
                Ok(Ok(Ok(()))) => Ok(()),
                Ok(Ok(Err(error))) => Err(TuiError::Task(error)),
                Ok(Err(_)) => Err(TuiError::Task(
                    "daemon writer stopped before close acknowledgement".to_owned(),
                )),
                Err(_) => Err(TuiError::Task(
                    "daemon writer did not acknowledge close in time".to_owned(),
                )),
            },
            Ok(Err(_)) => Err(TuiError::Task("daemon writer was already closed".to_owned())),
            Err(_) => Err(TuiError::Task(
                "daemon writer did not accept close before the deadline".to_owned(),
            )),
        };

        self.reader_task.abort();
        let reader_result = (&mut self.reader_task).await;
        if let Err(error) = reader_result
            && !error.is_cancelled()
        {
            return Err(TuiError::Task(error.to_string()));
        }
        let Ok(joined) = tokio::time::timeout_at(deadline, &mut self.writer_task).await else {
            self.writer_task.abort();
            let _ = (&mut self.writer_task).await;
            return Err(TuiError::Task(
                "daemon writer did not stop before the close deadline".to_owned(),
            ));
        };
        joined?;
        write_result
    }
}

impl Drop for ClientSession {
    fn drop(&mut self) {
        // Terminal failures and cancellation can bypass orderly close. Never detach
        // a reader or writer that still owns a live daemon connection.
        self.reader_task.abort();
        self.writer_task.abort();
    }
}

async fn reader_loop<R>(
    mut reader: R,
    limits: AppProtocolLimits,
    context: ProtocolContext,
    events: mpsc::Sender<ClientEvent>,
) where
    R: AsyncRead + Unpin,
{
    loop {
        match read_frame(&mut reader, limits).await {
            Ok(message) => {
                if events.send(ClientEvent::Message { context, message }).await.is_err() {
                    return;
                }
            }
            Err(error) => {
                let _ = events
                    .send(ClientEvent::Disconnected { context, error: error.to_string() })
                    .await;
                return;
            }
        }
    }
}

async fn writer_loop<W>(
    mut writer: W,
    limits: AppProtocolLimits,
    context: ProtocolContext,
    mut commands: mpsc::Receiver<WriterCommand>,
    events: mpsc::Sender<ClientEvent>,
) where
    W: AsyncWrite + Unpin,
{
    while let Some(command) = commands.recv().await {
        match command {
            WriterCommand::Message(message) => {
                if let Err(error) = write_frame(&mut writer, &message, limits).await {
                    let _ = events
                        .send(ClientEvent::Disconnected { context, error: error.to_string() })
                        .await;
                    return;
                }
            }
            WriterCommand::Close { final_messages, completed } => {
                let result = async {
                    for message in final_messages {
                        write_frame(&mut writer, &message, limits).await?;
                    }
                    tokio::time::timeout(SHUTDOWN_GRACE, writer.shutdown())
                        .await
                        .map_err(|_| TuiError::Task("daemon socket shutdown timed out".to_owned()))?
                        .map_err(TuiError::from)
                }
                .await
                .map_err(|error| error.to_string());
                let _ = completed.send(result);
                return;
            }
        }
    }
    let _ = tokio::time::timeout(SHUTDOWN_GRACE, writer.shutdown()).await;
}

fn client_hello(
    protocol_id: ProtocolId,
    requested_session: Option<SessionId>,
    limits: AppProtocolLimits,
) -> Result<ClientHello, TuiError> {
    let optional = [
        WellKnownProtocolFeature::EventSubscriptions,
        WellKnownProtocolFeature::ArtifactTransfer,
        WellKnownProtocolFeature::ApprovalPrompts,
        WellKnownProtocolFeature::UserInput,
        WellKnownProtocolFeature::TerminalStreaming,
        WellKnownProtocolFeature::TerminalFailure,
        WellKnownProtocolFeature::TerminalOutputGaps,
        WellKnownProtocolFeature::TerminalPipes,
        WellKnownProtocolFeature::ReadOnlyDiagnostics,
        WellKnownProtocolFeature::ProductDiagnostics,
        WellKnownProtocolFeature::ProductActivityPages,
        WellKnownProtocolFeature::ProductRunArtifacts,
        WellKnownProtocolFeature::WorkbenchControl,
        WellKnownProtocolFeature::WorkbenchInputs,
        WellKnownProtocolFeature::WorkbenchInputMoves,
        WellKnownProtocolFeature::WorkbenchRequestSources,
        WellKnownProtocolFeature::WorkbenchExecution,
        WellKnownProtocolFeature::WorkbenchConversation,
        WellKnownProtocolFeature::WorkbenchContinuationReceipts,
        WellKnownProtocolFeature::WorkbenchRunBinding,
        WellKnownProtocolFeature::WorkbenchContext,
        WellKnownProtocolFeature::WorkbenchCompaction,
        WellKnownProtocolFeature::WorkbenchBrief,
        WellKnownProtocolFeature::WorkbenchImages,
        WellKnownProtocolFeature::WorkbenchFiles,
        WellKnownProtocolFeature::WorkbenchFileSources,
        WellKnownProtocolFeature::WorkbenchGoals,
        WellKnownProtocolFeature::WorkbenchReview,
        WellKnownProtocolFeature::WorkbenchPreview,
        WellKnownProtocolFeature::WorkbenchPreviewOutput,
        WellKnownProtocolFeature::WorkbenchCheckpoints,
        WellKnownProtocolFeature::WorkbenchCheckpointCoverage,
        WellKnownProtocolFeature::ConversationLibrary,
        WellKnownProtocolFeature::ConversationForks,
        WellKnownProtocolFeature::WorkbenchPermissions,
        WellKnownProtocolFeature::WorkbenchMemory,
        WellKnownProtocolFeature::WorkbenchInit,
        WellKnownProtocolFeature::GracefulShutdown,
    ]
    .into_iter()
    .map(ProtocolFeatureName::well_known)
    .collect::<Result<Vec<_>, _>>()?;
    Ok(ClientHello::new_with_session(
        protocol_id,
        requested_session,
        vec![peritus_app_protocol::CURRENT_PROTOCOL_RANGE],
        Vec::new(),
        optional,
        limits,
        format!("peritus-tui/{}", env!("CARGO_PKG_VERSION")),
    )?)
}

fn establish(
    expected_protocol: ProtocolId,
    hello: &ServerHello,
) -> Result<EstablishedConnection, TuiError> {
    if hello.protocol_id() != expected_protocol {
        return Err(TuiError::ProtocolViolation(
            "ServerHello did not echo the client protocol identity".to_owned(),
        ));
    }
    let session = hello.established_session().ok_or_else(|| {
        TuiError::ProtocolViolation("daemon rejected protocol negotiation".to_owned())
    })?;
    let (protocol, downgraded) = match hello.outcome() {
        NegotiationOutcome::Compatible(protocol) => (protocol, false),
        NegotiationOutcome::Downgraded(protocol) => (protocol, true),
        NegotiationOutcome::Incompatible(reason) => {
            return Err(TuiError::ProtocolViolation(format!(
                "daemon is protocol-incompatible: {reason:?}"
            )));
        }
    };
    if protocol.version() != peritus_app_protocol::CURRENT_PROTOCOL_VERSION {
        return Err(TuiError::ProtocolViolation(
            "daemon selected a version outside the offered range".to_owned(),
        ));
    }
    Ok(EstablishedConnection {
        features: protocol.features().as_slice().to_vec(),
        context: ProtocolContext::new(expected_protocol, protocol.version(), session),
        limits: protocol.limits(),
        server: hello.implementation().as_str().to_owned(),
        downgraded,
    })
}

async fn read_frame<R>(reader: &mut R, limits: AppProtocolLimits) -> Result<AppMessage, TuiError>
where
    R: AsyncRead + Unpin + ?Sized,
{
    let mut header = [0_u8; HEADER_LEN];
    reader.read_exact(&mut header).await?;
    if header[..4] != MAGIC {
        return Err(TuiError::ProtocolViolation("invalid PRTS frame magic".to_owned()));
    }
    let payload_len_bytes: [u8; 4] = header[12..16]
        .try_into()
        .map_err(|_| TuiError::ProtocolViolation("invalid PRTS header length".to_owned()))?;
    let payload_len = usize::try_from(u32::from_be_bytes(payload_len_bytes))
        .map_err(|_| TuiError::ProtocolViolation("PRTS payload length overflow".to_owned()))?;
    let codec = limits.codec();
    let frame_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or_else(|| TuiError::ProtocolViolation("PRTS frame length overflow".to_owned()))?;
    if payload_len > codec.max_payload_bytes || frame_len > codec.max_frame_bytes {
        return Err(TuiError::ProtocolViolation(
            "daemon frame exceeds the negotiated receive limit".to_owned(),
        ));
    }
    let mut frame = Vec::with_capacity(frame_len);
    frame.extend_from_slice(&header);
    frame.resize(frame_len, 0);
    reader.read_exact(&mut frame[HEADER_LEN..]).await?;
    decode_app_message(&frame, limits).map_err(TuiError::from)
}

async fn write_frame<W>(
    writer: &mut W,
    message: &AppMessage,
    limits: AppProtocolLimits,
) -> Result<(), TuiError>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let frame = encode_app_message(message, limits)?;
    writer.write_all(&frame).await?;
    writer.flush().await.map_err(TuiError::from)
}

#[cfg(unix)]
async fn connect_local(endpoint: &Path) -> Result<BoxedLocalIo, std::io::Error> {
    tokio::net::UnixStream::connect(endpoint).await.map(|stream| Box::new(stream) as BoxedLocalIo)
}

#[cfg(windows)]
fn connect_local(endpoint: &Path) -> std::future::Ready<Result<BoxedLocalIo, std::io::Error>> {
    use tokio::net::windows::named_pipe::ClientOptions;

    std::future::ready(
        ClientOptions::new().open(endpoint).map(|stream| Box::new(stream) as BoxedLocalIo),
    )
}

#[cfg(all(test, unix))]
mod unix_tests;

#[cfg(test)]
mod tests;
