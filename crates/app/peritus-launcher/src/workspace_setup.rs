//! Ergonomic repository selection, explicit trust, recent choices, and repair.

use std::{env, path::Path};

use peritus_product_state::{WorkspaceProfile, WorkspaceTrust};

use crate::{LauncherError, PreparedProduct, ProductBootstrap, terminal::Terminal};

mod discovery;
mod managed;

use discovery::{DiscoveredRepository, user_path};
use managed::{WorkspaceHealth, health, new_profile, trust};

/// Selects the requested/current/recent repository, prompting only when a choice is necessary.
pub fn ensure_configured(
    prepared: PreparedProduct,
    requested: Option<&Path>,
) -> Result<PreparedProduct, LauncherError> {
    if let Some(path) = requested {
        let discovered = DiscoveredRepository::open(path)?;
        return activate_repository(prepared, discovered);
    }
    if let Ok(current) = env::current_dir()
        && let Ok(discovered) = DiscoveredRepository::open(&current)
    {
        return activate_repository(prepared, discovered);
    }
    if prepared
        .state()
        .workspaces()
        .active()
        .is_some_and(|profile| health(profile) != WorkspaceHealth::NeedsRepair)
    {
        return Ok(prepared);
    }
    choose_workspace(prepared)
}

/// Opens focused workspace settings for switching, adding, trusting, repairing, or forgetting.
pub fn configure(mut prepared: PreparedProduct) -> Result<PreparedProduct, LauncherError> {
    let mut terminal = Terminal::stdio();
    loop {
        terminal.line("")?;
        terminal.line("Workspace settings")?;
        show_recent(&mut terminal, &prepared)?;
        terminal.line("  a. Add a repository path")?;
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
                && matches!(health(&profile), WorkspaceHealth::Ready | WorkspaceHealth::Dirty)
            {
                terminal.line("That workspace is already trusted and ready.")?;
                continue;
            }
            let repository = DiscoveredRepository::open(Path::new(profile.repository_root()))?;
            terminal.line(&format!("Trusting repository: {}", repository.root_text()))?;
            let trusted = trust(prepared.layout(), &repository, profile)?;
            prepared = persist_profile(&prepared, trusted)?;
            terminal.line("Workspace is ready in its Peritus-managed writable copy.")?;
            continue;
        }
        if let Some(index) = prefixed_index(&answer, 'r') {
            let profile = recent(&prepared, index)?.clone();
            prepared = ProductBootstrap::new(prepared.layout().clone())
                .remove_workspace(profile.workspace_id())?;
            if profile.trust_level() == WorkspaceTrust::Trusted {
                terminal.line(
                    "Removed from recent workspaces. Its managed copy is retained for safe recovery and later cleanup.",
                )?;
            } else {
                terminal.line("Removed from recent workspaces.")?;
            }
            continue;
        }
        if let Some(index) = parse_index(&answer) {
            let workspace_id = recent(&prepared, index)?.workspace_id().to_owned();
            prepared =
                ProductBootstrap::new(prepared.layout().clone()).select_workspace(&workspace_id)?;
            terminal.line("Active workspace changed.")?;
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
        let selected = ProductBootstrap::new(prepared.layout().clone())
            .select_workspace(existing.workspace_id())?;
        if health(&existing) != WorkspaceHealth::NeedsRepair {
            return Ok(selected);
        }
        let mut terminal = Terminal::stdio();
        terminal.line("")?;
        terminal.line("This workspace needs a quick repair before agent runs can resume.")?;
        terminal.line(&format!("Repository: {}", repository.root_text()))?;
        if !terminal.confirm("Repair its managed workspace now? [Y/n]: ", true)? {
            return Ok(selected);
        }
        let trusted = trust(selected.layout(), &repository, existing)?;
        return persist_profile(&selected, trusted);
    }

    let restricted = new_profile(&repository)?;
    let remembered = persist_profile(&prepared, restricted.clone())?;
    let mut terminal = Terminal::stdio();
    terminal.line("")?;
    terminal.line("Workspace")?;
    terminal.line(&format!("Repository: {}", repository.root_text()))?;
    terminal.line(
        "Peritus can browse it in restricted mode. Trust creates a separate managed worktree for edits, commands, builds, and tests; your current checkout is left alone.",
    )?;
    if !terminal.confirm("Trust this repository? [Y/n]: ", true)? {
        terminal.line("Continuing in restricted browse mode. You can trust it later with `peritus workspaces`.")?;
        return Ok(remembered);
    }
    terminal.line("Preparing a private writable workspace…")?;
    let trusted = trust(remembered.layout(), &repository, restricted)?;
    let configured = persist_profile(&remembered, trusted)?;
    terminal.line("Workspace ready. Your source checkout was not modified.")?;
    Ok(configured)
}

fn choose_workspace(prepared: PreparedProduct) -> Result<PreparedProduct, LauncherError> {
    let mut terminal = Terminal::stdio();
    terminal.line("")?;
    terminal.line("Choose a workspace")?;
    if prepared.state().workspaces().recent().is_empty() {
        terminal.line("Peritus could not find a Git repository in the current directory.")?;
        let repository = prompt_repository(&mut terminal)?;
        return activate_repository(prepared, repository);
    }
    loop {
        show_recent(&mut terminal, &prepared)?;
        terminal.line("  p. Enter another repository path")?;
        let answer = terminal.prompt("Choose a number, or p for a path: ")?;
        if answer.eq_ignore_ascii_case("p") {
            return activate_repository(prepared, prompt_repository(&mut terminal)?);
        }
        if let Some(index) = parse_index(&answer) {
            let profile = recent(&prepared, index)?.clone();
            let selected = ProductBootstrap::new(prepared.layout().clone())
                .select_workspace(profile.workspace_id())?;
            if health(&profile) != WorkspaceHealth::NeedsRepair {
                return Ok(selected);
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
    for (index, profile) in prepared.state().workspaces().recent().iter().enumerate() {
        let marker = if active == Some(profile.workspace_id()) { "active, " } else { "" };
        terminal.line(&format!(
            "  {}. {} — {marker}{}",
            index + 1,
            profile.repository_root(),
            health(profile).label(),
        ))?;
    }
    Ok(())
}

fn prompt_repository(terminal: &mut Terminal<'_>) -> Result<DiscoveredRepository, LauncherError> {
    loop {
        let answer = terminal.prompt("Repository path (q to cancel): ")?;
        let path = match user_path(&answer) {
            Ok(path) => path,
            Err(error) => {
                terminal.line(&error.to_string())?;
                continue;
            }
        };
        match DiscoveredRepository::open(&path) {
            Ok(repository) => return Ok(repository),
            Err(_) => terminal.line(
                "That path is not an accessible Git repository. Check it and choose another path.",
            )?,
        }
    }
}

fn persist_profile(
    prepared: &PreparedProduct,
    profile: WorkspaceProfile,
) -> Result<PreparedProduct, LauncherError> {
    ProductBootstrap::new(prepared.layout().clone()).configure_workspace(profile)
}

fn recent(prepared: &PreparedProduct, index: usize) -> Result<&WorkspaceProfile, LauncherError> {
    prepared
        .state()
        .workspaces()
        .recent()
        .get(index)
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
