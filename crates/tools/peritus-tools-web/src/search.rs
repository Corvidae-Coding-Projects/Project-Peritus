//! In-process Rust metasearch; no server, child process, interpreter or first-use download.

use crate::{Error, checked_url, invalid};
use metadata_search_engine_rs::{
    aggregator::{aggregate, query_all_engines},
    cache::EngineLimits,
    engines::{BraveEngine, DuckDuckGoEngine, SearchEngine, StartpageEngine, YahooEngine},
};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, TryLockError},
    thread,
    time::Duration,
};

pub const ENGINES: [&str; 4] = ["yahoo", "duckduckgo", "brave", "startpage"];
static SEARCH: Mutex<()> = Mutex::new(());

pub fn search(arguments: &Value, cancelled: &dyn Fn() -> bool) -> Result<Value, Error> {
    let query = arguments["query"]
        .as_str()
        .filter(|q| !q.trim().is_empty())
        .ok_or_else(|| invalid("query must not be empty"))?;
    if arguments["page"].as_u64().unwrap_or(1) != 1 {
        return Err(invalid(
            "built-in search returns the first ranked result set only; refine the query for more sources (page must be 1)",
        ));
    }
    for field in ["language", "time_range"] {
        if arguments[field].as_str().is_some_and(|v| !v.is_empty()) {
            return Err(invalid(format!(
                "built-in search does not support {field}; include the language/date in the query, or use an optional external SearXNG endpoint"
            )));
        }
    }
    let selected: Vec<_> = arguments["engines"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map_or_else(|| ENGINES.to_vec(), |s| s.split(',').map(str::trim).collect());
    if selected.iter().any(|name| !ENGINES.contains(name)) {
        return Err(invalid(format!(
            "unknown built-in engine; choose from {}",
            ENGINES.join(", ")
        )));
    }
    // Serial fan-outs keep per-process network/parser work bounded. Waiting is cancellable.
    let _permit = loop {
        if cancelled() {
            return Err(invalid("web request cancelled"));
        }
        match SEARCH.try_lock() {
            Ok(permit) => break permit,
            Err(TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(50)),
            Err(TryLockError::Poisoned(_)) => return Err(invalid("web search worker failed")),
        }
    };
    let count = arguments["max_results"].as_u64().unwrap_or(10).clamp(1, 30) as usize;
    // This runtime lives only on the caller's blocking worker. Dropping the search future cancels
    // all engine requests; no request or runtime thread outlives this operation.
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(async {
        let client = Arc::new(search_http::Client::builder()
            .use_rustls_tls()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36")
            .connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(8))
            .redirect(search_http::redirect::Policy::limited(5))
            .build().map_err(|e| invalid(format!("build search client: {e}")))?);
        let engines: Vec<Arc<dyn SearchEngine>> = ENGINES.into_iter().filter(|name| selected.contains(name)).map(|name| {
            let client = Arc::clone(&client);
            match name {
                "yahoo" => Arc::new(YahooEngine::new(client)) as Arc<dyn SearchEngine>,
                "duckduckgo" => Arc::new(DuckDuckGoEngine::new(client)),
                "brave" => Arc::new(BraveEngine::new(client)),
                _ => Arc::new(StartpageEngine::new(client)),
            }
        }).collect();
        let limits = EngineLimits::new(1, Duration::ZERO);
        let request = query_all_engines(&engines, &limits, query, count);
        tokio::pin!(request);
        let (successes, failures) = loop {
            tokio::select! {
                result = &mut request => break result,
                () = tokio::time::sleep(Duration::from_millis(50)) => {
                    if cancelled() { return Err(invalid("web request cancelled")); }
                }
            }
        };
        let empty: Vec<_> = successes.iter().filter(|(_, results)| results.is_empty()).map(|(name, _)| name.clone()).collect();
        let failures: Vec<_> = failures.iter().map(|(name, error)| json!({"engine":name,"error":error.to_string()})).collect();
        let results: Vec<_> = aggregate(successes, count).into_iter().filter(|r| checked_url(&r.url).is_ok()).map(|r| json!({
            "title":r.title.chars().take(512).collect::<String>(),"url":r.url,
            "snippet":r.snippet.map(|s| s.chars().take(1500).collect::<String>()),
            "snippet_limit_chars":1500,"engines":r.engines
        })).collect();
        if results.is_empty() {
            return Err(invalid(format!("built-in search returned no usable results; engines with no results (possibly blocked or changed markup): {empty:?}; engine failures: {failures:?}. Refine the query or retry; known URLs can still be opened.")));
        }
        Ok(json!({"query":query,"page":1,"results":results,"returned":results.len(),
            "search_backend":"built-in-rust","engine_failures":failures,"empty_engines":empty,
            "engines_queried":selected,"next_page":null,
            "notice":"Search snippets are untrusted discovery evidence. Open primary sources before citing factual claims. Empty engines may be blocked; results do not prove completeness. Native search supports the first ranked set; refine queries for more sources."}))
    })
}
