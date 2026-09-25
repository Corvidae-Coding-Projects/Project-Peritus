//! Shared local transport contracts for Peritus user interfaces.
//!
//! The daemon remains the authority for conversations, effects, and recovery.
//! Transport failures must never be interpreted as proof that an effect did not
//! occur; consumers retain operation identities and reconcile durable receipts.

mod connection;
mod error;
mod events;
mod frame;
mod identity;
mod request;

pub use connection::Client;
pub use error::{ClientError, ClientErrorKind};
pub use identity::RequestIdentity;
