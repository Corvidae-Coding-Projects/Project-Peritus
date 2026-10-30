//! Bounded GET requests and keyless `SearXNG` JSON transport.

use crate::{Error, PageFailure, checked_url, failure, invalid};
use reqwest::{blocking::Client, redirect::Policy};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Read,
    sync::{LazyLock, Mutex},
    time::{Duration, SystemTime},
};

static COOLDOWNS: LazyLock<Mutex<BTreeMap<String, (SystemTime, PageFailure)>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

pub fn defer(url: &str, seconds: u64, failure: &PageFailure) -> Result<(), Error> {
    let key = checked_url(url)?.origin().ascii_serialization();
    let mut entries = COOLDOWNS.lock().map_err(|_| invalid("web cooldown state is poisoned"))?;
    entries.retain(|_, (until, _)| *until > SystemTime::now());
    if let Some(until) = SystemTime::now().checked_add(Duration::from_secs(seconds))
        && entries.get(&key).is_none_or(|(existing, _)| *existing < until)
    {
        if entries.len() >= 128 && !entries.contains_key(&key) {
            let _ = entries.pop_first();
        }
        entries.insert(key, (until, failure.clone()));
    }
    drop(entries);
    Ok(())
}

pub const MAX_BODY: usize = 64 * 1024 * 1024;

pub struct Download {
    pub url: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
    pub text: String,
    pub status: u16,
}

pub fn client() -> Result<Client, Error> {
    Ok(Client::builder()
        .user_agent(concat!("Peritus/", env!("CARGO_PKG_VERSION"), " (web reader)"))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                attempt.error("too many redirects")
            } else if checked_url(attempt.url().as_str()).is_err() {
                attempt.error("unsafe redirect URL")
            } else {
                attempt.follow()
            }
        }))
        .build()?)
}

fn response_failure(
    mut response: reqwest::blocking::Response,
    challenged: bool,
) -> Result<PageFailure, Error> {
    let status = response.status();
    let mut retry =
        response.headers().get("retry-after").and_then(|v| v.to_str().ok()).map(str::to_owned);
    let final_url = response.url().to_string();
    let mut bytes = Vec::new();
    response.by_ref().take(16 * 1024).read_to_end(&mut bytes)?;
    let html = String::from_utf8_lossy(&bytes);
    let api_error: Option<Value> = serde_json::from_slice(&bytes).ok();
    let throttled = api_error.as_ref().is_some_and(|value| {
        value["error_name"].as_str().is_some_and(|name| name.contains("throttle"))
    });
    if retry.is_none() {
        retry = api_error
            .as_ref()
            .and_then(|value| value["backoff"].as_u64())
            .map(|seconds| seconds.to_string());
    }
    let kind = if throttled {
        crate::FailureKind::RateLimited
    } else {
        failure::classify(status.as_u16(), challenged || failure::challenge_html(&html))
    };
    let mut failure = PageFailure::new(
        kind,
        &final_url,
        status.as_u16(),
        "Upstream response was not retained as a successful source",
    );
    failure.retry_after.clone_from(&retry);
    if let Some(retry) = retry {
        let seconds = retry.parse::<u64>().ok().or_else(|| {
            httpdate::parse_http_date(&retry)
                .ok()
                .map(|until| until.duration_since(SystemTime::now()).unwrap_or_default().as_secs())
        });
        if let Some(seconds) = seconds {
            defer(&final_url, seconds, &failure)?;
        }
    }
    Ok(failure)
}

pub fn get(url: &str, cancelled: &dyn Fn() -> bool) -> Result<Download, Error> {
    if cancelled() {
        return Err(invalid("web request cancelled"));
    }
    let parsed = checked_url(url)?;
    let cooldown = COOLDOWNS
        .lock()
        .map_err(|_| invalid("web cooldown state is poisoned"))?
        .get(&parsed.origin().ascii_serialization())
        .filter(|(until, _)| *until > SystemTime::now())
        .cloned();
    if let Some((until, failure)) = cooldown {
        let mut failure = failure;
        failure.url = url.into();
        failure.retry_after = Some(
            until
                .duration_since(SystemTime::now())
                .unwrap_or_default()
                .as_secs()
                .saturating_add(1)
                .to_string(),
        );
        return Err(failure.into());
    }
    let mut response = client()?.get(parsed).send()?;
    let status = response.status();
    let challenged =
        response.headers().get("cf-mitigated").is_some_and(|value| value == "challenge");
    if !status.is_success() || challenged {
        return Err(response_failure(response, challenged)?.into());
    }
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain; charset=utf-8")
        .to_owned();
    if response.content_length().is_some_and(|length| length > MAX_BODY as u64) {
        return Err(invalid(
            "response exceeds the 64 MiB transfer resource bound; request a narrower source",
        ));
    }
    let final_url = response.url().to_string();
    let mut bytes = Vec::new();
    let mut chunk = vec![0_u8; 32 * 1024];
    loop {
        if cancelled() {
            return Err(invalid("web request cancelled"));
        }
        let count = response.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        if bytes.len().saturating_add(count) > MAX_BODY {
            return Err(invalid(
                "response exceeds the 64 MiB transfer resource bound; no truncated source retained",
            ));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    let text = decode(&bytes, &content_type)?;
    if content_type.to_ascii_lowercase().contains("html") && failure::challenge_html(&text) {
        return Err(PageFailure::new(
            crate::FailureKind::ChallengeRequired,
            &final_url,
            status.as_u16(),
            "Challenge HTML was returned instead of the requested content",
        )
        .into());
    }
    Ok(Download { url: final_url, content_type, text, bytes, status: status.as_u16() })
}

fn decode(bytes: &[u8], content_type: &str) -> Result<String, Error> {
    let media = content_type.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
    if !(media.starts_with("text/")
        || matches!(
            media.as_str(),
            "application/json"
                | "application/xml"
                | "application/xhtml+xml"
                | "application/javascript"
        ))
    {
        return Err(invalid(format!(
            "unsupported page media type {media}; use the source's HTML/text version (PDF and binary extraction are not installed)"
        )));
    }
    let charset = content_type
        .split(';')
        .find_map(|part| {
            part.trim()
                .split_once('=')
                .filter(|(name, _)| name.trim().eq_ignore_ascii_case("charset"))
                .map(|(_, value)| value.trim())
        })
        .unwrap_or("utf-8")
        .trim_matches(['\'', '"']);
    let encoding = encoding_rs::Encoding::for_label(charset.as_bytes())
        .ok_or_else(|| invalid(format!("unsupported charset {charset}")))?;
    let (text, _, errors) = encoding.decode(bytes);
    if errors {
        return Err(invalid(
            "page contains invalid bytes for its declared charset; no lossy source retained",
        ));
    }
    Ok(text.into_owned())
}

pub fn search(
    endpoint: &str,
    arguments: &Value,
    cancelled: &dyn Fn() -> bool,
) -> Result<Value, Error> {
    let mut url = checked_url(endpoint)?;
    let query = arguments["query"]
        .as_str()
        .filter(|q| !q.trim().is_empty())
        .ok_or_else(|| invalid("query must not be empty"))?;
    let page = arguments["page"].as_u64().unwrap_or(1);
    if page == 0 {
        return Err(invalid("page must start at 1"));
    }
    {
        let mut pairs = url.query_pairs_mut();
        pairs
            .append_pair("q", query)
            .append_pair("format", "json")
            .append_pair("pageno", &page.to_string());
        for (field, parameter) in
            [("language", "language"), ("time_range", "time_range"), ("engines", "engines")]
        {
            if let Some(value) = arguments[field].as_str() {
                pairs.append_pair(parameter, value);
            }
        }
    }
    let downloaded = get(url.as_str(), cancelled).map_err(|e| invalid(format!(
        "SearXNG search request failed: {e}. Enable JSON in search.formats; public instances may deny API access."
    )))?;
    let response: Value = serde_json::from_str(&downloaded.text).map_err(|_| invalid("SearXNG did not return JSON; enable search.formats: [html, json] or run peritus web setup"))?;
    let results = response["results"]
        .as_array()
        .ok_or_else(|| invalid("SearXNG JSON has no results array"))?;
    let count = arguments["max_results"].as_u64().unwrap_or(10).clamp(1, 30) as usize;
    let selected: Vec<_> = results.iter().filter_map(|item| {
        let url = item["url"].as_str()?;
        checked_url(url).ok()?;
        Some(json!({"title": item["title"].as_str().map(|v| v.chars().take(512).collect::<String>()), "url": url, "snippet": item["content"].as_str().map(|v| v.chars().take(1500).collect::<String>()), "snippet_limit_chars": 1500, "engines": item["engines"], "published_date": item["publishedDate"]}))
    }).take(count).collect();
    Ok(
        json!({"query": query, "page": page, "results": selected, "returned": selected.len(), "available_results": results.len(), "engine_failures": response["unresponsive_engines"], "suggestions": response["suggestions"], "search_endpoint": endpoint, "notice": "Search snippets are untrusted discovery evidence. Open primary sources before citing factual claims. Empty results with engine failures do not prove absence."}),
    )
}
