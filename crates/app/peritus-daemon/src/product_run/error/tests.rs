use super::*;

#[test]
fn unavailable_run_state_is_not_mislabeled_as_subscription_backpressure() {
    let AppResponsePayload::Error(error) = ProductRunServiceError::Unavailable.response() else {
        panic!("error response");
    };
    assert_eq!(error.code(), AppErrorCode::Internal);
    assert_eq!(error.subsystem(), ResponsibleSubsystem::Daemon);
    assert_eq!(error.retry(), RetryDisposition::AfterRecovery);
    assert!(error.diagnostic().unwrap().as_str().contains("daemon log"));
}

#[test]
fn persistence_error_keeps_operation_cause_and_recovery_action() {
    let cause = std::io::Error::from_raw_os_error(13);
    let cause_text = cause.to_string();
    let error =
        ProductRunServiceError::persistence("replace the durable product-run record", cause);
    let message = error.describe();
    assert!(message.contains("replace the durable product-run record"));
    assert!(message.contains(&cause_text));
    assert!(message.contains("Restore write access"));
}

#[test]
fn provider_and_workspace_failures_keep_distinct_public_ownership() {
    for (failure, subsystem) in [
        (ProductRunServiceError::ProviderUnavailable, ResponsibleSubsystem::Provider),
        (ProductRunServiceError::WorkspaceUnavailable, ResponsibleSubsystem::Workspace),
    ] {
        let AppResponsePayload::Error(error) = failure.response() else {
            panic!("error response");
        };
        assert_eq!(error.code(), AppErrorCode::InvalidIdentifier);
        assert_eq!(error.retry(), RetryDisposition::NewRequest);
        assert_eq!(error.subsystem(), subsystem);
    }
}

#[test]
fn product_prerequisite_failures_keep_distinct_public_ownership() {
    for (failure, subsystem, diagnostic) in [
        (
            ProductRunServiceError::GitRequired,
            ResponsibleSubsystem::Workspace,
            "requires a Git workspace",
        ),
        (
            ProductRunServiceError::EffortUnsupported,
            ResponsibleSubsystem::Provider,
            "unsupported by this provider",
        ),
    ] {
        let AppResponsePayload::Error(error) = failure.response() else {
            panic!("error response");
        };
        assert_eq!(error.code(), AppErrorCode::MissingRequiredFeature);
        assert_eq!(error.retry(), RetryDisposition::NewRequest);
        assert_eq!(error.subsystem(), subsystem);
        assert!(error.diagnostic().unwrap().as_str().contains(diagnostic));
    }
}

#[test]
fn invalid_provider_output_is_not_mislabeled_as_command_input() {
    let error = ProductRunServiceError::invalid_provider_output(
        "decode streamed assistant text",
        "source decoder rejected invalid bytes",
    );
    let AppResponsePayload::Error(error) = error.response() else {
        panic!("error response");
    };
    assert_eq!(error.code(), AppErrorCode::MalformedFrame);
    assert_eq!(error.retry(), RetryDisposition::AfterRecovery);
    assert_eq!(error.subsystem(), ResponsibleSubsystem::Provider);
    assert!(error.diagnostic().unwrap().as_str().contains("invalid UTF-8"));
}

#[test]
fn unsupported_control_schema_requires_a_new_protocol_relationship() {
    let AppResponsePayload::Error(error) = ProductRunServiceError::Control(
        peritus_product_runner::control::ControlError::UnsupportedSchema,
    )
    .response() else {
        panic!("error response");
    };
    assert_eq!(error.code(), AppErrorCode::UnsupportedSchema);
    assert_eq!(error.retry(), RetryDisposition::Reconnect);
    assert_eq!(error.subsystem(), ResponsibleSubsystem::Negotiation);
}
