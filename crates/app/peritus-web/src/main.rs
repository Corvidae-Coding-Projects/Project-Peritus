//! Local-only Peritus browser gateway. The daemon owns all agent execution.

mod api;
mod body;
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
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if let Some(request) = files::directory_owner_argument() {
        return files::run_directory_owner(&request)
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
    }
    if let Some(request) = git::inventory_owner_argument() {
        return git::run_inventory_owner(&request)
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
    }
    if let Some(request) = git::owner_argument() {
        return git::run_owner(&request)
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
    }
    if let Some(directory) = git::watchdog_argument() {
        return git::run_watchdog(&directory)
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
    }
    if let Some(request) = git::command_argument() {
        return git::run_command(&request)
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
    }
    if arguments.first().is_some_and(|argument| argument == "--console-owner") {
        if arguments.len() != 2 {
            return Err(Box::new(error::problem(
                "--console-owner requires exactly one launch manifest",
            )));
        }
        return terminal::run_owner(std::path::Path::new(&arguments[1]))
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
    }
    server::run()
}
