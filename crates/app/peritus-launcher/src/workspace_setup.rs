//! Ergonomic repository selection, explicit trust, recent choices, and repair.

use std::{env, path::Path};

use peritus_product_state::{WorkspaceProfile, WorkspaceTrust};

use crate::{LauncherError, PreparedProduct, ProductBootstrap, terminal::Terminal};

mod discovery;
#[cfg(test)]
mod folder_tests;
mod managed;

use discovery::{DiscoveredRepository, user_path};
pub(crate) use managed::validate_registration_publication;
use managed::{WorkspaceHealth, new_profile, observe_health, trust};
#[cfg(test)]
use managed::health;

/// Selects the requested/current/recent repository, prompting only when a choice is necessary.
pub fn ensure_configured(
    prepared: PreparedProduct,
    requested: Option<&Path>,
) -> Result<PreparedProduct, LauncherError> {
    if let Some(path) = requested {
        let discovered = DiscoveredRepository::open(path)?;
        return activate_repository(prepared, discovered);
    }
    let current = match env::current_dir() {
        Ok(current) => current,
        Err(error) => {
            return choose_workspace(
                prepared,
                LauncherError::filesystem(
                    "resolve implicit current workspace directory",
                    Path::new("."),
                    error,
                ),
            );
        }
    };
    match DiscoveredRepository::open(&current) {
        Ok(discovered) => activate_repository(prepared, discovered),
        Err(error) => choose_workspace(prepared, error),
    }
}

/// Opens focused workspace settings for switching, adding, trusting, repairing, or forgetting.
pub fn configure(mut prepared: PreparedProduct) -> Result<PreparedProduct, LauncherError> {
    let mut terminal = Terminal::stdio();
    loop {
        terminal.line("")?;
        terminal.line("Workspace settings")?;
        show_recent(&mut terminal, &prepared)?;
        terminal.line("  a. Add a folder or repository path")?;
        terminal.line("")?;
        let answer = terminal.prompt(
            "Enter a number to switch, t<number> to trust/repair, r<number> to forget, a to add, or Enter to finish: ",
        )?;
        if answer.is_empty() {
            return Ok(prepared);
        }
        if answer.eq_ignore_ascii_case("a") {
            let repository = prompt_repository(&mut terminal)?;
            return activate_repository(prepared, repository);
        }
        if let Some(index) = prefixed_index(&answer, 't') {
            let profile = recent(&prepared, index)?.clone();
            if profile.trust_level() == WorkspaceTrust::Trusted
                && matches!(
                    observe_health(&profile)?,
                    WorkspaceHealth::Ready | WorkspaceHealth::Dirty
                )
            {
                terminal.line("That workspace is already trusted and ready.")?;
                continue;
            }
            let repository = DiscoveredRepository::open(Path::new(profile.repository_root()))?;
            terminal.line(&format!("Trusting workspace: {}", repository.root_text()))?;
            let trusted = trust(prepared.layout(), &repository, profile)?;
            prepared = persist_profile(&prepared, trusted)?;
            terminal.line("Workspace is ready. Plain folders are edited in place; Git workspaces use their managed copy.")?;
            continue;
        }
        if let Some(index) = prefixed_index(&answer, 'r') {
            let profile = recent(&prepared, index)?.clone();
            prepared = ProductBootstrap::new(prepared.layout().clone())
                .remove_workspace(profile.workspace_id())?;
            if profile.trust_level() == WorkspaceTrust::Trusted && !profile.is_direct_folder() {
                terminal.line(
                    "Removed from recent workspaces. Its managed copy is retained for safe recovery and later cleanup.",
                )?;
            } else {
                terminal.line("Removed from recent workspaces.")?;
            }
            continue;
        }
        if let Some(index) = parse_index(&answer) {
            let profile = recent(&prepared, index)?.clone();
            match observe_health(&profile)? {
                WorkspaceHealth::Advanced => {
                    prepared = refresh_advanced(&prepared, profile)?;
                    terminal.line("Active workspace refreshed and selected.")?;
                }
                WorkspaceHealth::NeedsRepair => terminal.line(
                    "That workspace needs repair. Choose its t<number> action before selecting it.",
                )?,
                _ => {
                    prepared = ProductBootstrap::new(prepared.layout().clone())
                        .select_workspace(profile.workspace_id())?;
                    terminal.line("Active workspace changed.")?;
                }
            }
            continue;
        }
        terminal.line("Choose a listed number, t<number>, r<number>, a, or Enter.")?;
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the selected product and discovered adapter are consumed as one transition"
)]
fn activate_repository(
    prepared: PreparedProduct,
    repository: DiscoveredRepository,
) -> Result<PreparedProduct, LauncherError> {
    if let Some(existing) = prepared
        .state()
        .workspaces()
        .find_repository(repository.root_text(), repository.identity_text())
        .cloned()
    {
        match observe_health(&existing)? {
            WorkspaceHealth::Advanced => return refresh_advanced(&prepared, existing),
            WorkspaceHealth::NeedsRepair => {}
            _ => {
                return ProductBootstrap::new(prepared.layout().clone())
                    .select_workspace(existing.workspace_id());
            }
        }
        let mut terminal = Terminal::stdio();
        terminal.line("")?;
        terminal.line("This workspace needs a quick repair before agent runs can resume.")?;
        terminal.line(&format!("Repository: {}", repository.root_text()))?;
        if !terminal.confirm("Repair its managed workspace now? [Y/n]: ", true)? {
            return Err(LauncherError::Interaction(
                "the exact selected workspace still requires repair".to_owned(),
            ));
        }
        let trusted = trust(prepared.layout(), &repository, existing)?;
        return persist_profile(&prepared, trusted);
    }

    let restricted = new_profile(&repository)?;
    let remembered = persist_profile(&prepared, restricted.clone())?;
    let mut terminal = Terminal::stdio();
    terminal.line("")?;
    terminal.line("Workspace")?;
    terminal.line(&format!("Folder: {}", repository.root_text()))?;
    let direct = restricted.is_direct_folder();
    terminal.line(if direct {
        "Peritus can chat and inspect files without trust. Trust allows requested edits directly in this folder and local commands with your user-account permissions. No repository or managed copy will be created."
    } else {
        "Peritus can browse it in restricted mode. Trust creates a separate managed worktree for edits, commands, builds, and tests; your current checkout is left alone."
    })?;
    if !terminal.confirm("Trust this workspace? [Y/n]: ", true)? {
        terminal.line("Continuing in restricted browse mode. You can trust it later with `peritus workspaces`.")?;
        return Ok(remembered);
    }
    terminal.line(if direct {
        "Enabling requested in-place work…"
    } else {
        "Preparing a private writable workspace…"
    })?;
    let trusted = trust(remembered.layout(), &repository, restricted)?;
    let configured = persist_profile(&remembered, trusted)?;
    terminal.line(if direct {
        "Folder ready. Files have not been changed; requested edits will happen here."
    } else {
        "Workspace ready. Your source checkout was not modified."
    })?;
    Ok(configured)
}

fn choose_workspace(
    prepared: PreparedProduct,
    discovery_error: LauncherError,
) -> Result<PreparedProduct, LauncherError> {
    let mut terminal = Terminal::stdio();
    terminal.line("")?;
    terminal.line("Choose a workspace")?;
    terminal.line(&format!(
        "Current-directory workspace discovery failed: {discovery_error}"
    ))?;
    if prepared.state().workspaces().recent().is_empty() {
        terminal.line("Select a folder explicitly to continue.")?;
        let repository = prompt_repository(&mut terminal)?;
        return activate_repository(prepared, repository);
    }
    loop {
        show_recent(&mut terminal, &prepared)?;
        terminal.line("  p. Enter another folder path")?;
        let answer = terminal.prompt("Choose a number, or p for a path: ")?;
        if answer.eq_ignore_ascii_case("p") {
            return activate_repository(prepared, prompt_repository(&mut terminal)?);
        }
        if let Some(index) = parse_index(&answer) {
            let profile = recent(&prepared, index)?.clone();
            match observe_health(&profile)? {
                WorkspaceHealth::Advanced => return refresh_advanced(&prepared, profile),
                WorkspaceHealth::NeedsRepair => {}
                _ => {
                    return ProductBootstrap::new(prepared.layout().clone())
                        .select_workspace(profile.workspace_id());
                }
            }
            terminal.line("That workspace needs repair. Choose t<number> in `peritus workspaces`, or select another repository.")?;
        } else {
            terminal.line("Choose one of the listed numbers, or p.")?;
        }
    }
}

fn show_recent(
    terminal: &mut Terminal<'_>,
    prepared: &PreparedProduct,
) -> Result<(), LauncherError> {
    let active = prepared.state().workspaces().active().map(WorkspaceProfile::workspace_id);
    for (index, profile) in prepared.state().workspaces().recent().into_iter().enumerate() {
        let marker = if active == Some(profile.workspace_id()) { "active, " } else { "" };
        let status = match observe_health(profile) {
            Ok(health) => health.label().to_owned(),
            Err(error) => format!("Unavailable — {error}"),
        };
        terminal.line(&format!(
            "  {}. {} — {marker}{}",
            index + 1,
            profile.repository_root(),
            status,
        ))?;
    }
    Ok(())
}

fn prompt_repository(terminal: &mut Terminal<'_>) -> Result<DiscoveredRepository, LauncherError> {
    loop {
        let answer = terminal.prompt("Folder path (q to cancel): ")?;
        if answer.eq_ignore_ascii_case("q") {
            return Err(LauncherError::Interaction("workspace selection cancelled".to_owned()));
        }
        let path = match user_path(&answer) {
            Ok(path) => path,
            Err(error) => {
                terminal.line(&error.to_string())?;
                continue;
            }
        };
        match DiscoveredRepository::open(&path) {
            Ok(repository) => return Ok(repository),
            Err(error) => terminal.line(&format!(
                "That path could not be opened as the exact requested workspace: {error}"
            ))?,
        }
    }
}

fn persist_profile(
    prepared: &PreparedProduct,
    profile: WorkspaceProfile,
) -> Result<PreparedProduct, LauncherError> {
    ProductBootstrap::new(prepared.layout().clone()).configure_workspace(profile)
}

fn refresh_advanced(
    prepared: &PreparedProduct,
    profile: WorkspaceProfile,
) -> Result<PreparedProduct, LauncherError> {
    let repository = DiscoveredRepository::open(Path::new(profile.repository_root()))?;
    let refreshed = trust(prepared.layout(), &repository, profile)?;
    persist_profile(prepared, refreshed)
}

fn recent(prepared: &PreparedProduct, index: usize) -> Result<&WorkspaceProfile, LauncherError> {
    prepared
        .state()
        .workspaces()
        .recent_at(index)
        .ok_or_else(|| LauncherError::Interaction("that workspace number is not listed".to_owned()))
}

fn parse_index(answer: &str) -> Option<usize> {
    answer.trim().parse::<usize>().ok()?.checked_sub(1)
}

fn prefixed_index(answer: &str, prefix: char) -> Option<usize> {
    let answer = answer.trim();
    (answer.starts_with(prefix) || answer.starts_with(prefix.to_ascii_uppercase()))
        .then(|| parse_index(&answer[1..]))
        .flatten()
}
