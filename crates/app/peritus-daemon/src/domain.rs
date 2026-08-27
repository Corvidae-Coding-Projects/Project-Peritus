//! Closed application-command dispatch into existing authoritative domain reducers.

mod dispatch;

pub use dispatch::{DomainOutcome, DomainSubmission, dispatch};
