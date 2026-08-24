//! Bounded filesystem tools for immutable and authorized Peritus workspaces.

mod catalog;
mod decoder;
mod dispatcher;
mod error;
mod input;
mod mutation;
mod read;
mod read_digest;
mod render;
mod schemas;
mod verified;

pub use catalog::{descriptor_catalog, descriptor_digest};
pub use dispatcher::{FsDispatchKind, FsDispatcher};
pub use error::{FsToolError, FsToolErrorKind, FsToolOperation, RecoveryClass};
pub use input::{
    CreateInput, DiscoverInput, MetadataInput, PatchEdit, PatchInput, ReadInput, RemoveInput,
    ReplaceInput, SearchInput, WriteInput,
};
pub use mutation::{CompiledMutation, WorkspaceVersion};
pub use read::{
    DiscoverEntry, DiscoverObservation, FileContent, FileObservation, FsReadService,
    MetadataObservation, SearchMatch, SearchObservation,
};
pub use render::RenderedOutput;
