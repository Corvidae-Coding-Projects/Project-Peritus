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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn declining_installation_never_downloads_or_runs_a_provider() {
        let mut input = Cursor::new(b"n\n");
        let mut output = Vec::new();
        let mut terminal = Terminal::for_test(&mut input, &mut output);
        assert!(!offer(&mut terminal, ProviderKind::CodexAccount).expect("declined"));
        drop(terminal);
        assert!(String::from_utf8(output).expect("output").contains("[y/N]"));
    }
}
