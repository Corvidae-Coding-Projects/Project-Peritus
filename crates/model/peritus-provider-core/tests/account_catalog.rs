//! Official runtime protocol fixtures must receive metadata requests only.

#![cfg(unix)]
#[path = "support/runtime.rs"]
mod runtime;

use peritus_provider_core::{
    CancellationToken,
    catalog::{AccountCatalog, discover_account_models},
};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

// Concurrent fork/exec can temporarily inherit another fixture's writable executable descriptor
// before close-on-exec runs, producing ETXTBSY. Publish and reap these tiny scripts serially.
static EXECUTABLE_PUBLICATION: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn executable(directory: &tempfile::TempDir, script: &str) -> PathBuf {
    let path = directory.path().join("official-runtime-fixture");
    fs::write(&path, script).expect("fixture executable");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("executable permission");
    path
}

#[test]
fn codex_metadata_initializes_and_lists_without_starting_a_thread_or_turn() {
    let _publication = EXECUTABLE_PUBLICATION.lock().expect("fixture publication");
    runtime::block_on(async {
        let directory = tempfile::tempdir().expect("fixture");
        let path = executable(
            &directory,
            r#"#!/bin/sh
set -eu
test "$1" = app-server
IFS= read -r line
case "$line" in *'"method":"initialize"'*) ;; *) exit 21 ;; esac
printf '%s\n' '{"id":1,"result":{}}'
IFS= read -r line
case "$line" in *'"method":"initialized"'*) ;; *) exit 22 ;; esac
IFS= read -r line
case "$line" in *'"method":"model/list"'*) ;; *) exit 23 ;; esac
printf '%s\n' '{"id":2,"result":{"data":[{"id":"route-id","model":"actual-model","displayName":"Account choice"}],"nextCursor":"page-two"}}'
IFS= read -r line
case "$line" in *'"cursor":"page-two"'*'"method":"model/list"'*|*'"method":"model/list"'*'"cursor":"page-two"'*) ;; *) exit 24 ;; esac
printf '%s\n' '{"id":3,"result":{"data":[],"nextCursor":null}}'
"#,
        );
        let models =
            discover_account_models(&path, AccountCatalog::Codex, &CancellationToken::new())
                .await
                .expect("catalog");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id.as_str(), "actual-model");
        assert_eq!(models[0].label, "Account choice");
    });
}

#[test]
fn claude_metadata_uses_control_initialization_without_a_user_prompt() {
    let _publication = EXECUTABLE_PUBLICATION.lock().expect("fixture publication");
    runtime::block_on(async {
        let directory = tempfile::tempdir().expect("fixture");
        let path = executable(
            &directory,
            r#"#!/bin/sh
set -eu
case " $* " in *" --no-session-persistence "*) ;; *) exit 21 ;; esac
case " $* " in *"disableAllHooks"*) ;; *) exit 22 ;; esac
test "$CLAUDE_CODE_DISABLE_TERMINAL_TITLE" = 1
IFS= read -r line
case "$line" in *'"type":"control_request"'*'"subtype":"initialize"'*|*'"subtype":"initialize"'*'"type":"control_request"'*) ;; *) exit 23 ;; esac
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"peritus-models","response":{"models":[{"value":"provider-alias","displayName":"Advertised alias"}]}}}'
"#,
        );
        let models =
            discover_account_models(&path, AccountCatalog::Claude, &CancellationToken::new())
                .await
                .expect("catalog");
        assert_eq!(models[0].id.as_str(), "provider-alias");
        assert_eq!(models[0].tools, None);
    });
}

#[test]
fn unavailable_runtime_and_presend_cancellation_do_not_invent_models() {
    let _publication = EXECUTABLE_PUBLICATION.lock().expect("fixture publication");
    runtime::block_on(async {
        let directory = tempfile::tempdir().expect("fixture");
        let path =
            executable(&directory, "#!/bin/sh\nprintf '%s\\n' 'secret-invalid-runtime-output'\n");
        let error =
            discover_account_models(&path, AccountCatalog::Codex, &CancellationToken::new())
                .await
                .expect_err("invalid runtime");
        assert!(!error.to_string().contains("secret-invalid-runtime-output"));
        let cancellation = CancellationToken::new();
        let _ = cancellation.cancel();
        assert!(
            discover_account_models(
                &directory.path().join("must-not-run"),
                AccountCatalog::Claude,
                &cancellation
            )
            .await
            .is_err()
        );
    });
}
