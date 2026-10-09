//! Local-only Peritus browser gateway. The daemon owns all agent execution.

mod api;
mod config;
mod consoles;
mod daemon;
mod error;
mod files;
mod git;
mod operations;
mod server;
mod sessions;
mod state;
mod terminal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    server::run()
}
