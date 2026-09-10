//! Inputs and effects for the deterministic UI reducer.

use crossterm::event::Event;
use peritus_app_protocol::{AppMessage, AppProtocolLimits, ProtocolContext};
use peritus_types::Sha256Digest;
use std::path::PathBuf;

/// One external observation consumed by the UI reducer.
#[allow(
    clippy::large_enum_variant,
    reason = "the reducer owns one bounded A3 observation at a time without extra allocation"
)]
#[derive(Debug)]
pub enum Action {
    FileRead {
        operation: peritus_app_protocol::ControlOperationId,
        result: Result<crate::file_import::FileBytes, &'static str>,
    },
    FileReadFailed,
    ImageRead {
        operation: peritus_app_protocol::ControlOperationId,
        result: Result<crate::image_import::ImageBytes, &'static str>,
    },
    ImageReadFailed,
    NegotiatedFeatures {
        context: ProtocolContext,
        features: Vec<peritus_app_protocol::ProtocolFeatureName>,
    },
    Connected {
        context: ProtocolContext,
        limits: AppProtocolLimits,
        server: String,
        downgraded: bool,
    },
    ConnectionFailed(String),
    Disconnected(String),
    Message(AppMessage),
    TerminalEvent(Event),
    Tick(std::time::Instant),
}

/// One external operation requested by the reducer.
#[allow(
    clippy::large_enum_variant,
    reason = "effects transfer one bounded A3 frame directly to the single writer"
)]
#[derive(Clone, Debug)]
pub enum Effect {
    ReadFile {
        operation: peritus_app_protocol::ControlOperationId,
        path: PathBuf,
        range: peritus_app_protocol::WorkbenchFileRange,
    },
    ReadImage {
        operation: peritus_app_protocol::ControlOperationId,
        path: PathBuf,
    },
    Send(AppMessage),
    RunCandidate {
        workspace: PathBuf,
        instruction: String,
        candidate_digest: Sha256Digest,
    },
    Reconnect,
    Quit,
}
