//! Daemon-owned authoritative C0 product-control journal. Legacy run JSON is not a writer here.

mod compaction;
mod context;
mod error;
mod goal;
pub mod guidance;
mod init;
mod inputs;
mod replies;
mod storage;

pub use init::{discover_init, prepare_init_patch};

pub use error::ControlStoreError;
pub use storage::ControlStore;

#[cfg(test)]
pub fn test_request(text: &str) -> peritus_model_protocol::ModelRequest {
    inputs::tests::request(text)
}
