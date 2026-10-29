// Ordinary consumer of full production managed-workspace observations. No synthetic snapshots.
use peritus_workspace::task_candidate::managed::ManagedBaseline;
use serde_json::{Value, json};
use std::{
    error::Error,
    io::{BufRead as _, Write as _},
    path::Path,
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn observe(request: &Value) -> Result<Value> {
    let root = Path::new(request["root"].as_str().ok_or("root required")?);
    let started = Instant::now();
    for attempt in 1..=3 {
        let observed = match request["mode"].as_str() {
            Some("legacy") => {
                let before = ManagedBaseline::repository_fingerprint(root)?;
                let source = ManagedBaseline::source_digest(root)?;
                let after = ManagedBaseline::repository_fingerprint(root)?;
                (before == after).then_some((after, source))
            }
            Some("shared") => {
                let before = ManagedBaseline::repository_and_source_digests(root)?;
                let after = ManagedBaseline::repository_and_source_digests(root)?;
                (before == after).then_some(after)
            }
            _ => return Err("unknown observation mode".into()),
        };
        if let Some((repository, source)) = observed {
            return Ok(json!({"repository":repository,"source":source.into_bytes(),
                "attempts":attempt,"elapsed_seconds":started.elapsed().as_secs_f64()}));
        }
    }
    Err("workspace changed repeatedly while observing exact identities".into())
}

fn main() -> Result<()> {
    for line in std::io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        let response = match observe(&request) {
            Ok(value) => value,
            Err(error) => json!({"error":error.to_string()}),
        };
        println!("{response}");
        std::io::stdout().flush()?;
    }
    Ok(())
}
