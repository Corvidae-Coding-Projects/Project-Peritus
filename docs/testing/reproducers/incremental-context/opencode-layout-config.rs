//! Operate the public daemon route decoder; no network request or credential resolution.
use peritus_daemon::{ProviderDeclaration, ProviderRoute};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Value = serde_json::from_slice(&std::fs::read(
        std::env::args().nth(1).ok_or("route file required")?,
    )?)?;
    let output = match serde_json::from_value::<ProviderRoute>(input) {
        Err(error) => json!({"rejected":"decode", "detail":error.to_string()}),
        Ok(route) => match route.declaration() {
            Err(error) => json!({"rejected":"declaration", "detail":error.to_string()}),
            Ok(ProviderDeclaration::OpenCodeRuntime { request_layout, profile, .. }) => {
                json!({"layout":format!("{request_layout:?}"), "model":profile.model().as_str()})
            }
            Ok(_) => json!({"other_adapter":true}),
        },
    };
    println!("{output}");
    Ok(())
}
