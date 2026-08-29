//! Bounded streaming release download, checksum verification, and native extraction.

use std::time::Duration;
use std::{fs, path::PathBuf, process::Command};

use futures_util::StreamExt as _;
use sha2::{Digest as _, Sha256};
use tokio::io::AsyncWriteExt as _;

use crate::{AppLayout, LauncherError};

use super::release::Release;

const RELEASE_BASE: &str =
    "https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases/download";
const MAX_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;

pub(super) async fn package(
    layout: &AppLayout,
    release: &Release,
) -> Result<PathBuf, LauncherError> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_mins(30))
        .build()
        .map_err(|error| network("construct update download client", &error))?;
    let asset = asset_name()?;
    let root = layout.cache_root().join("updates").join(release.tag());
    if root.exists() {
        fs::remove_dir_all(&root).map_err(|error| {
            LauncherError::filesystem("clear update staging directory", &root, error)
        })?;
    }
    fs::create_dir_all(&root).map_err(|error| {
        LauncherError::filesystem("create update staging directory", &root, error)
    })?;
    let archive = root.join(&asset);
    let url = format!("{RELEASE_BASE}/{}/{}", release.tag(), asset);
    let expected = checksum(&client, &format!("{url}.sha256")).await?;
    receive(&client, &url, &archive, &expected).await?;
    extract(&archive, &root)?;
    let bundle = root.join(asset.trim_end_matches(archive_suffix()));
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
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "peritus-updater")
        .send()
        .await
        .map_err(|error| network("download release archive", &error))?
        .error_for_status()
        .map_err(|error| network("download release archive", &error))?;
    if response.content_length().is_some_and(|length| length > MAX_ARCHIVE_BYTES) {
        return Err(LauncherError::Update("release archive exceeds the 1 GiB limit".to_owned()));
    }
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|error| LauncherError::filesystem("create update archive", path, error))?;
    let mut stream = response.bytes_stream();
    let mut size = 0_u64;
    let mut hasher = Sha256::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| network("stream release archive", &error))?;
        size = size
            .checked_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX))
            .ok_or_else(|| LauncherError::Update("release archive size overflowed".to_owned()))?;
        if size > MAX_ARCHIVE_BYTES {
            return Err(LauncherError::Update(
                "release archive exceeds the 1 GiB limit".to_owned(),
            ));
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|error| LauncherError::filesystem("write update archive", path, error))?;
    }
    file.sync_all()
        .await
        .map_err(|error| LauncherError::filesystem("sync update archive", path, error))?;
    let actual: [u8; 32] = hasher.finalize().into();
    if &actual != expected {
        return Err(LauncherError::Update("release archive checksum did not match".to_owned()));
    }
    Ok(())
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
fn extract(archive: &std::path::Path, root: &std::path::Path) -> Result<(), LauncherError> {
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(archive)
        .arg("-C")
        .arg(root)
        .status()
        .map_err(|error| LauncherError::Update(format!("start release extraction: {error}")))?;
    success(status.success(), "release extraction failed")
}

#[cfg(windows)]
fn extract(archive: &std::path::Path, root: &std::path::Path) -> Result<(), LauncherError> {
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath $env:PERITUS_ARCHIVE_SOURCE -DestinationPath $env:PERITUS_ARCHIVE_DESTINATION -Force",
        ])
        .env("PERITUS_ARCHIVE_SOURCE", archive)
        .env("PERITUS_ARCHIVE_DESTINATION", root)
        .status()
        .map_err(|error| LauncherError::Update(format!("start release extraction: {error}")))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_are_exact_hex() {
        let digest =
            parse_checksum(b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n")
                .expect("checksum");
        assert_eq!(digest[0], 0x01);
        assert_eq!(digest[31], 0xef);
        for invalid in
            [b"abc".as_slice(), b"g123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"]
        {
            assert!(parse_checksum(invalid).is_err());
        }
    }

    #[test]
    fn host_asset_matches_release_naming() {
        let asset = asset_name().expect("supported qualification platform");
        assert!(asset.starts_with("peritus-"));
        assert!(asset.ends_with(archive_suffix()));
    }
}
