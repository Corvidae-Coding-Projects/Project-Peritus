//! Real public-library consumer: callback cancellation against a slow HTTP document, then recovery.
//! Arguments: STORE_DIRECTORY SLOW_URL FAST_URL. The HTTP server is an ordinary real fixture.

use peritus_tools_web::Store;
use serde_json::json;
use std::{path::PathBuf, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let store = Store::new(PathBuf::from(&arguments[1]))?;
    let first = store.open(&json!({"url":arguments[2]}), &|| true);
    assert!(first.is_err_and(|error| error.to_string().contains("cancelled")));
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&cancelled);
    let start = Instant::now();
    let outcome = std::thread::scope(|scope| {
        scope.spawn(|| { std::thread::sleep(Duration::from_millis(100)); signal.store(true, Ordering::Release); });
        store.open(&json!({"url":arguments[2]}), &|| cancelled.load(Ordering::Acquire))
    });
    assert!(outcome.is_err_and(|error| error.to_string().contains("cancelled")));
    assert!(start.elapsed() < Duration::from_secs(3));
    let page = store.open(&json!({"url":arguments[3]}), &|| false)?;
    assert!(page["text"].as_str().is_some_and(|text| text.contains("AFTER-CANCELLATION")));
    let found = store.find(&json!({"source":page["source"],"query":"AFTER-CANCELLATION"}))?;
    assert_eq!(found["matches"].as_array().map(Vec::len), Some(1));
    println!("{}", json!({"cancelled_before_request":true,"cancelled_during_transfer":true,"recovered":true,"elapsed_ms":start.elapsed().as_millis()}));
    Ok(())
}
