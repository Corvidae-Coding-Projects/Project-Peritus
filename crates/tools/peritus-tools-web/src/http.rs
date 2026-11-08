//! Bounded GET requests and keyless `SearXNG` JSON transport.

use crate::{Error, PageFailure, checked_url, failure, invalid};
use reqwest::{blocking::Client, redirect::Policy};
use serde_json::Value;
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
            httpdate::parse_http_date(&retry).ok().map(|until| {
                u64::try_from(
                    until
                        .duration_since(SystemTime::now())
                        .unwrap_or_default()
                        .as_millis()
                        .div_ceil(1000),
                )
                .unwrap_or(u64::MAX)
            })
        });
        if let Some(seconds) = seconds {
            defer(&final_url, seconds, &failure)?;
        }
    }
    Ok(failure)
}

pub fn get(url: &str, cancelled: &dyn Fn() -> bool) -> Result<Download, Error> {
    get_with_client(url, cancelled, &client()?, MAX_BODY)
}

pub fn get_with_client(
    url: &str,
    cancelled: &dyn Fn() -> bool,
    client: &Client,
    max_body: usize,
) -> Result<Download, Error> {
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
    let mut response = client.get(parsed).send()?;
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
    if response.content_length().is_some_and(|length| length > max_body as u64) {
        return Err(invalid(format!(
            "response exceeds the {max_body} byte transfer bound; request a narrower source"
        )));
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
        if bytes.len().saturating_add(count) > max_body {
            return Err(invalid(format!(
                "response exceeds the {max_body} byte transfer bound; no truncated source retained"
            )));
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
