//! Narrow `SQLite` adapter for shared-file projection generations.

mod contention;
mod progress;
mod schema;
mod store;
mod swap;

pub(crate) use progress::{StoredProgress, WorkPhase};
pub use store::{ProjectionStore, StoreOptions};
