//! Real HTTP/public Store replay. The controlled server returns upstream fixtures;
//! all HTTP, parsing, persistence, process locking and ranking are production code.
use peritus_tools_web::{SearchBackend, Store};
use serde_json::{Value, json};
use std::{
    io::{Read as _, Write as _},
    net::TcpListener,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, SystemTime},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
struct Server {
    url: String,
    count: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn start(status: &str, headers: &str, body: &str) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}/search", listener.local_addr()?);
        let response = format!(
            "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let count = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let (c, s) = (count.clone(), stop.clone());
        let join = thread::spawn(move || {
            while !s.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        let _ = socket.set_read_timeout(Some(Duration::from_secs(2)));
                        let mut request = [0; 8192];
                        let read = socket.read(&mut request).unwrap_or(0);
                        captured
                            .lock()
                            .expect("fixture request capture")
                            .push(String::from_utf8_lossy(&request[..read]).into_owned());
                        c.fetch_add(1, Ordering::AcqRel);
                        let _ = socket.write_all(response.as_bytes());
                    }
                    Err(_) => thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        Ok(Self { url, count, requests, stop, join: Some(join) })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}
fn search(root: &std::path::Path, name: &str, url: &str, query: &str) -> Result<Value> {
    Ok(Store::new(root.to_path_buf())?.search_with_backends(
        &[SearchBackend::new(name, url)?],
        &json!({"query":query}),
        &|| false,
    )?)
}
fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|s| s == "child") {
        let result = search(
            std::path::Path::new(&args[2]),
            "brave",
            &args[3],
            "different query after restart",
        )?;
        assert_eq!(result["status"], "unavailable");
        assert_eq!(result["engine_failures"][0]["kind"], "rate_limited");
        return Ok(());
    }
    let root = tempfile::tempdir()?;
    assert!(Store::new(root.path().join("invalid"))?.search(&Value::Null, &|| false).is_err());
    backoff_replay(root.path())?;
    parser_replay(root.path())?;
    let good = Server::start(
        "200 OK",
        "Content-Type: text/html\r\n",
        "<div class='result'><a class='result__a' href='https://example.org/retained'>Survives other engine failure</a></div>",
    )?;
    let blocked = Server::start("403 Forbidden", "Content-Type: text/html\r\n", "forbidden")?;
    let result = Store::new(root.path().join("partial"))?.search_with_backends(
        &[SearchBackend::new("brave", &blocked.url)?, SearchBackend::new("duckduckgo", &good.url)?],
        &json!({"query":"partial"}),
        &|| false,
    )?;
    assert_eq!(result["status"], "partial");
    assert_eq!(result["returned"], 1);
    let state = root.path().join("corrupt-state");
    search(&state, "brave", &blocked.url, "establish operational state")?;
    for entry in std::fs::read_dir(state.join("search-health-v1"))? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "json") {
            std::fs::write(path, b"corrupt")?;
        }
    }
    let result = Store::new(state)?.search_with_backends(
        &[SearchBackend::new("brave", &blocked.url)?, SearchBackend::new("duckduckgo", &good.url)?],
        &json!({"query":"healthy results survive unrelated state corruption"}),
        &|| false,
    )?;
    assert_eq!(result["status"], "partial");
    assert_eq!(result["returned"], 1);
    assert_eq!(result["engine_failures"][0]["kind"], "state_unavailable");
    let searx = Server::start(
        "200 OK",
        "Content-Type: application/json\r\n",
        r#"{"results":[{"url":"file:///not-a-web-source"},{"url":"https://example.org/doc","title":"Doc","content":"Authoritative","engines":["one","two"],"publishedDate":"2026-11-07"}],"number_of_results":100,"suggestions":["specific query"],"unresponsive_engines":[["three","timeout"]]}"#,
    )?;
    let endpoint = format!("{}?category_general=1", searx.url);
    let backend = SearchBackend::new("searxng", &endpoint)?;
    let store = Store::new(root.path().join("searx"))?;
    assert!(
        store
            .search_with_backends(
                std::slice::from_ref(&backend),
                &json!({"query":"external","page":0}),
                &|| false
            )
            .is_err()
    );
    assert_eq!(searx.count.load(Ordering::Acquire), 0);
    let result = store.search_with_backends(
        &[backend],
        &json!({"query":"external","max_results":1}),
        &|| false,
    )?;
    assert!(
        searx.requests.lock().expect("fixture requests")[0]
            .contains("category_general=1&q=external")
    );
    assert_eq!(result["status"], "partial");
    assert_eq!(result["available_results"], 2);
    assert_eq!(result["estimated_total_results"], 100);
    assert_eq!(result["suggestions"], json!(["specific query"]));
    assert_eq!(result["results"][0]["engines"], json!(["one", "two"]));
    assert_eq!(result["results"][0]["published_date"], "2026-11-07");
    assert!(
        Store::new(root.path().join("cancel"))?
            .search_with_backends(
                &[SearchBackend::new("brave", &good.url)?],
                &json!({"query":"cancel"}),
                &|| true
            )
            .is_err()
    );
    println!(
        "PASS: real HTTP, all four HTML parsers, 429 delta/date, 16 changed queries, fresh-process cooldown, expiry probe, challenge/drift/empty distinctions, partial results including corrupt backend state, SearXNG metadata/endpoint compatibility, invalid-page rejection and cancellation"
    );
    Ok(())
}

fn backoff_replay(root: &std::path::Path) -> Result<()> {
    for retry in
        ["3".to_owned(), httpdate::fmt_http_date(SystemTime::now() + Duration::from_mins(1))]
    {
        let state = root.join(format!("retry-{}", retry.len()));
        let upstream = Server::start(
            "429 Too Many Requests",
            &format!("Retry-After: {retry}\r\nContent-Type: text/html\r\n"),
            "rate limited",
        )?;
        let first = search(&state, "brave", &upstream.url, "initial query")?;
        assert_eq!(first["engine_failures"][0]["kind"], "rate_limited");
        assert_eq!(first["engine_failures"][0]["retry_after"], retry);
        assert!(
            first["engine_failures"][0]["retry_after_seconds"].as_u64().is_some_and(|n| n >= 2)
        );
        for n in 0..8 {
            let result = search(&state, "brave", &upstream.url, &format!("changed query {n}"))?;
            assert_eq!(result["success"], false);
        }
        let status = Command::new(std::env::current_exe()?)
            .arg("child")
            .arg(&state)
            .arg(&upstream.url)
            .status()?;
        assert!(status.success());
        assert_eq!(
            upstream.count.load(Ordering::Acquire),
            1,
            "different query/process cannot bypass cooldown"
        );
        if retry == "3" {
            thread::sleep(Duration::from_millis(3100));
            search(&state, "brave", &upstream.url, "probe after expiry")?;
            assert_eq!(upstream.count.load(Ordering::Acquire), 2);
        }
    }
    Ok(())
}

fn parser_replay(root: &std::path::Path) -> Result<()> {
    for (name, body) in [
        (
            "yahoo",
            "<div class='algo-sr'><div class='compTitle'><a href='https://example.org/yahoo'><h3><span>Yahoo fixture</span></h3></a></div><div class='compText'>primary source</div></div>",
        ),
        (
            "duckduckgo",
            "<div class='result'><a class='result__a' href='//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.org%2Fduck'>Duck fixture</a><a class='result__snippet'>primary source</a></div>",
        ),
        (
            "brave",
            "<div data-type='web'><a class='l1' href='https://example.org/brave'><div class='search-snippet-title'>Brave fixture</div></a><div class='generic-snippet'>primary source</div></div>",
        ),
        (
            "startpage",
            "<div class='result'><a class='result-title' href='https://example.org/start'><h2 class='wgl-title'>Start fixture</h2></a><p class='description'>primary source</p></div>",
        ),
    ] {
        let upstream = Server::start("200 OK", "Content-Type: text/html; charset=utf-8\r\n", body)?;
        let result = search(&root.join(name), name, &upstream.url, "normal")?;
        assert_eq!(result["status"], "ok", "{result}");
        assert_eq!(result["returned"], 1);
        assert!(
            result["results"][0]["url"]
                .as_str()
                .is_some_and(|s| s.starts_with("https://example.org/"))
        );
    }
    for (name, body, expected) in [
        (
            "challenge",
            "<title>CAPTCHA</title><form id=\"challenge-form\">verify</form>",
            "challenge_required",
        ),
        (
            "drift",
            "<title>New site</title><main>New unrecognized DOM</main>",
            "unrecognized_response",
        ),
        ("empty", "<div class='no-results'>No results found</div>", "no_results"),
    ] {
        let upstream = Server::start("200 OK", "Content-Type: text/html\r\n", body)?;
        let result = search(&root.join(name), "brave", &upstream.url, "no lucky assumption")?;
        if expected == "no_results" {
            assert_eq!(result["status"], expected);
            assert_eq!(result["empty_engines"], json!(["brave"]));
        } else {
            assert_eq!(result["status"], "unavailable");
            assert_eq!(result["engine_failures"][0]["kind"], expected);
        }
    }
    Ok(())
}
