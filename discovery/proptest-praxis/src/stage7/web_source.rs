//! Real loopback HTTP and immutable source properties against an independent byte model.

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use peritus_tools_web::Store;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

const ATOMS: [&str; 7] = ["A", "α", "👩‍💻", "xy", "\n", " ", "e\u{301}"];
const NEEDLES: [&str; 6] = ["A", "α", "👩‍💻", "xy", "e\u{301}", "absent"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub atoms: Vec<u8>,
    pub replacement: Vec<u8>,
    pub needle: u8,
    pub page_bytes: u8,
    pub find_limit: u8,
    pub redirect: bool,
    pub age_cache: bool,
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    (
        prop::collection::vec(0_u8..ATOMS.len() as u8, 0..=max_steps.min(64)),
        prop::collection::vec(0_u8..ATOMS.len() as u8, 0..=max_steps.min(32)),
        0_u8..NEEDLES.len() as u8,
        4_u8..=48,
        1_u8..=5,
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(atoms, replacement, needle, page_bytes, find_limit, redirect, age_cache)| {
            Case { atoms, replacement, needle, page_bytes, find_limit, redirect, age_cache }
        })
        .boxed()
}

pub fn calibration() -> Case {
    Case {
        atoms: vec![0, 1, 2, 3, 4, 0, 3, 1, 0, 2, 6, 3],
        replacement: vec![3, 4, 1],
        needle: 3,
        page_bytes: 7,
        find_limit: 1,
        redirect: false,
        age_cache: true,
    }
}

pub fn examples() -> Vec<Case> {
    vec![
        calibration(),
        Case {
            atoms: vec![],
            replacement: vec![0],
            needle: 5,
            page_bytes: 4,
            find_limit: 1,
            redirect: true,
            age_cache: false,
        },
        Case {
            atoms: vec![1, 1, 1, 4, 1, 1],
            replacement: vec![2],
            needle: 1,
            page_bytes: 4,
            find_limit: 2,
            redirect: true,
            age_cache: true,
        },
        Case {
            atoms: vec![2, 6, 2, 6, 2],
            replacement: vec![0, 3],
            needle: 2,
            page_bytes: 5,
            find_limit: 3,
            redirect: false,
            age_cache: false,
        },
    ]
}

fn text(atoms: &[u8]) -> String {
    let mut output = String::new();
    for atom in atoms {
        output.push_str(ATOMS[usize::from(*atom) % ATOMS.len()]);
    }
    output
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

struct Loopback {
    port: u16,
    source_hits: Arc<AtomicUsize>,
    redirect_hits: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Loopback {
    fn start(initial: String, replacement: String) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let source_hits = Arc::new(AtomicUsize::new(0));
        let redirect_hits = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let sources = Arc::clone(&source_hits);
        let redirects = Arc::clone(&redirect_hits);
        let stopping = Arc::clone(&stop);
        let join = thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let mut request = [0_u8; 2048];
                        let length = stream.read(&mut request).unwrap_or(0);
                        let first = String::from_utf8_lossy(&request[..length]);
                        let path = first.split_whitespace().nth(1).unwrap_or("");
                        if path == "/redirect" {
                            redirects.fetch_add(1, Ordering::SeqCst);
                            let _ = stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: /source\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        } else if path == "/cooldown" {
                            let _ = stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: /retry\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        } else if path == "/retry" {
                            let _ = stream.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 7\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        } else if path == "/unknown" {
                            let _ = stream.write_all(b"HTTP/1.1 599 Unknown\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        } else if path == "/source" {
                            let ordinal = sources.fetch_add(1, Ordering::SeqCst);
                            let body = if ordinal == 0 { &initial } else { &replacement };
                            let header = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.write_all(body.as_bytes());
                        } else {
                            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self { port, source_hits, redirect_hits, stop, join: Some(join) })
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    fn hits(&self) -> (usize, usize) {
        (self.source_hits.load(Ordering::SeqCst), self.redirect_hits.load(Ordering::SeqCst))
    }
}

impl Drop for Loopback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key].as_str().ok_or_else(|| format!("missing text field {key}"))
}

fn page_check(
    store: &Store,
    reference: &str,
    expected: &str,
    maximum: usize,
) -> Result<usize, String> {
    let mut gathered = String::new();
    let mut start = 0_usize;
    let mut pages = 0;
    loop {
        let page = store
            .read(&json!({"source":reference,"offset":start,"max_bytes":maximum}))
            .map_err(|e| e.to_string())?;
        let end_limit = start.saturating_add(maximum).min(expected.len());
        let end = (start..=end_limit)
            .rev()
            .find(|n| expected.is_char_boundary(*n))
            .ok_or("no UTF-8 boundary in page interval")?;
        let fragment = field(&page, "text")?;
        if fragment != &expected[start..end]
            || page["offset"] != start
            || page["total_bytes"] != expected.len()
            || page["text_sha256"] != digest(expected.as_bytes())
            || page["start_line"]
                != 1 + expected[..start].bytes().filter(|byte| *byte == b'\n').count()
            || page["end_line"] != 1 + expected[..end].bytes().filter(|byte| *byte == b'\n').count()
        {
            return Err(format!("web source page at {start} differs from byte oracle: {page}"));
        }
        gathered.push_str(fragment);
        pages += 1;
        match page["next_offset"].as_u64() {
            Some(next) => {
                let next = usize::try_from(next).map_err(|e| e.to_string())?;
                if next != end || next <= start || pages > expected.len() + 1 {
                    return Err(format!("web source continuation did not advance: {page}"));
                }
                start = next;
            }
            None => break,
        }
    }
    if gathered != expected {
        return Err("web source pages did not reassemble exact text".into());
    }
    Ok(pages)
}

fn independent_matches(text: &str, query: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut cursor = 0;
    while cursor < text.len() {
        if text[cursor..].starts_with(query) {
            positions.push(cursor);
            cursor += query.len();
        } else {
            cursor += text[cursor..].chars().next().map_or(1, char::len_utf8);
        }
    }
    positions
}

fn find_check(
    store: &Store,
    reference: &str,
    expected: &str,
    query: &str,
    maximum: usize,
) -> Result<usize, String> {
    let wanted = independent_matches(expected, query);
    let mut got = Vec::new();
    let mut start = 0_usize;
    for _ in 0..=wanted.len() + 1 {
        let result = store
            .find(&json!({"source":reference,"query":query,"offset":start,"max_results":maximum}))
            .map_err(|e| e.to_string())?;
        let matches = result["matches"].as_array().ok_or("find omitted matches array")?;
        for item in matches {
            let offset = usize::try_from(item["offset"].as_u64().ok_or("match offset absent")?)
                .map_err(|e| e.to_string())?;
            if item["line"] != 1 + expected[..offset].bytes().filter(|byte| *byte == b'\n').count()
                || !field(item, "excerpt")?.contains(query)
            {
                return Err(format!("web find match metadata disagreed: {item}"));
            }
            got.push(offset);
        }
        match result["next_offset"].as_u64() {
            Some(next) => {
                let next = usize::try_from(next).map_err(|e| e.to_string())?;
                if next <= start || matches.is_empty() {
                    return Err(format!("web find continuation did not advance: {result}"));
                }
                start = next;
            }
            None => break,
        }
    }
    if got != wanted {
        return Err(format!("web find offsets differ: expected {wanted:?}, got {got:?}"));
    }
    Ok(got.len())
}

pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.atoms.len() > 64
        || case.replacement.len() > 32
        || case.page_bytes < 4
        || case.find_limit == 0
    {
        return Err("web source fixture bounds exceeded".into());
    }
    let first = text(&case.atoms);
    let mut second = text(&case.replacement);
    second.push('!');
    if first == second {
        second.push('?');
    }
    let server = Loopback::start(first.clone(), second.clone())?;
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let store = Store::new(root.path().join("sources")).map_err(|e| e.to_string())?;
    let url = server.url(if case.redirect { "/redirect" } else { "/source" });
    let first_page = store
        .open(&json!({"url":url,"max_bytes":case.page_bytes,"mode":"http"}), &|| false)
        .map_err(|e| e.to_string())?;
    let reference = field(&first_page, "source")?.to_owned();
    if first_page["cached"] != false
        || first_page["requested_url"] != url
        || first_page["url"] != server.url("/source")
        || first_page["raw_sha256"] != digest(first.as_bytes())
    {
        return Err(format!("initial web source metadata differs: {first_page}"));
    }
    let (source_hits, redirect_hits) = server.hits();
    if source_hits != 1 || redirect_hits != usize::from(case.redirect) {
        return Err("initial web fetch count differs".into());
    }
    let mut metrics = crate::Metrics::new();
    let maximum = usize::from(case.page_bytes);
    let pages = page_check(&store, &reference, &first, maximum)?;
    *metrics.entry("source_pages".into()).or_default() += pages as u64;
    let needle = NEEDLES[usize::from(case.needle) % NEEDLES.len()];
    let found = find_check(&store, &reference, &first, needle, usize::from(case.find_limit))?;
    *metrics.entry("literal_matches".into()).or_default() += found as u64;
    let cached = store
        .open(&json!({"url":url,"max_bytes":case.page_bytes,"mode":"http"}), &|| false)
        .map_err(|e| e.to_string())?;
    if cached["cached"] != true || cached["source"] != reference || server.hits().0 != 1 {
        return Err("recent source was not reused without network I/O".into());
    }
    *metrics.entry("cache_hits".into()).or_default() += 1;
    if store.open(&json!({"url":url,"refresh":true,"mode":"http"}), &|| true).is_ok()
        || server.hits().0 != 1
    {
        return Err("cancelled refresh reached the network or succeeded".into());
    }
    let fresh = store
        .open(&json!({"url":url,"refresh":true,"mode":"http"}), &|| false)
        .map_err(|e| e.to_string())?;
    let fresh_reference = field(&fresh, "source")?.to_owned();
    if fresh["cached"] != false
        || server.hits().0 != 2
        || field(&fresh, "text")? != &second[..second.len().min(32 * 1024)]
    {
        return Err("refresh failed to fetch new exact source".into());
    }
    page_check(&store, &reference, &first, maximum)?;
    page_check(&store, &fresh_reference, &second, maximum)?;
    *metrics.entry("refreshed_sources".into()).or_default() += 1;
    if case.age_cache {
        let key = format!("http:{url}");
        let index =
            root.path().join("sources").join(format!("index-{}.json", digest(key.as_bytes())));
        std::fs::write(
            &index,
            serde_json::to_vec(&json!({"source":fresh_reference,"time":0}))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let aged =
            store.open(&json!({"url":url,"mode":"http"}), &|| false).map_err(|e| e.to_string())?;
        if aged["cached"] != false || server.hits().0 != 3 {
            return Err("expired source index did not force network fetch".into());
        }
        *metrics.entry("expired_cache_fetches".into()).or_default() += 1;
    }
    let bad_offsets = [
        first.len() + 1,
        (1..first.len()).find(|n| !first.is_char_boundary(*n)).unwrap_or(first.len() + 1),
    ];
    for bad in bad_offsets {
        if store.read(&json!({"source":reference,"offset":bad})).is_ok()
            || store.find(&json!({"source":reference,"query":needle,"offset":bad})).is_ok()
        {
            return Err(format!("invalid source byte offset {bad} was accepted"));
        }
    }
    if store.rendered(&url, &url, "<h1>denied</h1>", 403, &json!({})).is_ok() {
        return Err("HTTP 403 rendered page was retained".into());
    }
    *metrics.entry("failed_pages_rejected".into()).or_default() += 1;
    // A 429 starts a host cooldown; check the independent 599 response first.
    for (path, status) in [("/unknown", "HTTP 599"), ("/cooldown", "HTTP 429")] {
        let failed_url = server.url(path);
        let failure = store
            .open(&json!({"url":failed_url,"mode":"http"}), &|| false)
            .err()
            .ok_or_else(|| format!("{status} page was retained"))?
            .to_string();
        if !failure.contains(status) || (path == "/cooldown" && !failure.contains("Retry-After: 7"))
        {
            return Err(format!("{status} failure lost status or cooldown detail: {failure}"));
        }
        let key = format!("http:{failed_url}");
        let index =
            root.path().join("sources").join(format!("index-{}.json", digest(key.as_bytes())));
        if index.exists() {
            return Err(format!("{status} failure created a successful cache index"));
        }
        *metrics.entry("failed_pages_rejected".into()).or_default() += 1;
    }
    for invalid_url in [
        "file:///tmp/page.html",
        "data:text/plain,hello",
        "relative/page",
        "http://user:secret@127.0.0.1/private",
    ] {
        if store.open(&json!({"url":invalid_url,"mode":"http"}), &|| false).is_ok() {
            return Err(format!("invalid source URL was accepted: {invalid_url}"));
        }
    }
    *metrics.entry("invalid_urls_rejected".into()).or_default() += 4;
    let source_path = root.path().join("sources").join(format!("{reference}.json"));
    let mut corrupted = std::fs::read(&source_path).map_err(|e| e.to_string())?;
    if let Some(first_byte) = corrupted.first_mut() {
        *first_byte ^= 1;
    }
    std::fs::write(&source_path, corrupted).map_err(|e| e.to_string())?;
    if store.read(&json!({"source":reference})).is_ok() {
        return Err("modified retained source passed integrity check".into());
    }
    page_check(&store, &fresh_reference, &second, maximum)?;
    *metrics.entry("corrupted_sources_rejected".into()).or_default() += 1;
    Ok(metrics)
}
