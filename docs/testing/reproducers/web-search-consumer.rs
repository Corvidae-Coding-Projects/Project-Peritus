//! Real metasearch public consumer. Arguments: STORE_DIRECTORY [JSON_ARGUMENTS].
//! With JSON_ARGUMENTS, forwards the exact request; without it, exercises cancellation/recovery.
use peritus_tools_web::Store;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let store = Store::new(PathBuf::from(&args[1]))?;
    if let Some(arguments) = args.get(2) {
        let request: Value = serde_json::from_str(arguments)?;
        println!("{}", store.search(&request, &|| false)?);
        return Ok(());
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    let (active, waiting) = std::thread::scope(|scope| {
        let active = scope.spawn(|| store.search(&json!({"query":"Rust ownership documentation cancellation active"}), &|| cancelled.load(Ordering::Acquire)));
        std::thread::sleep(Duration::from_millis(30));
        let waiting = scope.spawn(|| store.search(&json!({"query":"Tokio spawn documentation cancellation waiting"}), &|| cancelled.load(Ordering::Acquire)));
        std::thread::sleep(Duration::from_millis(80));
        cancelled.store(true, Ordering::Release);
        (active.join().expect("consumer active thread"), waiting.join().expect("consumer waiting thread"))
    });
    assert!(active.is_err_and(|e| e.to_string().contains("cancelled")));
    assert!(waiting.is_err_and(|e| e.to_string().contains("cancelled")));
    let cancellation_ms = started.elapsed().as_millis();
    assert!(cancellation_ms < 2000);
    let before = store.search(&json!({"query":"Rust"}), &|| true);
    assert!(before.is_err_and(|e| e.to_string().contains("cancelled")));
    for request in [json!({"query":" "}),json!({"query":"Rust","page":2}),json!({"query":"Rust","engines":"unknown"}),json!({"query":"Rust","language":"en"}),json!({"query":"Rust","time_range":"day"})] {
        assert!(store.search(&request, &|| false).is_err(), "invalid request: {request}");
    }
    let result = store.search(&json!({"query":"Rust ownership official documentation"}), &|| false)?;
    assert!(result["results"].as_array().is_some_and(|items| items.iter().any(|item| item["url"].as_str().is_some_and(|url| url.starts_with("https://doc.rust-lang.org/")))));
    println!("{}", json!({"cancelled_active_and_waiting":true,"cancellation_ms":cancellation_ms,"invalid_requests_rejected":5,"recovered":result}));
    Ok(())
}
