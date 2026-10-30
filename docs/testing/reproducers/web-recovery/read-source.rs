//! Minimal real consumer of retained web sources, driven through JSON lines.
use peritus_tools_web::Store;
use serde_json::{Value, json};
use std::io::{self, BufRead};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args().nth(1).ok_or("source directory argument missing")?;
    let store = Store::new(root.into())?;
    for line in io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        let result = match request["action"].as_str().unwrap_or("open") {
            "read" => store.read(&request),
            "find" => store.find(&request),
            "selected" => store.browser_document(
                request["url"].as_str().ok_or("url missing")?,
                request["url"].as_str().ok_or("url missing")?,
                request["html"].as_str().ok_or("HTML missing")?, &request),
            _ => store.open(&request, &|| false),
        };
        println!("{}", match result {
            Ok(value) => value,
            Err(error) => error.page_failure().map_or_else(
                ||json!({"success":false,"error":error.to_string()}),
                |failure|failure.observation()),
        });
    }
    Ok(())
}
