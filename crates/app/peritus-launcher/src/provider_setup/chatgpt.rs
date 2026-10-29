//! Explicit independent `ChatGPT` registration selection and browser OAuth.
use crate::{LauncherError, terminal::Terminal};
use peritus_product_state::{DirectProviderProfile, ProviderKind};
use peritus_provider_core::CancellationToken;
use peritus_provider_onboarding::{ChatGptAccount, ChatGptLogin, OnboardingError};
use std::path::Path;

pub(super) fn setup(
    terminal: &mut Terminal<'_>,
    root: &Path,
) -> Result<DirectProviderProfile, LauncherError> {
    terminal.line("ChatGPT direct OAuth (preview)")?;
    terminal.line(
        "Continue with ChatGPT and authorize plan usage. Codex remains available separately.",
    )?;
    let accounts = ChatGptAccount::list(root).map_err(failure)?;
    let account = choose_account(terminal, root, &accounts)?;
    if !account.authorized() {
        return Err(LauncherError::Interaction(
            "ChatGPT sign-in succeeded, but plan usage was not granted. Sign in again and authorize direct plan access.".to_owned(),
        ));
    }
    terminal.line(&format!("Signed in: {}. Discovering current models…", account.label()))?;
    let discovery = run(async { account.models(&CancellationToken::new()).await });
    let (model, _) = super::models::choose_direct(terminal, ProviderKind::ChatGptOauth, discovery)?;
    let profile = DirectProviderProfile::new(
        ProviderKind::ChatGptOauth,
        account.credential_reference(),
        None,
        model,
        None,
        None,
    )?;
    super::connection::offer(terminal, &profile)?;
    Ok(profile)
}
fn choose_account(
    terminal: &mut Terminal<'_>,
    root: &Path,
    accounts: &[ChatGptAccount],
) -> Result<ChatGptAccount, LauncherError> {
    if accounts.is_empty() {
        return login(terminal, root, None);
    }
    for (index, account) in accounts.iter().enumerate() {
        let status = if account.authorized() { "saved" } else { "sign-in required" };
        terminal.line(&format!("  {}. {} ({status})", index + 1, account.label()))?;
    }
    terminal.line("  a. Add account")?;
    loop {
        let answer =
            terminal.prompt("Choose a saved account, a to add, or r NUMBER to sign in again: ")?;
        if answer.eq_ignore_ascii_case("a") {
            return login(terminal, root, None);
        }
        let (reauth, number) =
            answer.strip_prefix("r ").map_or((false, answer.as_str()), |n| (true, n));
        if let Ok(number) = number.parse::<usize>()
            && let Some(account) = number.checked_sub(1).and_then(|n| accounts.get(n))
        {
            return if reauth || !account.authorized() {
                login(terminal, root, Some(&account.credential_reference()))
            } else {
                Ok(account.clone())
            };
        }
        terminal.line("Choose an account number, a, or r NUMBER.")?;
    }
}
fn login(
    terminal: &mut Terminal<'_>,
    root: &Path,
    existing: Option<&str>,
) -> Result<ChatGptAccount, LauncherError> {
    let cancel = CancellationToken::new();
    std::thread::scope(|scope| {
        let _guard = CancelOnDrop(cancel.clone());
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let cancel_ref = &cancel;
        let worker = scope.spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| {
                    LauncherError::Interaction("ChatGPT login runtime is unavailable".to_owned())
                })?;
            runtime.block_on(async {
                let pending = ChatGptLogin::begin(root, existing).await.map_err(failure)?;
                sender.send(pending.authorization_url().to_owned()).map_err(|_| {
                    LauncherError::Interaction("ChatGPT sign-in cancelled".to_owned())
                })?;
                open_browser(pending.authorization_url()).await;
                pending.finish(cancel_ref).await.map_err(failure)
            })
        });
        // A failed setup drops the sender, so this cannot wait for an authorization that never began.
        if let Ok(url) = receiver.recv() {
            terminal.line("Open this URL in your browser and complete ChatGPT sign-in:")?;
            terminal.line(&url)?;
            terminal.line("Waiting for the local browser callback (up to 30 minutes)…")?;
        }
        worker
            .join()
            .map_err(|_| LauncherError::Interaction("ChatGPT login worker stopped".to_owned()))?
    })
}
pub(super) fn sign_out(
    terminal: &mut Terminal<'_>,
    profile: &DirectProviderProfile,
) -> Result<(), LauncherError> {
    let account =
        ChatGptAccount::from_reference(profile.credential_reference()).map_err(failure)?;
    let cancellation = CancellationToken::new();
    let revoked = run(account.sign_out(&cancellation))?;
    terminal.line(if revoked {
        "Signed out. This registration is retained for the next sign-in."
    } else {
        "Local credentials cleared. Remote revocation could not be confirmed; review connected apps in ChatGPT."
    })
}
fn run<T: Send>(
    future: impl Future<Output = Result<T, peritus_provider_core::ProviderCoreError>> + Send,
) -> Result<T, OnboardingError> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| OnboardingError::ModelCatalog)?;
                runtime.block_on(future).map_err(OnboardingError::ModelDiscovery)
            })
            .join()
            .map_err(|_| OnboardingError::ModelCatalog)?
    })
}
const fn failure(error: peritus_provider_core::ProviderCoreError) -> LauncherError {
    LauncherError::Provider(OnboardingError::ModelDiscovery(error))
}
struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        let _ = self.0.cancel();
    }
}

/// The URL remains available if the OS browser opener is absent or cannot reach the desktop.
async fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let mut command = tokio::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut command = tokio::process::Command::new("open");
    #[cfg(unix)]
    command.arg(url);
    #[cfg(windows)]
    let mut command = {
        let Some(system_root) = std::env::var_os("SystemRoot")
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_absolute())
        else {
            return;
        };
        let mut command = tokio::process::Command::new(system_root.join("System32/rundll32.exe"));
        command.args(["url.dll,FileProtocolHandler", url]);
        command
    };
    command
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), command.status()).await;
}
