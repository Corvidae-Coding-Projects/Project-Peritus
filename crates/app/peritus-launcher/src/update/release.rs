//! Exact release identity and current GitHub release discovery.

use crate::LauncherError;

const LATEST_RELEASE: &str =
    "https://api.github.com/repos/Corvidae-Coding-Projects/Project-Peritus/releases/latest";
const MAX_RELEASE_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Release {
    tag: String,
    version: Version,
}

impl Release {
    pub(super) fn from_tag(tag: &str) -> Result<Self, LauncherError> {
        let version = Version::parse(tag.strip_prefix('v').ok_or_else(|| malformed(tag))?)?;
        Ok(Self { tag: tag.to_owned(), version })
    }

    pub(super) fn tag(&self) -> &str {
        &self.tag
    }

    pub(super) fn version(&self) -> String {
        self.version.render()
    }

    pub(super) fn is_newer(&self) -> bool {
        Version::parse(env!("CARGO_PKG_VERSION")).is_ok_and(|current| self.version > current)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
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

    fn render(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

pub(super) async fn latest() -> Result<Option<Release>, LauncherError> {
    latest_from(LATEST_RELEASE).await
}

pub(super) async fn latest_from(url: &str) -> Result<Option<Release>, LauncherError> {
    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| update("construct release client", &error))?;
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "peritus-updater")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|error| update("query current release", &error))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let mut response =
        response.error_for_status().map_err(|error| update("query current release", &error))?;
    if response.content_length().is_some_and(|length| {
        usize::try_from(length).map_or(true, |length| length > MAX_RELEASE_RESPONSE_BYTES)
    }) {
        return Err(response_too_large());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) =
        response.chunk().await.map_err(|error| update("read current release", &error))?
    {
        if chunk.len() > MAX_RELEASE_RESPONSE_BYTES - bytes.len() {
            return Err(response_too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    let document: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| LauncherError::Update(format!("decode current release: {error}")))?;
    let tag = document
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| LauncherError::Update("current release omitted tag_name".to_owned()))?;
    Release::from_tag(tag).map(Some)
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

fn response_too_large() -> LauncherError {
    LauncherError::Update("current release response exceeds 4 MiB".to_owned())
}
