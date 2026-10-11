//! Human-approved installation at the selected account-provider boundary.

use peritus_product_state::ProviderKind;
use peritus_provider_onboarding::install_account_provider;

use crate::{LauncherError, terminal::Terminal};

pub(super) fn offer(
    terminal: &mut Terminal<'_>,
    kind: ProviderKind,
) -> Result<bool, LauncherError> {
    let executable = if kind == ProviderKind::CodexAccount { "codex" } else { "claude" };
    terminal.line(&format!("The {executable} tool is not installed."))?;
    if !terminal.confirm(
        &format!("Download and run the official {executable} installer for this user? [y/N]: "),
        false,
    )? {
        return Ok(false);
    }
    terminal
        .line("The provider owns this installer. No sign-in occurs until installation finishes.")?;
    match install_account_provider(kind) {
        Ok(_) => Ok(true),
        Err(error) => {
            terminal.line(&format!("Installation did not finish: {error}"))?;
            Ok(false)
        }
    }
}
