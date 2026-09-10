//! Transport-neutral application protocol for Peritus.
//!
//! A3 defines canonical client/daemon messages and pure validation state machines. Decoding a
//! message establishes syntax and bounded semantic validity only; it never authenticates an actor,
//! grants authority, consumes approval, proves durable commit, or performs an external effect.

#![allow(
    clippy::large_enum_variant,
    reason = "closed application envelopes retain their bounded typed payloads"
)]
#![allow(
    clippy::too_many_arguments,
    reason = "protocol constructors keep independent correlation and freshness bindings explicit"
)]

mod artifact;
mod command;
mod daemon;
mod envelope;
mod error;
mod family;
mod identity;
mod limits;
mod product;
mod prompt;
pub mod schema;
mod subscription;
mod terminal;
pub mod verified;
mod version;
pub mod wire;

pub use artifact::*;
pub use command::*;
pub use daemon::*;
pub use envelope::*;
pub use error::*;
pub use family::*;
pub use identity::*;
pub use limits::*;
pub use product::*;
pub use prompt::*;
pub use subscription::*;
pub use terminal::*;
pub use version::*;
pub use wire::*;
