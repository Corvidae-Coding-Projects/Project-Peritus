//! Real files, immutable chunk objects, independent preimages, and restart replay.
use super::*;
fn request() -> InitDiscoveryRequest {
    super::super::tests::request()
}
fn selection() -> InitArtifactDiscovery {
    InitArtifactDiscovery::new(request(), None, None, None).expect("selection")
}
fn review(root: &Path, proposal: InitArtifactProposal, step: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut offset = 0;
    loop {
        let page_request =
            InitArtifactPageRequest::new(proposal, offset, step).expect("page request");
        let page = init_artifact_page(root, page_request).expect("page");
        bytes.extend_from_slice(page.bytes());
        if let Some(next) = page.next() {
            offset = next;
        } else {
            break;
        }
    }
    assert_eq!(bytes.len() as u64, proposal.review().bytes());
    assert_eq!(peritus_codec::sha256(&bytes), proposal.review().digest());
    bytes
}
#[test]
fn large_sources_and_diff_use_exact_artifacts_pages_and_existing_patch_apply() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let transactions = tempfile::tempdir().expect("transactions");
    let original = "Preserve this exact guidance and Unicode 🪶.\r\n".repeat(18_001);
    assert!(original.len() > 768 * 1024);
    fs::write(root.path().join("AGENTS.md"), &original).expect("instructions");
    fs::write(root.path().join("README.md"), vec![b'x'; 2 * 1024 * 1024 + 17])
        .expect("large documentation");
    fs::write(root.path().join("package.json"), b"{ invalid json").expect("parse failure");
    let proposal =
        discover_init_artifacts(root.path(), state.path(), &selection()).expect("capture");
    assert!(proposal.review().bytes() > 1024 * 1024);
    let text = String::from_utf8(review(state.path(), proposal, 32 * 1024)).expect("review UTF8");
    assert!(text.contains("package.json: manifest could not be parsed"));
    assert_eq!(fs::read_to_string(root.path().join("AGENTS.md")).expect("unchanged"), original);
    // A fresh open of the archived manifest is enough after restart; no process-local proposal is needed.
    let patch = prepare_init_artifact_patch(
        root.path(),
        state.path(),
        request().query().workspace(),
        Generation::first(),
        RevisionNumber::first(),
        proposal,
    )
    .expect("exact patch");
    let plan = patch
        .plan(request().query().workspace(), Generation::first(), RevisionNumber::first())
        .expect("plan");
    peritus_patch::apply_patch(root.path(), transactions.path(), &plan).expect("apply");
    let applied = fs::read_to_string(root.path().join("AGENTS.md")).expect("applied");
    assert!(applied.starts_with(&original));
    assert!(applied.contains(peritus_app_protocol::INIT_SECTION_START));
}
#[test]
fn explicit_nested_source_and_command_toggle_bind_consent_and_reject_drift() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    fs::create_dir(root.path().join("tools")).expect("tools");
    fs::write(
        root.path().join("tools/package.json"),
        br#"{"scripts":{"custom":"touch should-not-execute","test":"echo test"}}"#,
    )
    .expect("source");
    let initial =
        discover_init_artifacts(root.path(), state.path(), &selection()).expect("initial");
    let source = peritus_app_protocol::InitSourceSelection::new(
        "tools/package.json".to_owned(),
        InitSourceKind::Manifest,
    )
    .expect("source");
    let extended = discover_init_artifacts(
        root.path(),
        state.path(),
        &InitArtifactDiscovery::new(request(), Some(initial.manifest()), Some(source), None)
            .expect("selection"),
    )
    .expect("extended");
    let text = String::from_utf8(review(state.path(), extended, 137)).expect("text");
    assert!(text.contains("npm --prefix tools run custom"));
    assert!(!root.path().join("should-not-execute").exists());
    let toggled = discover_init_artifacts(
        root.path(),
        state.path(),
        &InitArtifactDiscovery::new(request(), Some(extended.manifest()), None, Some(0))
            .expect("toggle"),
    )
    .expect("toggled");
    assert_ne!(toggled.manifest(), extended.manifest());
    assert!(
        String::from_utf8(review(state.path(), toggled, 307))
            .expect("text")
            .contains("1 [excluded]")
    );
    fs::write(
        root.path().join("tools/package.json"),
        br#"{"scripts":{"different":"echo changed"}}"#,
    )
    .expect("drift");
    assert_eq!(
        prepare_init_artifact_patch(
            root.path(),
            state.path(),
            request().query().workspace(),
            Generation::first(),
            RevisionNumber::first(),
            toggled
        )
        .expect_err("changed source")
        .code(),
        AppErrorCode::StaleRevision
    );
    assert!(!root.path().join("AGENTS.md").exists());
}
#[test]
fn references_cannot_swap_review_or_scope_and_corrupt_chunk_fails_closed() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let proposal =
        discover_init_artifacts(root.path(), state.path(), &selection()).expect("proposal");
    let swapped = InitArtifactProposal::new(
        proposal.request(),
        proposal.manifest(),
        InitContentReference::new(Sha256Digest::new([9; 32]), proposal.review().bytes()),
    );
    assert!(
        init_artifact_page(
            state.path(),
            InitArtifactPageRequest::new(swapped, 0, 17).expect("request")
        )
        .is_err()
    );
    let manifest = load_manifest(
        &config(state.path()).expect("config"),
        proposal.request(),
        proposal.manifest(),
    )
    .expect("manifest");
    let chunk = &manifest.review.chunks[0];
    let digest = ArtifactDigest::from_sha256(Sha256Digest::new(chunk.digest));
    let archive = Archive::open(state.path()).expect("archive");
    let metadata = archive.store.reopen_finalized(digest).expect("retained object");
    let _ = metadata;
    // Corrupt the exact store object discovered by its digest-derived storage path.
    let files = object_files(&state.path().join("objects"));
    let file = files
        .into_iter()
        .find(|file| {
            fs::read(file)
                .is_ok_and(|bytes| peritus_codec::sha256(&bytes).into_bytes() == chunk.digest)
        })
        .expect("chunk file");
    fs::write(file, b"corrupt").expect("corrupt fixture chunk");
    assert!(
        init_artifact_page(
            state.path(),
            InitArtifactPageRequest::new(proposal, 0, 17).expect("request")
        )
        .is_err()
    );
}
fn object_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root).expect("directory") {
        let entry = entry.expect("entry");
        if entry.file_type().expect("type").is_dir() {
            files.extend(object_files(&entry.path()));
        } else {
            files.push(entry.path());
        }
    }
    files
}

fn discover_init_artifacts(
    root: &Path,
    archive: &Path,
    request: &InitArtifactDiscovery,
) -> Result<InitArtifactProposal, AppProtocolError> {
    discover_init_artifacts_checked(root, archive, request, &|_| true)
}
fn init_artifact_page(
    archive: &Path,
    request: InitArtifactPageRequest,
) -> Result<InitArtifactPage, AppProtocolError> {
    init_artifact_page_checked(archive, request, &|_| true)
}
fn prepare_init_artifact_patch(
    root: &Path,
    archive: &Path,
    workspace: WorkspaceId,
    generation: Generation,
    revision: RevisionNumber,
    proposal: InitArtifactProposal,
) -> Result<PatchSet, AppProtocolError> {
    prepare_init_artifact_patch_checked(
        root,
        archive,
        workspace,
        generation,
        revision,
        proposal,
        &|_| true,
    )
}

#[test]
fn read_policy_blocks_source_capture_existing_review_and_apply() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    fs::write(root.path().join("README.md"), b"private instructions").expect("source");
    let allowed =
        discover_init_artifacts(root.path(), state.path(), &selection()).expect("allowed");
    let denied = |path: &str| path != "README.md";
    assert!(
        init_artifact_page_checked(
            state.path(),
            InitArtifactPageRequest::new(allowed, 0, 1024).expect("page"),
            &denied
        )
        .is_err()
    );
    assert!(
        prepare_init_artifact_patch_checked(
            root.path(),
            state.path(),
            request().query().workspace(),
            Generation::first(),
            RevisionNumber::first(),
            allowed,
            &denied
        )
        .is_err()
    );
    assert!(!root.path().join("AGENTS.md").exists());
    let blocked = discover_init_artifacts_checked(root.path(), state.path(), &selection(), &denied)
        .expect("diagnostic proposal");
    let manifest = load_manifest(
        &config(state.path()).expect("config"),
        blocked.request(),
        blocked.manifest(),
    )
    .expect("manifest");
    let source = manifest.sources.iter().find(|source| source.path == "README.md").expect("source");
    assert!(source.content.is_none());
    assert_eq!(source.diagnostic.as_deref(), Some("source is outside the current read policy"));
    assert!(
        discover_init_artifacts_checked(root.path(), state.path(), &selection(), &|path| path
            != "AGENTS.md")
        .is_err()
    );
}

#[test]
fn command_inventory_exceeds_legacy_count_and_selection_survives_recapture() {
    let root = tempfile::tempdir().expect("root");
    let state = tempfile::tempdir().expect("state");
    let scripts = (0..65_536)
        .map(|index| (format!("task{index:05}"), "echo unverified"))
        .collect::<std::collections::BTreeMap<_, _>>();
    fs::write(
        root.path().join("package.json"),
        serde_json::to_vec(&serde_json::json!({"scripts": scripts})).expect("json"),
    )
    .expect("source");
    let first =
        discover_init_artifacts(root.path(), state.path(), &selection()).expect("all commands");
    let toggled = discover_init_artifacts(
        root.path(),
        state.path(),
        &InitArtifactDiscovery::new(request(), Some(first.manifest()), None, Some(65_535))
            .expect("selection"),
    )
    .expect("toggle last command");
    let manifest = load_manifest(
        &config(state.path()).expect("config"),
        toggled.request(),
        toggled.manifest(),
    )
    .expect("manifest");
    assert_eq!(manifest.commands.len(), 65_536);
    assert_eq!(manifest.commands.iter().filter(|command| command.selected).count(), 65_535);
    assert!(!manifest.commands[65_535].selected);
    assert!(manifest.commands[65_534].selected);
    assert!(!root.path().join("AGENTS.md").exists());
}
