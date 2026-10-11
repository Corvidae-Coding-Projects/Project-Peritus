//! Native release evidence and publication effect boundary.

mod app;
mod args;
mod cargo_graph;
mod clock;
mod error;
mod evidence;
mod files;
mod package_record;
mod publish;
mod repository;

fn main() {
    if let Err(error) = app::run_from_env() {
        eprintln!("peritus-release-operator: {error}");
        std::process::exit(1);
    }
}
