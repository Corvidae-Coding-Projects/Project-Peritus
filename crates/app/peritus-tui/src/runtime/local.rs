//! Owned local file discovery and explicit clipboard-image capture.

use std::{
    collections::{BinaryHeap, VecDeque},
    fmt::Write as _,
    io,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::{Duration, SystemTime},
};

#[cfg(unix)]
use std::ffi::OsStr;

use tokio::{io::AsyncReadExt as _, process::Command, task::JoinSet, time::Instant};

use crate::{
    action::Action,
    local::{LocalOperation, LocalReply},
};

const FILE_ROWS: usize = 200;
const DISCOVERY_FILES: usize = 100_000;
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
const CLIPBOARD_BYTES: usize = 32 * 1024 * 1024;
const CLIPBOARD_TIMEOUT: Duration = Duration::from_secs(5);
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

type Outcome = (u64, Result<(LocalReply, Option<tempfile::TempDir>), String>);

#[derive(Default)]
pub(super) struct LocalOperations {
    jobs: JoinSet<Outcome>,
    generation: Option<u64>,
    replaceable: bool,
    images: VecDeque<tempfile::TempDir>,
}

impl LocalOperations {
    pub(super) fn active(&self) -> bool {
        !self.jobs.is_empty()
    }

    pub(super) fn start(&mut self, generation: u64, operation: LocalOperation) -> bool {
        let replaceable = matches!(
            operation,
            LocalOperation::FindFiles { .. } | LocalOperation::SearchTranscript { .. }
        );
        if self.active() {
            if !self.replaceable {
                return false;
            }
            // Search is disposable presentation work. Dropping the old set aborts its
            // future and kills any owned rg child; generation fencing protects the UI.
            self.jobs = JoinSet::new();
        }
        self.replaceable = replaceable;
        self.generation = Some(generation);
        self.jobs.spawn(async move {
            let result = match operation {
                LocalOperation::FindFiles { root, query } => {
                    find_files(root, &query).await.map(|files| (LocalReply::Files(files), None))
                }
                LocalOperation::ReadClipboardImage => clipboard_image().await,
                LocalOperation::ReadClipboardText(source) => super::clipboard_read::read(source)
                    .await
                    .map(|text| (LocalReply::ClipboardText(text), None)),
                LocalOperation::SearchTranscript { scope, query } => {
                    crate::transcript_archive::search(&scope, &query)
                        .await
                        .map(|page| (LocalReply::Transcript(page), None))
                }
                LocalOperation::EditDraft { .. } => {
                    Err("The external editor needs exclusive terminal ownership.".to_owned())
                }
            };
            (generation, result)
        });
        true
    }

    pub(super) fn cancel_search(&mut self) {
        if self.replaceable {
            self.jobs = JoinSet::new();
            self.generation = None;
        }
    }

    pub(super) async fn next(&mut self) -> Action {
        let generation = self.generation.unwrap_or_default();
        let (generation, result) = match self.jobs.join_next().await {
            Some(Ok((generation, result))) => (generation, result),
            Some(Err(error)) => (generation, Err(format!("Local operation failed: {error}"))),
            None => (generation, Err("Local operation stopped before returning a result.".into())),
        };
        self.generation = None;
        let result = result.map(|(reply, image)| {
            if let Some(image) = image {
                // Imports read a snapshot immediately; retain recent captures until client exit
                // as well, so preview/cancellation cannot race removal of the source file.
                if self.images.len() >= 8 {
                    self.images.pop_front();
                }
                self.images.push_back(image);
            }
            reply
        });
        Action::LocalResult { generation, result }
    }
}

/// One durable writer and one replaceable latest public snapshot. No private trace is copied.
#[derive(Default)]
pub(super) struct TranscriptWrites {
    jobs: JoinSet<Result<(), String>>,
    observed: Option<(String, Arc<[peritus_app_protocol::ProductActivity]>)>,
    pending: Option<(String, Arc<[peritus_app_protocol::ProductActivity]>)>,
}

impl TranscriptWrites {
    pub(super) fn observe(&mut self, model: &crate::model::AppModel) {
        let (Some(run), Some(snapshot)) = (model.chat.run_id, model.chat.snapshot.as_ref()) else {
            return;
        };
        let scope = run.as_bytes().iter().fold(
            String::with_capacity(run.as_bytes().len() * 2),
            |mut scope, byte| {
                write!(scope, "{byte:02x}").expect("writing to a String cannot fail");
                scope
            },
        );
        let activities = snapshot.activities();
        if self.observed.as_ref().is_some_and(|(previous_scope, previous)| {
            previous_scope == &scope && previous.as_ref() == activities
        }) {
            return;
        }
        let activities: Arc<[_]> = activities.to_vec().into();
        self.observed = Some((scope.clone(), Arc::clone(&activities)));
        self.pending = Some((scope, activities));
        self.start_pending();
    }

    fn start_pending(&mut self) {
        if self.jobs.is_empty()
            && let Some((scope, activities)) = self.pending.take()
        {
            self.jobs
                .spawn_blocking(move || crate::transcript_archive::record(&scope, &activities));
        }
    }

    pub(super) fn active(&self) -> bool {
        !self.jobs.is_empty()
    }

    pub(super) async fn next(&mut self) -> Option<Action> {
        let result = match self.jobs.join_next().await {
            Some(Ok(result)) => result,
            Some(Err(error)) => Err(format!("Transcript history writer failed: {error}")),
            None => return None,
        };
        self.start_pending();
        result.err().map(Action::TranscriptArchiveFailed)
    }

    pub(super) async fn finish(&mut self) {
        self.start_pending();
        while self.active() {
            let _ = self.next().await;
        }
    }
}

async fn find_files(root: PathBuf, query: &str) -> Result<Vec<PathBuf>, String> {
    // Keep the existing @/absolute/path workflow alongside fuzzy workspace discovery.
    // Only inspect explicitly entered metadata here; content is read by the normal preview.
    let explicit = PathBuf::from(query);
    if explicit.is_absolute() {
        return match tokio::fs::symlink_metadata(&explicit).await {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                Ok(vec![explicit])
            }
            Ok(_) => Ok(Vec::new()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(format!("Cannot inspect the entered file: {error}")),
        };
    }
    let root = tokio::fs::canonicalize(root)
        .await
        .map_err(|error| format!("Cannot open the workspace for file discovery: {error}"))?;
    let mut command = Command::new("rg");
    command
        .args(["--no-config", "--threads", "1", "--files", "--null", "--"])
        .arg(&root)
        .current_dir(&root);
    match discover(&mut command, &root, query).await {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // Git provides identical ignore handling when ripgrep is not installed.
            let mut git = Command::new("git");
            git.args(["ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", "."])
                .current_dir(&root);
            match discover(&mut git, &root, query).await {
                Ok(files) => Ok(files),
                Err(_) => walk_files(&root, query).await,
            }
        }
        Ok(files) => Ok(files),
        Err(error) => Err(format!("File discovery failed: {error}")),
    }
}

async fn discover(command: &mut Command, root: &Path, query: &str) -> io::Result<Vec<PathBuf>> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdout =
        child.stdout.take().ok_or_else(|| io::Error::other("file list unavailable"))?;
    let work = async {
        let mut ranked = RankedFiles::new(query);
        let mut chunk = [0_u8; 8192];
        let mut pending = Vec::new();
        let mut count = 0_usize;
        loop {
            let length = stdout.read(&mut chunk).await?;
            if length == 0 {
                break;
            }
            for &byte in &chunk[..length] {
                if byte == 0 {
                    count += 1;
                    if count > DISCOVERY_FILES {
                        return Err(io::Error::other(
                            "file list exceeds 100,000 entries; choose a smaller workspace",
                        ));
                    }
                    if let Some(path) = path_from_bytes(&pending) {
                        ranked.insert(root, path);
                    }
                    pending.clear();
                } else if pending.len() < 64 * 1024 {
                    pending.push(byte);
                } else {
                    return Err(io::Error::other("file list contains an oversized path"));
                }
            }
        }
        let status = child.wait().await?;
        // rg exits 1 for an empty file set, including a fully ignored workspace.
        if !status.success() && status.code() != Some(1) {
            return Err(io::Error::other(format!("file listing exited with {status}")));
        }
        Ok(ranked.finish())
    };
    let result = tokio::time::timeout(DISCOVERY_TIMEOUT, work).await.unwrap_or_else(|_| {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "file discovery timed out; choose a smaller workspace",
        ))
    });
    if result.is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
    result
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt as _;
    (!bytes.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(bytes)))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> Option<PathBuf> {
    std::str::from_utf8(bytes).ok().filter(|path| !path.is_empty()).map(PathBuf::from)
}

struct RankedFiles {
    query: String,
    files: BinaryHeap<(usize, PathBuf)>,
}

impl RankedFiles {
    fn new(query: &str) -> Self {
        Self { query: query.to_lowercase(), files: BinaryHeap::new() }
    }

    fn insert(&mut self, root: &Path, path: PathBuf) {
        let path = if path.is_absolute() { path } else { root.join(path) };
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let text = relative.to_string_lossy().replace('\\', "/").to_lowercase();
        let mut previous = 0;
        let mut gaps = 0_usize;
        for character in self.query.chars() {
            let Some(offset) = text[previous..].find(character) else { return };
            gaps = gaps.saturating_add(offset);
            previous += offset + character.len_utf8();
        }
        let basename_match = path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().to_lowercase().contains(&self.query));
        let score = gaps
            .saturating_mul(4)
            .saturating_add(text.len())
            .saturating_add(if basename_match { 0 } else { 1_000 });
        self.files.push((score, path));
        if self.files.len() > FILE_ROWS {
            self.files.pop();
        }
    }

    fn finish(self) -> Vec<PathBuf> {
        self.files.into_sorted_vec().into_iter().map(|(_, path)| path).collect()
    }
}

async fn walk_files(root: &Path, query: &str) -> Result<Vec<PathBuf>, String> {
    let deadline = Instant::now() + DISCOVERY_TIMEOUT;
    let mut directories = vec![root.to_owned()];
    let mut ranked = RankedFiles::new(query);
    let mut entries = 0_usize;
    while let Some(directory) = directories.pop() {
        let mut reader = tokio::fs::read_dir(directory)
            .await
            .map_err(|error| format!("Cannot list workspace files: {error}"))?;
        while let Some(entry) = reader
            .next_entry()
            .await
            .map_err(|error| format!("Cannot list workspace files: {error}"))?
        {
            entries += 1;
            if entries > DISCOVERY_FILES || Instant::now() >= deadline {
                return Err("File discovery reached its local scan bound; choose a smaller workspace or install ripgrep for ignore-aware discovery.".into());
            }
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let kind = entry
                .file_type()
                .await
                .map_err(|error| format!("Cannot inspect workspace entry: {error}"))?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                if !matches!(
                    name.to_str(),
                    Some("target" | "node_modules" | "dist" | "build" | "vendor")
                ) {
                    directories.push(entry.path());
                }
            } else if kind.is_file() {
                ranked.insert(root, entry.path());
            }
        }
    }
    Ok(ranked.finish())
}

async fn clipboard_image() -> Result<(LocalReply, Option<tempfile::TempDir>), String> {
    let directory = private_directory("clipboard-")?;
    let path = directory.path().join("clipboard.png");
    let mut command = clipboard_command(&path)?;
    let bytes = helper_output(&mut command, CLIPBOARD_BYTES, CLIPBOARD_TIMEOUT).await?;
    if cfg!(any(target_os = "macos", windows)) {
        let metadata = tokio::fs::symlink_metadata(&path)
            .await
            .map_err(|_| "The clipboard does not contain an image.".to_owned())?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > CLIPBOARD_BYTES as u64
        {
            return Err("Clipboard image is not a regular PNG of at most 32 MiB.".into());
        }
    } else {
        if !bytes.starts_with(PNG_SIGNATURE) {
            return Err("The clipboard does not contain a PNG image. Copy a screenshot or use /images PATH.".into());
        }
        write_private(&path, &bytes)?;
    }
    let mut file = tokio::fs::File::open(&path)
        .await
        .map_err(|error| format!("Cannot inspect captured image: {error}"))?;
    let mut signature = [0_u8; 8];
    file.read_exact(&mut signature)
        .await
        .map_err(|_| "The clipboard does not contain a complete PNG image.".to_owned())?;
    if signature.as_slice() != PNG_SIGNATURE {
        return Err("The clipboard helper did not produce a PNG image.".into());
    }
    Ok((LocalReply::ClipboardImage(path), Some(directory)))
}

fn clipboard_command(path: &Path) -> Result<Command, String> {
    #[cfg(target_os = "linux")]
    {
        let _ = path;
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            let mut command = Command::new("wl-paste");
            command.args(["--no-newline", "--type", "image/png"]);
            return Ok(command);
        }
        if std::env::var_os("DISPLAY").is_some() {
            let mut command = Command::new("xclip");
            command.args(["-selection", "clipboard", "-target", "image/png", "-out"]);
            return Ok(command);
        }
        Err("Clipboard images require a local Wayland or X11 desktop. Use /images PATH in a remote terminal.".into())
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("osascript");
        command.args(["-l", "JavaScript", "-e", MAC_CLIPBOARD]).arg(path);
        Ok(command)
    }
    #[cfg(windows)]
    {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-STA", "-Command", WINDOWS_CLIPBOARD]);
        command.env("PERITUS_CLIPBOARD_IMAGE", path);
        Ok(command)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = path;
        Err("Clipboard-image import is unavailable on this platform. Use /images PATH.".into())
    }
}

#[cfg(target_os = "macos")]
const MAC_CLIPBOARD: &str = r#"ObjC.import('AppKit'); function run(args) { const board = $.NSPasteboard.generalPasteboard; const image = $.NSImage.alloc.initWithPasteboard(board); if (!image) throw Error('No clipboard image'); const bitmap = $.NSBitmapImageRep.imageRepWithData(image.TIFFRepresentation); if (bitmap.pixelsWide * bitmap.pixelsHigh > 16777216) throw Error('Clipboard image is too large'); const png = bitmap.representationUsingTypeProperties($.NSPNGFileType, $({})); if (png.length > 33554432) throw Error('Clipboard PNG is too large'); if (!png.writeToFileAtomically($(args[0]), true)) throw Error('Cannot save clipboard image'); }"#;

#[cfg(windows)]
const WINDOWS_CLIPBOARD: &str = r#"Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; $image = [System.Windows.Forms.Clipboard]::GetImage(); if ($null -eq $image) { exit 2 }; $stream = New-Object System.IO.MemoryStream; try { if ([long]$image.Width * $image.Height -gt 16777216) { exit 3 }; $image.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png); if ($stream.Length -gt 33554432) { exit 3 }; [System.IO.File]::WriteAllBytes($env:PERITUS_CLIPBOARD_IMAGE, $stream.ToArray()) } finally { $stream.Dispose(); $image.Dispose() }"#;

pub(super) async fn helper_output(
    command: &mut Command,
    limit: usize,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true)
        .spawn().map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                "The desktop clipboard helper is unavailable. Install wl-clipboard (Wayland) or xclip (X11).".to_owned()
            } else {
                format!("Cannot start the desktop clipboard helper: {error}")
            }
        })?;
    let stdout =
        child.stdout.take().ok_or_else(|| "Clipboard helper output is unavailable.".to_owned())?;
    let work = async {
        let mut bytes = Vec::new();
        stdout
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|error| format!("Cannot read clipboard data: {error}"))?;
        if bytes.len() > limit {
            return Err("Clipboard data exceeds the 32 MiB limit.".to_owned());
        }
        let status = child
            .wait()
            .await
            .map_err(|error| format!("Cannot collect clipboard helper: {error}"))?;
        if !status.success() {
            return Err("The clipboard does not contain accessible data in the requested format."
                .to_owned());
        }
        Ok(bytes)
    };
    let result = tokio::time::timeout(timeout, work).await.unwrap_or_else(|_| {
        Err("The desktop clipboard helper did not respond within five seconds.".into())
    });
    if result.is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
    result
}

pub(super) fn private_directory(prefix: &str) -> Result<tempfile::TempDir, String> {
    let root = cache_root()?;
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    builder.create(&root).map_err(|error| format!("Cannot prepare private TUI cache: {error}"))?;
    let metadata = std::fs::symlink_metadata(&root)
        .map_err(|error| format!("Cannot inspect private TUI cache: {error}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("The TUI cache must be a regular directory, not a symlink.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("Cannot secure private TUI cache: {error}"))?;
    }
    prune_abandoned(&root);
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(root)
        .map_err(|error| format!("Cannot create private TUI temporary directory: {error}"))
}

fn cache_root() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Caches"));
    #[cfg(not(any(windows, target_os = "macos")))]
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")));
    base.filter(|path| path.is_absolute()).map(|path| path.join("peritus/tui-local"))
        .ok_or_else(|| "A private user cache directory is unavailable; set the normal user home/cache location.".into())
}

fn prune_abandoned(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.take(128).flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("clipboard-")
            && !name.to_string_lossy().starts_with("editor-")
        {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else { continue };
        if metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && metadata
                .modified()
                .ok()
                .and_then(|time| SystemTime::now().duration_since(time).ok())
                .is_some_and(|age| age >= Duration::from_hours(24))
        {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

pub(super) fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file =
        options.open(path).map_err(|error| format!("Cannot create private TUI file: {error}"))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("Cannot save private TUI file: {error}"))
}
