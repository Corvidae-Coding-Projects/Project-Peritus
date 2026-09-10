//! Native console regression: even a title-changing provider must not inherit the host console.

use std::{fs, io::Read as _, os::windows::process::CommandExt as _, process::Command};

use windows_sys::Win32::System::Console::{
    GetConsoleCP, GetConsoleProcessList, GetConsoleTitleW, SetConsoleTitleW,
};

use super::{AccountProvider, ProviderKind, ProviderStatus};
use peritus_provider_core::{
    CancellationToken, ProcessExecutable, ProcessLimits, ProcessRequest, ProcessTransport,
    TokioProcessTransport,
    catalog::{AccountCatalog, discover_account_models},
};

const HOST: &str = "PERITUS_TITLE_TEST_HOST";

#[test]
fn status_probes_preserve_host_console() {
    let executable = std::env::current_exe().expect("test executable");
    if std::env::var_os(HOST).is_none() {
        // A separate console makes this deterministic on headless CI and isolates other tests.
        let output = Command::new(&executable)
            .args(["--exact", "account::windows_tests::status_probes_preserve_host_console"])
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
            "@set PERITUS_TITLE_TEST_PARENT={}\r\n@\"{}\" --exact account::windows_tests::title_changing_provider --nocapture >NUL\r\n@if errorlevel 1 exit /b 1\r\n@if \"%~1\"==\"--print\" goto catalog\r\n@echo {{\"loggedIn\":true}}\r\n@exit /b 0\r\n:catalog\r\n@set /p request=\r\n@echo {{\"type\":\"control_response\",\"response\":{{\"subtype\":\"success\",\"request_id\":\"peritus-models\",\"response\":{{\"models\":[{{\"value\":\"fixture-model\"}}]}}}}}}\r\n",
            std::process::id(), executable.display(),
        ),
    )
    .expect("provider fixture");
    // The old unisolated launch must let this exact fixture reach the host console.
    let unisolated = Command::new(&provider_exe)
        .env("PERITUS_TITLE_EXPECT_SHARED", "1")
        .output()
        .expect("negative control");
    assert!(unisolated.status.success(), "{unisolated:?}");
    assert!(change_title("Peritus regression sentinel"));
    let original = current_title();
    for kind in [ProviderKind::ClaudeAccount, ProviderKind::CodexAccount] {
        let mut command = super::status_command(kind, &provider_exe).expect("status command");
        let output = command.output().expect("provider status");
        assert!(output.status.success(), "provider retained console access: {output:?}");
        assert_eq!(super::parse_status(kind, true, &output.stdout), ProviderStatus::Ready);
        assert_eq!(current_title(), original);
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
            ProcessExecutable::pin(&executable).expect("pinned provider"),
            vec![
                "--exact".to_owned(),
                "account::windows_tests::title_changing_provider".to_owned(),
                "--nocapture".to_owned(),
            ],
            std::process::id().to_string().into_bytes(),
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
    assert_eq!(current_title(), original);
}

#[test]
fn title_changing_provider() {
    if std::env::var_os(HOST).is_none() {
        return;
    }
    let parent = std::env::var("PERITUS_TITLE_TEST_PARENT").ok();
    let direct = parent.is_none();
    let parent = parent
        .unwrap_or_else(|| {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).expect("parent process identity");
            input
        })
        .parse::<u32>()
        .expect("parent process ID");
    let shared = std::env::var_os("PERITUS_TITLE_EXPECT_SHARED").is_some();
    assert_eq!(shares_console(parent), shared, "unexpected access to the host console");
    let changed = change_title("claude");
    if shared {
        assert!(changed, "negative control must reproduce the title mutation");
    } else if direct {
        assert_eq!(console_code_page(), 0, "direct background process must have no console");
        assert!(!changed);
    }
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
fn change_title(value: &str) -> bool {
    let title = value.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    // SAFETY: The initialized, zero-terminated buffer remains alive throughout the synchronous call.
    unsafe { SetConsoleTitleW(title.as_ptr()) != 0 }
}

#[allow(unsafe_code, reason = "the native regression observes exact console ownership")]
fn shares_console(parent: u32) -> bool {
    let mut processes = [0_u32; 32];
    // SAFETY: Windows writes at most 32 process IDs into this initialized owned buffer.
    let count = unsafe { GetConsoleProcessList(processes.as_mut_ptr(), 32) };
    assert!(count <= 32, "fixture console process list exceeds its bound");
    assert!(count != 0 || console_code_page() == 0, "console observation failed");
    processes[..count as usize].contains(&parent)
}

#[allow(
    unsafe_code,
    reason = "the native regression observes the host title after child execution"
)]
fn current_title() -> Vec<u16> {
    let mut title = vec![0_u16; 65_536];
    // SAFETY: The initialized writable buffer contains the declared 65,536 UTF-16 units.
    let count = unsafe { GetConsoleTitleW(title.as_mut_ptr(), 65_536) };
    assert!(count > 0, "the fixture sets a nonempty title");
    title.truncate(count as usize);
    title
}
