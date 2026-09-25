use super::*;

#[test]
fn assigned_error_tags_round_trip() {
    for code in [
        AppErrorCode::UnsupportedFormat,
        AppErrorCode::IdempotencyConflict,
        AppErrorCode::ArtifactDigest,
        AppErrorCode::Internal,
    ] {
        assert_eq!(AppErrorCode::from_tag(code.tag()), Some(code));
    }
    assert_eq!(AppErrorCode::from_tag(0), None);
}

#[test]
fn default_retry_matches_each_recovery_shape() {
    assert_eq!(AppErrorCode::MalformedFrame.default_retry(), RetryDisposition::NewRequest);
    assert_eq!(AppErrorCode::Internal.default_retry(), RetryDisposition::AfterRecovery);
    assert_eq!(AppErrorCode::UnsupportedSchema.default_retry(), RetryDisposition::Reconnect);
    assert_eq!(AppErrorCode::Cancelled.default_retry(), RetryDisposition::NewRequest);
}

#[test]
fn runtime_subsystem_tags_round_trip() {
    for subsystem in [ResponsibleSubsystem::Provider, ResponsibleSubsystem::Workspace] {
        assert_eq!(ResponsibleSubsystem::from_tag(subsystem.tag()), Some(subsystem));
    }
}
