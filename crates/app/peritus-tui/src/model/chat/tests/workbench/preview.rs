use super::*;
use peritus_app_protocol::{
    ControlOperationId, ProductDeliverable, ProductRunPhase, WorkbenchCaptureCapability,
    WorkbenchCaptureReceipt, WorkbenchCaptureState, WorkbenchCaptureTarget, WorkbenchIntent,
    WorkbenchLaunchProfile, WorkbenchLaunchResult, WorkbenchLaunchSourceKind, WorkbenchLaunchState,
    WorkbenchLaunchText, WorkbenchResultPage, WorkbenchResultQuery, WorkbenchSnapshot,
};
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceStatus, SettlementCause,
    SettlementReducer,
};
use peritus_types::{ArtifactId, ProcessId, RunId, Sha256Digest};

fn text(value: &str) -> WorkbenchLaunchText {
    WorkbenchLaunchText::new(value.to_owned()).expect("launch text")
}

fn preview_model() -> (AppModel, peritus_app_protocol::WorkbenchQuery, RunId, Sha256Digest) {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchPreview)
            .expect("preview feature"),
    );
    let workspace = model.product.as_ref().expect("product").launch.workspace_id();
    let query = peritus_app_protocol::WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([71; 16]).expect("conversation"),
        workspace,
    );
    model.chat.workbench.selected = Some(query);
    model.chat.workbench.snapshot = Some(
        WorkbenchSnapshot::new(
            query,
            7,
            ConversationTitle::new("Preview fixture".to_owned()).expect("title"),
            false,
            false,
        )
        .expect("snapshot"),
    );
    let run = RunId::new([72; 16]).expect("run");
    let digest = Sha256Digest::new([73; 32]);
    let identity = CandidateIdentity::new(run, workspace, digest, 1, 1).expect("identity");
    let checkpoint = CandidateCheckpoint::new(
        identity,
        CandidateStage::Changed,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
    )
    .expect("checkpoint");
    let mut reducer = SettlementReducer::new();
    reducer.observe(checkpoint).expect("observe");
    let settlement = reducer.settle(SettlementCause::Provider).expect("settlement");
    let snapshot = peritus_app_protocol::ProductRunSnapshot::new(
        run,
        workspace,
        model.chat_providers().expect("providers"),
        ProductRunPhase::Complete,
        1,
        "build preview".to_owned(),
        "complete".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .expect("run snapshot")
    .with_deliverable(
        ProductDeliverable::candidate(
            "/managed/project".to_owned(),
            vec!["demo.py".to_owned()],
            Vec::new(),
            "python3 demo.py".to_owned(),
            CandidateStage::Changed,
        )
        .expect("deliverable"),
    );
    model.accept_product_settlement(
        &peritus_app_protocol::ProductRunSettlementSnapshot::new(snapshot, settlement)
            .expect("settled snapshot"),
    );
    (model, query, run, digest)
}

fn result_page(
    query: WorkbenchResultQuery,
    launch: ControlOperationId,
    profile: WorkbenchLaunchProfile,
) -> WorkbenchResultPage {
    let capture = WorkbenchCaptureReceipt::new(
        ControlOperationId::new([75; 16]).expect("capture"),
        WorkbenchCaptureState::Captured,
        WorkbenchCaptureTarget::x11_window(0x440_001).expect("window"),
        Some(ArtifactId::new([76; 16]).expect("artifact")),
        Some(Sha256Digest::new([77; 32])),
        Some((640, 480)),
        Some(1_700_000_000_000),
        text("selected window captured"),
    )
    .expect("capture receipt");
    let launch = WorkbenchLaunchResult::new(
        launch,
        profile,
        Some(ProcessId::new([74; 16]).expect("process")),
        WorkbenchLaunchState::Running,
        true,
        Vec::new(),
        vec![capture],
        Vec::new(),
        1,
        None,
        None,
    )
    .expect("launch result");
    WorkbenchResultPage::new(
        query,
        7,
        2,
        WorkbenchCaptureCapability::X11SelectedWindow,
        vec![launch],
    )
    .expect("result page")
}

fn preview_receipt(command: &WorkbenchCommand) -> AppResponsePayload {
    AppResponsePayload::WorkbenchReceipt(
        WorkbenchReceipt::new(
            command.operation(),
            command.query(),
            command.expected_revision(),
            Sha256Digest::new([78; 32]),
        )
        .expect("preview receipt"),
    )
}

#[test]
fn preview_launch_receipt_results_play_and_feedback_remain_exactly_bound() {
    let (mut model, query, run, candidate_digest) = preview_model();
    model.chat.buffer = "/preview launch python3 demo.py".to_owned();
    let sent = request(&model.slash_command(&model.chat.buffer.clone()));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
        panic!("preview command")
    };
    let WorkbenchIntent::StartPreview(profile) = command.intent() else { panic!("launch intent") };
    assert_eq!(command.query(), query);
    assert_eq!(command.expected_revision(), 7);
    assert_eq!(profile.run(), run);
    assert_eq!(profile.executable().as_str(), "python3");
    assert_eq!(profile.arguments()[0].as_str(), "demo.py");
    assert_eq!(profile.source().kind(), WorkbenchLaunchSourceKind::ManagedCandidate);
    assert_eq!(profile.source().digest(), candidate_digest);

    let result_query = request(&respond(&mut model, &sent, preview_receipt(command)));
    assert!(model.chat.buffer.is_empty());
    assert_eq!(model.view, View::Preview);
    assert_eq!(
        result_query.payload(),
        &AppRequestPayload::QueryWorkbenchResult(WorkbenchResultQuery::new(query, run))
    );
    let page =
        result_page(WorkbenchResultQuery::new(query, run), command.operation(), profile.clone());
    respond(&mut model, &result_query, AppResponsePayload::WorkbenchResult(page));
    assert_eq!(
        model.product.as_ref().expect("product").preview.as_ref().expect("page").launches()[0]
            .process(),
        Some(ProcessId::new([74; 16]).expect("process"))
    );

    model.chat.buffer = "/preview play MOVE_RIGHT".to_owned();
    let play = request(&model.slash_command(&model.chat.buffer.clone()));
    let AppRequestPayload::WorkbenchCommand(play_command) = play.payload() else {
        panic!("play command")
    };
    assert!(matches!(
        play_command.intent(),
        WorkbenchIntent::InteractPreview { launch, input }
            if *launch == command.operation() && input.bytes() == b"MOVE_RIGHT\n"
    ));
    let refresh = request(&respond(&mut model, &play, preview_receipt(play_command)));
    let refreshed_page =
        result_page(WorkbenchResultQuery::new(query, run), command.operation(), profile.clone());
    respond(&mut model, &refresh, AppResponsePayload::WorkbenchResult(refreshed_page));

    model.chat.buffer = "/preview feedback move the control right".to_owned();
    let feedback = request(&model.slash_command(&model.chat.buffer.clone()));
    assert!(matches!(
        feedback.payload(),
        AppRequestPayload::WorkbenchCommand(value)
            if matches!(value.intent(), WorkbenchIntent::AddArtifactFeedback { capture, message, .. }
                if *capture == ControlOperationId::new([75; 16]).expect("capture")
                    && message.as_str() == "move the control right")
    ));

    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 38)).expect("terminal");
    let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
    let rendered: String =
        frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
    for expected in [
        "Preview evidence",
        "X11 selected-window capture available",
        "behavior-checked=true",
        "640x480",
    ] {
        assert!(rendered.contains(expected), "missing {expected}: {rendered}");
    }
}

#[test]
fn preview_requires_feature_exact_scope_and_explicit_capture_target() {
    let (mut model, _, _, _) = preview_model();
    model
        .features
        .retain(|feature| feature.as_str() != WellKnownProtocolFeature::WorkbenchPreview.as_str());
    model.chat.buffer = "/preview results".to_owned();
    assert!(model.slash_command(&model.chat.buffer.clone()).is_empty());
    assert_eq!(model.chat.buffer, "/preview results");

    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchPreview)
            .expect("preview feature"),
    );
    model.chat.buffer = "/preview capture desktop".to_owned();
    assert!(model.slash_command(&model.chat.buffer.clone()).is_empty());
    assert_eq!(model.chat.buffer, "/preview capture desktop");
}

#[test]
fn preview_profile_selects_host_observed_build_and_plain_folder_source() {
    use peritus_app_protocol::{
        ProductModelChoice, WorkbenchFileMetadata, WorkbenchFileMode, WorkbenchFilePreview,
        WorkbenchFileRange, WorkbenchFileRequest,
    };
    for direct in [false, true] {
        let (mut model, query, _, _) = preview_model();
        if direct {
            let product = model.product.as_mut().unwrap();
            product.launch = product.launch.clone().with_direct_folder(true);
        }
        let digest = peritus_codec::sha256(b"exact observed program");
        let file = WorkbenchFileRequest::new(
            query,
            7,
            "demo.py".to_owned(),
            WorkbenchFileRange::All,
            WorkbenchFileMode::Snapshot,
            model.chat_providers().unwrap().writer(),
            ProductModelChoice::default(),
        )
        .unwrap();
        model.chat.workbench.files.preview = Some(
            WorkbenchFilePreview::new(
                file,
                peritus_codec::sha256(b"folder"),
                WorkbenchFileMetadata::new(digest, 22, (0, 22), digest).unwrap(),
                1,
                "fixture-model".to_owned(),
            )
            .unwrap(),
        );
        model.chat.buffer = if direct {
            "/preview launch --build demo.py --source demo.py -- python3 demo.py"
        } else {
            "/preview launch --build demo.py -- python3 demo.py"
        }
        .to_owned();
        let sent = request(&model.slash_command(&model.chat.buffer.clone()));
        let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
            panic!("command")
        };
        let WorkbenchIntent::StartPreview(profile) = command.intent() else { panic!("profile") };
        assert_eq!(profile.build().unwrap().path().as_str(), "demo.py");
        assert_eq!(profile.build().unwrap().digest(), digest);
        assert_eq!(
            profile.source().kind(),
            if direct {
                WorkbenchLaunchSourceKind::PlainFolderFile
            } else {
                WorkbenchLaunchSourceKind::ManagedCandidate
            }
        );
        if direct {
            assert_eq!(profile.source().digest(), digest);
        }
    }
}
