//! Redacted operating-system failure at the pinned executable boundary.

use super::{
    CancellationToken, ProcessExecutable, ProcessLimits, ProcessRequest, ProcessTransport,
    ProviderCoreErrorKind, TokioProcessTransport, runtime,
};

#[test]
fn spawn_error_retains_redacted_os_cause_after_pinned_path_disappears() {
    runtime::block_on(async {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("peritus-provider-secret-canary");
        std::fs::write(&path, b"removed before execution").expect("fixture file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .expect("fixture executable permissions");
        }
        let executable = ProcessExecutable::pin(&path).expect("pinned fixture");
        std::fs::remove_file(&path).expect("remove pinned file");
        let request = ProcessRequest::new(
            executable,
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
            ProcessLimits::PRODUCTION,
        )
        .expect("request");
        let error = TokioProcessTransport
            .run(request, &CancellationToken::new())
            .await
            .expect_err("missing executable cannot start");
        assert_eq!(error.kind(), ProviderCoreErrorKind::Connect);
        assert_eq!(error.operation(), "process_spawn");
        assert_eq!(error.io_kind(), Some(std::io::ErrorKind::NotFound));
        assert!(error.raw_os_error().is_some());
        let source = std::error::Error::source(&error).expect("redacted OS cause");
        assert!(!format!("{error:?} {error} {source}").contains("peritus-provider-secret-canary"));
    });
}
