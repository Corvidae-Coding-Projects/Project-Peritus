use super::*;
use crate::product_control::inputs::manifest::{MessageRole, MessageSource};
use peritus_product_runner::control::OperationId;
use peritus_types::{ActorId, WorkspaceId};

#[test]
fn complete_manifest_above_former_state_ceiling_remains_verifiable_and_inspectable() {
    let conversation = ConversationId::new([1; 16]).expect("conversation");
    let invocation = InvocationId::new([2; 16]).expect("invocation");
    let manifest = Manifest {
        version: 1,
        conversation,
        invocation,
        actor: [3; 16],
        workspace: [4; 16],
        revision: 1,
        generation: 1,
        request_id: "large-manifest".to_owned(),
        request_digest: [5; 32],
        included: Vec::new(),
        newly_incorporated: Vec::new(),
        public_replies: Vec::new(),
        sources: Vec::new(),
        messages: vec![
            MessageSource {
                role: MessageRole::User,
                digest: [255; 32],
                encoded_bytes: 20,
                blocks: 1,
            };
            100_000
        ],
        request_bytes: 2_000_000,
        brief: Vec::new(),
        images: Vec::new(),
        files: Vec::new(),
        guidance: None,
    };
    let bytes = serde_json::to_vec(&manifest).expect("manifest");
    assert!(bytes.len() > peritus_journal::MAX_STATE_BYTES);
    let operation = ControlOperation::new(
        OperationId::new([6; 16]).expect("operation"),
        conversation,
        ActorId::new(manifest.actor).expect("actor"),
        WorkspaceId::new(manifest.workspace).expect("workspace"),
        1,
        ControlIntent::Queue(QueueIntent::Incorporate {
            invocation,
            request_digest: manifest.request_digest,
            manifest_digest: peritus_codec::sha256(&bytes).into_bytes(),
            items: Vec::new(),
        }),
    );
    verify_manifest(&operation, &bytes, None).expect("complete manifest accepted");
    let (seal, request_bytes, rows) = inspect::inspect_manifest(&bytes).expect("inspect all rows");
    assert_eq!(seal.request_digest().into_bytes(), manifest.request_digest);
    assert_eq!(request_bytes, manifest.request_bytes);
    assert_eq!(rows.len(), manifest.messages.len());
    let mut changed = bytes;
    changed.push(b' ');
    assert!(verify_manifest(&operation, &changed, None).is_err(), "exact digest remains required");
}
