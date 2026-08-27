//! Local daemon connection, A3 negotiation, and independently owned frame directions.

use std::path::Path;

use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, ClientHello, NegotiationOutcome, ProtocolContext,
    ProtocolFeatureName, ProtocolId, ServerHello, VersionRange, WellKnownProtocolFeature,
    decode_app_message, encode_app_message,
};
use peritus_codec::{HEADER_LEN, MAGIC};
use peritus_types::SessionId;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::TuiError;

trait LocalIo: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T> LocalIo for T where T: AsyncRead + AsyncWrite + Send + Unpin {}
type BoxedLocalIo = Box<dyn LocalIo>;

/// Successful connection facts used to initialize the reducer.
#[derive(Clone, Debug)]
pub struct EstablishedConnection {
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
    Message(AppMessage),
    Disconnected(String),
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
        write_frame(&mut io, &AppMessage::ClientHello(hello), limits).await?;
        let response = read_frame(&mut io, limits).await?;
        let AppMessage::ServerHello(server_hello) = response else {
            return Err(TuiError::ProtocolViolation(
                "first daemon frame was not ServerHello".to_owned(),
            ));
        };
        let established = establish(protocol_id, &server_hello)?;
        let limits = established.limits;
        let (read_half, write_half) = tokio::io::split(io);
        let (writer, writer_rx) = mpsc::channel(256);
        let reader_events = events.clone();
        let reader_task = tokio::spawn(async move {
            reader_loop(read_half, limits, reader_events).await;
        });
        let writer_task = tokio::spawn(async move {
            writer_loop(write_half, limits, writer_rx, events).await;
        });
        Ok(Self { established, writer, reader_task, writer_task })
    }

    pub(crate) const fn established(&self) -> &EstablishedConnection {
        &self.established
    }

    pub(crate) async fn send(&self, message: AppMessage) -> Result<(), TuiError> {
        self.writer
            .send(WriterCommand::Message(message))
            .await
            .map_err(|_| TuiError::Task("daemon writer is no longer available".to_owned()))
    }

    pub(crate) async fn close(self, final_messages: Vec<AppMessage>) -> Result<(), TuiError> {
        let (completed_tx, completed_rx) = oneshot::channel();
        let sent = self
            .writer
            .send(WriterCommand::Close { final_messages, completed: completed_tx })
            .await;
        let write_result = if sent.is_ok() {
            match completed_rx.await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(TuiError::Task(error)),
                Err(_) => Err(TuiError::Task(
                    "daemon writer stopped before close acknowledgement".to_owned(),
                )),
            }
        } else {
            Err(TuiError::Task("daemon writer was already closed".to_owned()))
        };

        self.reader_task.abort();
        let reader_result = self.reader_task.await;
        if let Err(error) = reader_result
            && !error.is_cancelled()
        {
            return Err(TuiError::Task(error.to_string()));
        }
        self.writer_task.await?;
        write_result
    }
}

async fn reader_loop<R>(mut reader: R, limits: AppProtocolLimits, events: mpsc::Sender<ClientEvent>)
where
    R: AsyncRead + Unpin,
{
    loop {
        match read_frame(&mut reader, limits).await {
            Ok(message) => {
                if events.send(ClientEvent::Message(message)).await.is_err() {
                    return;
                }
            }
            Err(error) => {
                let _ = events.send(ClientEvent::Disconnected(error.to_string())).await;
                return;
            }
        }
    }
}

async fn writer_loop<W>(
    mut writer: W,
    limits: AppProtocolLimits,
    mut commands: mpsc::Receiver<WriterCommand>,
    events: mpsc::Sender<ClientEvent>,
) where
    W: AsyncWrite + Unpin,
{
    while let Some(command) = commands.recv().await {
        match command {
            WriterCommand::Message(message) => {
                if let Err(error) = write_frame(&mut writer, &message, limits).await {
                    let _ = events.send(ClientEvent::Disconnected(error.to_string())).await;
                    return;
                }
            }
            WriterCommand::Close { final_messages, completed } => {
                let result = async {
                    for message in final_messages {
                        write_frame(&mut writer, &message, limits).await?;
                    }
                    writer.shutdown().await.map_err(TuiError::from)
                }
                .await
                .map_err(|error| error.to_string());
                let _ = completed.send(result);
                return;
            }
        }
    }
    let _ = writer.shutdown().await;
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
        WellKnownProtocolFeature::ReadOnlyDiagnostics,
        WellKnownProtocolFeature::GracefulShutdown,
    ]
    .into_iter()
    .map(ProtocolFeatureName::well_known)
    .collect::<Result<Vec<_>, _>>()?;
    Ok(ClientHello::new_with_session(
        protocol_id,
        requested_session,
        vec![VersionRange::new(1, 0, 0)?],
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
    Ok(EstablishedConnection {
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
    writer.flush().await?;
    Ok(())
}

#[cfg(unix)]
async fn connect_local(endpoint: &Path) -> Result<BoxedLocalIo, std::io::Error> {
    tokio::net::UnixStream::connect(endpoint).await.map(|stream| Box::new(stream) as BoxedLocalIo)
}

#[cfg(windows)]
async fn connect_local(endpoint: &Path) -> Result<BoxedLocalIo, std::io::Error> {
    use tokio::net::windows::named_pipe::ClientOptions;

    ClientOptions::new().open(endpoint).map(|stream| Box::new(stream) as BoxedLocalIo)
}
