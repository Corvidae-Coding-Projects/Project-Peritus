//! Deterministic parent tool caller bindings for workspace gateway tests.

use peritus_policy::ActorRole;
use peritus_types::{ActionId, CapabilityName, Sha256Digest};
use peritus_workspace::WorkspaceCallerBinding;

use super::authority_support::Ids;

pub fn tool_binding(
    ids: &Ids,
    action_seed: u8,
    capability: &str,
    descriptor_seed: u8,
    prepared_seed: u8,
) -> WorkspaceCallerBinding {
    WorkspaceCallerBinding::new(
        ActionId::new([action_seed; 16]).expect("parent tool action"),
        ids.actor,
        ActorRole::Writer,
        ids.workspace,
        ids.environment,
        ids.resource,
        CapabilityName::new(capability.to_owned()).expect("parent tool capability"),
        Sha256Digest::new([descriptor_seed; 32]),
        Sha256Digest::new([prepared_seed; 32]),
    )
}
