//! Closed application-command dispatch into existing authoritative domain reducers.

mod dispatch;

pub(crate) use dispatch::{DomainOutcome, DomainSubmission, dispatch};
