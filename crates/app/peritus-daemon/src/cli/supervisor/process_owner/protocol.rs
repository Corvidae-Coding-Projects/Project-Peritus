//! Checksummed private pipe protocol between one daemon attempt and its service owner.

use std::io::{Read, Write};

use peritus_process::{
    CancellationReason, ControlRejection, ErrorCode, ProcessCursor, ProcessError, ProcessEvent,
    NativePlatform, NativeRecoveryPhase, NativeSessionRecovery, OutputStream, ProcessOperation,
    ProcessSignal, ProcessTreeIdentity, RecoveryClass, RetainedOwnerNonce,
    RetainedOwnerObservation, RetainedProcessKey, RetainedStreamPage, TerminalResult, TerminalSize,
};
use peritus_types::{ProcessId, Sha256Digest};

const REQUEST_MAGIC: &[u8] = b"PERITUS-OWNER-RPC-REQUEST-V1\0";
const RESPONSE_MAGIC: &[u8] = b"PERITUS-OWNER-RPC-RESPONSE-V1\0";

pub(super) enum Command {
    Launch { key: RetainedProcessKey, request_digest: Sha256Digest, request: Vec<u8> },
    Write { key: RetainedProcessKey, wait: bool, bytes: Vec<u8> },
    Close { key: RetainedProcessKey, wait: bool },
    Resize { key: RetainedProcessKey, size: TerminalSize },
    Signal { key: RetainedProcessKey, signal: ProcessSignal },
    Cancel {
        key: RetainedProcessKey,
        request_digest: Sha256Digest,
        reason: CancellationReason,
    },
    Observe { key: RetainedProcessKey, cursor: ProcessCursor, max_events: usize, wait: Option<std::time::Duration> },
    Stream { key: RetainedProcessKey, stream: OutputStream, snapshot_digest: Option<Sha256Digest>, offset: u64, max_bytes: usize },
    Wait { key: RetainedProcessKey },
    LaunchStatus { key: RetainedProcessKey, request_digest: Sha256Digest },
}

pub(super) enum Reply {
    Unit,
    Observation(RetainedOwnerObservation),
    StreamPage(RetainedStreamPage),
    Terminal(TerminalResult),
    Pending,
    Error(ProcessError),
}

pub(super) fn write_command(writer: &mut impl Write, command: &Command) -> std::io::Result<()> {
    let payload = encode_command(command)?;
    write_frame(writer, REQUEST_MAGIC, &payload)
}

pub(super) fn read_command(reader: &mut impl Read) -> std::io::Result<Option<Command>> {
    read_frame(reader, REQUEST_MAGIC)?.map(|payload| decode_command(&payload)).transpose()
}

pub(super) fn write_reply(writer: &mut impl Write, reply: &Reply) -> std::io::Result<()> {
    let payload = encode_reply(reply)?;
    write_frame(writer, RESPONSE_MAGIC, &payload)
}

pub(super) fn read_reply(reader: &mut impl Read) -> std::io::Result<Option<Reply>> {
    read_frame(reader, RESPONSE_MAGIC)?.map(|payload| decode_reply(&payload)).transpose()
}

fn encode_command(command: &Command) -> std::io::Result<Vec<u8>> {
    let mut writer = Writer::new();
    match command {
        Command::Launch { key, request_digest, request } => {
            writer.u8(1); writer.key(*key); writer.digest(*request_digest); writer.frame(request)?;
        }
        Command::Write { key, wait, bytes } => {
            writer.u8(2); writer.key(*key); writer.boolean(*wait); writer.frame(bytes)?;
        }
        Command::Close { key, wait } => { writer.u8(3); writer.key(*key); writer.boolean(*wait); }
        Command::Resize { key, size } => {
            writer.u8(4); writer.key(*key);
            for value in [size.rows(), size.columns(), size.pixel_width(), size.pixel_height()] { writer.u16(value); }
        }
        Command::Signal { key, signal } => { writer.u8(5); writer.key(*key); writer.u8(signal_tag(*signal)); }
        Command::Cancel { key, request_digest, reason } => {
            writer.u8(6); writer.key(*key); writer.digest(*request_digest);
            writer.u8(reason_tag(*reason));
        }
        Command::Observe { key, cursor, max_events, wait } => {
            writer.u8(7); writer.key(*key); writer.frame(&cursor.encode_retained_owner())?;
            writer.u32(u32::try_from(*max_events).map_err(|_| invalid_data())?);
            match wait {
                None => writer.u8(0),
                Some(wait) => {
                    writer.u8(1);
                    writer.u64(u64::try_from(wait.as_millis()).map_err(|_| invalid_data())?);
                }
            }
        }
        Command::Stream { key, stream, snapshot_digest, offset, max_bytes } => {
            writer.u8(9); writer.key(*key); writer.u8(stream_tag(*stream));
            optional_digest(&mut writer, *snapshot_digest);
            writer.u64(*offset);
            writer.u32(u32::try_from(*max_bytes).map_err(|_| invalid_data())?);
        }
        Command::Wait { key } => { writer.u8(8); writer.key(*key); }
        Command::LaunchStatus { key, request_digest } => {
            writer.u8(10); writer.key(*key); writer.digest(*request_digest);
        }
    }
    Ok(writer.finish())
}

fn decode_command(bytes: &[u8]) -> std::io::Result<Command> {
    let mut reader = Reader::new(bytes);
    let command = match reader.u8()? {
        1 => Command::Launch { key: reader.key()?, request_digest: reader.digest()?, request: reader.frame()?.to_vec() },
        2 => Command::Write { key: reader.key()?, wait: reader.boolean()?, bytes: reader.frame()?.to_vec() },
        3 => Command::Close { key: reader.key()?, wait: reader.boolean()? },
        4 => Command::Resize { key: reader.key()?, size: TerminalSize::new(reader.u16()?, reader.u16()?, reader.u16()?, reader.u16()?).map_err(|_| invalid_data())? },
        5 => Command::Signal { key: reader.key()?, signal: decode_signal(reader.u8()?)? },
        6 => Command::Cancel {
            key: reader.key()?,
            request_digest: reader.digest()?,
            reason: decode_reason(reader.u8()?)?,
        },
        7 => {
            let key = reader.key()?;
            let cursor = ProcessCursor::decode_retained_owner(reader.frame()?).map_err(|_| invalid_data())?;
            let max_events = usize::try_from(reader.u32()?).map_err(|_| invalid_data())?;
            let wait = match reader.u8()? { 0 => None, 1 => Some(std::time::Duration::from_millis(reader.u64()?)), _ => return Err(invalid_data()) };
            Command::Observe { key, cursor, max_events, wait }
        }
        8 => Command::Wait { key: reader.key()? },
        9 => Command::Stream {
            key: reader.key()?,
            stream: decode_stream(reader.u8()?)?,
            snapshot_digest: decode_optional_digest(&mut reader)?,
            offset: reader.u64()?,
            max_bytes: usize::try_from(reader.u32()?).map_err(|_| invalid_data())?,
        },
        10 => Command::LaunchStatus { key: reader.key()?, request_digest: reader.digest()? },
        _ => return Err(invalid_data()),
    };
    reader.finish()?;
    Ok(command)
}

fn encode_reply(reply: &Reply) -> std::io::Result<Vec<u8>> {
    let mut writer = Writer::new();
    match reply {
        Reply::Unit => writer.u8(1),
        Reply::Observation(observation) => {
            writer.u8(2);
            writer.u32(u32::try_from(observation.events().len()).map_err(|_| invalid_data())?);
            for event in observation.events() { writer.frame(&event.encode_retained_owner().map_err(|_| invalid_data())?)?; }
            for stream in [peritus_process::OutputStream::Stdout, peritus_process::OutputStream::Stderr, peritus_process::OutputStream::Terminal] { writer.frame(observation.stream(stream))?; }
            match observation.terminal_result() { None => writer.u8(0), Some(terminal) => { writer.u8(1); writer.frame(&terminal.encode_retained_owner().map_err(|_| invalid_data())?)?; } }
            writer.boolean(observation.owner_finished());
            encode_tree(&mut writer, observation.tree_identity());
            encode_native_recovery(&mut writer, observation.native_recovery())?;
        }
        Reply::Terminal(terminal) => { writer.u8(3); writer.frame(&terminal.encode_retained_owner().map_err(|_| invalid_data())?)?; }
        Reply::Pending => writer.u8(5),
        Reply::StreamPage(page) => {
            writer.u8(6); writer.u8(stream_tag(page.stream()));
            writer.digest(page.snapshot_digest()); writer.u64(page.total_bytes());
            writer.u64(page.offset()); writer.frame(page.bytes())?;
        }
        Reply::Error(error) => {
            writer.u8(4); writer.u8(code_tag(error.code())); writer.u8(operation_tag(error.operation())); writer.u8(recovery_tag(error.recovery()));
            writer.u8(error.control_rejection().map_or(0, control_tag));
        }
    }
    Ok(writer.finish())
}

fn decode_reply(bytes: &[u8]) -> std::io::Result<Reply> {
    let mut reader = Reader::new(bytes);
    let reply = match reader.u8()? {
        1 => Reply::Unit,
        2 => {
            let count = usize::try_from(reader.u32()?).map_err(|_| invalid_data())?;
            let mut events = Vec::new();
            events.try_reserve_exact(count).map_err(|_| invalid_data())?;
            for _ in 0..count { events.push(ProcessEvent::decode_retained_owner(reader.frame()?).map_err(|_| invalid_data())?); }
            let stdout = reader.frame()?.to_vec();
            let stderr = reader.frame()?.to_vec();
            let terminal_output = reader.frame()?.to_vec();
            let terminal = match reader.u8()? { 0 => None, 1 => Some(TerminalResult::decode_retained_owner(reader.frame()?).map_err(|_| invalid_data())?), _ => return Err(invalid_data()) };
            let owner_finished = reader.boolean()?;
            let tree = decode_tree(&mut reader)?;
            let native_recovery = decode_native_recovery(&mut reader)?;
            Reply::Observation(RetainedOwnerObservation::new(events, stdout, stderr, terminal_output, terminal, owner_finished, tree, native_recovery))
        }
        3 => Reply::Terminal(TerminalResult::decode_retained_owner(reader.frame()?).map_err(|_| invalid_data())?),
        4 => {
            let code = decode_code(reader.u8()?)?;
            let operation = decode_operation(reader.u8()?)?;
            let recovery = decode_recovery(reader.u8()?)?;
            let rejection = match reader.u8()? { 0 => None, value => Some(decode_control(value)?) };
            Reply::Error(ProcessError::from_retained_owner(code, operation, recovery, rejection))
        }
        5 => Reply::Pending,
        6 => Reply::StreamPage(
            RetainedStreamPage::new(
                decode_stream(reader.u8()?)?,
                reader.digest()?,
                reader.u64()?,
                reader.u64()?,
                reader.frame()?.to_vec(),
            )
            .map_err(|_| invalid_data())?,
        ),
        _ => return Err(invalid_data()),
    };
    reader.finish()?;
    Ok(reply)
}

fn write_frame(writer: &mut impl Write, magic: &[u8], payload: &[u8]) -> std::io::Result<()> {
    let length = u32::try_from(payload.len()).map_err(|_| invalid_data())?;
    let mut header = Vec::with_capacity(magic.len() + 4);
    header.extend_from_slice(magic); header.extend_from_slice(&length.to_be_bytes());
    let mut digest_input = header.clone(); digest_input.extend_from_slice(payload);
    let checksum = peritus_codec::sha256(&digest_input);
    writer.write_all(&header)?; writer.write_all(payload)?; writer.write_all(checksum.as_bytes())?; writer.flush()
}

fn read_frame(reader: &mut impl Read, magic: &[u8]) -> std::io::Result<Option<Vec<u8>>> {
    let mut first = [0_u8; 1];
    match reader.read(&mut first) { Ok(0) => return Ok(None), Ok(1) => {}, Ok(_) => unreachable!(), Err(error) => return Err(error) }
    let mut header = vec![0_u8; magic.len() + 4]; header[0] = first[0]; reader.read_exact(&mut header[1..])?;
    if &header[..magic.len()] != magic { return Err(invalid_data()); }
    let length = usize::try_from(u32::from_be_bytes(header[magic.len()..].try_into().map_err(|_| invalid_data())?)).map_err(|_| invalid_data())?;
    let mut payload = Vec::new(); payload.try_reserve_exact(length).map_err(|_| invalid_data())?; payload.resize(length, 0); reader.read_exact(&mut payload)?;
    let mut checksum = [0_u8; 32]; reader.read_exact(&mut checksum)?;
    let mut digest_input = header; digest_input.extend_from_slice(&payload);
    if peritus_codec::sha256(&digest_input).as_bytes() != &checksum { return Err(invalid_data()); }
    Ok(Some(payload))
}

fn encode_tree(writer: &mut Writer, tree: Option<ProcessTreeIdentity>) {
    match tree { None => writer.u8(0), Some(tree) => { writer.u8(1); writer.u32(tree.root_pid()); optional_u64(writer, tree.start_token()); optional_u32(writer, tree.process_group()); writer.boolean(tree.complete_containment()); } }
}
fn decode_tree(reader: &mut Reader<'_>) -> std::io::Result<Option<ProcessTreeIdentity>> { match reader.u8()? { 0 => Ok(None), 1 => Ok(Some(ProcessTreeIdentity::new(reader.u32()?, decode_optional_u64(reader)?, decode_optional_u32(reader)?, reader.boolean()?))), _ => Err(invalid_data()) } }
fn encode_native_recovery(
    writer: &mut Writer,
    recovery: Option<&NativeSessionRecovery>,
) -> std::io::Result<()> {
    let Some(recovery) = recovery else {
        writer.u8(0);
        return Ok(());
    };
    writer.u8(1);
    writer.u8(platform_tag(recovery.platform()));
    writer.raw(recovery.process_id().as_bytes());
    writer.u8(native_phase_tag(recovery.phase()));
    encode_tree(writer, recovery.tree_identity());
    optional_digest(writer, recovery.owner_operation_digest());
    optional_digest(writer, recovery.service_owner_digest());
    writer.boolean(recovery.custody_complete());
    writer.frame(recovery.record())
}
fn decode_native_recovery(
    reader: &mut Reader<'_>,
) -> std::io::Result<Option<NativeSessionRecovery>> {
    match reader.u8()? {
        0 => Ok(None),
        1 => NativeSessionRecovery::new(
            decode_platform(reader.u8()?)?,
            ProcessId::new(reader.array()?).map_err(|_| invalid_data())?,
            decode_native_phase(reader.u8()?)?,
            decode_tree(reader)?,
            decode_optional_digest(reader)?,
            decode_optional_digest(reader)?,
            reader.boolean()?,
            reader.frame()?.to_vec(),
        )
        .map(Some)
        .map_err(|_| invalid_data()),
        _ => Err(invalid_data()),
    }
}
fn optional_u64(writer: &mut Writer, value: Option<u64>) { match value { None => writer.u8(0), Some(value) => { writer.u8(1); writer.u64(value); } } }
fn optional_u32(writer: &mut Writer, value: Option<u32>) { match value { None => writer.u8(0), Some(value) => { writer.u8(1); writer.u32(value); } } }
fn decode_optional_u64(reader: &mut Reader<'_>) -> std::io::Result<Option<u64>> { match reader.u8()? { 0 => Ok(None), 1 => reader.u64().map(Some), _ => Err(invalid_data()) } }
fn decode_optional_u32(reader: &mut Reader<'_>) -> std::io::Result<Option<u32>> { match reader.u8()? { 0 => Ok(None), 1 => reader.u32().map(Some), _ => Err(invalid_data()) } }
fn optional_digest(writer: &mut Writer, value: Option<Sha256Digest>) { match value { None => writer.u8(0), Some(value) => { writer.u8(1); writer.digest(value); } } }
fn decode_optional_digest(reader: &mut Reader<'_>) -> std::io::Result<Option<Sha256Digest>> { match reader.u8()? { 0 => Ok(None), 1 => reader.digest().map(Some), _ => Err(invalid_data()) } }

struct Writer { bytes: Vec<u8> }
impl Writer {
    const fn new() -> Self { Self { bytes: Vec::new() } }
    fn finish(self) -> Vec<u8> { self.bytes }
    fn u8(&mut self, value: u8) { self.bytes.push(value); }
    fn boolean(&mut self, value: bool) { self.u8(u8::from(value)); }
    fn u16(&mut self, value: u16) { self.bytes.extend_from_slice(&value.to_be_bytes()); }
    fn u32(&mut self, value: u32) { self.bytes.extend_from_slice(&value.to_be_bytes()); }
    fn u64(&mut self, value: u64) { self.bytes.extend_from_slice(&value.to_be_bytes()); }
    fn digest(&mut self, value: Sha256Digest) { self.bytes.extend_from_slice(value.as_bytes()); }
    fn raw(&mut self, value: &[u8]) { self.bytes.extend_from_slice(value); }
    fn key(&mut self, key: RetainedProcessKey) { self.bytes.extend_from_slice(key.process_id().as_bytes()); self.bytes.extend_from_slice(&key.nonce().as_bytes()); self.digest(key.operation_digest()); }
    fn frame(&mut self, value: &[u8]) -> std::io::Result<()> { self.u32(u32::try_from(value.len()).map_err(|_| invalid_data())?); self.bytes.try_reserve(value.len()).map_err(|_| invalid_data())?; self.bytes.extend_from_slice(value); Ok(()) }
}

struct Reader<'a> { bytes: &'a [u8], offset: usize }
impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self { Self { bytes, offset: 0 } }
    fn take(&mut self, length: usize) -> std::io::Result<&'a [u8]> { let end = self.offset.checked_add(length).ok_or_else(invalid_data)?; let value = self.bytes.get(self.offset..end).ok_or_else(invalid_data)?; self.offset = end; Ok(value) }
    fn array<const N: usize>(&mut self) -> std::io::Result<[u8; N]> { self.take(N)?.try_into().map_err(|_| invalid_data()) }
    fn u8(&mut self) -> std::io::Result<u8> { Ok(self.array::<1>()?[0]) }
    fn boolean(&mut self) -> std::io::Result<bool> { match self.u8()? { 0 => Ok(false), 1 => Ok(true), _ => Err(invalid_data()) } }
    fn u16(&mut self) -> std::io::Result<u16> { Ok(u16::from_be_bytes(self.array()?)) }
    fn u32(&mut self) -> std::io::Result<u32> { Ok(u32::from_be_bytes(self.array()?)) }
    fn u64(&mut self) -> std::io::Result<u64> { Ok(u64::from_be_bytes(self.array()?)) }
    fn digest(&mut self) -> std::io::Result<Sha256Digest> { Ok(Sha256Digest::new(self.array()?)) }
    fn key(&mut self) -> std::io::Result<RetainedProcessKey> { let process = ProcessId::new(self.array()?).map_err(|_| invalid_data())?; let nonce = RetainedOwnerNonce::from_bytes(self.array()?).map_err(|_| invalid_data())?; Ok(RetainedProcessKey::new(process, nonce, self.digest()?)) }
    fn frame(&mut self) -> std::io::Result<&'a [u8]> { let length = usize::try_from(self.u32()?).map_err(|_| invalid_data())?; self.take(length) }
    fn finish(&self) -> std::io::Result<()> { if self.offset == self.bytes.len() { Ok(()) } else { Err(invalid_data()) } }
}

const fn signal_tag(value: ProcessSignal) -> u8 { match value { ProcessSignal::Interrupt => 1, ProcessSignal::Terminate => 2 } }
fn decode_signal(value: u8) -> std::io::Result<ProcessSignal> { match value { 1 => Ok(ProcessSignal::Interrupt), 2 => Ok(ProcessSignal::Terminate), _ => Err(invalid_data()) } }
const fn stream_tag(value: OutputStream) -> u8 { match value { OutputStream::Stdout => 1, OutputStream::Stderr => 2, OutputStream::Terminal => 3 } }
fn decode_stream(value: u8) -> std::io::Result<OutputStream> { match value { 1 => Ok(OutputStream::Stdout), 2 => Ok(OutputStream::Stderr), 3 => Ok(OutputStream::Terminal), _ => Err(invalid_data()) } }
const fn platform_tag(value: NativePlatform) -> u8 { match value { NativePlatform::Linux => 1, NativePlatform::Macos => 2, NativePlatform::Windows => 3 } }
fn decode_platform(value: u8) -> std::io::Result<NativePlatform> { match value { 1 => Ok(NativePlatform::Linux), 2 => Ok(NativePlatform::Macos), 3 => Ok(NativePlatform::Windows), _ => Err(invalid_data()) } }
const fn native_phase_tag(value: NativeRecoveryPhase) -> u8 { match value { NativeRecoveryPhase::Prepared => 1, NativeRecoveryPhase::Active => 2, NativeRecoveryPhase::Cancelling => 3, NativeRecoveryPhase::Terminated => 4, NativeRecoveryPhase::Released => 5 } }
fn decode_native_phase(value: u8) -> std::io::Result<NativeRecoveryPhase> { match value { 1 => Ok(NativeRecoveryPhase::Prepared), 2 => Ok(NativeRecoveryPhase::Active), 3 => Ok(NativeRecoveryPhase::Cancelling), 4 => Ok(NativeRecoveryPhase::Terminated), 5 => Ok(NativeRecoveryPhase::Released), _ => Err(invalid_data()) } }
const fn reason_tag(value: CancellationReason) -> u8 { match value { CancellationReason::User => 1, CancellationReason::Deadline => 2, CancellationReason::OutputLimit => 3, CancellationReason::ResourceLimit => 4, CancellationReason::LeaseFence => 5, CancellationReason::SupervisorShutdown => 6, CancellationReason::BackendFailure => 7 } }
fn decode_reason(value: u8) -> std::io::Result<CancellationReason> { match value { 1 => Ok(CancellationReason::User), 2 => Ok(CancellationReason::Deadline), 3 => Ok(CancellationReason::OutputLimit), 4 => Ok(CancellationReason::ResourceLimit), 5 => Ok(CancellationReason::LeaseFence), 6 => Ok(CancellationReason::SupervisorShutdown), 7 => Ok(CancellationReason::BackendFailure), _ => Err(invalid_data()) } }

const fn code_tag(value: ErrorCode) -> u8 { match value { ErrorCode::InvalidInput=>1,ErrorCode::InvalidWorkingDirectory=>2,ErrorCode::InvalidEnvironment=>3,ErrorCode::PlanMismatch=>4,ErrorCode::Unsupported=>5,ErrorCode::AuthorizationMismatch=>6,ErrorCode::MissingDispatch=>7,ErrorCode::BudgetMismatch=>8,ErrorCode::LeaseMismatch=>9,ErrorCode::ReceiptReused=>10,ErrorCode::Persistence=>11,ErrorCode::Spawn=>12,ErrorCode::Pty=>13,ErrorCode::Input=>14,ErrorCode::Output=>15,ErrorCode::ProcessTree=>16,ErrorCode::ResourceLimit=>17,ErrorCode::Artifact=>18,ErrorCode::CorruptRecovery=>19,ErrorCode::Indeterminate=>20,ErrorCode::Supervisor=>21 } }
fn decode_code(v:u8)->std::io::Result<ErrorCode>{Ok(match v{1=>ErrorCode::InvalidInput,2=>ErrorCode::InvalidWorkingDirectory,3=>ErrorCode::InvalidEnvironment,4=>ErrorCode::PlanMismatch,5=>ErrorCode::Unsupported,6=>ErrorCode::AuthorizationMismatch,7=>ErrorCode::MissingDispatch,8=>ErrorCode::BudgetMismatch,9=>ErrorCode::LeaseMismatch,10=>ErrorCode::ReceiptReused,11=>ErrorCode::Persistence,12=>ErrorCode::Spawn,13=>ErrorCode::Pty,14=>ErrorCode::Input,15=>ErrorCode::Output,16=>ErrorCode::ProcessTree,17=>ErrorCode::ResourceLimit,18=>ErrorCode::Artifact,19=>ErrorCode::CorruptRecovery,20=>ErrorCode::Indeterminate,21=>ErrorCode::Supervisor,_=>return Err(invalid_data())})}
const fn operation_tag(v:ProcessOperation)->u8{match v{ProcessOperation::Validate=>1,ProcessOperation::OpenStore=>2,ProcessOperation::Authorize=>3,ProcessOperation::Persist=>4,ProcessOperation::Spawn=>5,ProcessOperation::Stream=>6,ProcessOperation::Control=>7,ProcessOperation::Wait=>8,ProcessOperation::PublishArtifact=>9,ProcessOperation::Reconcile=>10,ProcessOperation::InspectQuiescence=>11}}
fn decode_operation(v:u8)->std::io::Result<ProcessOperation>{Ok(match v{1=>ProcessOperation::Validate,2=>ProcessOperation::OpenStore,3=>ProcessOperation::Authorize,4=>ProcessOperation::Persist,5=>ProcessOperation::Spawn,6=>ProcessOperation::Stream,7=>ProcessOperation::Control,8=>ProcessOperation::Wait,9=>ProcessOperation::PublishArtifact,10=>ProcessOperation::Reconcile,11=>ProcessOperation::InspectQuiescence,_=>return Err(invalid_data())})}
const fn recovery_tag(v:RecoveryClass)->u8{match v{RecoveryClass::CorrectRequest=>1,RecoveryClass::Reauthorize=>2,RecoveryClass::SelectBackend=>3,RecoveryClass::RetryPreparation=>4,RecoveryClass::CancelAndReap=>5,RecoveryClass::ReopenAndReconcile=>6,RecoveryClass::Quarantine=>7,RecoveryClass::RetryPublication=>8,RecoveryClass::Terminal=>9}}
fn decode_recovery(v:u8)->std::io::Result<RecoveryClass>{Ok(match v{1=>RecoveryClass::CorrectRequest,2=>RecoveryClass::Reauthorize,3=>RecoveryClass::SelectBackend,4=>RecoveryClass::RetryPreparation,5=>RecoveryClass::CancelAndReap,6=>RecoveryClass::ReopenAndReconcile,7=>RecoveryClass::Quarantine,8=>RecoveryClass::RetryPublication,9=>RecoveryClass::Terminal,_=>return Err(invalid_data())})}
const fn control_tag(v:ControlRejection)->u8{match v{ControlRejection::Backpressure=>1,ControlRejection::InvalidRequest=>2,ControlRejection::AdmissionClosed=>3}}
fn decode_control(v:u8)->std::io::Result<ControlRejection>{match v{1=>Ok(ControlRejection::Backpressure),2=>Ok(ControlRejection::InvalidRequest),3=>Ok(ControlRejection::AdmissionClosed),_=>Err(invalid_data())}}

fn invalid_data() -> std::io::Error { std::io::Error::new(std::io::ErrorKind::InvalidData, "retained owner protocol frame is invalid") }
