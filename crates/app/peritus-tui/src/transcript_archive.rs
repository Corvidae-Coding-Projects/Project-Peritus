//! Private, incremental public-transcript cache and local literal search.
//!
//! Only activities actually observed by this client are retained. The first sequence is recorded
//! separately so callers can explain when an older, never-captured portion is unavailable.

use peritus_app_protocol::{MAX_PRODUCT_ACTIVITY_BYTES, ProductActivity, ProductActivityKind};
use regex::{Regex, RegexBuilder};
use sha2::{Digest as _, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead as _, BufReader, Read as _, Write as _},
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use tokio::{io::AsyncReadExt as _, process::Command};

const MAX_HITS: usize = 200;
const MAX_BODY_BYTES: usize = MAX_PRODUCT_ACTIVITY_BYTES * 2 + 1;
const MAX_HEADER_BYTES: usize = 256;
const MAX_QUERY_BYTES: usize = MAX_PRODUCT_ACTIVITY_BYTES;
const MAX_SEARCH_OUTPUT_BYTES: usize = 256 * 1024;
const SEARCH_TIME: Duration = Duration::from_secs(5);
static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

/// One complete captured public activity; bodies remain exact until presentation sanitizes them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SearchHit {
    pub(crate) sequence: u64,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) body: String,
}

/// Bounded local matches, including the beginning of the actually captured history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SearchPage {
    pub(crate) hits: Vec<SearchHit>,
    pub(crate) truncated: bool,
    pub(crate) first_sequence: Option<u64>,
}

/// Records an observed projection on the caller's owned blocking worker.
///
/// Same-sequence streaming updates replace one record atomically. Unchanged content performs no
/// writes, and neither startup nor recording enumerates the complete run archive.
pub(crate) fn record(scope: &str, activities: &[ProductActivity]) -> Result<(), String> {
    validate_scope(scope)?;
    if activities.is_empty() {
        return Ok(());
    }
    let directory = archive_directory(scope, true)?
        .ok_or_else(|| "The transcript cache directory is unavailable.".to_owned())?;
    let lock = open_private(&directory.join("archive.lock"), true)?;
    lock_archive(&lock)?;
    check_directory(&directory, true)?;
    let mut changed = false;
    let first = activities.iter().map(ProductActivity::sequence).min();
    if let Some(first) = first {
        let captured = first_sequence(&directory)?;
        if captured.is_none_or(|existing| first < existing) {
            atomic_write(&directory, "first-sequence", first.to_string().as_bytes())?;
            changed = true;
        }
    }
    for activity in activities {
        let (header, body) = encode(activity)?;
        let name = activity_name(activity.sequence());
        let path = directory.join(&name);
        if read_header_if_present(&path)?.as_deref() == Some(header.as_str()) {
            continue;
        }
        let mut bytes = header.into_bytes();
        bytes.extend_from_slice(body.as_bytes());
        atomic_write(&directory, &name, &bytes)?;
        changed = true;
    }
    if changed {
        sync_directory(&directory)?;
    }
    Ok(())
}

/// Searches only this run's captured public transcript, without any daemon/provider operation.
pub(crate) async fn search(scope: &str, query: &str) -> Result<SearchPage, String> {
    validate_scope(scope)?;
    if query.is_empty() || query.len() > MAX_QUERY_BYTES {
        return Err(format!("Enter a search between 1 and {MAX_QUERY_BYTES} bytes."));
    }
    let scope = scope.to_owned();
    let literal = query.to_owned();
    let prepared = tokio::task::spawn_blocking(move || {
        let matcher = RegexBuilder::new(&regex::escape(&literal))
            .case_insensitive(true)
            .size_limit(2 * 1024 * 1024)
            .build()
            .map_err(|error| format!("Cannot prepare local literal transcript search: {error}"))?;
        let directory = archive_directory(&scope, false)?;
        let first = directory.as_deref().map(first_sequence).transpose()?.flatten();
        Ok::<_, String>((directory, first, matcher))
    })
    .await
    .map_err(|error| format!("Transcript cache inspection failed: {error}"))??;
    let (directory, first, matcher) = prepared;
    let Some(directory) = directory else {
        return Ok(SearchPage { hits: Vec::new(), truncated: false, first_sequence: None });
    };
    // Ripgrep treats LF-separated patterns as alternatives. The Rust path preserves one literal
    // multiline query, and also supports NUL without trying to put it in a process argument.
    if query.contains(['\n', '\0']) {
        return fallback_search(directory, matcher, first).await;
    }
    let mut command = Command::new("rg");
    command
        .args([
            "--no-config",
            "--no-ignore",
            "--hidden",
            "--max-depth",
            "1",
            "--glob",
            "*.activity",
            "--threads",
            "1",
            "--no-mmap",
            "--text",
            "--fixed-strings",
            "--ignore-case",
            "--files-with-matches",
            "--null",
            "--max-filesize",
        ])
        .arg((MAX_BODY_BYTES + MAX_HEADER_BYTES).to_string())
        .arg("--")
        .arg(query)
        .arg(".")
        .current_dir(&directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return fallback_search(directory, matcher, first).await;
        }
        Err(error) => return Err(format!("Cannot start local transcript search: {error}")),
    };
    let Some(stdout) = child.stdout.take() else {
        stop_search(&mut child).await;
        return Err("Local transcript search has no output stream.".to_owned());
    };
    let mut hits = Vec::new();
    let result =
        tokio::time::timeout(SEARCH_TIME, read_matches(stdout, &directory, &matcher, &mut hits))
            .await;
    let mut truncated = match result {
        Ok(Ok(truncated)) => truncated,
        Ok(Err(error)) => {
            stop_search(&mut child).await;
            return Err(error);
        }
        Err(_) => true,
    };
    if truncated {
        stop_search(&mut child).await;
    } else {
        match tokio::time::timeout(Duration::from_secs(1), child.wait()).await {
            Ok(Ok(status)) if status.success() || status.code() == Some(1) => {}
            Ok(Ok(status)) => {
                return Err(format!("Local transcript search failed ({status})."));
            }
            Ok(Err(error)) => return Err(format!("Cannot finish transcript search: {error}")),
            Err(_) => {
                stop_search(&mut child).await;
                truncated = true;
            }
        }
    }
    Ok(finish_page(hits, truncated, first))
}

async fn read_matches(
    mut stdout: tokio::process::ChildStdout,
    directory: &Path,
    matcher: &Regex,
    hits: &mut Vec<SearchHit>,
) -> Result<bool, String> {
    let mut buffer = [0_u8; 4096];
    let mut pending = Vec::new();
    let mut total = 0_usize;
    loop {
        let count = stdout
            .read(&mut buffer)
            .await
            .map_err(|error| format!("Cannot read transcript search output: {error}"))?;
        if count == 0 {
            if !pending.is_empty() {
                return Err("Local transcript search returned an incomplete filename.".to_owned());
            }
            return Ok(false);
        }
        total = total.saturating_add(count);
        if total > MAX_SEARCH_OUTPUT_BYTES {
            return Ok(true);
        }
        for byte in &buffer[..count] {
            if *byte != 0 {
                if pending.len() >= 128 {
                    return Err("Local transcript search returned an invalid filename.".to_owned());
                }
                pending.push(*byte);
                continue;
            }
            let name = std::str::from_utf8(&pending)
                .map_err(|_| "Local transcript search returned a non-UTF-8 filename.".to_owned())?;
            let name = name.strip_prefix("./").or_else(|| name.strip_prefix(".\\")).unwrap_or(name);
            if let Some(sequence) = sequence_from_name(name) {
                let path = directory.join(name);
                let matcher = matcher.clone();
                let found =
                    tokio::task::spawn_blocking(move || matching_hit(&path, sequence, &matcher))
                        .await
                        .map_err(|error| format!("Transcript match loading failed: {error}"))??;
                if let Some(hit) = found {
                    hits.push(hit);
                    if hits.len() > MAX_HITS {
                        return Ok(true);
                    }
                }
            }
            pending.clear();
        }
    }
}

async fn stop_search(child: &mut tokio::process::Child) {
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
}

async fn fallback_search(
    directory: PathBuf,
    matcher: Regex,
    first: Option<u64>,
) -> Result<SearchPage, String> {
    tokio::task::spawn_blocking(move || {
        let started = Instant::now();
        let mut hits = Vec::new();
        let mut truncated = false;
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("Cannot read the transcript cache: {error}"))?
        {
            if started.elapsed() >= SEARCH_TIME {
                truncated = true;
                break;
            }
            let entry =
                entry.map_err(|error| format!("Cannot inspect a transcript file: {error}"))?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Some(sequence) = sequence_from_name(&name) else {
                continue;
            };
            if let Some(hit) = matching_hit(&entry.path(), sequence, &matcher)? {
                hits.push(hit);
                if hits.len() > MAX_HITS {
                    truncated = true;
                    break;
                }
            }
        }
        Ok(finish_page(hits, truncated, first))
    })
    .await
    .map_err(|error| format!("Local transcript search failed: {error}"))?
}

fn finish_page(
    mut hits: Vec<SearchHit>,
    truncated: bool,
    first_sequence: Option<u64>,
) -> SearchPage {
    hits.sort_by_key(|hit| hit.sequence);
    hits.truncate(MAX_HITS);
    SearchPage { hits, truncated, first_sequence }
}

fn matching_hit(path: &Path, sequence: u64, matcher: &Regex) -> Result<Option<SearchHit>, String> {
    let file = open_private(path, false)?;
    let mut bytes = Vec::new();
    file.take((MAX_BODY_BYTES + MAX_HEADER_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read a captured transcript activity: {error}"))?;
    if bytes.len() > MAX_BODY_BYTES + MAX_HEADER_BYTES {
        return Err("A captured transcript activity exceeds its declared public size.".to_owned());
    }
    let Some(newline) = bytes.iter().position(|byte| *byte == b'\n') else {
        return Err("A captured transcript activity has an invalid header.".to_owned());
    };
    let header = std::str::from_utf8(&bytes[..newline])
        .map_err(|_| "A captured transcript activity has an invalid header.".to_owned())?;
    let fields = header.split(' ').collect::<Vec<_>>();
    if fields.len() != 7
        || fields[0] != "PERITUS-TUI-1"
        || fields[1].parse::<u64>().ok() != Some(sequence)
    {
        return Err("A captured transcript activity has an invalid identity.".to_owned());
    }
    let kind =
        fields[2].parse::<u16>().ok().and_then(ProductActivityKind::from_tag).ok_or_else(|| {
            "A captured transcript activity has an invalid public kind.".to_owned()
        })?;
    let title_bytes = fields[3]
        .parse::<usize>()
        .map_err(|_| "A captured transcript activity has an invalid title length.".to_owned())?;
    let body_bytes = fields[4]
        .parse::<usize>()
        .map_err(|_| "A captured transcript activity has an invalid body length.".to_owned())?;
    if fields[5] != "sha256"
        || body_bytes > MAX_BODY_BYTES
        || title_bytes > MAX_PRODUCT_ACTIVITY_BYTES
    {
        return Err("A captured transcript activity has invalid size metadata.".to_owned());
    }
    let body = std::str::from_utf8(&bytes[newline + 1..])
        .map_err(|_| "A captured transcript activity is not complete UTF-8 text.".to_owned())?;
    if body.len() != body_bytes || digest(kind.tag(), title_bytes, body) != fields[6] {
        return Err("A captured transcript activity failed its content check.".to_owned());
    }
    let title = body.get(..title_bytes).ok_or_else(|| {
        "A captured transcript activity has an invalid title boundary.".to_owned()
    })?;
    // The on-disk header is not public conversation content and must never become a match.
    if !matcher.is_match(body) {
        return Ok(None);
    }
    Ok(Some(SearchHit {
        sequence,
        kind: match kind {
            ProductActivityKind::User => "user",
            ProductActivityKind::Assistant => "assistant",
            ProductActivityKind::Tool => "tool",
            ProductActivityKind::Status => "status",
            ProductActivityKind::Error => "error",
        }
        .to_owned(),
        title: title.to_owned(),
        body: body.to_owned(),
    }))
}

fn encode(activity: &ProductActivity) -> Result<(String, String), String> {
    if activity.sequence() == 0
        || activity.text().len() > MAX_PRODUCT_ACTIVITY_BYTES
        || activity.detail().len() > MAX_PRODUCT_ACTIVITY_BYTES
    {
        return Err("Cannot cache an invalid public transcript activity.".to_owned());
    }
    let mut body = activity.text().to_owned();
    if !activity.detail().is_empty() {
        body.push('\n');
        body.push_str(activity.detail());
    }
    let digest = digest(activity.kind().tag(), activity.text().len(), &body);
    Ok((
        format!(
            "PERITUS-TUI-1 {} {} {} {} sha256 {digest}\n",
            activity.sequence(),
            activity.kind().tag(),
            activity.text().len(),
            body.len()
        ),
        body,
    ))
}

fn digest(kind: u16, title_bytes: usize, body: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(kind.to_be_bytes());
    hash.update((title_bytes as u64).to_be_bytes());
    hash.update(body.as_bytes());
    hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn read_header_if_present(path: &Path) -> Result<Option<String>, String> {
    if fs::symlink_metadata(path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound) {
        return Ok(None);
    }
    let file = open_private(path, false)?;
    let mut bytes = Vec::new();
    BufReader::new(file.take(MAX_HEADER_BYTES as u64))
        .read_until(b'\n', &mut bytes)
        .map_err(|error| format!("Cannot inspect captured transcript activity: {error}"))?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "A captured transcript activity has an invalid header.".to_owned())
}

fn first_sequence(directory: &Path) -> Result<Option<u64>, String> {
    let path = directory.join("first-sequence");
    if fs::symlink_metadata(&path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return Ok(None);
    }
    let mut value = String::new();
    open_private(&path, false)?
        .take(32)
        .read_to_string(&mut value)
        .map_err(|error| format!("Cannot read captured transcript history metadata: {error}"))?;
    value
        .parse::<u64>()
        .ok()
        .filter(|sequence| *sequence != 0)
        .map(Some)
        .ok_or_else(|| "Captured transcript history metadata is invalid.".to_owned())
}

fn activity_name(sequence: u64) -> String {
    format!("{sequence:020}.activity")
}

fn sequence_from_name(name: &str) -> Option<u64> {
    let sequence = name.strip_suffix(".activity")?;
    if sequence.len() != 20 || !sequence.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    sequence.parse::<u64>().ok().filter(|sequence| *sequence != 0)
}

fn validate_scope(scope: &str) -> Result<(), String> {
    if scope.len() != 32 || !scope.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(
            "Transcript search requires an exact 32-digit hexadecimal run identity.".to_owned()
        );
    }
    Ok(())
}

fn archive_directory(scope: &str, create: bool) -> Result<Option<PathBuf>, String> {
    validate_scope(scope)?;
    let base = cache_home()?;
    if !base.is_absolute() || base.components().any(|part| matches!(part, Component::ParentDir)) {
        return Err(
            "The transcript cache location must be an absolute path without '..'.".to_owned()
        );
    }
    if create {
        create_directory(&base, true)?;
    } else if !base.exists() {
        return Ok(None);
    }
    let base = base
        .canonicalize()
        .map_err(|error| format!("Cannot resolve the transcript cache location: {error}"))?;
    check_directory(&base, false)?;
    check_ancestors(&base)?;
    let mut path = base;
    let scope = scope.to_ascii_lowercase();
    for (name, private) in [("peritus", false), ("tui-transcripts", true), (scope.as_str(), true)] {
        path.push(name);
        if create {
            create_directory(&path, false)?;
        } else if fs::symlink_metadata(&path)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            return Ok(None);
        }
        check_directory(&path, private)?;
        if path
            .canonicalize()
            .map_err(|error| format!("Cannot resolve transcript directory: {error}"))?
            != path
        {
            return Err("The transcript cache must not contain symlinked directories.".to_owned());
        }
    }
    Ok(Some(path))
}

fn cache_home() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("XDG_CACHE_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "No user cache directory is configured for transcript search.".to_owned())?;
    Ok(PathBuf::from(home).join(if cfg!(target_os = "macos") {
        "Library/Caches"
    } else {
        ".cache"
    }))
}

fn create_directory(path: &Path, recursive: bool) -> Result<(), String> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(recursive);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(format!("Cannot create private transcript cache directory: {error}")),
    }
}

fn check_directory(path: &Path, private: bool) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Cannot inspect transcript cache directory: {error}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || is_reparse(&metadata) {
        return Err(
            "The transcript cache must use regular directories without symlinks.".to_owned()
        );
    }
    check_owner(&metadata, private)
}

fn check_ancestors(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let user = nix::unistd::geteuid().as_raw();
        for ancestor in path.ancestors().skip(1) {
            let metadata = fs::symlink_metadata(ancestor)
                .map_err(|error| format!("Cannot inspect the transcript cache parent: {error}"))?;
            let trusted_owner = metadata.uid() == user || metadata.uid() == 0;
            // Root-owned sticky directories such as /tmp protect each user's child from rename.
            let protected_shared = metadata.uid() == 0 && metadata.mode() & 0o1000 != 0;
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || !trusted_owner
                || (metadata.mode() & 0o022 != 0 && !protected_shared)
            {
                return Err("A transcript cache parent can be replaced by another user; choose a private user cache location.".to_owned());
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn open_private(path: &Path, create: bool) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true).write(create).create(create).truncate(false);
    no_follow(&mut options);
    let file = options
        .open(path)
        .map_err(|error| format!("Cannot open private transcript cache file: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Cannot inspect private transcript cache file: {error}"))?;
    if !metadata.is_file() || is_reparse(&metadata) {
        return Err("The transcript cache must contain regular private files.".to_owned());
    }
    check_owner(&metadata, true)?;
    Ok(file)
}

fn no_follow(options: &mut OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    #[cfg(not(any(unix, windows)))]
    let _ = options;
}

fn check_owner(metadata: &fs::Metadata, private: bool) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let forbidden = if private { 0o077 } else { 0o022 };
        if metadata.uid() != nix::unistd::geteuid().as_raw() || metadata.mode() & forbidden != 0 {
            return Err("The transcript cache must be owned by this user and private (chmod 700 for transcript directories, 600 for files).".to_owned());
        }
    }
    #[cfg(not(unix))]
    let _ = (metadata, private);
    Ok(())
}

fn is_reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

fn atomic_write(directory: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    check_directory(directory, true)?;
    let mut temporary = None;
    for _ in 0..32 {
        let id = TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(".pending-{}-{id}", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        no_follow(&mut options);
        match options.open(&path) {
            Ok(file) => {
                temporary = Some((path, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("Cannot prepare a transcript cache write: {error}")),
        }
    }
    let (path, mut file) =
        temporary.ok_or_else(|| "Cannot reserve a unique transcript cache write.".to_owned())?;
    let result = (|| {
        file.write_all(bytes).map_err(|error| format!("Cannot write transcript cache: {error}"))?;
        file.sync_all().map_err(|error| format!("Cannot persist transcript cache: {error}"))?;
        drop(file);
        check_directory(directory, true)?;
        fs::rename(&path, directory.join(name))
            .map_err(|error| format!("Cannot commit transcript cache write: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

fn lock_archive(file: &File) -> Result<(), String> {
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(()),
            Err(fs::TryLockError::WouldBlock) if started.elapsed() < Duration::from_secs(2) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(fs::TryLockError::WouldBlock) => {
                return Err(
                    "The public transcript cache is busy; its next observation will retry."
                        .to_owned(),
                );
            }
            Err(fs::TryLockError::Error(error)) => {
                return Err(format!("Cannot lock the transcript cache: {error}"));
            }
        }
    }
}

fn sync_directory(directory: &Path) -> Result<(), String> {
    #[cfg(unix)]
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("Cannot persist transcript cache directory: {error}"))?;
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}
