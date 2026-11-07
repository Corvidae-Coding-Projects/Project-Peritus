//! Search operations own durable backend health independently of model queries.
mod health;
mod parse;
use crate::{Error, checked_url, http, invalid};
use health::{BackendOutcome, SearchFailure};
use metadata_search_engine_rs::aggregator::aggregate;
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
pub const ENGINES: [&str; 4] = ["yahoo", "duckduckgo", "brave", "startpage"];

/// A host-selected endpoint. Model proposals never choose transport URLs.
#[derive(Clone, Debug)]
pub struct SearchBackend {
    name: String,
    endpoint: String,
}
impl SearchBackend {
    /// Selects a known parser and credential-free HTTP(S) endpoint.
    ///
    /// # Errors
    /// Rejects unknown parsers and invalid or credential-bearing HTTP(S) endpoints.
    pub fn new(name: &str, endpoint: &str) -> Result<Self, Error> {
        if !ENGINES.contains(&name) && name != "searxng" {
            return Err(invalid("unknown search backend"));
        }
        Ok(Self { name: name.to_owned(), endpoint: checked_url(endpoint)?.to_string() })
    }
}
fn defaults() -> Vec<SearchBackend> {
    [
        ("yahoo", "https://search.yahoo.com/search"),
        ("duckduckgo", "https://html.duckduckgo.com/html/"),
        ("brave", "https://search.brave.com/search"),
        ("startpage", "https://www.startpage.com/search"),
    ]
    .into_iter()
    .map(|(name, endpoint)| SearchBackend { name: name.into(), endpoint: endpoint.into() })
    .collect()
}
fn client() -> Result<reqwest::blocking::Client, Error> {
    static CLIENT: OnceLock<Result<reqwest::blocking::Client, String>> = OnceLock::new();
    CLIENT.get_or_init(|| reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36")
        .connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || checked_url(attempt.url().as_str()).is_err() { attempt.error("unsafe or excessive search redirect") } else { attempt.follow() }
        })).build().map_err(|error| error.to_string()))
        .as_ref().cloned().map_err(|error| invalid(format!("search transport unavailable: {error}")))
}
pub fn search(
    root: &Path,
    endpoint: Option<&str>,
    arguments: &Value,
    cancelled: &dyn Fn() -> bool,
) -> Result<Value, Error> {
    let backends = endpoint.map_or_else(
        || Ok(defaults()),
        |url| SearchBackend::new("searxng", url).map(|b| vec![b]),
    )?;
    search_with_backends(root, &backends, arguments, cancelled)
}
pub fn canonical_arguments(arguments: &Value) -> Value {
    let mut result = arguments.clone();
    if !result.is_object() {
        return result;
    }
    if result.get("page").is_none() {
        result["page"] = json!(1);
    }
    if result.get("max_results").is_none() {
        result["max_results"] = json!(10);
    }
    if let Some(text) = result["engines"].as_str() {
        let mut names =
            text.split(',').map(str::trim).filter(|n| !n.is_empty()).collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        result["engines"] = json!(names.join(","));
    }
    result
}
pub fn search_with_backends(
    root: &Path,
    backends: &[SearchBackend],
    arguments: &Value,
    cancelled: &dyn Fn() -> bool,
) -> Result<Value, Error> {
    let external = backend_capabilities(backends, arguments)?;
    let query = arguments["query"]
        .as_str()
        .filter(|q| !q.trim().is_empty())
        .ok_or_else(|| invalid("query must not be empty"))?;
    let selected = arguments["engines"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(str::trim).collect::<Vec<_>>());
    if !external && selected.as_ref().is_some_and(|s| s.iter().any(|n| !ENGINES.contains(n))) {
        return Err(invalid("unknown built-in search engine"));
    }
    if !external
        && !backends.iter().any(|b| selected.as_ref().is_none_or(|s| s.contains(&b.name.as_str())))
    {
        return Err(invalid("no selected search engine is available in the host backend set"));
    }
    let count = arguments["max_results"].as_u64().unwrap_or(10).clamp(1, 30) as usize;
    let client = client()?;
    let flag = AtomicBool::new(cancelled());
    let outcomes = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for backend in backends
            .iter()
            .filter(|b| external || selected.as_ref().is_none_or(|s| s.contains(&b.name.as_str())))
        {
            let client = client.clone();
            let flag = &flag;
            handles.push(scope.spawn(move || {
                match run_backend(root, backend, arguments, query, count, &client, flag) {
                    Ok(outcome) => Ok(outcome),
                    Err(error) if flag.load(Ordering::Acquire) => Err(error),
                    Err(_) => Ok(BackendOutcome::state_unavailable(&backend.name)),
                }
            }));
        }
        while handles.iter().any(|h| !h.is_finished()) {
            if cancelled() {
                flag.store(true, Ordering::Release);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        handles
            .into_iter()
            .map(|h| h.join().map_err(|_| invalid("search worker failed"))?)
            .collect::<Result<Vec<_>, Error>>()
    })?;
    if flag.load(Ordering::Acquire) || cancelled() {
        return Err(invalid("web request cancelled"));
    }
    let successes = outcomes
        .iter()
        .filter(|o| o.failure.is_none())
        .map(|o| (o.name.clone(), o.results.clone()))
        .collect::<Vec<_>>();
    let mut failures = outcomes
        .iter()
        .filter_map(|o| o.failure.as_ref().map(|f| f.observation(&o.name)))
        .collect::<Vec<_>>();
    failures.extend(outcomes.iter().flat_map(|o| o.diagnostics.clone()));
    let results = aggregate(successes, count).into_iter().filter(|r| checked_url(&r.url).is_ok()).map(|r| {
        let mut value = json!({"title":r.title.chars().take(512).collect::<String>(),"url":r.url,"snippet":r.snippet.map(|s|s.chars().take(1500).collect::<String>()),"snippet_limit_chars":1500,"engines":r.engines});
        if let Some(extra) = outcomes.iter().find_map(|o| o.metadata.get(&r.url)) {
            if extra["engines"].is_array() { value["engines"] = extra["engines"].clone(); }
            value["published_date"] = extra["published_date"].clone();
        }
        value
    }).collect::<Vec<_>>();
    let empty = outcomes
        .iter()
        .filter(|o| o.failure.is_none() && o.results.is_empty() && o.diagnostics.is_empty())
        .map(|o| o.name.clone())
        .collect::<Vec<_>>();
    let unavailable = results.is_empty() && !failures.is_empty();
    let status = if unavailable {
        "unavailable"
    } else if !failures.is_empty() {
        "partial"
    } else if results.is_empty() {
        "no_results"
    } else {
        "ok"
    };
    let mut response = json!({"success":!unavailable,"status":status,"query":query,"page":arguments["page"].as_u64().unwrap_or(1),"returned":results.len(),"results":results,"search_backend":if external {"external-searxng"}else{"built-in-rust"},"engine_failures":failures,"empty_engines":empty,"engines_queried":outcomes.iter().map(|o|o.name.clone()).collect::<Vec<_>>(),"next_page":if external {Some(arguments["page"].as_u64().unwrap_or(1).saturating_add(1))}else{None},"notice":"Search snippets are discovery evidence. Open primary sources before relying on claims. Backend failures do not indicate a bad query or absence of evidence. Cooldowns are shared across queries and processes; changing the query does not bypass them. Use already retained sources or known authoritative URLs while unavailable backends recover."});
    if external {
        response["search_endpoint"] = json!(backends[0].endpoint);
        if let Some(outcome) = outcomes.first() {
            response["available_results"] = outcome.summary["available_results"].clone();
            response["suggestions"] = outcome.summary["suggestions"].clone();
            response["estimated_total_results"] =
                outcome.summary["estimated_total_results"].clone();
        }
    }
    Ok(response)
}
fn backend_capabilities(backends: &[SearchBackend], arguments: &Value) -> Result<bool, Error> {
    if backends.is_empty() || backends.len() > 4 {
        return Err(invalid("search needs one to four host-selected backends"));
    }
    let external = backends.iter().any(|b| b.name == "searxng");
    if external && backends.len() != 1 {
        return Err(invalid("external search is a single configured metasearch endpoint"));
    }
    if arguments["page"].as_u64() == Some(0) {
        return Err(invalid("page must start at 1"));
    }
    if !external {
        if arguments["page"].as_u64().unwrap_or(1) != 1 {
            return Err(invalid("built-in search supports page=1 only"));
        }
        if ["language", "time_range"]
            .iter()
            .any(|k| arguments[*k].as_str().is_some_and(|v| !v.is_empty()))
        {
            return Err(invalid("these filters require an external SearXNG backend"));
        }
    }
    Ok(external)
}

fn run_backend(
    root: &Path,
    backend: &SearchBackend,
    arguments: &Value,
    query: &str,
    count: usize,
    client: &reqwest::blocking::Client,
    cancelled: &AtomicBool,
) -> Result<BackendOutcome, Error> {
    let is_cancelled = || cancelled.load(Ordering::Acquire);
    let mut lease = health::Lease::acquire(root, &backend.endpoint, &is_cancelled)?;
    if let Some(failure) = lease.unavailable(&is_cancelled)? {
        return Ok(BackendOutcome {
            name: backend.name.clone(),
            results: Vec::new(),
            failure: Some(failure),
            diagnostics: Vec::new(),
            summary: Value::Null,
            metadata: std::collections::BTreeMap::new(),
        });
    }
    if is_cancelled() {
        return Err(invalid("web request cancelled"));
    }
    let mut url = checked_url(&backend.endpoint)?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair(if backend.name == "yahoo" { "p" } else { "q" }, query);
        if backend.name == "searxng" {
            pairs
                .append_pair("format", "json")
                .append_pair("pageno", &arguments["page"].as_u64().unwrap_or(1).to_string());
            for field in ["language", "time_range", "engines"] {
                if let Some(value) = arguments[field].as_str() {
                    pairs.append_pair(field, value);
                }
            }
        }
    }
    let outcome = match http::get_with_client(url.as_str(), &is_cancelled, client, 2 * 1024 * 1024)
    {
        Ok(download) => parse::response(&backend.name, &download, count),
        Err(error) => Err(SearchFailure::from_error(&error)),
    };
    if is_cancelled() {
        return Err(invalid("web request cancelled"));
    }
    match outcome {
        Ok(parsed) => {
            lease.succeeded()?;
            Ok(BackendOutcome {
                name: backend.name.clone(),
                results: parsed.results,
                failure: None,
                diagnostics: parsed.diagnostics,
                summary: parsed.summary,
                metadata: parsed.metadata,
            })
        }
        Err(failure) => {
            let failure = lease.failed(failure)?;
            Ok(BackendOutcome {
                name: backend.name.clone(),
                results: Vec::new(),
                failure: Some(failure),
                diagnostics: Vec::new(),
                summary: Value::Null,
                metadata: std::collections::BTreeMap::new(),
            })
        }
    }
}
