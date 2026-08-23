//! Closed B1 policy enum tags.

use peritus_codec::{CanonicalReader, CodecError, CodecErrorKind};
use peritus_policy::OperationClass;

pub const fn operation_class_tag(value: OperationClass) -> u16 {
    match value {
        OperationClass::Inspection => 1,
        OperationClass::WorkspaceMutation => 2,
        OperationClass::Execution => 3,
        OperationClass::Network => 4,
        OperationClass::DependencyEnvironment => 5,
        OperationClass::RepositoryHistoryMutation => 6,
        OperationClass::SecretUse => 7,
        OperationClass::ExternalSideEffect => 8,
        OperationClass::Acceptance => 9,
        OperationClass::Waiver => 10,
        OperationClass::PolicyAmendment => 11,
        OperationClass::HarnessPromotion => 12,
        OperationClass::HumanAuthority => 13,
        OperationClass::RawEffect => 14,
    }
}

pub fn read_operation_class(
    reader: &mut CanonicalReader<'_>,
) -> Result<OperationClass, CodecError> {
    let offset = reader.offset();
    match reader.read_u16()? {
        1 => Ok(OperationClass::Inspection),
        2 => Ok(OperationClass::WorkspaceMutation),
        3 => Ok(OperationClass::Execution),
        4 => Ok(OperationClass::Network),
        5 => Ok(OperationClass::DependencyEnvironment),
        6 => Ok(OperationClass::RepositoryHistoryMutation),
        7 => Ok(OperationClass::SecretUse),
        8 => Ok(OperationClass::ExternalSideEffect),
        9 => Ok(OperationClass::Acceptance),
        10 => Ok(OperationClass::Waiver),
        11 => Ok(OperationClass::PolicyAmendment),
        12 => Ok(OperationClass::HarnessPromotion),
        13 => Ok(OperationClass::HumanAuthority),
        14 => Ok(OperationClass::RawEffect),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}
