//! Canonical replay-capsule encoding and bounded decoding.

use peritus_codec::{CodecLimits, decode_message, encode_message, sha256};
use peritus_protocol::{CommandEnvelopeDto, KernelCommandDto};
use peritus_types::{ProjectId, SessionId, Sha256Digest};

use super::{
    CapsuleKind, KernelInputReference, KernelReplayCapsule, MAX_INPUT_REFERENCES, corrupt,
    validate_inputs,
};
use crate::{ExactFrame, JournalError, JournalErrorKind, domain::encoding};

const VALUE_KIND: u16 = 5;

pub(super) fn encode_capsule(capsule: &KernelReplayCapsule) -> Vec<u8> {
    let mut payload = Vec::with_capacity(256);
    encoding::u8_value(
        &mut payload,
        match capsule.kind {
            CapsuleKind::Genesis => 1,
            CapsuleKind::Transition => 2,
        },
    );
    payload.extend_from_slice(capsule.project_id.as_bytes());
    payload.extend_from_slice(capsule.session_id.as_bytes());
    encoding::bytes_value(&mut payload, capsule.envelope_frame.bytes());
    match &capsule.command_frame {
        Some(frame) => {
            encoding::u8_value(&mut payload, 1);
            encoding::bytes_value(&mut payload, frame.bytes());
        }
        None => encoding::u8_value(&mut payload, 0),
    }
    encoding::u64_value(&mut payload, capsule.inputs.len() as u64);
    for input in &capsule.inputs {
        encoding::u16_value(&mut payload, input.kind);
        encoding::bytes_value(&mut payload, &input.identity);
        encoding::digest(&mut payload, input.digest);
    }
    encoding::digest(&mut payload, capsule.successor_digest);
    encoding::value(VALUE_KIND, &payload)
}

pub(super) fn decode_capsule(value: &[u8]) -> Result<KernelReplayCapsule, JournalError> {
    let payload = encoding::payload(value, VALUE_KIND)
        .ok_or_else(|| corrupt("kernel replay capsule header is malformed"))?;
    let mut cursor = Cursor::new(payload);
    let kind = match cursor.u8()? {
        1 => CapsuleKind::Genesis,
        2 => CapsuleKind::Transition,
        _ => return Err(corrupt("kernel replay capsule kind is unknown")),
    };
    let project_id = ProjectId::new(cursor.array()?)
        .map_err(|_| corrupt("kernel capsule project identity is invalid"))?;
    let session_id = SessionId::new(cursor.array()?)
        .map_err(|_| corrupt("kernel capsule session identity is invalid"))?;
    let envelope_frame = ExactFrame::new(cursor.bytes()?.to_vec())
        .map_err(|_| corrupt("kernel capsule envelope frame is invalid"))?;
    let envelope =
        decode_message::<CommandEnvelopeDto>(envelope_frame.bytes(), CodecLimits::PRODUCTION)
            .map_err(|_| corrupt("kernel capsule envelope cannot be decoded"))?
            .into_domain();
    let command_frame = match cursor.u8()? {
        0 => None,
        1 => Some(
            ExactFrame::new(cursor.bytes()?.to_vec())
                .map_err(|_| corrupt("kernel capsule command frame is invalid"))?,
        ),
        _ => return Err(corrupt("kernel capsule command option is malformed")),
    };
    let command = command_frame
        .as_ref()
        .map(|frame| {
            decode_message::<KernelCommandDto>(frame.bytes(), CodecLimits::PRODUCTION)
                .map(KernelCommandDto::into_domain)
                .map_err(|_| corrupt("kernel capsule command cannot be decoded"))
        })
        .transpose()?;
    let count = usize::try_from(cursor.u64()?)
        .map_err(|_| corrupt("kernel capsule input count overflows"))?;
    if count > MAX_INPUT_REFERENCES {
        return Err(corrupt("kernel capsule has too many input references"));
    }
    let mut inputs = Vec::with_capacity(count);
    for _ in 0..count {
        inputs.push(
            KernelInputReference::new(cursor.u16()?, cursor.bytes()?.to_vec(), cursor.digest()?)
                .map_err(|_| corrupt("kernel capsule input reference is invalid"))?,
        );
    }
    validate_inputs(&inputs).map_err(|_| corrupt("kernel capsule input order is invalid"))?;
    let successor_digest = cursor.digest()?;
    cursor.finish()?;
    if matches!(kind, CapsuleKind::Genesis) != command.is_none() {
        return Err(corrupt("kernel capsule kind and command presence disagree"));
    }
    Ok(KernelReplayCapsule {
        kind,
        project_id,
        session_id,
        envelope,
        envelope_frame,
        command,
        command_frame,
        inputs,
        successor_digest,
    })
}

pub(super) fn revision_digest(revision: peritus_types::RevisionTuple) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(112);
    encoding::revision(&mut bytes, revision);
    sha256(&bytes)
}

pub(super) fn exact<T: peritus_codec::CanonicalEncode>(
    value: &T,
    operation: &'static str,
) -> Result<ExactFrame, JournalError> {
    let bytes = encode_message(value, CodecLimits::PRODUCTION).map_err(|_| {
        JournalError::new(JournalErrorKind::InvalidInput, operation, "canonical B3 encoding failed")
    })?;
    ExactFrame::new(bytes)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], JournalError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| corrupt("kernel capsule offset overflowed"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| corrupt("kernel capsule is truncated"))?;
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, JournalError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, JournalError> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().map_err(|_| corrupt("kernel u16 is malformed"))?,
        ))
    }

    fn u64(&mut self) -> Result<u64, JournalError> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| corrupt("kernel u64 is malformed"))?,
        ))
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], JournalError> {
        self.take(N)?.try_into().map_err(|_| corrupt("kernel fixed-width field is malformed"))
    }

    fn bytes(&mut self) -> Result<&'a [u8], JournalError> {
        let count =
            usize::try_from(self.u64()?).map_err(|_| corrupt("kernel byte length overflows"))?;
        self.take(count)
    }

    fn digest(&mut self) -> Result<Sha256Digest, JournalError> {
        Ok(Sha256Digest::new(self.array()?))
    }

    const fn finish(self) -> Result<(), JournalError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(corrupt("kernel capsule has trailing bytes"))
        }
    }
}
