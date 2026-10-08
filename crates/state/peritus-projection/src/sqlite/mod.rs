//! Narrow `SQLite` adapter for shared-file projection generations.

mod contention;
mod schema;
mod store;
mod swap;

pub use store::{ProjectionStore, StoreOptions};
