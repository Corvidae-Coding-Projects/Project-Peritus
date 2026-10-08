//! Canonical immutable pages for exact native observation history.

use peritus_sandbox::{
    CapabilityDomain, EnforcementObservation, ObservationDisposition, ObservationKind,
};
use peritus_types::{ProcessId, Sha256Digest};
use sha2::{Digest as _, Sha256};

use super::{
    NativeObservationFrontier, NativeObservationPhase, native_observation_prefix_digest,
    observation_shape_valid,
};
use crate::{
    ErrorCode, ProcessError, ProcessOperation, RecoveryClass,
    native::{NATIVE_OBSERVATION_PAGE_RECORDS, native_mismatch},
};

const MAGIC: &[u8] = b"PERITUS-NATIVE-OBSERVATION-PAGE-V1\0";
pub(crate) const MAX_PAGE_BYTES: usize = 24 * 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StoredObservationPage {
    process_id: ProcessId,
    ordinal: u64,
    predecessor: Option<Sha256Digest>,
    plan_digest: Sha256Digest,
    backend_digest: Sha256Digest,
    producer_binding_digest: Sha256Digest,
    previous_producer_prefix_digest: Option<Sha256Digest>,
    resulting_producer_prefix_digest: Sha256Digest,
    first_sequence: u64,
    initial_phase: NativeObservationPhase,
    resulting_phase: NativeObservationPhase,
    resulting_next_sequence: u64,
    resulting_total_count: u64,
    observations: Vec<EnforcementObservation>,
}

impl StoredObservationPage {
    pub(crate) fn from_records(
        process_id: ProcessId,
        plan_digest: Sha256Digest,
        backend_digest: Sha256Digest,
        producer_binding_digest: Sha256Digest,
        prior: Option<NativeObservationFrontier>,
        observations: Vec<EnforcementObservation>,
    ) -> Result<(Self, NativeObservationFrontier), ProcessError> {
        if observations.is_empty() || observations.len() > NATIVE_OBSERVATION_PAGE_RECORDS {
            return Err(native_mismatch("native observation page record count is invalid"));
        }
        let (
            ordinal,
            predecessor,
            previous_producer_prefix_digest,
            first_sequence,
            initial_phase,
            total_count,
        ) = match prior {
            Some(prior) => {
                if prior.plan_digest != plan_digest
                    || prior.backend_digest != backend_digest
                    || prior.producer_binding_digest != producer_binding_digest
                {
                    return Err(native_mismatch("native observation frontier binding changed"));
                }
                (
                    prior.page_count,
                    Some(prior.last_page_digest),
                    Some(prior.producer_prefix_digest),
                    prior.next_sequence,
                    prior.phase,
                    prior.total_count,
                )
            }
            None => (0, None, None, 1, NativeObservationPhase::AwaitingPrepared, 0),
        };
        let mut phase = initial_phase;
        let mut expected = first_sequence;
        for observation in observations.iter().copied() {
            if observation.sequence() != expected
                || observation.plan_digest() != plan_digest
                || observation.backend_digest() != backend_digest
                || !observation_shape_valid(observation)
            {
                return Err(native_mismatch("native observation page binding or sequence is invalid"));
            }
            phase = phase.advance(observation.kind())?;
            expected = expected
                .checked_add(1)
                .ok_or_else(|| native_mismatch("native observation sequence overflowed"))?;
        }
        let count = u64::try_from(observations.len())
            .map_err(|_| native_mismatch("native observation page count is not representable"))?;
        let resulting_total_count = total_count
            .checked_add(count)
            .ok_or_else(|| native_mismatch("native observation count overflowed"))?;
        let resulting_producer_prefix_digest = native_observation_prefix_digest(
            producer_binding_digest,
            previous_producer_prefix_digest,
            &observations,
        );
        let page = Self {
            process_id,
            ordinal,
            predecessor,
            plan_digest,
            backend_digest,
            producer_binding_digest,
            previous_producer_prefix_digest,
            resulting_producer_prefix_digest,
            first_sequence,
            initial_phase,
            resulting_phase: phase,
            resulting_next_sequence: expected,
            resulting_total_count,
            observations,
        };
        let page_digest = page.digest()?;
        let page_count = ordinal
            .checked_add(1)
            .ok_or_else(|| native_mismatch("native observation page ordinal overflowed"))?;
        Ok((
            page,
            NativeObservationFrontier {
                plan_digest,
                backend_digest,
                producer_binding_digest,
                producer_prefix_digest: resulting_producer_prefix_digest,
                page_count,
                total_count: resulting_total_count,
                next_sequence: expected,
                last_page_digest: page_digest,
                phase,
            },
        ))
    }

    pub(crate) const fn process_id(&self) -> ProcessId {
        self.process_id
    }

    pub(crate) const fn ordinal(&self) -> u64 {
        self.ordinal
    }

    pub(crate) fn digest(&self) -> Result<Sha256Digest, ProcessError> {
        let bytes = self.encode()?;
        let start = bytes.len() - Sha256Digest::LENGTH;
        let digest: [u8; 32] = bytes[start..]
            .try_into()
            .map_err(|_| corrupt("native observation page checksum is malformed"))?;
        Ok(Sha256Digest::new(digest))
    }

    pub(crate) fn frontier(&self) -> Result<NativeObservationFrontier, ProcessError> {
        Ok(NativeObservationFrontier {
            plan_digest: self.plan_digest,
            backend_digest: self.backend_digest,
            producer_binding_digest: self.producer_binding_digest,
            producer_prefix_digest: self.resulting_producer_prefix_digest,
            page_count: self
                .ordinal
                .checked_add(1)
                .ok_or_else(|| corrupt("native observation page ordinal overflowed"))?,
            total_count: self.resulting_total_count,
            next_sequence: self.resulting_next_sequence,
            last_page_digest: self.digest()?,
            phase: self.resulting_phase,
        })
    }

    pub(crate) fn follows(
        &self,
        prior: Option<NativeObservationFrontier>,
    ) -> Result<(), ProcessError> {
        let expected = match prior {
            Some(prior) => {
                let count = u64::try_from(self.observations.len())
                    .map_err(|_| corrupt("native observation page count is not representable"))?;
                let expected_total = prior
                    .total_count
                    .checked_add(count)
                    .ok_or_else(|| corrupt("native observation count overflowed"))?;
                if self.ordinal != prior.page_count
                    || self.predecessor != Some(prior.last_page_digest)
                    || self.plan_digest != prior.plan_digest
                    || self.backend_digest != prior.backend_digest
                    || self.producer_binding_digest != prior.producer_binding_digest
                    || self.previous_producer_prefix_digest
                        != Some(prior.producer_prefix_digest)
                    || self.first_sequence != prior.next_sequence
                    || self.initial_phase != prior.phase
                    || self.resulting_total_count != expected_total
                {
                    return Err(corrupt("native observation page does not follow its frontier"));
                }
                prior.next_sequence
            }
            None => {
                let count = u64::try_from(self.observations.len())
                    .map_err(|_| corrupt("native observation page count is not representable"))?;
                if self.ordinal != 0
                    || self.predecessor.is_some()
                    || self.previous_producer_prefix_digest.is_some()
                    || self.first_sequence != 1
                    || self.initial_phase != NativeObservationPhase::AwaitingPrepared
                    || self.resulting_total_count != count
                {
                    return Err(corrupt("first native observation page is not canonical"));
                }
                1
            }
        };
        if self.observations.first().map(|value| value.sequence()) != Some(expected) {
            return Err(corrupt("native observation page first record differs from its frontier"));
        }
        Ok(())
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, ProcessError> {
        let mut bytes = Vec::with_capacity(512 + self.observations.len() * 76);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(self.process_id.as_bytes());
        u64_value(&mut bytes, self.ordinal);
        optional_digest(&mut bytes, self.predecessor);
        digest(&mut bytes, self.plan_digest);
        digest(&mut bytes, self.backend_digest);
        digest(&mut bytes, self.producer_binding_digest);
        optional_digest(&mut bytes, self.previous_producer_prefix_digest);
        digest(&mut bytes, self.resulting_producer_prefix_digest);
        u64_value(&mut bytes, self.first_sequence);
        bytes.push(phase_tag(self.initial_phase));
        bytes.push(phase_tag(self.resulting_phase));
        u64_value(&mut bytes, self.resulting_next_sequence);
        u64_value(&mut bytes, self.resulting_total_count);
        let count = u16::try_from(self.observations.len())
            .map_err(|_| native_mismatch("native observation page count is not representable"))?;
        bytes.extend_from_slice(&count.to_be_bytes());
        for observation in &self.observations {
            encode_observation(&mut bytes, *observation);
        }
        if bytes.len() + Sha256Digest::LENGTH > MAX_PAGE_BYTES {
            return Err(native_mismatch("native observation page exceeds its canonical bound"));
        }
        let checksum: [u8; 32] = Sha256::digest(&bytes).into();
        bytes.extend_from_slice(&checksum);
        Ok(bytes)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, ProcessError> {
        if bytes.len() < MAGIC.len() + Sha256Digest::LENGTH
            || bytes.len() > MAX_PAGE_BYTES
            || !bytes.starts_with(MAGIC)
        {
            return Err(corrupt("native observation page has invalid framing"));
        }
        let payload_end = bytes.len() - Sha256Digest::LENGTH;
        let expected: [u8; 32] = Sha256::digest(&bytes[..payload_end]).into();
        if bytes[payload_end..] != expected {
            return Err(corrupt("native observation page checksum differs"));
        }
        let mut reader = Reader::new(&bytes[MAGIC.len()..payload_end]);
        let process_id = ProcessId::new(reader.array()?)
            .map_err(|_| corrupt("native observation page contains a zero process id"))?;
        let ordinal = reader.u64()?;
        let predecessor = reader.optional_digest()?;
        let plan_digest = reader.digest()?;
        let backend_digest = reader.digest()?;
        let producer_binding_digest = reader.digest()?;
        let previous_producer_prefix_digest = reader.optional_digest()?;
        let resulting_producer_prefix_digest = reader.digest()?;
        let first_sequence = reader.u64()?;
        let initial_phase = decode_phase(reader.u8()?)?;
        let resulting_phase = decode_phase(reader.u8()?)?;
        let resulting_next_sequence = reader.u64()?;
        let resulting_total_count = reader.u64()?;
        let count = usize::from(reader.u16()?);
        if count == 0 || count > NATIVE_OBSERVATION_PAGE_RECORDS {
            return Err(corrupt("native observation page record count is invalid"));
        }
        let mut observations = Vec::with_capacity(count);
        for _ in 0..count {
            observations.push(decode_observation(&mut reader)?);
        }
        if !reader.is_empty() {
            return Err(corrupt("native observation page contains trailing bytes"));
        }
        let page = Self {
            process_id,
            ordinal,
            predecessor,
            plan_digest,
            backend_digest,
            producer_binding_digest,
            previous_producer_prefix_digest,
            resulting_producer_prefix_digest,
            first_sequence,
            initial_phase,
            resulting_phase,
            resulting_next_sequence,
            resulting_total_count,
            observations,
        };
        let prior = if ordinal == 0 {
            None
        } else {
            Some(NativeObservationFrontier {
                plan_digest,
                backend_digest,
                producer_binding_digest,
                producer_prefix_digest: previous_producer_prefix_digest
                    .ok_or_else(|| corrupt("native observation producer prefix is absent"))?,
                page_count: ordinal,
                total_count: first_sequence
                    .checked_sub(1)
                    .ok_or_else(|| corrupt("native observation page sequence is zero"))?,
                next_sequence: first_sequence,
                last_page_digest: predecessor
                    .ok_or_else(|| corrupt("native observation page predecessor is absent"))?,
                phase: initial_phase,
            })
        };
        page.follows(prior)?;
        let mut phase = initial_phase;
        let mut sequence = first_sequence;
        for observation in page.observations.iter().copied() {
            if observation.sequence() != sequence
                || observation.plan_digest() != plan_digest
                || observation.backend_digest() != backend_digest
                || !observation_shape_valid(observation)
            {
                return Err(corrupt("native observation page binding or sequence is invalid"));
            }
            phase = phase
                .advance(observation.kind())
                .map_err(|_| corrupt("native observation page lifecycle is invalid"))?;
            sequence = sequence
                .checked_add(1)
                .ok_or_else(|| corrupt("native observation page sequence overflowed"))?;
        }
        if phase != resulting_phase || sequence != resulting_next_sequence {
            return Err(corrupt("native observation page resulting cursor is invalid"));
        }
        let expected_prefix = native_observation_prefix_digest(
            producer_binding_digest,
            previous_producer_prefix_digest,
            &page.observations,
        );
        if resulting_producer_prefix_digest != expected_prefix {
            return Err(corrupt("native observation producer prefix digest differs"));
        }
        if page.encode()? != bytes {
            return Err(corrupt("native observation page encoding is noncanonical"));
        }
        Ok(page)
    }
}

fn encode_observation(bytes: &mut Vec<u8>, value: EnforcementObservation) {
    u64_value(bytes, value.sequence());
    digest(bytes, value.plan_digest());
    digest(bytes, value.backend_digest());
    bytes.push(kind_tag(value.kind()));
    bytes.push(domain_tag(value.domain()));
    bytes.push(disposition_tag(value.disposition()));
}

fn decode_observation(reader: &mut Reader<'_>) -> Result<EnforcementObservation, ProcessError> {
    Ok(EnforcementObservation::new(
        reader.u64()?,
        reader.digest()?,
        reader.digest()?,
        decode_kind(reader.u8()?)?,
        decode_domain(reader.u8()?)?,
        decode_disposition(reader.u8()?)?,
    ))
}

const fn phase_tag(value: NativeObservationPhase) -> u8 {
    match value {
        NativeObservationPhase::AwaitingPrepared => 0,
        NativeObservationPhase::Prepared => 1,
        NativeObservationPhase::Activated => 2,
        NativeObservationPhase::Cancelling => 3,
        NativeObservationPhase::Terminated => 4,
        NativeObservationPhase::ReleasedFromPrepared => 5,
        NativeObservationPhase::ReleasedFromTerminated => 6,
    }
}

const fn decode_phase(value: u8) -> Result<NativeObservationPhase, ProcessError> {
    match value {
        0 => Ok(NativeObservationPhase::AwaitingPrepared),
        1 => Ok(NativeObservationPhase::Prepared),
        2 => Ok(NativeObservationPhase::Activated),
        3 => Ok(NativeObservationPhase::Cancelling),
        4 => Ok(NativeObservationPhase::Terminated),
        5 => Ok(NativeObservationPhase::ReleasedFromPrepared),
        6 => Ok(NativeObservationPhase::ReleasedFromTerminated),
        _ => Err(corrupt("native observation page has an unknown lifecycle phase")),
    }
}

const fn kind_tag(value: ObservationKind) -> u8 {
    match value {
        ObservationKind::Prepared => 1,
        ObservationKind::Activated => 2,
        ObservationKind::CapabilityEvaluated => 3,
        ObservationKind::ResourceCharged => 4,
        ObservationKind::Cancellation => 5,
        ObservationKind::Terminated => 6,
        ObservationKind::Released => 7,
        ObservationKind::FaultInjected => 8,
    }
}

const fn decode_kind(value: u8) -> Result<ObservationKind, ProcessError> {
    match value {
        1 => Ok(ObservationKind::Prepared),
        2 => Ok(ObservationKind::Activated),
        3 => Ok(ObservationKind::CapabilityEvaluated),
        4 => Ok(ObservationKind::ResourceCharged),
        5 => Ok(ObservationKind::Cancellation),
        6 => Ok(ObservationKind::Terminated),
        7 => Ok(ObservationKind::Released),
        8 => Ok(ObservationKind::FaultInjected),
        _ => Err(corrupt("native observation page has an unknown event kind")),
    }
}

const fn domain_tag(value: Option<CapabilityDomain>) -> u8 {
    match value {
        None => 0,
        Some(CapabilityDomain::Filesystem) => 1,
        Some(CapabilityDomain::Process) => 2,
        Some(CapabilityDomain::Environment) => 3,
        Some(CapabilityDomain::Network) => 4,
        Some(CapabilityDomain::Secret) => 5,
        Some(CapabilityDomain::Resource) => 6,
        Some(CapabilityDomain::Terminal) => 7,
    }
}

const fn decode_domain(value: u8) -> Result<Option<CapabilityDomain>, ProcessError> {
    match value {
        0 => Ok(None),
        1 => Ok(Some(CapabilityDomain::Filesystem)),
        2 => Ok(Some(CapabilityDomain::Process)),
        3 => Ok(Some(CapabilityDomain::Environment)),
        4 => Ok(Some(CapabilityDomain::Network)),
        5 => Ok(Some(CapabilityDomain::Secret)),
        6 => Ok(Some(CapabilityDomain::Resource)),
        7 => Ok(Some(CapabilityDomain::Terminal)),
        _ => Err(corrupt("native observation page has an unknown capability domain")),
    }
}

const fn disposition_tag(value: ObservationDisposition) -> u8 {
    match value {
        ObservationDisposition::Allowed => 1,
        ObservationDisposition::Denied => 2,
        ObservationDisposition::Completed => 3,
        ObservationDisposition::Accepted => 4,
        ObservationDisposition::AlreadyAccepted => 5,
        ObservationDisposition::Failed => 6,
    }
}

const fn decode_disposition(value: u8) -> Result<ObservationDisposition, ProcessError> {
    match value {
        1 => Ok(ObservationDisposition::Allowed),
        2 => Ok(ObservationDisposition::Denied),
        3 => Ok(ObservationDisposition::Completed),
        4 => Ok(ObservationDisposition::Accepted),
        5 => Ok(ObservationDisposition::AlreadyAccepted),
        6 => Ok(ObservationDisposition::Failed),
        _ => Err(corrupt("native observation page has an unknown disposition")),
    }
}

fn digest(bytes: &mut Vec<u8>, value: Sha256Digest) {
    bytes.extend_from_slice(value.as_bytes());
}

fn optional_digest(bytes: &mut Vec<u8>, value: Option<Sha256Digest>) {
    match value {
        Some(value) => {
            bytes.push(1);
            digest(bytes, value);
        }
        None => bytes.push(0),
    }
}

fn u64_value(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ProcessError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| corrupt("native observation page length overflowed"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| corrupt("native observation page is truncated"))?;
        self.offset = end;
        Ok(value)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ProcessError> {
        self.take(N)?
            .try_into()
            .map_err(|_| corrupt("native observation page field has invalid width"))
    }

    fn u8(&mut self) -> Result<u8, ProcessError> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, ProcessError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, ProcessError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn digest(&mut self) -> Result<Sha256Digest, ProcessError> {
        Ok(Sha256Digest::new(self.array()?))
    }

    fn optional_digest(&mut self) -> Result<Option<Sha256Digest>, ProcessError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.digest()?)),
            _ => Err(corrupt("native observation page has an invalid optional digest")),
        }
    }

    const fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

const fn corrupt(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::CorruptRecovery,
        ProcessOperation::Reconcile,
        RecoveryClass::Quarantine,
        detail,
    )
}
