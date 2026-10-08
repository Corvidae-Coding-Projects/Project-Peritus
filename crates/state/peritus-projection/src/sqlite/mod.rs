//! Narrow `SQLite` adapter for shared-file projection generations.

mod contention;
mod progress;
mod schema;
mod store;
mod swap;

pub(crate) use progress::{StoredProgress, WorkPhase};
pub(crate) use store::{CatalogCorruption, CatalogObservation, ContainmentReason};
pub use store::{ProjectionStore, StoreOptions};
