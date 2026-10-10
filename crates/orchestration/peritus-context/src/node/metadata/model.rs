//! Recursive specification of metadata admission and failure priority.

use crate::{AuthorityClass, ContentKind};

pub(super) const fn kind_matches_authority_exec(
    kind: ContentKind,
    authority: AuthorityClass,
) -> bool {
    match kind {
        ContentKind::SystemPolicy => matches!(authority, AuthorityClass::SystemPolicy),
        ContentKind::ApplicationPolicy => matches!(authority, AuthorityClass::ApplicationPolicy),
        ContentKind::ImmutableSpecification => {
            matches!(authority, AuthorityClass::AcceptanceSpecification)
        }
        ContentKind::ActiveUserInstruction => matches!(authority, AuthorityClass::UserInstruction),
        ContentKind::CapabilityFact => !matches!(authority, AuthorityClass::NonAuthoritative),
        _ => matches!(authority, AuthorityClass::NonAuthoritative),
    }
}
