//! Versioned line-delimited controller protocol.

mod artifact;
mod request;
mod response;

pub(super) use request::{EncodedRequest, Stage, encode_request};
pub(super) use response::{ValidatedStage, parse_response};
