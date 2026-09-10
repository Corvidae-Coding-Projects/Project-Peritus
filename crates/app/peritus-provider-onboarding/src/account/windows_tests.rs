//! Native console regression: even a title-changing provider must not inherit the host console.

use std::{fs, os::windows::process::CommandExt as _, process::Command};

use windows_sys::Win32::System::Console::{GetConsoleCP, SetConsoleTitleW};

use super::{AccountProvider, ProviderKind, ProviderStatus};
use peritus_provider_core::{
    CancellationToken, ProcessExecutable, ProcessLimits, ProcessRequest, ProcessTransport,
    TokioProcessTransport,
    catalog::{AccountCatalog, discover_account_models},
};

const HOST: &str = "PERITUS_TITLE_TEST_HOST";

#[test]
fn status_probes_have_no_console() {
    let executable = std::env::current_exe().expect("test executable");
    if std::env::var_os(HOST).is_none() {
        // A separate console makes this deterministic on headless CI and isolates other tests.
        let output = Command::new(&executable)
            .args(["--exact", "account::windows_tests::status_probes_have_no_console"])
            .env(HOST, "1")
            .creation_flags(0x0000_0010) // CREATE_NEW_CONSOLE
            .output()
            .expect("isolated console host");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
        return;
    }
    assert_ne!(console_code_page(), 0, "the host must own a real console");
    let temporary = tempfile::tempdir().expect("fixture directory");
    let provider_exe = temporary.path().join("provider.cmd");
    fs::write(
        &provider_exe,
        format!(
            "@\"{}\" --exact account::windows_tests::title_changing_provider --nocapture >NUL\r\n@if errorlevel 1 exit /b 1\r\n@if \"%~1\"==\"--print\" goto catalog\r\n@echo {{\"loggedIn\":true}}\r\n@exit /b 0\r\n:catalog\r\n@set /p request=\r\n@echo {{\"type\":\"control_response\",\"response\":{{\"subtype\":\"success\",\"request_id\":\"peritus-models\",\"response\":{{\"models\":[{{\"value\":\"fixture-model\"}}]}}}}}}\r\n",
            executable.display(),
        ),
    )
    .expect("provider fixture");
    for kind in [ProviderKind::ClaudeAccount, ProviderKind::CodexAccount] {
        let mut command = super::status_command(kind, &provider_exe).expect("status command");
        let output = command.output().expect("provider status");
        assert!(output.status.success(), "provider retained console access: {output:?}");
        assert_eq!(super::parse_status(kind, true, &output.stdout), ProviderStatus::Ready);
    }
    // Also exercise the public observation path, not just the command builder.
    let provider =
        AccountProvider { kind: ProviderKind::ClaudeAccount, executable: provider_exe.clone() };
    assert_eq!(provider.status().status(), ProviderStatus::Ready);
    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    runtime.block_on(async {
        let models = discover_account_models(
            &provider_exe,
            AccountCatalog::Claude,
            &CancellationToken::new(),
        )
        .await
        .expect("headless model discovery");
        assert_eq!(models[0].id.as_str(), "fixture-model");
        let request = ProcessRequest::new(
            ProcessExecutable::pin(&provider_exe).expect("pinned provider"),
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
            ProcessLimits::new(1024, 4096, 4096, std::time::Duration::from_secs(10))
                .expect("limits"),
        )
        .expect("provider request");
        let output = TokioProcessTransport
            .run(request, &CancellationToken::new())
            .await
            .expect("headless provider transport");
        assert!(output.exit().success(), "provider transport retained console access");
    });
}

#[test]
fn title_changing_provider() {
    if std::env::var_os(HOST).is_none() {
        return;
    }
    assert_eq!(console_code_page(), 0, "a background provider inherited the host console");
    assert!(!change_title(), "a background provider changed the host console title");
}

#[allow(unsafe_code, reason = "the Windows regression observes its own process console")]
fn console_code_page() -> u32 {
    // SAFETY: GetConsoleCP takes no pointers and observes only the calling process's console.
    unsafe { GetConsoleCP() }
}

#[allow(
    unsafe_code,
    reason = "the fixture attempts the provider's native console-title side effect"
)]
fn change_title() -> bool {
    let title = "claude".encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    // SAFETY: The initialized, zero-terminated buffer remains alive throughout the synchronous call.
    unsafe { SetConsoleTitleW(title.as_ptr()) != 0 }
}
