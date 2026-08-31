//! Store-bound deterministic qualification identities.

use peritus_journal::StoreId;
use peritus_types::{CommandId, EventId, Sha256Digest};

use crate::{EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery};

pub(super) fn digest(domain: &[u8], store: StoreId) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(domain.len() + store.as_bytes().len());
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(store.as_bytes());
    peritus_codec::sha256(&bytes)
}

pub(super) fn nominal(domain: &[u8], store: StoreId) -> [u8; 16] {
    let value = digest(domain, store);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&value.as_bytes()[..16]);
    if bytes == [0; 16] {
        bytes[15] = 1;
    }
    bytes
}

pub(super) fn command(domain: &[u8], store: StoreId) -> Result<CommandId, EvolutionError> {
    CommandId::new(nominal(domain, store)).map_err(|_| invalid("qualification command is zero"))
}

pub(super) fn event(domain: &[u8], store: StoreId) -> Result<EventId, EvolutionError> {
    EventId::new(nominal(domain, store)).map_err(|_| invalid("qualification event is zero"))
}

pub(super) const fn invalid(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::InvalidInput,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::CorrectInput,
        detail,
    )
}

pub(super) const fn journal() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Journal,
        EvolutionOperation::Commit,
        EvolutionRecovery::Replay,
        "C0 rejected qualification prerequisite state",
    )
}
