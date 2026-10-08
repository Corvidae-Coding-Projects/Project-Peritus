use peritus_model_protocol::{
    CancellationKind, CapabilityMatrix, CapabilityProvenance, ModelLimits, ModelName,
    OutputLimitEnforcement, ProviderName, ResumeKind, StateMode, WireDialect,
};
use peritus_types::ProviderProfileId;
use std::io::Cursor;

use super::*;

pub(super) fn encoded(format: image::ImageFormat) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::new(2, 2))
        .write_to(&mut output, format)
        .expect("encode fixture");
    output.into_inner()
}

#[test]
fn non_visual_image_file_work_does_not_require_image_input() {
    let root = tempfile::tempdir().expect("workspace");
    fs::write(root.path().join("photo.jpg"), b"image bytes to checksum").expect("file");
    let task = "Compute the SHA-256 checksum of photo.jpg with the workspace tools.";

    let images = discover(root.path(), task, &profile(false)).expect("file work needs no vision");
    let (prompt, attachments) = images.into_parts(task.to_owned());

    assert!(prompt.starts_with(task));
    assert!(prompt.contains("cannot inspect image pixels"));
    assert!(prompt.contains("scoped file and command tools"));
    assert!(prompt.contains("photo.jpg"));
    assert!(attachments.is_empty());
    assert_eq!(fs::read(root.path().join("photo.jpg")).unwrap(), b"image bytes to checksum");
}

#[test]
fn direct_visual_file_requests_still_require_image_input() {
    let root = tempfile::tempdir().expect("workspace");
    fs::write(root.path().join("photo.jpg"), b"image bytes").expect("file");
    for task in ["Describe photo.jpg", "Inspect `photo.jpg`", "Classify photo.jpg"] {
        assert!(discover(root.path(), task, &profile(false)).is_err(), "{task}");
        assert!(discover_explicit(root.path(), task, &profile(false), &[]).is_err(), "{task}");
    }
}

#[test]
fn mentioned_workspace_image_is_attached_with_its_path() {
    let root = tempfile::tempdir().expect("workspace");
    fs::create_dir(root.path().join("in")).expect("input directory");
    fs::write(root.path().join("in/reference.png"), encoded(image::ImageFormat::Png))
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
        fs::write(scans.join(format!("page-{index}.jpg")), encoded(image::ImageFormat::Jpeg))
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
fn discovery_keeps_more_than_sixteen_images_and_deep_descendants() {
    let root = tempfile::tempdir().expect("workspace");
    let mut directory = root.path().to_path_buf();
    for depth in 0..20 {
        directory.push(format!("nested-{depth}"));
        fs::create_dir(&directory).expect("nested directory");
    }
    for index in 0..17 {
        fs::write(root.path().join(format!("image-{index}.png")), encoded(image::ImageFormat::Png))
            .expect("image");
    }
    fs::write(directory.join("deep.png"), encoded(image::ImageFormat::Png)).expect("deep image");
    let task = "Describe all image files in the workspace";
    let images = discover(root.path(), task, &profile(true)).expect("complete traversal");
    let (prompt, attachments) = images.into_parts(task.to_owned());
    assert_eq!(attachments.len(), 18);
    assert!(prompt.contains("nested-19"));
}

#[test]
fn discovery_cursor_exposes_a_resumable_continuation_after_each_bounded_page() {
    let root = tempfile::tempdir().expect("workspace");
    for index in 0..257 {
        fs::write(root.path().join(format!("image-{index:03}.png")), b"candidate")
            .expect("image candidate");
    }
    let mut cursor = WorkspaceImageDiscovery::new(root.path());
    let first = cursor.next_page().expect("first page");
    // The first bounded traversal visits the root directory entry as well as image candidates.
    assert_eq!(first.paths().len(), 255);
    assert!(first.has_more());

    let second = cursor.next_page().expect("resumed page");
    assert_eq!(second.paths().len(), 2);
    assert!(!second.has_more());
}

#[test]
fn unreadable_or_invalid_selected_media_is_reported_without_dropping_valid_neighbors() {
    let root = tempfile::tempdir().expect("workspace");
    let directory = root.path().join("scans");
    fs::create_dir(&directory).expect("directory");
    fs::write(directory.join("valid.png"), encoded(image::ImageFormat::Png)).expect("image");
    fs::write(directory.join("broken.png"), b"not a raster image").expect("broken image");
    let task = format!("Describe the images in {}/scans/", root.path().display());
    let images = discover(root.path(), &task, &profile(true)).expect("good neighbor is usable");
    let (prompt, attachments) = images.into_parts(task);
    assert_eq!(attachments.len(), 1);
    assert!(prompt.contains("Skipped selected media"));
    assert!(prompt.contains("scans/broken.png"));
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
