//! Source-mounted parser probe for the launcher's private release identity logic.

#[derive(Debug)]
enum LauncherError {
    Update(String),
}

#[path = "stage7_release_probe/update.rs"]
mod update;

fn main() {
    let tag = std::env::args().nth(1).unwrap_or_default();
    let (version, error) = update::version(&tag);
    println!("{}", serde_json::json!({"tag":tag,"version":version,"error":error}));
}
