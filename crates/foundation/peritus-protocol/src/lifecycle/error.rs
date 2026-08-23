//! Canonical inert reducer failure record.

#![allow(
    clippy::missing_errors_doc,
    reason = "canonical error-record failures use the shared CodecError vocabulary"
)]

use crate::SCHEMA_V1;
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_kernel::{AuthorityInputKind, KernelError, KernelErrorKind, LifecycleEntity};

/// Reducer failure decoded as diagnostic data, not as a kernel-produced result.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KernelErrorDto {
    /// Stable failure class.
    pub kind: KernelErrorKind,
    /// Affected lifecycle entity, when applicable.
    pub entity: Option<LifecycleEntity>,
    /// Required external authority input, when applicable.
    pub authority: Option<AuthorityInputKind>,
}

impl From<KernelError> for KernelErrorDto {
    fn from(error: KernelError) -> Self {
        Self {
            kind: error.kind(),
            entity: error.affected_entity(),
            authority: error.authority_input(),
        }
    }
}

impl CanonicalEncode for KernelErrorDto {
    const FAMILY: u16 = 4;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_u16(error_tag(self.kind))?;
        writer.write_option_tag(self.entity.is_some())?;
        if let Some(entity) = self.entity {
            writer.write_u16(entity_tag(entity))?;
        }
        writer.write_option_tag(self.authority.is_some())?;
        if let Some(authority) = self.authority {
            writer.write_u16(authority_tag(authority))?;
        }
        Ok(())
    }
}

impl CanonicalDecode for KernelErrorDto {
    const FAMILY: u16 = 4;
    const SCHEMA_VERSION: u16 = SCHEMA_V1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let kind = read_error_kind(reader)?;
        let entity = if reader.read_option_tag()? { Some(read_entity(reader)?) } else { None };
        let authority =
            if reader.read_option_tag()? { Some(read_authority(reader)?) } else { None };
        Ok(Self { kind, entity, authority })
    }
}

const fn error_tag(kind: KernelErrorKind) -> u16 {
    use KernelErrorKind as K;
    match kind {
        K::RevisionMismatch => 1,
        K::ContractMismatch => 2,
        K::CausalHeadMismatch => 3,
        K::DuplicateCommand => 4,
        K::DuplicateEvent => 5,
        K::MissingEntity => 6,
        K::DuplicateEntity => 7,
        K::ParentMismatch => 8,
        K::IllegalPhase => 9,
        K::MissingAuthorityInput => 10,
        K::AuthorityMismatch => 11,
        K::BudgetUnavailable => 12,
        K::BudgetExceeded => 13,
        K::LiveChild => 14,
        K::SequenceOverflow => 15,
        K::InvalidAggregate => 16,
    }
}

fn read_error_kind(reader: &mut CanonicalReader<'_>) -> Result<KernelErrorKind, CodecError> {
    use KernelErrorKind as K;
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(K::RevisionMismatch),
        2 => Ok(K::ContractMismatch),
        3 => Ok(K::CausalHeadMismatch),
        4 => Ok(K::DuplicateCommand),
        5 => Ok(K::DuplicateEvent),
        6 => Ok(K::MissingEntity),
        7 => Ok(K::DuplicateEntity),
        8 => Ok(K::ParentMismatch),
        9 => Ok(K::IllegalPhase),
        10 => Ok(K::MissingAuthorityInput),
        11 => Ok(K::AuthorityMismatch),
        12 => Ok(K::BudgetUnavailable),
        13 => Ok(K::BudgetExceeded),
        14 => Ok(K::LiveChild),
        15 => Ok(K::SequenceOverflow),
        16 => Ok(K::InvalidAggregate),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

const fn entity_tag(entity: LifecycleEntity) -> u16 {
    match entity {
        LifecycleEntity::Session => 1,
        LifecycleEntity::Run => 2,
        LifecycleEntity::Attempt => 3,
        LifecycleEntity::Turn => 4,
        LifecycleEntity::Action => 5,
        LifecycleEntity::Review => 6,
        LifecycleEntity::Waiver => 7,
        LifecycleEntity::Acceptance => 8,
    }
}

fn read_entity(reader: &mut CanonicalReader<'_>) -> Result<LifecycleEntity, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(LifecycleEntity::Session),
        2 => Ok(LifecycleEntity::Run),
        3 => Ok(LifecycleEntity::Attempt),
        4 => Ok(LifecycleEntity::Turn),
        5 => Ok(LifecycleEntity::Action),
        6 => Ok(LifecycleEntity::Review),
        7 => Ok(LifecycleEntity::Waiver),
        8 => Ok(LifecycleEntity::Acceptance),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

const fn authority_tag(authority: AuthorityInputKind) -> u16 {
    match authority {
        AuthorityInputKind::RunBudget => 1,
        AuthorityInputKind::AttemptBudget => 2,
        AuthorityInputKind::ParentBudget => 3,
        AuthorityInputKind::CapabilityUse => 4,
        AuthorityInputKind::AcceptanceEvidence => 5,
    }
}

fn read_authority(reader: &mut CanonicalReader<'_>) -> Result<AuthorityInputKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(AuthorityInputKind::RunBudget),
        2 => Ok(AuthorityInputKind::AttemptBudget),
        3 => Ok(AuthorityInputKind::ParentBudget),
        4 => Ok(AuthorityInputKind::CapabilityUse),
        5 => Ok(AuthorityInputKind::AcceptanceEvidence),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
