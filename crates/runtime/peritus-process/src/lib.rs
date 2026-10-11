//! Authorized process backplane; [`ExecutionGateway`] consumes authority into an [`OwnedProcess`].

#![allow(clippy::redundant_pub_crate, reason = "documents internal contracts")]
mod authorization;
mod caller_binding;
mod cancellation;
mod command;
mod consumption;
mod control;
mod environment;
mod error;
mod events;
mod gateway;
mod identity;
mod intent;
mod io_policy;
mod lifecycle;
mod native;
mod output;
mod plan;
mod plan_canonical;
mod platform;
mod public_api;
mod quiescence;
mod recovery;
mod refinement;
mod registry_storage;
mod resource;
mod result_api;
mod supervisor;
mod terminal;
mod verified;
mod working_directory;

pub use public_api::*;
