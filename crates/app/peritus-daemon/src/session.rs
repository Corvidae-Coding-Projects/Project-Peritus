//! Authenticated A3 session establishment and connection ownership.

mod connection;
mod heartbeat;
mod negotiation;

pub(crate) use connection::run_connection;
pub use negotiation::ConnectionContext;
