//! Deterministic bounded raster-image discovery for grounded model turns.

use std::{
    fs::{self, DirEntry},
    path::{Path, PathBuf},
};

use peritus_model_protocol::{
    Capability, MediaInput, MediaKind, MediaType, ProtocolLimits, ProviderProfile,
};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

const MAX_IMAGES: usize = 16;
const MAX_DISCOVERED_IMAGES: usize = 1_024;
const MAX_IMAGE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 12 * 1024 * 1024;
const MAX_DEPTH: usize = 16;

pub struct WorkspaceImages {
    attachments: Vec<MediaInput>,
    manifest: String,
}

impl WorkspaceImages {
    pub fn into_parts(self, prompt: String) -> (String, Vec<MediaInput>) {
        if self.attachments.is_empty() {
            (prompt, self.attachments)
        } else {
            (
                format!(
                    "{prompt}\n\nPeritus attached the following workspace images. Inspect their actual pixels before making image claims; attachment indexes match this manifest:\n{}",
                    self.manifest
                ),
                self.attachments,
            )
        }
    }
}

#[allow(
    clippy::format_push_string,
    reason = "formal-boundary policy models format! but not writeln!"
)]
pub fn discover(
    root: &Path,
    task: &str,
    profile: &ProviderProfile,
) -> Result<WorkspaceImages, ProductRunnerError> {
    let discovered = discover_paths(root)?;
    let mut paths = discovered
        .iter()
        .filter(|path| task_mentions_path(task, root, path))
        .cloned()
        .collect::<Vec<_>>();
    if paths.is_empty() && requests_visual_inspection(task) {
        paths = discovered;
    }
    paths.truncate(MAX_IMAGES);
    if paths.is_empty() {
        return Ok(empty());
    }
    if !profile.capabilities().supports(Capability::ImageInput) {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::Provider,
            "attach workspace images",
            "the selected provider cannot inspect image inputs; choose an image-capable provider",
        ));
    }

    let provider_limit = profile.limits().max_inline_media_bytes().min(MAX_IMAGE_BYTES);
    let mut attachments = Vec::new();
    let mut manifest = String::new();
    let mut total = 0_u64;
    for path in paths {
        let metadata = fs::metadata(&path).map_err(|error| repository(error.to_string()))?;
        let bytes_len = metadata.len();
        if bytes_len == 0
            || bytes_len > provider_limit
            || total.saturating_add(bytes_len) > MAX_TOTAL_BYTES
        {
            continue;
        }
        let bytes = fs::read(&path).map_err(|error| repository(error.to_string()))?;
        let media_type = media_type(&path, &bytes)?;
        let media = MediaInput::inline(
            MediaKind::Image,
            MediaType::new(media_type.to_owned()).map_err(|error| protocol(&error))?,
            bytes,
            ProtocolLimits::PRODUCTION,
        )
        .map_err(|error| protocol(&error))?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| repository("discovered image escaped the managed workspace".to_owned()))?;
        let relative = manifest_path(relative)?;
        let index = attachments.len();
        manifest.push_str(&format!("- attachment {index}: {relative} ({bytes_len} bytes)\n"));
        total = total.saturating_add(bytes_len);
        attachments.push(media);
    }
    if attachments.is_empty() {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::Provider,
            "attach workspace images",
            "workspace image inputs exceed the selected provider's bounded media limit",
        ));
    }
    Ok(WorkspaceImages { attachments, manifest })
}

fn manifest_path(path: &Path) -> Result<String, ProductRunnerError> {
    path.to_str()
        .map(|value| value.replace('\\', "/"))
        .ok_or_else(|| repository("image path is not representable as UTF-8".to_owned()))
}

fn discover_paths(root: &Path) -> Result<Vec<PathBuf>, ProductRunnerError> {
    let mut pending = vec![(root.to_path_buf(), 0_usize)];
    let mut images = Vec::new();
    while let Some((directory, depth)) = pending.pop() {
        if images.len() >= MAX_DISCOVERED_IMAGES || depth > MAX_DEPTH {
            break;
        }
        let mut entries = fs::read_dir(&directory)
            .map_err(|error| repository(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| repository(error.to_string()))?;
        entries.sort_by_key(DirEntry::file_name);
        for entry in entries.into_iter().rev() {
            let path = entry.path();
            let file_type = entry.file_type().map_err(|error| repository(error.to_string()))?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() && !ignored_directory(&entry.file_name()) {
                pending.push((path, depth.saturating_add(1)));
            } else if file_type.is_file() && supported_extension(&path) {
                images.push(path);
                if images.len() >= MAX_DISCOVERED_IMAGES {
                    break;
                }
            }
        }
    }
    images.sort();
    Ok(images)
}

fn requests_visual_inspection(task: &str) -> bool {
    let task = task.to_ascii_lowercase();
    [
        "image files",
        "photo files",
        "picture files",
        "jpeg files",
        "jpg files",
        "png files",
        "ocr",
        "describe the image",
        "describe the photo",
        "describe the picture",
        "describe the screenshot",
        "inspect the image",
        "inspect the photo",
        "inspect the picture",
        "inspect the screenshot",
        "edit the image",
        "edit the photo",
        "edit the picture",
        "edit the screenshot",
    ]
    .iter()
    .any(|needle| task.contains(needle))
}

fn task_mentions_path(task: &str, root: &Path, path: &Path) -> bool {
    let task = task.to_ascii_lowercase().replace('\\', "/");
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_ascii_lowercase()
        .replace('\\', "/");
    let filename =
        path.file_name().and_then(std::ffi::OsStr::to_str).unwrap_or_default().to_ascii_lowercase();
    task.contains(&relative)
        || !filename.is_empty() && task.contains(&filename)
        || scoped_parent_is_mentioned(&task, root, path)
}

fn scoped_parent_is_mentioned(task: &str, root: &Path, path: &Path) -> bool {
    let mut directory = path.parent();
    while let Some(parent) = directory {
        let Ok(relative) = parent.strip_prefix(root) else {
            break;
        };
        if relative.as_os_str().is_empty() {
            break;
        }
        let relative = relative.to_string_lossy().to_ascii_lowercase().replace('\\', "/");
        let absolute = parent.to_string_lossy().to_ascii_lowercase().replace('\\', "/");
        if task.contains(&format!("{relative}/")) || task.contains(&format!("{absolute}/")) {
            return true;
        }
        directory = parent.parent();
    }
    false
}

fn ignored_directory(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(".git" | ".worktrees" | "node_modules" | "target" | ".venv" | "dist")
    )
}

fn supported_extension(path: &Path) -> bool {
    path.extension().and_then(std::ffi::OsStr::to_str).is_some_and(|value| {
        matches!(value.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif")
    })
}

fn media_type(path: &Path, bytes: &[u8]) -> Result<&'static str, ProductRunnerError> {
    let extension =
        path.extension().and_then(std::ffi::OsStr::to_str).unwrap_or_default().to_ascii_lowercase();
    match extension.as_str() {
        "png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => Ok("image/png"),
        "jpg" | "jpeg" if bytes.starts_with(b"\xff\xd8\xff") => Ok("image/jpeg"),
        "gif" if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") => Ok("image/gif"),
        "webp" if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") => {
            Ok("image/webp")
        }
        _ => Err(repository("workspace image extension and content signature disagree".to_owned())),
    }
}

const fn empty() -> WorkspaceImages {
    WorkspaceImages { attachments: Vec::new(), manifest: String::new() }
}

fn protocol(error: &peritus_model_protocol::ProtocolError) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Provider,
        "attach workspace images",
        error.to_string(),
    )
}

fn repository(detail: String) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Repository, "discover workspace images", detail)
}

#[cfg(test)]
mod tests {
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

        let images = discover(
            root.path(),
            "Inspect in/reference.png and describe the image",
            &profile(true),
        )
        .expect("workspace images");
        let (prompt, attachments) = images.into_parts("task".to_owned());
        let expected_path = PathBuf::from("in").join("reference.png");

        assert_eq!(attachments.len(), 1);
        assert!(prompt.contains(&format!("attachment 0: {}", expected_path.display())));
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

    fn profile(images: bool) -> ProviderProfile {
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
}
