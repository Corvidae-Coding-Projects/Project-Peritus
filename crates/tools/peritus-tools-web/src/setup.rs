//! Optional external search configuration; legacy containers are touched only by explicit stop.

use crate::{Error, checked_url, data_root, http, invalid, source::digest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{env, fs, process::Command};

#[derive(Default, Serialize, Deserialize)]
struct Configuration {
    endpoint: String,
    #[serde(default)]
    external: bool,
    managed: Option<Managed>,
}
#[derive(Serialize, Deserialize)]
struct Managed {
    engine: String,
    name: String,
    id: String,
    #[serde(default)]
    endpoint: String,
}

fn load() -> Result<Configuration, Error> {
    let path = data_root()?.join("settings.json");
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Configuration::default()),
        Err(error) => Err(error.into()),
    }
}

fn save(config: &Configuration) -> Result<(), Error> {
    use std::io::Write;
    let root = data_root()?;
    fs::create_dir_all(&root)?;
    let mut file = tempfile::NamedTempFile::new_in(&root)?;
    file.write_all(&serde_json::to_vec_pretty(config)?)?;
    file.as_file().sync_all()?;
    file.persist(root.join("settings.json")).map_err(|e| Error::Io(e.error))?;
    Ok(())
}

pub fn endpoint() -> Result<Option<String>, Error> {
    if let Ok(value) = env::var("PERITUS_WEB_SEARCH_URL") {
        return checked_url(&value).map(|url| Some(url.into()));
    }
    let config = load()?;
    // Old automatic container endpoints are implementation state, not a user's external choice.
    // Retain their identity for explicit cleanup, but never require or restart that service.
    let legacy_endpoint =
        config.managed.as_ref().is_some_and(|managed| match managed.endpoint.as_str() {
            "" => config.endpoint.starts_with("http://127.0.0.1:"),
            endpoint => config.endpoint == endpoint,
        });
    if config.endpoint.is_empty() || (legacy_endpoint && !config.external) {
        Ok(None)
    } else {
        checked_url(&config.endpoint).map(|url| Some(url.into()))
    }
}

/// Configures an existing `SearXNG` JSON search endpoint; does not modify external services.
///
/// # Errors
///
/// Returns an error when the endpoint is invalid or configuration cannot be saved.
pub fn configure(value: &str) -> Result<Value, Error> {
    let mut url = checked_url(value)?;
    if url.path() == "/" {
        url.set_path("/search");
    }
    if url.query().is_some() {
        return Err(invalid("SearXNG endpoint must not contain query parameters"));
    }
    let mut config = load()?;
    config.endpoint = url.into();
    config.external = true;
    save(&config)?;
    status()
}

/// Selects built-in Rust search. No service, download, or external runtime is needed.
///
/// # Errors
///
/// Returns an error when configuration cannot be loaded or saved.
pub fn setup_local() -> Result<Value, Error> {
    let mut config = load()?;
    config.endpoint.clear();
    config.external = false;
    save(&config)?;
    status()
}

/// Reports configured search readiness separately from browser-independent page reading.
///
/// # Errors
///
/// Returns an error when saved configuration or the data directory is unavailable.
pub fn status() -> Result<Value, Error> {
    let config = load()?;
    let endpoint = endpoint()?;
    Ok(json!({
        "page_reading": "ready; native HTTP(S), no browser or search key required",
        "search_backend": if endpoint.is_some() { "external-searxng" } else { "built-in-rust" },
        "search_endpoint": endpoint,
        "search_ready": true,
        "search_diagnostic": "Ready to query; upstream connectivity and engine blocking are reported on each search.",
        "engines": crate::search::ENGINES,
        "managed_container": config.managed.map(|m| json!({"engine":m.engine,"name":m.name,"id":m.id,"legacy":true})),
        "data_directory": data_root()?,
        "setup": "None required. peritus web setup selects built-in Rust search; an optional SEARXNG_URL selects an existing service. PERITUS_WEB_SEARCH_URL overrides saved selection.",
        "limits": {"native_transfer_bytes": http::MAX_BODY, "query_cache_entries":128, "native_search_concurrency":1, "engine_timeout_seconds":8}
    }))
}

/// Stops only a container whose exact identity and owner label match this installation.
///
/// # Errors
///
/// Returns an error unless the retained legacy container is owned by this installation and stops.
pub fn stop() -> Result<Value, Error> {
    let config = load()?;
    let managed = config.managed.as_ref().ok_or_else(|| {
        invalid("built-in Rust search has no background service to stop; external endpoints are never stopped")
    })?;
    ensure_owned(managed)?;
    run(&managed.engine, &["stop", "--time", "10", &managed.id])?;
    Ok(json!({"stopped": managed.id, "restart": "peritus web setup"}))
}

fn ensure_owned(managed: &Managed) -> Result<(), Error> {
    let expected = digest(data_root()?.canonicalize()?.to_string_lossy().as_bytes());
    let output = run(
        &managed.engine,
        &["inspect", "--format", "{{index .Config.Labels \"io.peritus.web-owner\"}}", &managed.id],
    )?;
    if output.trim() != expected {
        return Err(invalid("search container ownership mismatch; refusing to modify it"));
    }
    Ok(())
}

fn run(engine: &str, arguments: &[&str]) -> Result<String, Error> {
    let output = Command::new(engine).args(arguments).output()?;
    if !output.status.success() {
        return Err(invalid(format!(
            "{engine} {} failed: {}",
            arguments.first().unwrap_or(&"command"),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    String::from_utf8(output.stdout).map_err(|e| invalid(e.to_string()))
}
