//! Explicit search configuration and ownership of a loopback-only SearXNG container.

use crate::{Error, checked_url, data_root, http, invalid, source::digest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    env, fs,
    net::TcpListener,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const IMAGE: &str = "docker.io/searxng/searxng:latest";

#[derive(Default, Serialize, Deserialize)]
struct Configuration {
    endpoint: String,
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

pub(crate) fn endpoint() -> Result<String, Error> {
    let selected =
        if let Ok(value) = env::var("PERITUS_WEB_SEARCH_URL") { value } else { load()?.endpoint };
    if selected.is_empty() {
        return Err(invalid(
            "Web search is not configured. Run peritus web setup for local keyless SearXNG, or peritus web setup https://YOUR-SERVER/search. Known URLs can still be opened.",
        ));
    }
    Ok(checked_url(&selected)?.to_string())
}

/// Configures an existing SearXNG JSON search endpoint; does not modify external services.
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
    save(&config)?;
    status()
}

/// Starts/reuses only this data directory's managed, resource-bounded SearXNG service.
pub fn setup_local() -> Result<Value, Error> {
    let mut config = load()?;
    if let Some(managed) = &config.managed {
        ensure_owned(managed)?;
        run(&managed.engine, &["start", &managed.id])?;
        if !managed.endpoint.is_empty() {
            config.endpoint = managed.endpoint.clone();
        }
        save(&config)?;
        wait_ready(&config.endpoint)?;
        return status();
    }
    let engine = ["podman", "docker"].into_iter().find(|name| {
        Command::new(name).arg("info").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
    }).ok_or_else(|| invalid("Local search setup needs a working Docker or Podman installation. Alternatively run peritus web setup https://YOUR-SEARXNG/search; page reading needs neither."))?;
    let root = data_root()?;
    let directory = root.join("searxng");
    fs::create_dir_all(&directory)?;
    let root = root.canonicalize()?;
    let owner = digest(root.to_string_lossy().as_bytes());
    let name = format!("peritus-search-{}", &owner[..16]);
    // Persist only app-owned configuration. No workspace or browser-profile mount.
    let secret = digest(
        format!("{owner}:{:?}:{}", std::time::SystemTime::now(), std::process::id()).as_bytes(),
    );
    fs::write(
        directory.join("settings.yml"),
        format!(
            "use_default_settings:\n  engines:\n    keep_only: [duckduckgo, bing, google, brave, wikipedia, yahoo, mwmbl]\nserver:\n  secret_key: '{secret}'\n  limiter: false\n  image_proxy: false\nsearch:\n  formats: [html, json]\n  safe_search: 0\nengines:\n  - name: duckduckgo\n    disabled: false\n  - name: bing\n    disabled: false\n  - name: google\n    disabled: false\n  - name: brave\n    disabled: false\n  - name: wikipedia\n    disabled: false\n  - name: yahoo\n    disabled: false\n  - name: mwmbl\n    disabled: false\nui:\n  static_use_hash: true\noutgoing:\n  request_timeout: 8.0\n  max_request_timeout: 12.0\n"
        ),
    )?;
    // The small bind reservation is released immediately before container creation.
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let publish = format!("127.0.0.1:{port}:8080");
    let volume = format!("{}:/etc/searxng:ro,Z", directory.canonicalize()?.display());
    let label = format!("io.peritus.web-owner={owner}");
    let id = run(
        engine,
        &[
            "run",
            "-d",
            "--name",
            &name,
            "--label",
            &label,
            "--cpus",
            "2",
            "--memory",
            "512m",
            "--pids-limit",
            "128",
            "-p",
            &publish,
            "-v",
            &volume,
            "-e",
            "GRANIAN_WORKERS=1",
            "-e",
            "FORCE_OWNERSHIP=false",
            IMAGE,
        ],
    )?;
    let id = id.trim().to_owned();
    config.endpoint = format!("http://127.0.0.1:{port}/search");
    config.managed = Some(Managed {
        engine: engine.into(),
        name,
        id: id.clone(),
        endpoint: config.endpoint.clone(),
    });
    if let Err(error) = save(&config) {
        let _ = run(engine, &["rm", "-f", &id]);
        return Err(error);
    }
    wait_ready(&config.endpoint)?;
    status()
}

/// Reports configured search readiness separately from browser-independent page reading.
pub fn status() -> Result<Value, Error> {
    let config = load()?;
    let endpoint = endpoint();
    let readiness = endpoint.as_ref().map_err(ToString::to_string).and_then(|url| {
        let url = checked_url(url).map_err(|e| e.to_string())?;
        let health = url.join("./healthz").map_err(|e| e.to_string())?;
        http::client()
            .map_err(|e| e.to_string())?
            .get(health)
            .send()
            .map_err(|e| e.to_string())
            .and_then(|r| {
                if r.status().is_success() {
                    Ok(())
                } else {
                    Err(format!(
                        "health endpoint returned HTTP {}; try peritus web search QUERY",
                        r.status().as_u16()
                    ))
                }
            })
    });
    Ok(
        json!({"page_reading": "ready; native HTTP(S), no browser or search key required", "search_endpoint": endpoint.ok(),
        "search_ready": readiness.is_ok(), "search_diagnostic": readiness.err(), "managed_container": config.managed.map(|m| json!({"engine":m.engine,"name":m.name,"id":m.id})),
        "data_directory": data_root()?, "setup": "peritus web setup [SEARXNG_URL]; JSON search must be enabled. Upstream engines may still rate-limit or return CAPTCHA.",
        "limits": {"native_transfer_bytes": http::MAX_BODY, "query_cache_entries": 128, "managed_cpu":2,"managed_memory_mib":512}}),
    )
}

/// Stops only a container whose exact identity and owner label match this installation.
pub fn stop() -> Result<Value, Error> {
    let config = load()?;
    let managed = config.managed.as_ref().ok_or_else(|| {
        invalid("no Peritus-managed search container exists; external endpoints are never stopped")
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

fn wait_ready(endpoint: &str) -> Result<(), Error> {
    let health = checked_url(endpoint)?.join("./healthz").map_err(|e| invalid(e.to_string()))?;
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if http::client()?
            .get(health.clone())
            .timeout(Duration::from_secs(2))
            .send()
            .is_ok_and(|r| r.status().is_success())
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(invalid(
                "SearXNG did not become ready; inspect the named container's logs, then retry peritus web setup. Its identity was retained.",
            ));
        }
        thread::sleep(Duration::from_millis(250));
    }
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
