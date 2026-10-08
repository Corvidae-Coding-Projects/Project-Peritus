//! Human-approved installation at the selected account-provider boundary.

use peritus_product_state::ProviderKind;
use peritus_provider_core::CancellationToken;
use peritus_provider_onboarding::{
    OnboardingError, ProviderEffectStore, install_account_provider,
};

use crate::{LauncherError, terminal::Terminal};

pub(super) async fn offer(
    terminal: &mut Terminal<'_>,
    kind: ProviderKind,
    cancellation: &CancellationToken,
    effects: &ProviderEffectStore,
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
    let mut installation = Box::pin(install_account_provider(kind, cancellation, effects));
    let result = tokio::select! {
        result = &mut installation => result,
        signal = tokio::signal::ctrl_c() => {
            if signal.is_ok() {
                let _ = cancellation.cancel();
            }
            installation.await
        }
    };
    match result {
        Ok(_) => Ok(true),
        Err(error @ OnboardingError::Cancelled) => Err(LauncherError::Provider(error)),
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

    #[tokio::test]
    async fn declining_installation_never_downloads_or_runs_a_provider() {
        let mut input = Cursor::new(b"n\n");
        let mut output = Vec::new();
        let mut terminal = Terminal::for_test(&mut input, &mut output);
        let temporary = tempfile::tempdir().expect("effect root");
        let effects = ProviderEffectStore::open(temporary.path().join("provider-effects"))
            .expect("effect store");
        assert!(
            !offer(
                &mut terminal,
                ProviderKind::CodexAccount,
                &CancellationToken::new(),
                &effects,
            )
                .await
                .expect("declined")
        );
        drop(terminal);
        assert!(String::from_utf8(output).expect("output").contains("[y/N]"));
    }
}
