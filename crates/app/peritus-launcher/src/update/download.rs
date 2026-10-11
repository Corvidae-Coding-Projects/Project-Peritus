//! Resumable checksum-bound release download and cancellable native extraction.

use std::{fs, io::Write as _, path::PathBuf, process::Command};

use futures_util::StreamExt as _;
use sha2::{Digest as _, Sha256};
use tokio::io::AsyncReadExt as _;

use crate::{AppLayout, LauncherError};

use super::release::Release;

const RELEASE_BASE: &str =
    "https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases/download";
pub(super) async fn package(
    layout: &AppLayout,
    release: &Release,
) -> Result<PathBuf, LauncherError> {
    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| network("construct update download client", &error))?;
    let asset = asset_name()?;
    let root = layout.cache_root().join("updates").join(release.tag());
    fs::create_dir_all(&root).map_err(|error| {
        LauncherError::filesystem("create update staging directory", &root, error)
    })?;
    let lock_path = root.join("download.lock");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| LauncherError::filesystem("open update lock", &lock_path, error))?;
    lock.try_lock().map_err(|error| {
        LauncherError::Update(format!("cannot exclusively own release staging: {error}"))
    })?;
    let archive = root.join(&asset);
    let url = format!("{RELEASE_BASE}/{}/{}", release.tag(), asset);
    let expected = checksum(&client, &format!("{url}.sha256")).await?;
    tokio::select! {
        result = receive(&client, &url, &archive, &expected) => result?,
        signal = tokio::signal::ctrl_c() => {
            signal.map_err(|error| LauncherError::Update(format!("listen for download cancellation: {error}")))?;
            return Err(LauncherError::Update("download cancelled; exact staging is retained for retry".into()));
        }
    }
    extract_package(&archive, &root, asset.trim_end_matches(archive_suffix())).await
}

async fn extract_package(
    archive: &std::path::Path,
    root: &std::path::Path,
    name: &str,
) -> Result<PathBuf, LauncherError> {
    // Each install consumes an immutable attempt directory. Another download may reuse
    // the release archive, but cannot replace scripts/binaries beneath a running installer.
    let attempt = tempfile::Builder::new()
        .prefix("attempt-")
        .tempdir_in(root)
        .map_err(|error| {
            LauncherError::filesystem("create exact extraction directory", root, error)
        })?
        .keep();
    extract(archive, &attempt).await?;
    let bundle = attempt.join(name);
    if !bundle.is_dir() {
        return Err(LauncherError::Update(format!(
            "release archive omitted package directory {}",
            bundle.display()
        )));
    }
    Ok(bundle)
}

async fn checksum(client: &reqwest::Client, url: &str) -> Result<[u8; 32], LauncherError> {
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "peritus-updater")
        .send()
        .await
        .map_err(|error| network("download release checksum", &error))?
        .error_for_status()
        .map_err(|error| network("download release checksum", &error))?;
    let bytes = response.bytes().await.map_err(|error| network("read release checksum", &error))?;
    parse_checksum(&bytes)
}

async fn receive(
    client: &reqwest::Client,
    url: &str,
    path: &std::path::Path,
    expected: &[u8; 32],
) -> Result<(), LauncherError> {
    let identity = path.with_extension("sha256-bound");
    let prior = fs::read(&identity).ok();
    if prior.as_deref() != Some(expected.as_slice()) {
        // A checksum change invalidates only the exact archive, never unrelated staging.
        if path.exists() {
            fs::remove_file(path)
                .map_err(|error| LauncherError::filesystem("discard stale archive", path, error))?;
        }
        fs::write(&identity, expected)
            .map_err(|error| LauncherError::filesystem("bind update checksum", &identity, error))?;
    }
    let (mut hasher, offset) = hash_prefix(path).await?;
    let digest: [u8; 32] = hasher.clone().finalize().into();
    if path.exists() && &digest == expected {
        return Ok(());
    }
    let mut request = client.get(url).header(reqwest::header::USER_AGENT, "peritus-updater");
    if offset != 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
    }
    let mut response =
        request.send().await.map_err(|error| network("download release archive", &error))?;
    // An unverified or unsatisfiable range must never be appended to the prior bytes.
    if offset != 0 && response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        response = client
            .get(url)
            .header(reqwest::header::USER_AGENT, "peritus-updater")
            .send()
            .await
            .map_err(|error| network("restart release archive", &error))?;
    }
    let response =
        response.error_for_status().map_err(|error| network("download release archive", &error))?;
    let resumed = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if resumed {
        let range = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok());
        if !range.is_some_and(|value| valid_range(value, offset, response.content_length())) {
            return Err(LauncherError::Update(
                "server returned an invalid archive range; staging retained".into(),
            ));
        }
    } else if response.status() != reqwest::StatusCode::OK {
        return Err(LauncherError::Update(
            "server returned an unsupported archive response".into(),
        ));
    } else {
        hasher = Sha256::new();
    }
    // Complete each write under the staging lock before an await can cancel the owner.
    // Tokio file writes may otherwise continue in a blocking task after that lock is dropped.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .append(resumed)
        .truncate(!resumed)
        .open(path)
        .map_err(|error| LauncherError::filesystem("open update archive", path, error))?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| network("stream release archive", &error))?;
        for bytes in chunk.chunks(64 * 1024) {
            file.write_all(bytes)
                .map_err(|error| LauncherError::filesystem("write update archive", path, error))?;
        }
        hasher.update(&chunk);
    }
    file.sync_all()
        .map_err(|error| LauncherError::filesystem("sync update archive", path, error))?;
    let actual: [u8; 32] = hasher.finalize().into();
    if &actual != expected {
        return Err(LauncherError::Update(
            "release archive checksum did not match; staging retained".to_owned(),
        ));
    }
    Ok(())
}

async fn hash_prefix(path: &std::path::Path) -> Result<(Sha256, u64), LauncherError> {
    let mut hasher = Sha256::new();
    let mut offset = 0_u64;
    if path.exists() {
        let mut prior = tokio::fs::File::open(path)
            .await
            .map_err(|error| LauncherError::filesystem("open partial archive", path, error))?;
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let count = prior
                .read(&mut buffer)
                .await
                .map_err(|error| LauncherError::filesystem("hash partial archive", path, error))?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
            offset = offset
                .checked_add(u64::try_from(count).unwrap_or(u64::MAX))
                .ok_or_else(|| LauncherError::Update("archive offset overflow".into()))?;
        }
    }
    Ok((hasher, offset))
}

fn valid_range(value: &str, offset: u64, length: Option<u64>) -> bool {
    let Some((range, total)) = value.strip_prefix("bytes ").and_then(|v| v.split_once('/')) else {
        return false;
    };
    let Some((start, end)) = range.split_once('-') else {
        return false;
    };
    let (Ok(start), Ok(end), Ok(total)) =
        (start.parse::<u64>(), end.parse::<u64>(), total.parse::<u64>())
    else {
        return false;
    };
    start == offset
        && end >= start
        && end.checked_add(1) == Some(total)
        && length.is_none_or(|length| {
            end.checked_sub(start).and_then(|n| n.checked_add(1)) == Some(length)
        })
}

fn parse_checksum(bytes: &[u8]) -> Result<[u8; 32], LauncherError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| LauncherError::Update("release checksum is not UTF-8".to_owned()))?
        .trim();
    if text.len() != 64 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(LauncherError::Update("release checksum is malformed".to_owned()));
    }
    let mut result = [0_u8; 32];
    for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        result[index] = u8::from_str_radix(std::str::from_utf8(pair).unwrap_or(""), 16)
            .map_err(|_| LauncherError::Update("release checksum is malformed".to_owned()))?;
    }
    Ok(result)
}

#[cfg(not(windows))]
async fn extract(archive: &std::path::Path, root: &std::path::Path) -> Result<(), LauncherError> {
    let status = super::process::status(
        Command::new("tar").args(["-xzf"]).arg(archive).arg("-C").arg(root),
        "extract release archive",
    )
    .await?;
    success(status.success(), "release extraction failed")
}

#[cfg(windows)]
async fn extract(archive: &std::path::Path, root: &std::path::Path) -> Result<(), LauncherError> {
    let status = super::process::status(
        Command::new("powershell")
            .args([
            "-NoProfile",
            "-Command",
            "$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath $env:PERITUS_ARCHIVE_SOURCE -DestinationPath $env:PERITUS_ARCHIVE_DESTINATION -Force",
            ])
            .env("PERITUS_ARCHIVE_SOURCE", archive)
            .env("PERITUS_ARCHIVE_DESTINATION", root),
        "extract release archive",
    ).await?;
    success(status.success(), "release extraction failed")
}

fn asset_name() -> Result<String, LauncherError> {
    let platform = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        return Err(LauncherError::Update(
            "self-update is unsupported on this platform".to_owned(),
        ));
    };
    let architecture = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => {
            return Err(LauncherError::Update(format!("self-update is unsupported on {other}")));
        }
    };
    Ok(format!("peritus-{platform}-{architecture}{}", archive_suffix()))
}

const fn archive_suffix() -> &'static str {
    if cfg!(windows) { ".zip" } else { ".tar.gz" }
}

fn success(success: bool, detail: &'static str) -> Result<(), LauncherError> {
    if success { Ok(()) } else { Err(LauncherError::Update(detail.to_owned())) }
}

fn network(operation: &'static str, error: &reqwest::Error) -> LauncherError {
    LauncherError::Update(format!("{operation}: {error}"))
}
