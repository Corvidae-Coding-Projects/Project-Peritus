//! Stable application-protocol schema and PRTS family assignments.

/// Version-one application protocol schema number.
pub const APP_SCHEMA_V1: u16 = 1;

/// Canonical PRTS family for client negotiation input.
pub const CLIENT_HELLO_FAMILY: u16 = 94;
/// Canonical PRTS family for server negotiation output.
pub const SERVER_HELLO_FAMILY: u16 = 95;
/// Canonical PRTS family for application requests.
pub const REQUEST_FAMILY: u16 = 96;
/// Canonical PRTS family for application responses.
pub const RESPONSE_FAMILY: u16 = 97;
/// Canonical PRTS family for application events.
pub const EVENT_FAMILY: u16 = 98;
/// Canonical PRTS family for application controls.
pub const CONTROL_FAMILY: u16 = 99;
