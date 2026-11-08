//! Parser success requires recognized results or an explicit empty-result signal.
use super::health::{Kind, SearchFailure};
use crate::{checked_url, http::Download};
use metadata_search_engine_rs::models::SearchResult;
use scraper::{Html, Selector};
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;

pub(super) struct Parsed {
    pub results: Vec<SearchResult>,
    pub diagnostics: Vec<Value>,
    pub metadata: BTreeMap<String, Value>,
    pub summary: Value,
}

impl Parsed {
    const fn native(results: Vec<SearchResult>) -> Self {
        Self { results, diagnostics: Vec::new(), metadata: BTreeMap::new(), summary: Value::Null }
    }
}

pub(super) fn response(
    engine: &str,
    download: &Download,
    count: usize,
) -> Result<Parsed, SearchFailure> {
    if engine == "searxng" {
        return searx(download, count);
    }
    if !download.content_type.to_ascii_lowercase().contains("html") {
        return Err(SearchFailure::new(
            Kind::UnrecognizedResponse,
            download.status,
            "Search backend returned an unexpected media type",
        ));
    }
    let document = Html::parse_document(&download.text);
    let (result, link, title, snippet) = match engine {
        "yahoo" => ("div.algo-sr", "div.compTitle a", "div.compTitle a h3 span", "div.compText"),
        "duckduckgo" => ("div.result", "a.result__a", "a.result__a", "a.result__snippet"),
        "brave" => {
            ("div[data-type='web']", "a.l1", "div.search-snippet-title", "div.generic-snippet")
        }
        "startpage" => ("div.result", "a.result-title", "h2.wgl-title", "p.description"),
        _ => return Err(SearchFailure::new(Kind::ParseFailure, 0, "No parser for this backend")),
    };
    let selector = |text| {
        Selector::parse(text).map_err(|_| {
            SearchFailure::new(Kind::ParseFailure, 0, "Invalid built-in result selector")
        })
    };
    let (result, link, title, snippet) =
        (selector(result)?, selector(link)?, selector(title)?, selector(snippet)?);
    let mut results = Vec::new();
    for entry in document.select(&result) {
        let Some(anchor) = entry.select(&link).next() else { continue };
        let href = anchor.value().attr("href").unwrap_or_default();
        let url = destination(engine, href);
        if checked_url(&url).is_err() {
            continue;
        }
        let Some(title) = entry
            .select(&title)
            .next()
            .map(|e| e.text().collect::<String>())
            .filter(|s| !s.trim().is_empty())
        else {
            continue;
        };
        let snippet = entry
            .select(&snippet)
            .next()
            .map(|e| e.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" "));
        results.push(SearchResult {
            title: title.trim().into(),
            url,
            snippet,
            source_engine: engine.into(),
        });
        if results.len() >= count {
            break;
        }
    }
    if !results.is_empty() {
        return Ok(Parsed::native(results));
    }
    let title = Selector::parse("title")
        .ok()
        .and_then(|s| document.select(&s).next())
        .map(|e| e.text().collect::<String>().to_ascii_lowercase())
        .unwrap_or_default();
    let lower = download.text.to_ascii_lowercase();
    if title.contains("captcha")
        || title.contains("verify you are")
        || lower.contains("id=\"challenge-form\"")
        || lower.contains("class=\"anomaly-modal")
        || lower.contains("id=\"anomaly-modal")
    {
        return Err(SearchFailure::new(
            Kind::ChallengeRequired,
            download.status,
            "Backend returned a challenge, not search results",
        ));
    }
    let empty = match engine {
        "duckduckgo" => "div.no-results, div.result--no-result",
        "brave" => "div.no-results, div[data-type='no-results']",
        "startpage" => "div.no-results, div.no-results-message",
        _ => "div.no-results, div#no-results",
    };
    let empty_selector = Selector::parse(empty)
        .map_err(|_| SearchFailure::new(Kind::ParseFailure, 0, "Invalid empty-result selector"))?;
    if document.select(&empty_selector).any(|e| {
        let text = e.text().collect::<String>().to_ascii_lowercase();
        text.contains("no results")
            || text.contains("no result found")
            || text.contains("did not match")
            || text.contains("could not find")
    }) {
        return Ok(Parsed::native(Vec::new()));
    }
    Err(SearchFailure::new(
        Kind::UnrecognizedResponse,
        download.status,
        "HTTP succeeded but no known result or explicit no-results structure was recognized; blocked content or parser drift is unresolved",
    ))
}
fn destination(engine: &str, href: &str) -> String {
    if engine == "duckduckgo"
        && let Ok(base) = url::Url::parse("https://html.duckduckgo.com/")
        && let Ok(url) = base.join(href)
        && let Some((_, target)) = url.query_pairs().find(|(k, _)| k == "uddg")
    {
        return target.into_owned();
    }
    if engine == "yahoo"
        && let Some((_, tail)) = href.split_once("/RU=")
    {
        let end = ["/RS=", "/RK="].iter().filter_map(|s| tail.find(s)).min().unwrap_or(tail.len());
        if let Some((_, target)) =
            url::form_urlencoded::parse(format!("u={}", &tail[..end]).as_bytes()).next()
        {
            return target.into_owned();
        }
    }
    href.to_owned()
}
fn searx(download: &Download, count: usize) -> Result<Parsed, SearchFailure> {
    let response: Value = serde_json::from_str(&download.text).map_err(|_| {
        SearchFailure::new(
            Kind::ParseFailure,
            download.status,
            "SearXNG did not return JSON; enable its JSON search format",
        )
    })?;
    let items = response["results"].as_array().ok_or_else(|| {
        SearchFailure::new(
            Kind::ParseFailure,
            download.status,
            "SearXNG response has no results array",
        )
    })?;
    let results = items
        .iter()
        .filter_map(|item| {
            let url = item["url"].as_str()?;
            checked_url(url).ok()?;
            Some(SearchResult {
                title: item["title"].as_str().unwrap_or_default().into(),
                url: url.into(),
                snippet: item["content"].as_str().map(str::to_owned),
                source_engine: "searxng".into(),
            })
        })
        .take(count)
        .collect::<Vec<_>>();
    if results.is_empty()
        && response["unresponsive_engines"].as_array().is_some_and(|a| !a.is_empty())
    {
        return Err(SearchFailure::new(
            Kind::UpstreamUnavailable,
            download.status,
            "SearXNG reported downstream engine failures, not a complete empty result",
        ));
    }
    if !items.is_empty() && results.is_empty() {
        return Err(SearchFailure::new(
            Kind::ParseFailure,
            download.status,
            "SearXNG returned no admissible result URLs",
        ));
    }
    let metadata = items
        .iter()
        .filter_map(|item| {
            let url = item["url"].as_str()?;
            checked_url(url).ok()?;
            Some((
                url.to_owned(),
                json!({"engines":item["engines"],"published_date":item["publishedDate"]}),
            ))
        })
        .take(count)
        .collect();
    let diagnostics = response["unresponsive_engines"].as_array().into_iter().flatten().take(16).map(|item| json!({"engine":"searxng","kind":"upstream_unavailable","diagnostic":item.to_string().chars().take(1024).collect::<String>()})).collect();
    Ok(Parsed {
        results,
        diagnostics,
        metadata,
        summary: json!({"available_results":items.len(),"estimated_total_results":response["number_of_results"],"suggestions":response["suggestions"].as_array().into_iter().flatten().take(30).filter_map(Value::as_str).map(|s|s.chars().take(512).collect::<String>()).collect::<Vec<_>>()}),
    })
}
