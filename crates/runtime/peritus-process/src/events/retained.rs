//! Canonical retained-owner transport encoding for bounded process events.

use peritus_types::{ProcessId, Sha256Digest};
use sha2::{Digest as _, Sha256};

use crate::{
    CancellationReason, ErrorCode, OutputStream, ProcessError, ProcessOperation, ProcessSignal,
    RecoveryClass, TerminalSize,
};

use super::{ProcessCursor, ProcessEvent, ProcessEventKind, ProcessEventLoss};

const MAGIC: &[u8] = b"PERITUS-PROCESS-EVENT-OWNER-V1\0";

impl ProcessEvent {
    /// Encodes this bounded event for the authenticated retained-owner transport.
    ///
    /// # Errors
    /// Returns a typed failure when event data exceeds the canonical physical frame.
    pub fn encode_retained_owner(&self) -> Result<Vec<u8>, ProcessError> {
        let data_len = u32::try_from(self.data.len()).map_err(|_| invalid())?;
        let mut bytes = Vec::with_capacity(MAGIC.len() + self.data.len() + 192);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(self.process_id.as_bytes());
        bytes.extend_from_slice(self.plan_digest.as_bytes());
        bytes.extend_from_slice(&self.sequence.to_be_bytes());
        optional_u64(&mut bytes, self.stream_offset);
        optional_offsets(&mut bytes, self.stream_offsets_before);
        optional_offsets(&mut bytes, self.stream_offsets_after);
        match self.loss {
            None => bytes.push(0),
            Some(loss) => {
                bytes.push(1);
                bytes.extend_from_slice(&loss.cursor_sequence.to_be_bytes());
                bytes.extend_from_slice(&loss.through_sequence.to_be_bytes());
                match loss.output_offsets {
                    None => bytes.push(0),
                    Some(offsets) => {
                        bytes.push(1);
                        for range in offsets {
                            bytes.extend_from_slice(&range[0].to_be_bytes());
                            bytes.extend_from_slice(&range[1].to_be_bytes());
                        }
                    }
                }
            }
        }
        encode_kind(&mut bytes, &self.kind);
        bytes.extend_from_slice(&data_len.to_be_bytes());
        bytes.extend_from_slice(&self.data);
        let checksum: [u8; 32] = Sha256::digest(&bytes).into();
        bytes.extend_from_slice(&checksum);
        Ok(bytes)
    }

    /// Decodes one canonical event returned by the authenticated retained owner.
    ///
    /// # Errors
    /// Rejects malformed, noncanonical, or checksum-divergent bytes.
    pub fn decode_retained_owner(bytes: &[u8]) -> Result<Self, ProcessError> {
        if !bytes.starts_with(MAGIC) || bytes.len() < MAGIC.len() + 32 {
            return Err(corrupt());
        }
        let checksum_at = bytes.len() - 32;
        let checksum: [u8; 32] = Sha256::digest(&bytes[..checksum_at]).into();
        if bytes[checksum_at..] != checksum {
            return Err(corrupt());
        }
        let mut reader = Reader { bytes: &bytes[MAGIC.len()..checksum_at], offset: 0 };
        let process_id = ProcessId::new(reader.array()?).map_err(|_| corrupt())?;
        let plan_digest = Sha256Digest::new(reader.array()?);
        let sequence = reader.u64()?;
        if sequence == 0 {
            return Err(corrupt());
        }
        let stream_offset = reader.optional_u64()?;
        let stream_offsets_before = reader.optional_offsets()?;
        let stream_offsets_after = reader.optional_offsets()?;
        let loss = match reader.u8()? {
            0 => None,
            1 => {
                let cursor_sequence = reader.u64()?;
                let through_sequence = reader.u64()?;
                if through_sequence < cursor_sequence || through_sequence >= sequence {
                    return Err(corrupt());
                }
                let output_offsets = match reader.u8()? {
                    0 => None,
                    1 => {
                        let mut offsets = [[0_u64; 2]; 3];
                        for range in &mut offsets {
                            *range = [reader.u64()?, reader.u64()?];
                            if range[1] < range[0] {
                                return Err(corrupt());
                            }
                        }
                        Some(offsets)
                    }
                    _ => return Err(corrupt()),
                };
                Some(ProcessEventLoss::new(
                    cursor_sequence,
                    through_sequence,
                    output_offsets,
                ))
            }
            _ => return Err(corrupt()),
        };
        let kind = decode_kind(&mut reader)?;
        let data_length = usize::try_from(reader.u32()?).map_err(|_| corrupt())?;
        let data = reader.bytes(data_length)?.to_vec();
        if !reader.is_empty() {
            return Err(corrupt());
        }
        let event = Self {
            process_id,
            plan_digest,
            sequence,
            stream_offset,
            stream_offsets_before,
            stream_offsets_after,
            loss,
            kind,
            data,
        };
        if event.encode_retained_owner()? != bytes {
            return Err(corrupt());
        }
        Ok(event)
    }
}

impl ProcessCursor {
    /// Encodes the exact retained-owner event frontier, including known stream offsets.
    #[must_use]
    pub fn encode_retained_owner(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(33);
        bytes.extend_from_slice(&self.after_sequence.to_be_bytes());
        optional_offsets(&mut bytes, self.stream_offsets);
        bytes
    }

    /// Decodes one exact retained-owner event frontier.
    ///
    /// # Errors
    /// Rejects malformed or trailing cursor bytes.
    pub fn decode_retained_owner(bytes: &[u8]) -> Result<Self, ProcessError> {
        let mut reader = Reader { bytes, offset: 0 };
        let after_sequence = reader.u64()?;
        let stream_offsets = reader.optional_offsets()?;
        if !reader.is_empty() || (after_sequence == 0 && stream_offsets != Some([0; 3])) {
            return Err(corrupt());
        }
        Ok(Self { after_sequence, stream_offsets })
    }
}

fn encode_kind(bytes: &mut Vec<u8>, kind: &ProcessEventKind) {
    match kind {
        ProcessEventKind::IntentPersisted => bytes.push(1),
        ProcessEventKind::SpawnAttempt => bytes.push(2),
        ProcessEventKind::Started { root_pid } => { bytes.push(3); bytes.extend_from_slice(&root_pid.to_be_bytes()); }
        ProcessEventKind::Output(stream) => { bytes.push(4); bytes.push(stream_tag(*stream)); }
        ProcessEventKind::StdinAccepted { bytes: count } => { bytes.push(5); bytes.extend_from_slice(&count.to_be_bytes()); }
        ProcessEventKind::StdinClosed => bytes.push(6),
        ProcessEventKind::Resized(size) => {
            bytes.push(7);
            for value in [size.rows(), size.columns(), size.pixel_width(), size.pixel_height()] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
        ProcessEventKind::Signalled(signal) => { bytes.push(8); bytes.push(signal_tag(*signal)); }
        ProcessEventKind::Cancellation(reason) => { bytes.push(9); bytes.push(reason_tag(*reason)); }
        ProcessEventKind::Escalated => bytes.push(10),
        ProcessEventKind::ResourceSample => bytes.push(11),
        ProcessEventKind::ResourceLimit => bytes.push(12),
        ProcessEventKind::SandboxObservation => bytes.push(13),
        ProcessEventKind::OsExit => bytes.push(14),
        ProcessEventKind::TreeQuiescent => bytes.push(15),
        ProcessEventKind::OutputClosed => bytes.push(16),
        ProcessEventKind::ArtifactPublished => bytes.push(17),
        ProcessEventKind::TerminalPublished => bytes.push(18),
    }
}

fn decode_kind(reader: &mut Reader<'_>) -> Result<ProcessEventKind, ProcessError> {
    Ok(match reader.u8()? {
        1 => ProcessEventKind::IntentPersisted,
        2 => ProcessEventKind::SpawnAttempt,
        3 => ProcessEventKind::Started { root_pid: reader.u32()? },
        4 => ProcessEventKind::Output(decode_stream(reader.u8()?)?),
        5 => ProcessEventKind::StdinAccepted { bytes: reader.u64()? },
        6 => ProcessEventKind::StdinClosed,
        7 => ProcessEventKind::Resized(
            TerminalSize::new(reader.u16()?, reader.u16()?, reader.u16()?, reader.u16()?)
                .map_err(|_| corrupt())?,
        ),
        8 => ProcessEventKind::Signalled(decode_signal(reader.u8()?)?),
        9 => ProcessEventKind::Cancellation(decode_reason(reader.u8()?)?),
        10 => ProcessEventKind::Escalated,
        11 => ProcessEventKind::ResourceSample,
        12 => ProcessEventKind::ResourceLimit,
        13 => ProcessEventKind::SandboxObservation,
        14 => ProcessEventKind::OsExit,
        15 => ProcessEventKind::TreeQuiescent,
        16 => ProcessEventKind::OutputClosed,
        17 => ProcessEventKind::ArtifactPublished,
        18 => ProcessEventKind::TerminalPublished,
        _ => return Err(corrupt()),
    })
}

fn optional_u64(bytes: &mut Vec<u8>, value: Option<u64>) {
    match value { None => bytes.push(0), Some(value) => { bytes.push(1); bytes.extend_from_slice(&value.to_be_bytes()); } }
}

fn optional_offsets(bytes: &mut Vec<u8>, value: Option<[u64; 3]>) {
    match value {
        None => bytes.push(0),
        Some(value) => { bytes.push(1); for offset in value { bytes.extend_from_slice(&offset.to_be_bytes()); } }
    }
}

const fn stream_tag(value: OutputStream) -> u8 { match value { OutputStream::Stdout => 1, OutputStream::Stderr => 2, OutputStream::Terminal => 3 } }
fn decode_stream(value: u8) -> Result<OutputStream, ProcessError> { match value { 1 => Ok(OutputStream::Stdout), 2 => Ok(OutputStream::Stderr), 3 => Ok(OutputStream::Terminal), _ => Err(corrupt()) } }
const fn signal_tag(value: ProcessSignal) -> u8 { match value { ProcessSignal::Interrupt => 1, ProcessSignal::Terminate => 2 } }
fn decode_signal(value: u8) -> Result<ProcessSignal, ProcessError> { match value { 1 => Ok(ProcessSignal::Interrupt), 2 => Ok(ProcessSignal::Terminate), _ => Err(corrupt()) } }
const fn reason_tag(value: CancellationReason) -> u8 { match value { CancellationReason::User => 1, CancellationReason::Deadline => 2, CancellationReason::OutputLimit => 3, CancellationReason::ResourceLimit => 4, CancellationReason::LeaseFence => 5, CancellationReason::SupervisorShutdown => 6, CancellationReason::BackendFailure => 7 } }
fn decode_reason(value: u8) -> Result<CancellationReason, ProcessError> { match value { 1 => Ok(CancellationReason::User), 2 => Ok(CancellationReason::Deadline), 3 => Ok(CancellationReason::OutputLimit), 4 => Ok(CancellationReason::ResourceLimit), 5 => Ok(CancellationReason::LeaseFence), 6 => Ok(CancellationReason::SupervisorShutdown), 7 => Ok(CancellationReason::BackendFailure), _ => Err(corrupt()) } }

struct Reader<'a> { bytes: &'a [u8], offset: usize }
impl<'a> Reader<'a> {
    fn bytes(&mut self, length: usize) -> Result<&'a [u8], ProcessError> { let end = self.offset.checked_add(length).ok_or_else(corrupt)?; let value = self.bytes.get(self.offset..end).ok_or_else(corrupt)?; self.offset = end; Ok(value) }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], ProcessError> { let mut value = [0; N]; value.copy_from_slice(self.bytes(N)?); Ok(value) }
    fn u8(&mut self) -> Result<u8, ProcessError> { Ok(self.array::<1>()?[0]) }
    fn u16(&mut self) -> Result<u16, ProcessError> { Ok(u16::from_be_bytes(self.array()?)) }
    fn u32(&mut self) -> Result<u32, ProcessError> { Ok(u32::from_be_bytes(self.array()?)) }
    fn u64(&mut self) -> Result<u64, ProcessError> { Ok(u64::from_be_bytes(self.array()?)) }
    fn optional_u64(&mut self) -> Result<Option<u64>, ProcessError> { match self.u8()? { 0 => Ok(None), 1 => self.u64().map(Some), _ => Err(corrupt()) } }
    fn optional_offsets(&mut self) -> Result<Option<[u64; 3]>, ProcessError> { match self.u8()? { 0 => Ok(None), 1 => Ok(Some([self.u64()?, self.u64()?, self.u64()?])), _ => Err(corrupt()) } }
    fn is_empty(&self) -> bool { self.offset == self.bytes.len() }
}

const fn invalid() -> ProcessError { ProcessError::new(ErrorCode::InvalidInput, ProcessOperation::Stream, RecoveryClass::CorrectRequest, "retained owner event exceeds its canonical frame") }
const fn corrupt() -> ProcessError { ProcessError::new(ErrorCode::CorruptRecovery, ProcessOperation::Reconcile, RecoveryClass::Quarantine, "retained owner event frame is corrupt") }
