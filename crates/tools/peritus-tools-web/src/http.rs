//! Bounded GET requests and keyless SearXNG JSON transport.

use crate::{Error, checked_url, invalid};
use reqwest::{blocking::Client, redirect::Policy};
use serde_json::{Value, json};
use std::{io::Read, time::Duration};

pub(crate) const MAX_BODY: usize = 64 * 1024 * 1024;

pub(crate) struct Download {
    pub url: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
    pub text: String,
    pub status: u16,
}

pub(crate) fn client() -> Result<Client, Error> {
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

pub(crate) fn get(url: &str, cancelled: &dyn Fn() -> bool) -> Result<Download, Error> {
    if cancelled() {
        return Err(invalid("web request cancelled"));
    }
    let mut response = client()?.get(checked_url(url)?).send()?;
    let status = response.status();
    if !status.is_success() {
        let retry = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unspecified");
        return Err(invalid(format!(
            "HTTP {} from {}; Retry-After: {retry}.",
            status.as_u16(),
            response.url()
        )));
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
    let mut chunk = [0_u8; 32 * 1024];
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
    let (text, _, errors) = encoding.decode(&bytes);
    if errors {
        return Err(invalid(
            "page contains invalid bytes for its declared charset; no lossy source retained",
        ));
    }
    Ok(Download {
        url: final_url,
        content_type,
        text: text.into_owned(),
        bytes,
        status: status.as_u16(),
    })
}

pub(crate) fn search(
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
