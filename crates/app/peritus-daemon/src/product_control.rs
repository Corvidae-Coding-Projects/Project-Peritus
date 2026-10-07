//! Daemon-owned authoritative C0 product-control journal. Legacy run JSON is not a writer here.

mod compaction;
mod context;
mod error;
mod goal;
pub(crate) use goal::{GoalSemanticBinding, PreparedGoalSettlement};
pub mod guidance;
mod init;
mod inputs;
mod replies;
mod storage;

pub use init::{discover_init, prepare_init_patch};

pub use error::ControlStoreError;
pub(crate) use inputs::{
    CapturedConversation, CapturedFileReaders, RequestSourceBody, RequestSourceSnapshot,
};
pub(crate) use storage::open_public_reply_artifact;
pub(crate) use storage::reply_publication_claim;
pub use storage::CheckpointSnapshots;
pub use storage::ControlStore;
pub(crate) use storage::{
    AuthorityKey, AuthoritySet, BootstrapRecoveryToken, CommittedControlAppend,
    ControlAuthority, ControlGeneration, ControlReconciliation, ControlScope,
    PreparedCheckpointSnapshots, PreparedControlAppend, PreparedFilePublication,
    PreparedImagePublication, PreparedPublicReply, PreparedRestoreEvidence,
    PublicationClaim, PublicationPurpose, PublishedPublicReply, RetainedPublication,
};

#[cfg(test)]
pub fn test_request(text: &str) -> peritus_model_protocol::ModelRequest {
    inputs::tests::request(text)
}
