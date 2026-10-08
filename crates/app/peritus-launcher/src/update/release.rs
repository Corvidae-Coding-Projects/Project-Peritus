//! Exact release identity and bounded current GitHub release discovery.

use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};

use crate::LauncherError;

const LATEST_RELEASE: &str =
    "https://api.github.com/repos/Corvidae-Coding-Projects/Project-Peritus/releases/latest";
const RELEASE_BASE: &str =
    "https://github.com/Corvidae-Coding-Projects/Project-Peritus/releases/download";
const MAX_RELEASE_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_RELEASE_ASSETS: usize = 64;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReleaseAsset {
    id: u64,
    name: String,
    size: u64,
    url: String,
}

impl ReleaseAsset {
    pub(super) const fn id(&self) -> u64 { self.id }
    pub(super) fn name(&self) -> &str { &self.name }
    pub(super) const fn size(&self) -> u64 { self.size }
    pub(super) fn url(&self) -> &str { &self.url }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Release {
    id: u64,
    tag: String,
    version: Version,
    assets: Vec<ReleaseAsset>,
}

impl Release {
    pub(super) fn from_tag(tag: &str) -> Result<Self, LauncherError> {
        let version = Version::parse(tag.strip_prefix('v').ok_or_else(|| malformed(tag))?)?;
        Ok(Self { id: 1, tag: tag.to_owned(), version, assets: Vec::new() })
    }

    pub(super) const fn id(&self) -> u64 { self.id }
    pub(super) fn tag(&self) -> &str { &self.tag }
    pub(super) fn version(&self) -> String { self.version.render() }
    pub(super) fn is_newer(&self) -> bool {
        Version::parse(env!("CARGO_PKG_VERSION")).is_ok_and(|current| self.version > current)
    }

    pub(super) fn asset(&self, name: &str) -> Result<&ReleaseAsset, LauncherError> {
        let mut matching = self.assets.iter().filter(|asset| asset.name == name);
        let asset = matching.next().ok_or_else(|| {
            LauncherError::Update(format!("release {} omitted asset {name}", self.tag))
        })?;
        if matching.next().is_some() {
            return Err(LauncherError::Update(format!(
                "release {} repeats asset {name}", self.tag
            )));
        }
        Ok(asset)
    }

    pub(super) fn validate(&self) -> Result<(), LauncherError> {
        let parsed = Version::parse(
            self.tag.strip_prefix('v').ok_or_else(|| malformed(&self.tag))?,
        )?;
        if self.id == 0
            || parsed != self.version
            || self.assets.is_empty()
            || self.assets.len() > MAX_RELEASE_ASSETS
        {
            return Err(LauncherError::Update("release identity is malformed".to_owned()));
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut names = std::collections::BTreeSet::new();
        for asset in &self.assets {
            if asset.id == 0
                || asset.size == 0
                || asset.name.is_empty()
                || !asset.name.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                })
                || !ids.insert(asset.id)
                || !names.insert(asset.name.as_str())
                || asset.url != format!("{RELEASE_BASE}/{}/{}", self.tag, asset.name)
            {
                return Err(LauncherError::Update(
                    "release asset identity is malformed".to_owned(),
                ));
            }
        }
        Ok(())
    }

    fn from_document(document: &serde_json::Value) -> Result<Self, LauncherError> {
        let id = document.get("id").and_then(serde_json::Value::as_u64)
            .ok_or_else(|| LauncherError::Update("current release omitted id".to_owned()))?;
        let tag = document.get("tag_name").and_then(serde_json::Value::as_str)
            .ok_or_else(|| LauncherError::Update("current release omitted tag_name".to_owned()))?;
        let version = Version::parse(tag.strip_prefix('v').ok_or_else(|| malformed(tag))?)?;
        let values = document.get("assets").and_then(serde_json::Value::as_array)
            .ok_or_else(|| LauncherError::Update("current release omitted assets".to_owned()))?;
        if values.len() > MAX_RELEASE_ASSETS {
            return Err(LauncherError::Update(format!(
                "current release exceeds {MAX_RELEASE_ASSETS} assets"
            )));
        }
        let mut assets = Vec::new();
        assets.try_reserve(values.len()).map_err(|_| {
            LauncherError::Update("current release asset allocation is unavailable".to_owned())
        })?;
        for value in values {
            assets.push(ReleaseAsset {
                id: value.get("id").and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| LauncherError::Update("release asset omitted id".to_owned()))?,
                name: value.get("name").and_then(serde_json::Value::as_str)
                    .ok_or_else(|| LauncherError::Update("release asset omitted name".to_owned()))?
                    .to_owned(),
                size: value.get("size").and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| LauncherError::Update("release asset omitted size".to_owned()))?,
                url: value.get("browser_download_url").and_then(serde_json::Value::as_str)
                    .ok_or_else(|| LauncherError::Update(
                        "release asset omitted download URL".to_owned(),
                    ))?
                    .to_owned(),
            });
        }
        let release = Self { id, tag: tag.to_owned(), version, assets };
        release.validate()?;
        Ok(release)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    fn parse(value: &str) -> Result<Self, LauncherError> {
        let mut parts = value.split('.');
        let major = component(parts.next(), value)?;
        let minor = component(parts.next(), value)?;
        let patch = component(parts.next(), value)?;
        if parts.next().is_some() {
            return Err(malformed(value));
        }
        Ok(Self { major, minor, patch })
    }

    fn render(self) -> String { format!("{}.{}.{}", self.major, self.minor, self.patch) }
}

pub(super) async fn latest() -> Result<Option<Release>, LauncherError> {
    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| update("construct release client", &error))?;
    let response = client
        .get(LATEST_RELEASE)
        .header(reqwest::header::USER_AGENT, "peritus-updater")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|error| update("query current release", &error))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response = response.error_for_status()
        .map_err(|error| update("query current release", &error))?;
    let bytes = bounded_body(response, MAX_RELEASE_RESPONSE_BYTES, "read current release").await?;
    let document: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| LauncherError::Update(format!("decode current release: {error}")))?;
    Release::from_document(&document).map(Some)
}

pub(super) async fn bounded_body(
    response: reqwest::Response,
    limit: usize,
    operation: &'static str,
) -> Result<Vec<u8>, LauncherError> {
    if response.content_length()
        .is_some_and(|length| length > u64::try_from(limit).unwrap_or(u64::MAX))
    {
        return Err(LauncherError::Update(format!(
            "{operation}: response exceeds {limit} bytes"
        )));
    }
    let capacity = response.content_length()
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or(0);
    let mut bytes = Vec::new();
    bytes.try_reserve(capacity).map_err(|_| {
        LauncherError::Update(format!("{operation}: response allocation is unavailable"))
    })?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| update(operation, &error))?;
        let next = bytes.len().checked_add(chunk.len()).ok_or_else(|| {
            LauncherError::Update(format!("{operation}: response size overflowed"))
        })?;
        if next > limit {
            return Err(LauncherError::Update(format!(
                "{operation}: response exceeds {limit} bytes"
            )));
        }
        bytes.try_reserve(chunk.len()).map_err(|_| {
            LauncherError::Update(format!("{operation}: response allocation is unavailable"))
        })?;
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn component(value: Option<&str>, version: &str) -> Result<u64, LauncherError> {
    let value = value.ok_or_else(|| malformed(version))?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(malformed(version));
    }
    value.parse().map_err(|_| malformed(version))
}

fn malformed(value: &str) -> LauncherError {
    LauncherError::Update(format!("release version is not vMAJOR.MINOR.PATCH: {value}"))
}

fn update(operation: &'static str, error: &reqwest::Error) -> LauncherError {
    LauncherError::Update(format!("{operation}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_tags_are_exact_and_order_numerically() {
        let ten = Release::from_tag("v10.2.3").expect("release");
        let nine = Version::parse("9.99.99").expect("version");
        assert!(ten.version > nine);
        assert_eq!(ten.version(), "10.2.3");
        for malformed in ["1.2.3", "v1.2", "v1.2.3.4", "v1.2.3-rc1", "v1.two.3"] {
            assert!(Release::from_tag(malformed).is_err(), "accepted {malformed}");
        }
    }
}
