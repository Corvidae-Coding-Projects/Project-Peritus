//! Branch-ordered typed rejection relation for total reducer outcomes.

#![allow(
    clippy::redundant_pub_crate,
    reason = "Verus rejection symbols require crate visibility across the private model tree"
)]

pub(crate) mod allocation;
mod reservation;
