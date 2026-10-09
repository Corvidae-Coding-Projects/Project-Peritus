use super::*;

fn query() -> WorkbenchQuery {
    WorkbenchQuery::new(
        crate::ConversationId::new([0x21; 16]).expect("conversation id"),
        peritus_types::WorkspaceId::new([0x22; 16]).expect("workspace id"),
    )
}

fn version(seed: u8) -> WorkbenchCheckpointVersion {
    WorkbenchCheckpointVersion::Present {
        digest: Sha256Digest::new([seed; 32]),
        bytes: 4,
        mode: WorkbenchCheckpointFileMode::Regular,
    }
}

fn receipt(
    paths: Vec<WorkbenchCheckpointPath>,
    exclusions: Vec<String>,
) -> WorkbenchCheckpointReceipt {
    WorkbenchCheckpointReceipt::new(
        ControlOperationId::new([0x23; 16]).expect("checkpoint id"),
        query(),
        3,
        WorkbenchCheckpointName::new("test checkpoint".to_owned()).expect("name"),
        WorkbenchCheckpointReferences::new(2, 1, 0, None),
        paths,
        exclusions,
        vec!["external state is excluded".to_owned()],
    )
    .expect("receipt")
}

fn path(name: String, current: u8, owned: Option<u8>) -> WorkbenchCheckpointPath {
    WorkbenchCheckpointPath::new(name, version(current), owned.map(version))
        .expect("checkpoint path")
}

fn preview(
    request: WorkbenchRewindRequest,
    receipt: &WorkbenchCheckpointReceipt,
    observed: WorkbenchCheckpointVersion,
    disposition: WorkbenchRewindDisposition,
) -> WorkbenchRewindPreview {
    let paths = receipt
        .paths()
        .iter()
        .map(|stored| {
            WorkbenchRewindPath::new(
                stored.path().to_owned(),
                stored.checkpoint(),
                stored.expected_current(),
                observed,
                disposition,
            )
            .expect("rewind path")
        })
        .collect();
    WorkbenchRewindPreview::new(
        request,
        paths,
        receipt.exclusions().to_vec(),
        receipt.external_effects().to_vec(),
    )
    .expect("rewind preview")
}

#[test]
fn checkpoint_coverage_accepts_more_than_u16_paths_and_pages_share_fingerprint() {
    let paths = (0..=u16::MAX)
        .map(|index| path(format!("src/file-{index:05}"), 0x31, Some(0x32)))
        .collect::<Vec<_>>();
    let receipt = receipt(paths.clone(), Vec::new());
    let fingerprint = receipt.page_fingerprint(5).expect("full coverage fingerprint");
    let next = Some(WorkbenchCoverageCursor::new(WorkbenchCoverageSection::Paths, 1, fingerprint));
    let first = WorkbenchCheckpointCoveragePage::new(
        &receipt,
        5,
        fingerprint,
        u64::from(u16::MAX) + 1,
        0,
        1,
        WorkbenchCoverageSection::Paths,
        0,
        vec![paths[0].clone()],
        Vec::new(),
        Vec::new(),
        next,
    )
    .expect("first bounded page");
    let second = WorkbenchCheckpointCoveragePage::new(
        &receipt,
        5,
        fingerprint,
        u64::from(u16::MAX) + 1,
        0,
        1,
        WorkbenchCoverageSection::Paths,
        1,
        vec![paths[1].clone()],
        Vec::new(),
        Vec::new(),
        Some(WorkbenchCoverageCursor::new(WorkbenchCoverageSection::Paths, 2, fingerprint)),
    )
    .expect("second bounded page");
    assert_eq!(first.fingerprint(), second.fingerprint());
    assert_eq!(first.fingerprint(), fingerprint);
    assert_eq!(first.total_paths(), u64::from(u16::MAX) + 1);
}

#[test]
fn compact_confirmation_changes_with_any_retained_manifest_fact() {
    let query = query();
    let checkpoint = ControlOperationId::new([0x23; 16]).expect("checkpoint id");
    let request = WorkbenchRewindRequest::new(query, 8, checkpoint).expect("request");
    let baseline = receipt(
        vec![path("src/a".to_owned(), 0x31, Some(0x32))],
        vec!["excluded: partial selection".to_owned()],
    );
    let live = preview(request, &baseline, version(0x32), WorkbenchRewindDisposition::Restore);
    let confirmation = WorkbenchRewindConfirmation::for_preview(request, &baseline, &live)
        .expect("full checkpoint confirmation");
    let changed = receipt(
        vec![path("src/a".to_owned(), 0x31, Some(0x33))],
        vec!["excluded: partial selection".to_owned()],
    );
    let changed_live =
        preview(request, &changed, version(0x33), WorkbenchRewindDisposition::Restore);
    let changed_confirmation =
        WorkbenchRewindConfirmation::for_preview(request, &changed, &changed_live)
            .expect("changed manifest confirmation");
    assert_ne!(confirmation, changed_confirmation);

    let altered_live =
        preview(request, &baseline, version(0x34), WorkbenchRewindDisposition::Conflict);
    assert_ne!(
        confirmation,
        WorkbenchRewindConfirmation::for_preview(request, &baseline, &altered_live)
            .expect("changed live preimage confirmation")
    );
    let stale = WorkbenchRewindRequest::new(query, 2, checkpoint).expect("stale request");
    assert!(WorkbenchRewindConfirmation::for_preview(stale, &baseline, &live).is_err());
    let other_query = WorkbenchQuery::new(
        crate::ConversationId::new([0x24; 16]).expect("other conversation"),
        peritus_types::WorkspaceId::new([0x22; 16]).expect("workspace id"),
    );
    let mismatched =
        WorkbenchRewindRequest::new(other_query, 8, checkpoint).expect("mismatched request");
    assert!(WorkbenchRewindConfirmation::for_preview(mismatched, &baseline, &live).is_err());
}

#[test]
fn empty_manifest_and_conversation_only_preview_have_terminal_pages() {
    let query = query();
    let checkpoint = ControlOperationId::new([0x23; 16]).expect("checkpoint id");
    let receipt = WorkbenchCheckpointReceipt::new(
        checkpoint,
        query,
        3,
        WorkbenchCheckpointName::new("empty checkpoint".to_owned()).expect("name"),
        WorkbenchCheckpointReferences::new(2, 1, 0, None),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect("empty checkpoint receipt");
    let fingerprint = receipt.page_fingerprint(3).expect("page fingerprint");
    let checkpoint_page = WorkbenchCheckpointCoveragePage::new(
        &receipt,
        3,
        fingerprint,
        0,
        0,
        0,
        WorkbenchCoverageSection::Paths,
        0,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
    )
    .expect("empty checkpoint terminal page");
    assert_eq!(checkpoint_page.section(), WorkbenchCoverageSection::Paths);
    assert!(checkpoint_page.paths().is_empty());
    assert!(checkpoint_page.next().is_none());

    let request = WorkbenchRewindRequest::new(query, 3, checkpoint)
        .expect("conversation-only rewind request");
    let preview = WorkbenchRewindPreview::new(request, Vec::new(), Vec::new(), Vec::new())
        .expect("conversation-only preview");
    let confirmation = WorkbenchRewindConfirmation::for_preview(request, &receipt, &preview)
        .expect("full-manifest confirmation");
    let rewind_page = WorkbenchRewindCoveragePage::new(
        confirmation,
        0,
        0,
        0,
        WorkbenchCoverageSection::Paths,
        0,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
    )
    .expect("empty rewind terminal page");
    assert_eq!(rewind_page.total_paths(), 0);
    assert!(rewind_page.paths().is_empty());
    assert!(rewind_page.next().is_none());
}
