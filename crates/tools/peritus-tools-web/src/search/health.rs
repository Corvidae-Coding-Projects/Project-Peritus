//! Query-independent, cross-process backend leases and operational backoff.
use crate::{Error, FailureKind, checked_url, invalid};
use metadata_search_engine_rs::models::SearchResult;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub(super) struct BackendOutcome {
    pub name: String,
    pub results: Vec<SearchResult>,
    pub failure: Option<SearchFailure>,
    pub diagnostics: Vec<Value>,
    pub summary: Value,
    pub metadata: std::collections::BTreeMap<String, Value>,
}
impl BackendOutcome {
    pub fn state_unavailable(name: &str) -> Self {
        Self {
            name: name.into(),
            results: Vec::new(),
            failure: Some(SearchFailure::new(
                Kind::StateUnavailable,
                0,
                "Backend operational state could not be read or persisted; no cooldown reset was attempted",
            )),
            diagnostics: Vec::new(),
            summary: Value::Null,
            metadata: std::collections::BTreeMap::new(),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    RateLimited,
    ChallengeRequired,
    Forbidden,
    AuthenticationRequired,
    Transient,
    UnrecognizedResponse,
    ParseFailure,
    UpstreamUnavailable,
    HttpFailure,
    Cooldown,
    StateUnavailable,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SearchFailure {
    pub kind: Kind,
    status: u16,
    detail: String,
    retry_after: Option<String>,
    retry_after_seconds: u64,
}
impl SearchFailure {
    pub fn new(kind: Kind, status: u16, detail: &str) -> Self {
        Self { kind, status, detail: detail.into(), retry_after: None, retry_after_seconds: 0 }
    }
    pub fn from_error(error: &Error) -> Self {
        if let Some(page) = error.page_failure() {
            let kind = match page.kind {
                FailureKind::RateLimited => Kind::RateLimited,
                FailureKind::ChallengeRequired => Kind::ChallengeRequired,
                FailureKind::Forbidden => Kind::Forbidden,
                FailureKind::AuthenticationRequired => Kind::AuthenticationRequired,
                FailureKind::Http => Kind::HttpFailure,
                _ => Kind::Transient,
            };
            let mut failure = Self::new(kind, page.status, &page.diagnostic);
            failure.retry_after = page.retry_after;
            failure
        } else {
            Self::new(Kind::ParseFailure, 0, "Search response could not be retrieved or decoded")
        }
    }
    pub fn observation(&self, engine: &str) -> Value {
        json!({"engine":engine,"kind":self.kind,"http_status":self.status,"diagnostic":self.detail,"retry_after":self.retry_after,"retry_after_seconds":self.retry_after_seconds,"next_action":"Use healthy backends, retained evidence or known authoritative URLs. Query changes do not bypass backend cooldowns."})
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Health {
    schema: u32,
    next_attempt_unix_ms: u64,
    failures: u32,
    failure: Option<SearchFailure>,
}
pub(super) struct Lease {
    _lock: File,
    path: PathBuf,
    state: Health,
}
fn now_ms() -> Result<u64, Error> {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("system clock precedes epoch"))?
        .as_millis();
    u64::try_from(ms).map_err(|_| invalid("clock exceeds search state range"))
}
impl Lease {
    pub fn acquire(
        root: &Path,
        endpoint: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, Error> {
        let directory = root.join("search-health-v1");
        fs::create_dir_all(&directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        }
        let origin = checked_url(endpoint)?.origin().ascii_serialization();
        let key = crate::source::digest(origin.as_bytes());
        let path = directory.join(format!("{key}.json"));
        let lock_path = directory.join(format!("{key}.lock"));
        if let Ok(m) = fs::symlink_metadata(&lock_path)
            && (!m.is_file() || m.file_type().is_symlink())
        {
            return Err(invalid("invalid search health lock"));
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let lock = options.open(lock_path)?;
        loop {
            if cancelled() {
                return Err(invalid("web request cancelled"));
            }
            match lock.try_lock() {
                Ok(()) => break,
                Err(fs::TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(25)),
                Err(fs::TryLockError::Error(error)) => return Err(error.into()),
            }
        }
        let state = match fs::symlink_metadata(&path) {
            Ok(m) => {
                if !m.is_file() || m.file_type().is_symlink() || m.len() > 65536 {
                    return Err(invalid("invalid search health record"));
                }
                let value: Health = serde_json::from_slice(&fs::read(&path)?)?;
                if value.schema != 1 {
                    return Err(invalid("unsupported search health record"));
                }
                value
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Health { schema: 1, ..Health::default() }
            }
            Err(e) => return Err(e.into()),
        };
        Ok(Self { _lock: lock, path, state })
    }
    pub fn unavailable(
        &self,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<SearchFailure>, Error> {
        while self.state.failure.is_none() && self.state.next_attempt_unix_ms > now_ms()? {
            if cancelled() {
                return Err(invalid("web request cancelled"));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let remaining = self.state.next_attempt_unix_ms.saturating_sub(now_ms()?);
        if remaining == 0 {
            return Ok(None);
        }
        let mut failure = self.state.failure.clone().unwrap_or_else(|| {
            SearchFailure::new(Kind::Cooldown, 0, "Backend request spacing is active")
        });
        failure.retry_after_seconds = remaining.div_ceil(1000);
        Ok(Some(failure))
    }
    pub fn succeeded(&mut self) -> Result<(), Error> {
        self.state = Health {
            schema: 1,
            next_attempt_unix_ms: now_ms()?.saturating_add(1000),
            failures: 0,
            failure: None,
        };
        self.persist()
    }
    pub fn failed(&mut self, mut failure: SearchFailure) -> Result<SearchFailure, Error> {
        self.state.failures = self.state.failures.saturating_add(1);
        let delay = failure
            .retry_after
            .as_deref()
            .and_then(|value| {
                value.parse::<u64>().ok().or_else(|| {
                    httpdate::parse_http_date(value).ok().map(|time| {
                        u64::try_from(
                            time.duration_since(SystemTime::now())
                                .unwrap_or_default()
                                .as_millis()
                                .div_ceil(1000),
                        )
                        .unwrap_or(u64::MAX)
                    })
                })
            })
            .unwrap_or_else(|| match failure.kind {
                Kind::RateLimited => {
                    60_u64.saturating_mul(1_u64 << self.state.failures.saturating_sub(1).min(4))
                }
                Kind::ChallengeRequired
                | Kind::Forbidden
                | Kind::AuthenticationRequired
                | Kind::UnrecognizedResponse
                | Kind::ParseFailure
                | Kind::HttpFailure => 300,
                _ => 2_u64.saturating_mul(1_u64 << self.state.failures.saturating_sub(1).min(4)),
            })
            .max(1);
        failure.retry_after_seconds = delay;
        self.state.next_attempt_unix_ms = now_ms()?.saturating_add(delay.saturating_mul(1000));
        self.state.failure = Some(failure.clone());
        self.persist()?;
        Ok(failure)
    }
    fn persist(&self) -> Result<(), Error> {
        let parent = self.path.parent().ok_or_else(|| invalid("search health has no parent"))?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&serde_json::to_vec(&self.state)?)?;
        file.as_file().sync_all()?;
        file.persist(&self.path).map_err(|e| e.error)?;
        Ok(())
    }
}
