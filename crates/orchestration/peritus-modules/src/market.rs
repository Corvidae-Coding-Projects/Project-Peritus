//! GitHub-backed catalog. Install only declared inert files at an immutable commit.
use crate::{Definition, Error, Registry, invalid, publish};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Official catalog; no credentials, Git executable or install scripts are required.
pub const MARKET_REPOSITORY: &str =
    "https://github.com/Corvidae-Coding-Projects/Peritus-Module-Market";
const API: &str = "https://api.github.com/repos/Corvidae-Coding-Projects/Peritus-Module-Market";
const RAW: &str =
    "https://raw.githubusercontent.com/Corvidae-Coding-Projects/Peritus-Module-Market";

/// Explicit human registry actions shared by local menus.
#[derive(Clone, Debug)]
pub enum ModuleOperation {
    /// Read cached discovery and installed selections.
    Browse,
    /// Browse discovery with this exact run's native module actions.
    Session {
        /// Hexadecimal product run identity.
        run: String,
    },
    /// Fetch a new catalog snapshot.
    Refresh,
    /// Install exactly the version displayed by the menu.
    Install {
        /// Module identity.
        id: String,
        /// Expected package digest.
        digest: String,
    },
    /// Change enablement without widening permissions.
    Enable {
        /// Module identity.
        id: String,
        /// Desired state.
        enabled: bool,
    },
    /// Remove an installed selection, retaining history.
    Remove {
        /// Module identity.
        id: String,
    },
}
impl ModuleOperation {
    /// Runs a local management operation outside UI/async executor threads.
    pub fn execute(self) -> Result<Value, Error> {
        let registry = Registry::for_user()?;
        let result = match self {
            Self::Browse => return registry.market(),
            Self::Session { run } => return registry.market_for_session(Some(&run)),
            Self::Refresh => return registry.refresh_market(),
            Self::Install { id, digest } => registry.install_market(&id, &digest),
            Self::Enable { id, enabled } => registry.enable(&id, enabled),
            Self::Remove { id } => registry.remove(&id),
        }?;
        let mut view = registry.market()?;
        view["result"] = result;
        Ok(view)
    }
}

/// One independently versioned package advertised by the market.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketEntry {
    /// Stable package identifier, checked against the downloaded manifest.
    pub id: String,
    /// Human-readable label.
    pub name: String,
    /// Numeric package version.
    pub version: String,
    /// Plain text purpose displayed before installation.
    pub description: String,
    /// Safe package directory relative to the repository root.
    pub path: String,
    /// Exact module digest, including all declared resources.
    pub digest: String,
    /// Optional discovery categories; never grant capabilities.
    #[serde(default)]
    pub tags: Vec<String>,
}
/// Versioned, extensible inventory containing many independent modules.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    /// Catalog format, currently one.
    pub schema: u32,
    /// Human label.
    pub name: String,
    /// Independent entries with unique identities and paths.
    pub modules: Vec<MarketEntry>,
}
impl Catalog {
    /// Validates a catalog independently of network and local registry state.
    pub fn parse(text: &str) -> Result<Self, Error> {
        if text.len() > 4 * 1024 * 1024 {
            return Err(invalid("market catalog exceeds 4 MiB"));
        }
        let catalog: Self = toml::from_str(text)?;
        catalog.validate()?;
        Ok(catalog)
    }
    fn validate(&self) -> Result<(), Error> {
        if self.schema != 1 || !plain(&self.name) {
            return Err(invalid("unsupported or invalid market catalog"));
        }
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for entry in &self.modules {
            if !crate::definition::identifier(&entry.id)
                || !ids.insert(&entry.id)
                || !paths.insert(&entry.path)
                || !plain(&entry.name)
                || !plain(&entry.description)
                || !crate::definition::version(&entry.version)
                || !hex(&entry.digest, 64)
                || !safe_path(&entry.path)
                || entry.tags.iter().any(|s| !plain(s))
            {
                return Err(invalid(
                    "invalid/repeated market identity, version, metadata, path or digest",
                ));
            }
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema: u32,
    commit: String,
    fetched_at_unix: u64,
    catalog: Catalog,
}
fn plain(s: &str) -> bool {
    !s.trim().is_empty() && !s.chars().any(char::is_control)
}
fn hex(s: &str, count: usize) -> bool {
    s.len() == count && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn safe_path(s: &str) -> bool {
    !s.is_empty()
        && s.split('/').all(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
}
fn download(url: &str, maximum: u64) -> Result<Vec<u8>, Error> {
    let mut response = reqwest::blocking::Client::builder()
        .user_agent(concat!("Peritus/", env!("CARGO_PKG_VERSION"), " module-market"))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?
        .get(url)
        .send()?
        .error_for_status()?;
    if response.content_length().is_some_and(|n| n > maximum) {
        return Err(invalid("market response exceeds the per-file transfer bound"));
    }
    let mut bytes = Vec::new();
    response.by_ref().take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(invalid("market response exceeds the per-file transfer bound"));
    }
    Ok(bytes)
}
impl Registry {
    /// Saves only generic UI actions from a native plugin, bound to its exact package and run.
    pub fn publish_session_view(
        &self,
        run: &str,
        id: &str,
        digest: &str,
        view: &Value,
    ) -> Result<(), Error> {
        if !hex(run, 32) || !crate::definition::identifier(id) || !hex(digest, 64) {
            return Err(invalid("invalid plugin presentation identity"));
        }
        publish(
            &self.root().join("sessions").join(format!("{run}.json")),
            &serde_json::to_vec(&json!({"schema":1,"module":id,"digest":digest,"view":view}))?,
        )
    }
    /// Reads cached discovery and the selected run's plugin-supplied action menu.
    pub fn market_for_session(&self, run: Option<&str>) -> Result<Value, Error> {
        let mut view = self.market()?;
        if let Some(run) = run {
            if !hex(run, 32) {
                return Err(invalid("invalid module presentation run"));
            }
            match fs::read(self.root().join("sessions").join(format!("{run}.json"))) {
                Ok(bytes) => {
                    let session: Value = serde_json::from_slice(&bytes)?;
                    if session["schema"] != 1 {
                        return Err(invalid("unsupported plugin presentation"));
                    }
                    view["session"] = session;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(view)
    }
    fn market_snapshot(&self) -> Result<Snapshot, Error> {
        let snapshot: Snapshot =
            serde_json::from_slice(&fs::read(self.root().join("market/catalog.json"))?)?;
        if snapshot.schema != 1 || !hex(&snapshot.commit, 40) {
            return Err(invalid("invalid market snapshot"));
        }
        snapshot.catalog.validate()?;
        Ok(snapshot)
    }
    /// Refreshes explicitly from main, resolving it once to an immutable Git commit.
    /// On network/validation failure the last usable snapshot and selections remain intact.
    pub fn refresh_market(&self) -> Result<Value, Error> {
        let reference: Value =
            serde_json::from_slice(&download(&format!("{API}/git/ref/heads/main"), 65536)?)?;
        let commit = reference["object"]["sha"]
            .as_str()
            .filter(|s| hex(s, 40))
            .ok_or_else(|| invalid("market main did not resolve to a Git commit"))?;
        let bytes = download(&format!("{RAW}/{commit}/market.toml"), 4 * 1024 * 1024)?;
        let catalog = Catalog::parse(
            std::str::from_utf8(&bytes).map_err(|_| invalid("market catalog must be UTF-8"))?,
        )?;
        let snapshot = Snapshot {
            schema: 1,
            commit: commit.into(),
            fetched_at_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| invalid(e.to_string()))?
                .as_secs(),
            catalog,
        };
        publish(&self.root().join("market/catalog.json"), &serde_json::to_vec_pretty(&snapshot)?)?;
        self.market()
    }
    /// Cached discovery plus installed state; opening this view never requires network access.
    pub fn market(&self) -> Result<Value, Error> {
        let installed = self.list()?;
        let snapshot = self.market_snapshot();
        let mut view = json!({"repository":MARKET_REPOSITORY,"installed":installed["modules"],"modules":[],"commit":null,"fetched_at_unix":null});
        match snapshot {
            Ok(s) => {
                view["modules"] = json!(s.catalog.modules);
                view["commit"] = json!(s.commit);
                view["fetched_at_unix"] = json!(s.fetched_at_unix);
            }
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                view["notice"] = json!("Refresh the market to discover downloadable modules.");
            }
            Err(e) => {
                view["notice"] = json!(format!(
                    "Cached market unavailable: {e}. Refresh to recover. Installed modules are still available."
                ));
            }
        }
        Ok(view)
    }
    /// Installs the selected catalog version at its pinned commit, disabled pending enablement.
    /// The caller supplies the digest it displayed to prevent silently installing an updated row.
    pub fn install_market(&self, id: &str, expected_digest: &str) -> Result<Value, Error> {
        let snapshot = self.market_snapshot()?;
        let entry = snapshot.catalog.modules.iter().find(|m| m.id == id).ok_or_else(|| {
            invalid("module is absent from the cached market; refresh and select again")
        })?;
        if entry.digest != expected_digest {
            return Err(invalid(
                "market selection changed; refresh the menu and select the intended version",
            ));
        }
        let directory = tempfile::tempdir()?;
        let base = format!("{RAW}/{}/{}", snapshot.commit, entry.path);
        let manifest = download(&format!("{base}/module.toml"), 256 * 1024)?;
        let parsed = crate::Manifest::parse(
            std::str::from_utf8(&manifest).map_err(|_| invalid("module manifest is not UTF-8"))?,
        )?;
        if parsed.id != entry.id
            || parsed.version != entry.version
            || parsed.name != entry.name
            || parsed.description != entry.description
        {
            return Err(invalid("market entry does not match its pinned package"));
        }
        fs::write(directory.path().join("module.toml"), manifest)?;
        let mut total = 0usize;
        for resource in &parsed.resources {
            if !safe_path(resource) {
                return Err(invalid("market resource path contains unsupported URL characters"));
            }
            let bytes = download(&format!("{base}/{resource}"), 16 * 1024 * 1024)?;
            total = total.saturating_add(bytes.len());
            if total > 64 * 1024 * 1024 {
                return Err(invalid("package exceeds 64 MiB installation transfer bound"));
            }
            let target = directory.path().join(resource);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(target, bytes)?;
        }
        let definition = Definition::from_directory(directory.path())?;
        if definition.digest != entry.digest {
            return Err(invalid(
                "downloaded module digest differs from the catalog; prior selection was preserved",
            ));
        }
        let mut result = self.install(directory.path())?;
        let origin = json!({"repository":MARKET_REPOSITORY,"commit":snapshot.commit,"path":entry.path,"id":entry.id,"digest":entry.digest});
        publish(
            &self.root().join("origins").join(format!("{}-{}.json", entry.id, entry.digest)),
            &serde_json::to_vec_pretty(&origin)?,
        )?;
        result["origin"] = origin;
        Ok(result)
    }
}
