//! Managed-workspace tools exposed to the D0 developer loop.

mod catalog;
mod effect;
mod evidence;
mod executor;
mod grounding;
mod inspection;
mod ownership;
mod path;
mod process;
mod receipt;
mod removal;
mod wire;

pub use catalog::{definitions, read_only_definitions};
pub use executor::WorkspaceDeveloperTools;
pub use ownership::WorkspaceOwnership;

pub fn merge_rendered(retained: &mut String, incoming: &str) {
    evidence::merge_rendered(retained, incoming);
}
