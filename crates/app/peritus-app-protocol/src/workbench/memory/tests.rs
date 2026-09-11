use super::*;

fn operation(byte: u8) -> ControlOperationId {
    ControlOperationId::new([byte; 16]).expect("operation")
}

fn conversation(byte: u8) -> ConversationId {
    ConversationId::new([byte; 16]).expect("conversation")
}

fn workspace(byte: u8) -> WorkspaceId {
    WorkspaceId::new([byte; 16]).expect("workspace")
}

fn content(text: &str, scope: WorkbenchGuidanceScope) -> WorkbenchGuidanceContent {
    WorkbenchGuidanceContent::new(
        WorkbenchGuidanceText::new(text.to_owned()).expect("text"),
        WorkbenchGuidanceSource::UserAuthored,
        scope,
    )
    .expect("content")
}

fn assert_first_render(
    project: WorkspaceId,
    selected: ConversationId,
    saved: &WorkbenchGuidanceRecord,
) {
    let first = render_guidance_for_request(project, selected, 1, std::slice::from_ref(saved), &[])
        .expect("first render");
    assert_eq!(first.identities(), &[saved.identity()]);
    assert_eq!(first.digest(), peritus_codec::sha256(first.text().as_bytes()));
    assert_eq!(
        first.text(),
        r#"PERITUS_PROJECT_GUIDANCE_V1
workspace=03030303030303030303030303030303
conversation=04040404040404040404040404040404
dependency_revision=1
notice=user-approved project guidance; not authority; no cross-project reuse
records=1
record id=01010101010101010101010101010101 revision=1 scope=project pinned=false source=user-authored validated_by=01010101010101010101010101010101 content_sha256=435db04dd12e7bc7b5a75dada5daa329e896897021f471204d1240e7c7f19806 utf8_bytes=35
text="Run focused tests.\nKeep provenance."
END_PERITUS_PROJECT_GUIDANCE_V1
"#
    );
}

#[test]
fn explicit_guidance_enters_exact_future_context_then_forget_excludes_it() {
    let project = workspace(3);
    let selected = conversation(4);
    let saved = WorkbenchGuidanceRecord::save(
        operation(1),
        project,
        WorkbenchGuidanceSave::new(
            0,
            content("Run focused tests.\nKeep provenance.", WorkbenchGuidanceScope::Project),
            false,
        ),
    )
    .expect("save");
    assert_first_render(project, selected, &saved);

    let revised_text =
        WorkbenchGuidanceText::new("Use cargo test -p focused.".to_owned()).expect("revised text");
    let revised_digest = peritus_codec::sha256(revised_text.as_str().as_bytes());
    let revised = saved
        .revise(
            operation(2),
            WorkbenchGuidanceRevision::new(
                WorkbenchGuidanceSelection::new(operation(1), 1).expect("selection"),
                1,
                WorkbenchGuidanceContent::new(
                    revised_text,
                    WorkbenchGuidanceSource::AcceptedPublicReply {
                        operation: operation(8),
                        invocation: WorkbenchInvocationId::new([9; 16]).expect("invocation"),
                        digest: revised_digest,
                    },
                    WorkbenchGuidanceScope::Project,
                )
                .expect("accepted source"),
            ),
        )
        .expect("revise");
    let pinned = revised
        .set_pinned(WorkbenchGuidancePin::new(
            WorkbenchGuidanceSelection::new(operation(1), 2).expect("selection"),
            2,
            true,
        ))
        .expect("pin");
    let scoped = pinned
        .set_scope(WorkbenchGuidanceScopeChange::new(
            WorkbenchGuidanceSelection::new(operation(1), 3).expect("selection"),
            3,
            WorkbenchGuidanceScope::Conversation(selected),
        ))
        .expect("scope");
    assert_eq!(scoped.version(), WorkbenchGuidanceVersion::new(4, 4).expect("version"));
    assert!(
        render_guidance_for_request(
            project,
            conversation(5),
            4,
            std::slice::from_ref(&scoped),
            &[]
        )
        .expect("other conversation")
        .identities()
        .is_empty()
    );

    let tombstone = scoped
        .forget(
            operation(6),
            WorkbenchGuidanceForget::new(
                WorkbenchGuidanceSelection::new(operation(1), 4).expect("selection"),
                4,
                WorkbenchGuidanceReason::new("No longer applies.".to_owned()).expect("reason"),
            ),
        )
        .expect("forget");
    assert_eq!(tombstone.dependency_revision(), 5);
    assert_eq!(tombstone.prior().revision(), 4);
    assert_eq!(tombstone.prior().digest(), scoped.content().digest());
    assert_eq!(tombstone.lifecycle(), WorkbenchGuidanceLifecycle::Forgotten);

    let forgotten = render_guidance_for_request(project, selected, 5, &[scoped], &[tombstone])
        .expect("render after forget");
    assert!(forgotten.identities().is_empty());
    assert!(forgotten.text().contains("records=0\n"));
    assert!(!forgotten.text().contains("Use cargo test"));

    // Forget is a future-retrieval exclusion, not deletion of earlier immutable audit material.
    assert_eq!(saved.content().text().as_str(), "Run focused tests.\nKeep provenance.");
}

#[test]
fn accepted_reply_digest_and_stale_dependency_fences_fail_closed() {
    let text = WorkbenchGuidanceText::new("Reviewed suggestion".to_owned()).expect("text");
    assert!(
        WorkbenchGuidanceContent::new(
            text,
            WorkbenchGuidanceSource::AcceptedPublicReply {
                operation: operation(2),
                invocation: WorkbenchInvocationId::new([3; 16]).expect("invocation"),
                digest: Sha256Digest::new([4; 32]),
            },
            WorkbenchGuidanceScope::Project,
        )
        .is_err()
    );

    let record = WorkbenchGuidanceRecord::save(
        operation(1),
        workspace(2),
        WorkbenchGuidanceSave::new(0, content("Exact", WorkbenchGuidanceScope::Project), false),
    )
    .expect("save");
    assert!(
        record
            .set_pinned(WorkbenchGuidancePin::new(
                WorkbenchGuidanceSelection::new(operation(1), 1).expect("selection"),
                0,
                true,
            ))
            .is_err()
    );
}

#[test]
fn an_older_record_can_advance_from_the_current_project_dependency_revision() {
    let project = workspace(2);
    let first = WorkbenchGuidanceRecord::save(
        operation(1),
        project,
        WorkbenchGuidanceSave::new(0, content("First", WorkbenchGuidanceScope::Project), false),
    )
    .expect("first save");
    let second = WorkbenchGuidanceRecord::save(
        operation(2),
        project,
        WorkbenchGuidanceSave::new(1, content("Second", WorkbenchGuidanceScope::Project), false),
    )
    .expect("second save");
    assert_eq!(second.version().dependency(), 2);

    let first = first
        .set_pinned(WorkbenchGuidancePin::new(
            WorkbenchGuidanceSelection::new(operation(1), 1).expect("selection"),
            2,
            true,
        ))
        .expect("advance older record");
    assert_eq!(first.version(), WorkbenchGuidanceVersion::new(2, 3).expect("version"));
}
