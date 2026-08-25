//! Stable inert records used to persist and replay one D0 agent turn.
//!
//! These DTOs deliberately do not reconstruct an agent reducer command, an effect request, or an
//! authority receipt. D0 performs the checked conversion after decoding.

mod command;
mod event;
mod phase;
mod state;
mod wire;

pub use command::{AgentCommandDto, AgentCommandKindDto};
pub use event::{AgentEventDto, AgentEventKindDto};
pub use phase::{AgentPhaseDto, AgentResumablePhaseDto};
pub use state::{AgentCountersDto, AgentStateDto};
