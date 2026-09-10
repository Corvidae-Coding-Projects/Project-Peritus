use super::store as open_store;
use super::*;
use crate::product_control::inputs::{
    CapturedConversation, InputAdmission,
    tests::{image_profile, request_images},
};
use peritus_product_runner::{
    attachment::ValidatedImage,
    control::{ImageAttachment, InputSelection, InvocationId, QueueIntent},
};
use peritus_types::ArtifactId;

#[path = "images/preview.rs"]
mod preview;

fn image(padded: bool) -> ValidatedImage {
    // Complete one-pixel GIF: palette, LZW raster, sub-block terminator and trailer.
    let mut bytes = vec![
        71, 73, 70, 56, 57, 97, 1, 0, 1, 0, 128, 0, 0, 0, 0, 0, 255, 255, 255, 44, 0, 0, 0, 0, 1,
        0, 1, 0, 0, 2, 2, 68, 1, 0, 59,
    ];
    if padded {
        bytes.push(0);
    }
    ValidatedImage::decode(bytes, &image_profile(true)).expect("valid GIF")
}

fn attach(image: &ValidatedImage) -> ControlOperation {
    let id = operation(2, 1, ControlIntent::PinConversation { pinned: true }).id();
    let reference = ImageAttachment::from_validated(
        id,
        ArtifactId::new([8; 16]).expect("artifact"),
        ControlText::new("explicit reference.gif".to_owned()).expect("label"),
        image,
    )
    .expect("reference");
    operation(
        2,
        1,
        ControlIntent::AttachImage {
            image: reference,
            text: ControlText::new("Use this image as a visual reference".to_owned())
                .expect("caption"),
        },
    )
}

fn capture(store: &ControlStore) -> CapturedConversation {
    store
        .capture_inputs(
            create().conversation(),
            ActorId::new([3; 16]).expect("actor"),
            WorkspaceId::new([4; 16]).expect("workspace"),
        )
        .expect("capture")
}

#[test]
fn image_import_requires_exact_decoded_bytes_and_replays_with_its_artifact_after_restart() {
    let root = tempfile::tempdir().expect("root");
    let mut store = open_store(root.path());
    store.accept(&create()).expect("create");
    let image = image(false);
    let import = attach(&image);
    assert!(store.accept(&import).is_err(), "metadata alone cannot publish an image");
    assert!(store.accept_image(&import, &self::image(true)).is_err());
    assert!(store.resolve(&import).expect("absence").is_none());
    assert_eq!(store.load(create().conversation()).expect("load").expect("record").revision(), 1);
    let receipt = store.accept_image(&import, &image).expect("atomic import");
    let captured = capture(&store);
    assert_eq!(captured.images(), std::slice::from_ref(image.media()));
    assert!(captured.inputs().conversation().contains("Use this image as a visual reference"));
    assert!(
        store
            .capture_inputs(
                create().conversation(),
                ActorId::new([9; 16]).expect("other"),
                WorkspaceId::new([4; 16]).expect("workspace")
            )
            .is_err()
    );
    drop(store);
    let mut store = open_store(root.path());
    assert_eq!(store.accept_image(&import, &image).expect("replay"), receipt);
    assert_eq!(capture(&store).images(), captured.images());
    let changed_label = operation(2, 1, ControlIntent::PinConversation { pinned: true });
    assert!(matches!(
        store.accept(&changed_label),
        Err(Error::Control(ControlError::IdempotencyConflict))
    ));
}

#[test]
fn request_rejects_omitted_extra_or_changed_images_and_seals_exact_selection() {
    let root = tempfile::tempdir().expect("root");
    let mut store = open_store(root.path());
    store.accept(&create()).expect("create");
    let image = image(false);
    let import = attach(&image);
    store.accept_image(&import, &image).expect("import");
    let captured = capture(&store);
    let invocation = InvocationId::new([12; 16]).expect("invocation");
    for wrong in
        [Vec::new(), vec![image.media().clone(); 2], vec![self::image(true).media().clone()]]
    {
        assert!(matches!(
            store.prepare_inputs(
                &captured,
                invocation,
                &request_images(captured.inputs().conversation(), &wrong)
            ),
            Err(Error::Control(ControlError::InvalidInput))
        ));
    }
    let request = request_images(captured.inputs().conversation(), captured.images());
    let admitted = store.prepare_inputs(&captured, invocation, &request).expect("admit");
    assert!(matches!(admitted, InputAdmission::Accepted(_)));
    store
        .accept(&operation(
            3,
            3,
            ControlIntent::SelectImage { attachment: import.id(), selected: false },
        ))
        .expect("deselect");
    assert!(capture(&store).images().is_empty());
    drop(store);
    let mut store = open_store(root.path());
    assert_eq!(
        store.prepare_inputs(&captured, invocation, &request).expect("original replay"),
        admitted
    );
    let record = store.load(create().conversation()).expect("verify history").expect("record");
    assert_eq!(
        record.inputs().invocations()[0].request_digest(),
        request.fingerprint().expect("fingerprint").digest()
    );
    assert!(!record.images().entries()[0].selected());
    assert!(capture(&store).images().is_empty());
}

#[test]
fn image_selection_and_caption_queue_changes_fence_captured_requests_without_changing_old_bytes() {
    let root = tempfile::tempdir().expect("root");
    let mut store = open_store(root.path());
    store.accept(&create()).expect("create");
    let image = image(false);
    let import = attach(&image);
    store.accept_image(&import, &image).expect("import");
    let stale = capture(&store);
    let input = stale.inputs().included()[0];
    store
        .accept(&operation(
            3,
            2,
            ControlIntent::SelectImage { attachment: import.id(), selected: false },
        ))
        .expect("deselect");
    let next = capture(&store);
    assert!(next.inputs().generation() > stale.inputs().generation());
    assert_eq!(next.inputs().conversation(), stale.inputs().conversation());
    assert_eq!(
        store
            .prepare_inputs(
                &stale,
                InvocationId::new([12; 16]).expect("invocation"),
                &request_images(stale.inputs().conversation(), stale.images())
            )
            .expect("stale"),
        InputAdmission::Stale
    );
    store
        .accept(&operation(
            4,
            3,
            ControlIntent::SelectImage { attachment: import.id(), selected: true },
        ))
        .expect("reselect");
    store
        .accept(&operation(
            5,
            4,
            ControlIntent::Queue(QueueIntent::Hold { selected: input, held: true }),
        ))
        .expect("hold caption");
    assert!(capture(&store).images().is_empty());
    store
        .accept(&operation(
            6,
            5,
            ControlIntent::Queue(QueueIntent::Edit {
                selected: input,
                text: ControlText::new("Corrected caption".to_owned()).expect("text"),
            }),
        ))
        .expect("edit caption");
    let revised = InputSelection::new(input.id(), 2).expect("revision");
    store
        .accept(&operation(
            7,
            6,
            ControlIntent::Queue(QueueIntent::Hold { selected: revised, held: false }),
        ))
        .expect("release");
    assert_eq!(capture(&store).images(), std::slice::from_ref(image.media()));
    assert!(capture(&store).inputs().conversation().contains("Corrected caption"));
    store
        .accept(&operation(8, 7, ControlIntent::Queue(QueueIntent::Withdraw(revised))))
        .expect("withdraw");
    assert!(capture(&store).images().is_empty());
    assert_eq!(
        store.load(create().conversation()).expect("load").expect("record").images().entries()[0]
            .image()
            .digest(),
        image.digest()
    );
}

#[test]
fn missing_changed_or_separately_published_image_bytes_fail_closed_on_replay() {
    for mutation in [
        "DELETE FROM state_records WHERE namespace = 3407",
        "UPDATE state_records SET producing_position = 1 WHERE namespace = 3407",
        "UPDATE state_records SET revision = 2 WHERE namespace = 3407",
        "UPDATE state_records SET value = x'00' WHERE namespace = 3407",
    ] {
        let root = tempfile::tempdir().expect("root");
        let mut store = open_store(root.path());
        store.accept(&create()).expect("create");
        let image = image(false);
        let import = attach(&image);
        store.accept_image(&import, &image).expect("import");
        drop(store);
        let connection =
            rusqlite::Connection::open(root.path().join("control.sqlite3")).expect("fixture");
        assert_eq!(connection.execute(mutation, []).expect("tamper"), 1);
        drop(connection);
        let store = open_store(root.path());
        assert!(store.load(create().conversation()).is_err());
        assert!(store.resolve(&import).is_err());
    }
}

#[test]
fn context_reports_original_image_digest_and_distinguishes_sealed_inclusion_from_deselection() {
    use peritus_app_protocol::{
        WorkbenchContextDisposition as D, WorkbenchContextSource as S, WorkbenchContextView as V,
    };
    let root = tempfile::tempdir().expect("root");
    let mut store = open_store(root.path());
    store.accept(&create()).expect("create");
    let image = image(false);
    let import = attach(&image);
    store.accept_image(&import, &image).expect("import");
    let query = |view| {
        peritus_app_protocol::WorkbenchContextQuery::new(
            peritus_app_protocol::WorkbenchQuery::new(
                peritus_app_protocol::ConversationId::new(*create().conversation().as_bytes())
                    .expect("conversation"),
                WorkspaceId::new([4; 16]).expect("workspace"),
            ),
            0,
            0,
            view,
        )
        .expect("query")
    };
    let actor = ActorId::new([3; 16]).expect("owner");
    let next = store.context_page(actor, query(V::Next)).expect("eligible view");
    let row =
        next.rows().iter().find(|row| matches!(row.source(), S::Image { .. })).expect("image row");
    assert_eq!(row.digest(), image.digest());
    assert_eq!(row.bytes(), image.byte_len());
    assert_eq!(row.disposition(), D::Eligible);
    let source = row.source();
    let captured = capture(&store);
    let invocation = InvocationId::new([12; 16]).expect("invocation");
    store
        .prepare_inputs(
            &captured,
            invocation,
            &request_images(captured.inputs().conversation(), captured.images()),
        )
        .expect("admit");
    store
        .accept(&operation(
            3,
            3,
            ControlIntent::SelectImage { attachment: import.id(), selected: false },
        ))
        .expect("deselect");
    drop(store);
    let store = open_store(root.path());
    let next = store.context_page(actor, query(V::Next)).expect("next");
    assert_eq!(
        next.rows().iter().find(|row| row.source() == source).expect("image").disposition(),
        D::Deselected
    );
    let sealed = store
        .context_page(
            actor,
            query(V::Invocation(
                peritus_app_protocol::WorkbenchInvocationId::new(*invocation.as_bytes())
                    .expect("invocation"),
            )),
        )
        .expect("sealed");
    let image_row = sealed.rows().iter().find(|row| row.source() == source).expect("sealed image");
    assert_eq!(image_row.disposition(), D::Included);
    assert_eq!(image_row.digest(), image.digest());
    assert_eq!(image_row.bytes(), image.byte_len());
}
