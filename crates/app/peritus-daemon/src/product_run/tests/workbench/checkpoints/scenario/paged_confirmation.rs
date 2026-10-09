//! End-to-end paged confirmation, stale-snapshot, conflict, and replay coverage.

use super::*;
use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, AppResponseEnvelope,
    CorrelationId, ProtocolContext, ProtocolId, ProtocolVersion, RequestId,
    WorkbenchRewindPageRequest, encode_app_message,
};

pub(super) async fn verify_paged_conflict_and_replay(
    service: &ProductRunService,
    folder: &std::path::Path,
    workspace: WorkspaceId,
    request: WorkbenchRewindRequest,
    revision: u64,
) {
    let limits = constrained_page_limits();
    let first_request = WorkbenchRewindPageRequest::new(request, None);
    let first_envelope = rewind_page_envelope(first_request, 0xe1);
    let AppResponsePayload::WorkbenchRewindPage(first_page) =
        service.rewind_coverage_page(actor(), first_request, &first_envelope, limits)
    else {
        panic!("first rewind coverage page was not returned")
    };
    assert_eq!(first_page.total_paths(), 1);
    let stale_cursor = first_page.next().expect("more full coverage to inspect");
    assert_page_response_fits(&first_envelope, first_page.clone(), limits);

    let changed = b"independent edit after first page\n";
    fs::write(folder.join("note.txt"), changed).expect("mutate covered path after preview");
    let stale_request = WorkbenchRewindPageRequest::new(request, Some(stale_cursor));
    let stale_envelope = rewind_page_envelope(stale_request, 0xe3);
    let stale_page = service.rewind_coverage_page(actor(), stale_request, &stale_envelope, limits);
    assert!(matches!(
        stale_page,
        AppResponsePayload::Error(ref error)
            if error.code() == peritus_app_protocol::AppErrorCode::StaleRevision
    ));

    let stale_command = WorkbenchCommand::new(
        ControlOperationId::new([0xe4; 16]).expect("stale restore operation"),
        query(workspace),
        revision,
        WorkbenchIntent::ConfirmRewind(first_page.confirmation()),
    );
    let before_stale = fs::read(folder.join("note.txt")).expect("stale preimage");
    let before_stale_revision = current_control_revision(service);
    let stale_command_envelope = workbench_command_envelope(&stale_command, 0xe9);
    let stale_result = service
        .apply_paged_workbench_rewind(
            actor(),
            SessionId::new([0xd1; 16]).expect("session"),
            &stale_command,
            &stale_command_envelope,
            limits,
        )
        .await;
    assert!(matches!(
        stale_result,
        AppResponsePayload::Error(ref error)
            if error.code() == peritus_app_protocol::AppErrorCode::StaleRevision
    ));
    assert_eq!(fs::read(folder.join("note.txt")).expect("still-stale preimage"), before_stale);
    assert_eq!(current_control_revision(service), before_stale_revision);

    let current_request = WorkbenchRewindPageRequest::new(request, None);
    let current_envelope = rewind_page_envelope(current_request, 0xe5);
    let AppResponsePayload::WorkbenchRewindPage(current_page) =
        service.rewind_coverage_page(actor(), current_request, &current_envelope, limits)
    else {
        panic!("fresh rewind coverage page was not returned")
    };
    assert_eq!(current_page.paths()[0].disposition(), WorkbenchRewindDisposition::Conflict);
    assert_page_response_fits(&current_envelope, current_page.clone(), limits);

    let next_cursor = current_page.next().expect("coverage continues after path page");
    let next_request = WorkbenchRewindPageRequest::new(request, Some(next_cursor));
    let next_envelope = rewind_page_envelope(next_request, 0xe7);
    let AppResponsePayload::WorkbenchRewindPage(next_page) =
        service.rewind_coverage_page(actor(), next_request, &next_envelope, limits)
    else {
        panic!("next rewind coverage page was not returned")
    };
    assert!(next_page.paths().len() <= limits.codec().max_collection_items);
    assert!(next_page.exclusions().len() <= limits.codec().max_collection_items);
    assert!(next_page.external_effects().len() <= limits.codec().max_collection_items);
    assert_page_response_fits(&next_envelope, next_page, limits);

    let confirm = WorkbenchCommand::new(
        ControlOperationId::new([0xe8; 16]).expect("restore operation"),
        query(workspace),
        revision,
        WorkbenchIntent::ConfirmRewind(current_page.confirmation()),
    );
    service
        .authorize_workbench_request(actor(), &AppRequestPayload::WorkbenchCommand(confirm.clone()))
        .expect("authorize full-checkpoint confirmation");
    let before_conflict = fs::read(folder.join("note.txt")).expect("conflict preimage");
    let confirm_envelope = workbench_command_envelope(&confirm, 0xeb);
    let AppResponsePayload::WorkbenchRestoreSummary(summary) = service
        .apply_paged_workbench_rewind(
            actor(),
            SessionId::new([0xd1; 16]).expect("session"),
            &confirm,
            &confirm_envelope,
            limits,
        )
        .await
    else {
        panic!("durable conflict summary was not returned")
    };
    assert_eq!(summary.status(), WorkbenchRestoreStatus::Conflict);
    assert_eq!(summary.conflicting_paths(), 1);
    assert_eq!(summary.restored_paths(), 0);
    assert_eq!(summary.fingerprint(), current_page.confirmation().preview_digest());
    assert_eq!(
        fs::read(folder.join("note.txt")).expect("conflict leaves bytes untouched"),
        before_conflict
    );

    let later = b"later workspace mutation after conflict settlement\n";
    fs::write(folder.join("note.txt"), later).expect("later unrelated workspace mutation");
    let revision_after_commit = current_control_revision(service);
    let retry_envelope = workbench_command_envelope(&confirm, 0xed);
    let AppResponsePayload::WorkbenchRestoreSummary(replayed) = service
        .apply_paged_workbench_rewind(
            actor(),
            SessionId::new([0xd1; 16]).expect("session"),
            &confirm,
            &retry_envelope,
            limits,
        )
        .await
    else {
        panic!("committed ConfirmRewind replay did not return its original summary")
    };
    assert_eq!(replayed, summary);
    assert_eq!(replayed.restore(), confirm.operation());
    assert_eq!(current_control_revision(service), revision_after_commit);
    assert_eq!(
        fs::read(folder.join("note.txt")).expect("retry leaves later bytes untouched"),
        later
    );

    fs::write(folder.join("note.txt"), b"checkpoint baseline\n")
        .expect("restore checkpoint bytes for a no-op confirmation");
    let no_op_revision = current_control_revision(service);
    let no_op_request =
        WorkbenchRewindRequest::new(query(workspace), no_op_revision, request.checkpoint())
            .expect("no-op rewind request");
    let no_op_page_request = WorkbenchRewindPageRequest::new(no_op_request, None);
    let no_op_page_envelope = rewind_page_envelope(no_op_page_request, 0xf0);
    let AppResponsePayload::WorkbenchRewindPage(no_op_page) =
        service.rewind_coverage_page(actor(), no_op_page_request, &no_op_page_envelope, limits)
    else {
        panic!("no-op rewind coverage page was not returned")
    };
    assert_eq!(no_op_page.paths()[0].disposition(), WorkbenchRewindDisposition::Unchanged);

    let no_op_confirm = WorkbenchCommand::new(
        ControlOperationId::new([0xf2; 16]).expect("no-op restore operation"),
        query(workspace),
        no_op_revision,
        WorkbenchIntent::ConfirmRewind(no_op_page.confirmation()),
    );
    service
        .authorize_workbench_request(
            actor(),
            &AppRequestPayload::WorkbenchCommand(no_op_confirm.clone()),
        )
        .expect("authorize no-op confirmation");
    crate::product_run::workbench::inject_rewind_fault(
        no_op_confirm.operation().into_bytes(),
        crate::product_run::workbench::RewindFaultPoint::AfterPrepare,
    );
    let interrupted_envelope = workbench_command_envelope(&no_op_confirm, 0xf3);
    assert!(matches!(
        service
            .apply_paged_workbench_rewind(
                actor(),
                SessionId::new([0xd1; 16]).expect("session"),
                &no_op_confirm,
                &interrupted_envelope,
                limits,
            )
            .await,
        AppResponsePayload::Error(_)
    ));
    assert_eq!(
        fs::read(folder.join("note.txt")).expect("interrupted no-op leaves preimage"),
        b"checkpoint baseline\n"
    );
    let no_op_envelope = workbench_command_envelope(&no_op_confirm, 0xf5);
    let AppResponsePayload::WorkbenchRestoreSummary(no_op_summary) = service
        .apply_paged_workbench_rewind(
            actor(),
            SessionId::new([0xd1; 16]).expect("session"),
            &no_op_confirm,
            &no_op_envelope,
            limits,
        )
        .await
    else {
        panic!("durable no-op summary was not returned")
    };
    assert_eq!(no_op_summary.status(), WorkbenchRestoreStatus::Applied);
    assert_eq!(no_op_summary.restored_paths(), 0);
    assert_eq!(no_op_summary.conflicting_paths(), 0);
    assert_eq!(
        no_op_summary.fingerprint(),
        no_op_page.confirmation().preview_digest(),
        "the summary retains the full confirmation binding"
    );

    let later_after_no_op = b"later mutation after no-op settlement\n";
    fs::write(folder.join("note.txt"), later_after_no_op)
        .expect("mutate workspace after no-op settlement");
    let no_op_revision_after_commit = current_control_revision(service);
    let no_op_retry_envelope = workbench_command_envelope(&no_op_confirm, 0xf7);
    let AppResponsePayload::WorkbenchRestoreSummary(no_op_replayed) = service
        .apply_paged_workbench_rewind(
            actor(),
            SessionId::new([0xd1; 16]).expect("session"),
            &no_op_confirm,
            &no_op_retry_envelope,
            limits,
        )
        .await
    else {
        panic!("committed no-op ConfirmRewind replay failed")
    };
    assert_eq!(no_op_replayed, no_op_summary);
    assert_eq!(no_op_replayed.restore(), no_op_confirm.operation());
    assert_eq!(current_control_revision(service), no_op_revision_after_commit);
    assert_eq!(
        fs::read(folder.join("note.txt")).expect("retry leaves post-settlement bytes untouched"),
        later_after_no_op
    );
}

fn constrained_page_limits() -> AppProtocolLimits {
    AppProtocolLimits::new(
        peritus_codec::CodecLimits::new(65_536, 60_000, 1, 60_000, 60_000, 32),
        1,
        1,
        1,
        1,
        1,
        1,
        1,
        1,
        1,
        1,
    )
    .expect("bounded test protocol limits")
}

fn rewind_page_envelope(page_request: WorkbenchRewindPageRequest, seed: u8) -> AppRequestEnvelope {
    let context = test_context();
    AppRequestEnvelope::new(
        context,
        RequestId::new([seed; 16]).expect("request id"),
        CorrelationId::new([seed + 1; 16]).expect("correlation id"),
        AppRequestPayload::QueryWorkbenchRewindPage(page_request),
    )
    .expect("page request envelope")
}

fn workbench_command_envelope(command: &WorkbenchCommand, seed: u8) -> AppRequestEnvelope {
    AppRequestEnvelope::new(
        test_context(),
        RequestId::new([seed; 16]).expect("request id"),
        CorrelationId::new([seed + 1; 16]).expect("correlation id"),
        AppRequestPayload::WorkbenchCommand(command.clone()),
    )
    .expect("workbench command envelope")
}

fn test_context() -> ProtocolContext {
    ProtocolContext::new(
        ProtocolId::new([0xd0; 16]).expect("protocol"),
        ProtocolVersion::new(1, 0).expect("version"),
        SessionId::new([0xd1; 16]).expect("session"),
    )
}

fn assert_page_response_fits(
    request: &AppRequestEnvelope,
    page: peritus_app_protocol::WorkbenchRewindCoveragePage,
    limits: AppProtocolLimits,
) {
    let response = AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        AppResponsePayload::WorkbenchRewindPage(page),
    );
    let encoded = encode_app_message(&AppMessage::Response(response), limits)
        .expect("page response fits the negotiated frame");
    assert!(encoded.len() <= limits.codec().max_frame_bytes);
}

fn current_control_revision(service: &ProductRunService) -> u64 {
    service
        .with_controls(false, |store| store.load(DomainConversationId::new([2; 16])?))
        .expect("load control record")
        .expect("control record")
        .revision()
}
