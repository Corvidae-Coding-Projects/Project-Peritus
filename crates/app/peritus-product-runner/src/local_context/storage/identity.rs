//! Stable lineage-derived C0 identities with no provider or invocation component.

use super::super::error;
use peritus_agent::DeveloperLoopError;
use peritus_codec::sha256;
use peritus_context::working::WorkingBinding;
use peritus_journal::{AggregateId, AggregateKey, AggregateKind, StoreId};
use peritus_role::HarnessRole;
use peritus_types::{CommandId, EventId, Sha256Digest};

pub(super) struct StorageIdentity {
    pub(super) store: StoreId,
    pub(super) aggregate: AggregateKey,
    pub(super) scope: Sha256Digest,
}

impl StorageIdentity {
    pub(super) fn new(binding: WorkingBinding) -> Result<Self, DeveloperLoopError> {
        let mut bytes = b"peritus-local-working-memory/lineage/v1".to_vec();
        bytes.extend_from_slice(binding.run().as_bytes());
        bytes.extend_from_slice(binding.workspace().as_bytes());
        bytes.extend_from_slice(binding.task().as_bytes());
        bytes.push(match binding.role() {
            HarnessRole::Writer => 0,
            HarnessRole::Reviewer => 1,
            HarnessRole::Fixer => 2,
            HarnessRole::Evaluator => 3,
            HarnessRole::Evolver => 4,
        });
        let scope = sha256(&bytes);
        let id = derive(scope, b"store", 0);
        let store = StoreId::new(id).map_err(|_| error("invalid storage identity"))?;
        let aggregate = AggregateKey::new(
            AggregateKind::Agent,
            AggregateId::new(derive(scope, b"aggregate", 0))
                .map_err(|_| error("invalid aggregate identity"))?,
        );
        Ok(Self { store, aggregate, scope })
    }
    pub(super) fn event(&self, sequence: u64) -> Result<EventId, DeveloperLoopError> {
        EventId::new(derive(self.scope, b"event", sequence))
            .map_err(|_| error("invalid event identity"))
    }
    pub(super) fn command(&self, sequence: u64) -> Result<CommandId, DeveloperLoopError> {
        CommandId::new(derive(self.scope, b"command", sequence))
            .map_err(|_| error("invalid command identity"))
    }
}

fn derive(scope: Sha256Digest, label: &[u8], sequence: u64) -> [u8; 16] {
    let mut bytes = scope.as_bytes().to_vec();
    bytes.extend_from_slice(label);
    bytes.extend_from_slice(&sequence.to_be_bytes());
    let digest = sha256(&bytes);
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id[0] |= 1;
    id
}
