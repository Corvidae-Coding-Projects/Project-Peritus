use peritus_model_protocol::{
    CancellationKind, CapabilityMatrix, CapabilityProvenance, ModelLimits, ModelName,
    OutputLimitEnforcement, ProviderName, ResumeKind, StateMode, WireDialect,
};
use peritus_types::ProviderProfileId;

use super::*;

#[test]
fn mentioned_workspace_image_is_attached_with_its_path() {
    let root = tempfile::tempdir().expect("workspace");
    fs::create_dir(root.path().join("in")).expect("input directory");
    fs::write(root.path().join("in/reference.png"), b"\x89PNG\r\n\x1a\nbounded-test-pixels")
        .expect("image");

    let images =
        discover(root.path(), "Inspect in/reference.png and describe the image", &profile(true))
            .expect("workspace images");
    let (prompt, attachments) = images.into_parts("task".to_owned());

    assert_eq!(attachments.len(), 1);
    assert!(prompt.contains("attachment 0: in/reference.png"));
    assert!(prompt.contains("actual pixels"));
}

#[test]
fn unrelated_image_is_not_added_to_a_text_task() {
    let root = tempfile::tempdir().expect("workspace");
    fs::write(root.path().join("icon.png"), b"\x89PNG\r\n\x1a\nicon").expect("image");

    let images = discover(root.path(), "Fix the Rust parser", &profile(true)).expect("scan");
    let (_, attachments) = images.into_parts("task".to_owned());

    assert!(attachments.is_empty());
}

#[test]
fn mentioned_image_directory_attaches_the_complete_bounded_collection() {
    let root = tempfile::tempdir().expect("workspace");
    let documents = root.path().join("documents");
    let scans = documents.join("scans");
    fs::create_dir_all(&scans).expect("documents directory");
    for index in 0..6 {
        fs::write(scans.join(format!("page-{index}.jpg")), b"\xff\xd8\xffbounded-test-pixels")
            .expect("image");
    }
    let task =
        format!("Classify the JPG files in {}/ by their actual content", documents.display());

    let images = discover(root.path(), &task, &profile(true)).expect("workspace images");
    let (prompt, attachments) = images.into_parts(task);

    assert_eq!(attachments.len(), 6);
    assert!(prompt.contains("attachment 5: documents/scans/page-5.jpg"));
}

#[test]
fn mentioned_source_directory_does_not_attach_descendant_screenshots() {
    let root = tempfile::tempdir().expect("workspace");
    let screenshots = root.path().join("doomgeneric/screenshots");
    fs::create_dir_all(&screenshots).expect("screenshots directory");
    fs::write(screenshots.join("sdl.png"), b"\x89PNG\r\n\x1a\npixels").expect("image");
    let task = format!(
        "Build the sources in {}/doomgeneric/ so node vm.js writes frames to frame.bmp.",
        root.path().display(),
    );

    let images = discover(root.path(), &task, &profile(false)).expect("non-visual source task");
    let (_, attachments) = images.into_parts(task);

    assert!(attachments.is_empty());
}

#[test]
fn manifest_paths_use_provider_neutral_separators() {
    assert_eq!(
        manifest_path(Path::new(r"documents\scans\page-5.jpg")).expect("manifest path"),
        "documents/scans/page-5.jpg"
    );
}

#[test]
fn image_task_fails_explicitly_for_a_text_only_provider() {
    let root = tempfile::tempdir().expect("workspace");
    fs::write(root.path().join("photo.jpg"), b"\xff\xd8\xffpixels").expect("image");

    let error = discover(root.path(), "Describe the photo", &profile(false))
        .err()
        .expect("text-only provider must fail");

    assert_eq!(error.kind(), ProductRunnerErrorKind::Provider);
    assert!(error.detail().contains("image-capable provider"));
}

#[test]
fn verifier_reference_image_does_not_attach_unrelated_workspace_media() {
    let root = tempfile::tempdir().expect("workspace");
    fs::create_dir(root.path().join("upstream")).expect("upstream directory");
    fs::write(root.path().join("upstream/texture.gif"), b"GIF89apixels").expect("image");

    let images = discover(
        root.path(),
        "Build the renderer. We will compare its output against a reference image.",
        &profile(false),
    )
    .expect("unrelated image is not required");
    let (_, attachments) = images.into_parts("task".to_owned());

    assert!(attachments.is_empty());
}

pub(super) fn profile(images: bool) -> ProviderProfile {
    let supported = if images { vec![Capability::ImageInput] } else { Vec::new() };
    ProviderProfile::new(
        ProviderProfileId::new([0x91; 16]).expect("profile ID"),
        1,
        ProviderName::new("test".to_owned()).expect("provider"),
        ModelName::new("test-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&supported, &[]).expect("capabilities"),
        CapabilityProvenance::Profiled,
        ModelLimits::new(128_000, 8_192, 32, 1, 4 * 1024 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}
