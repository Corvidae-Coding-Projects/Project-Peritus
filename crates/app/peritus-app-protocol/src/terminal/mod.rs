//! Bounded terminal attachment records and ordering state.

mod error;
mod messages;
mod state;
mod verified;

pub use error::{TerminalError, TerminalErrorKind};
pub use messages::{
    TerminalBinding, TerminalCancellation, TerminalDetach, TerminalExit, TerminalExitDisposition,
    TerminalInput, TerminalOutput, TerminalResize, TerminalStream,
};
pub use state::{TerminalPhase, TerminalState, TerminalTransitionDisposition};
pub use verified::{output_is_contiguous, output_position_is_valid};
